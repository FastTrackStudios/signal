// The browser guitar rig, page side: audio context, input (a guitar through
// the mic input, or the demo DI loop), assets, and the NAM worker pool.
//
//   const rig = await GuitarRig.create({ base: '/worklet/guitar/' });
//   await rig.useInput('demo');                       // or 'mic'
//   await rig.loadChain(blocks, fetchBytes);          // RigBlock objects
//
// Where each model runs is planned in wasm (planChain, the same code the
// worklet plans with): inline on the render thread while it fits, Amp R on a
// worker in parallel with Amp L, and anything else one quantum behind on its
// own worker. The planner is fed the costs the workers measure, and when the
// worklet reports misses the page re-plans with a smaller budget — so the
// rig trades latency, never dropouts.
//
// Needs cross-origin isolation (COOP: same-origin, COEP: require-corp) for
// SharedArrayBuffer. Without it, every model runs inline, at a quarter size
// if the board is long — still playable, just less accurate.

import init, * as wasm from './signal_guitar_worklet.js';

const MAX_WORKERS = 6; // two amps and four drive slots, the rig's most

export class GuitarRig {
  static async create({ base = './', latencyHint = 'interactive', budgetShare = 0.55 } = {}) {
    const rig = new GuitarRig();
    rig.base = base;
    rig.budgetShare = budgetShare;
    rig.wasmBytes = await (await fetch(base + 'signal_guitar_worklet_bg.wasm')).arrayBuffer();
    await init({ module_or_path: rig.wasmBytes.slice(0) });
    rig.ctx = new AudioContext({ latencyHint });
    await rig.ctx.audioWorklet.addModule(base + 'guitar_processor.js');
    rig.node = new AudioWorkletNode(rig.ctx, 'signal-guitar-rig', {
      numberOfInputs: 1,
      numberOfOutputs: 1,
      outputChannelCount: [2],
      channelCount: 1,
      channelCountMode: 'explicit',
    });
    rig.node.connect(rig.ctx.destination);
    rig.replies = new Map();
    rig.nextReply = 1;
    const ready = new Promise((resolve) => {
      rig.node.port.onmessage = (e) => {
        const m = e.data;
        if (m.kind === 'ready') resolve();
        else rig.settle(rig.replies, m);
      };
    });
    rig.node.port.postMessage({ kind: 'init', wasmBytes: rig.wasmBytes.slice(0), sampleRate: rig.ctx.sampleRate });
    await ready;
    rig.isolated = globalThis.crossOriginIsolated === true && typeof SharedArrayBuffer !== 'undefined';
    rig.workers = [];
    rig.costs = new Map(); // model key → measured µs per quantum
    rig.sentToWorklet = new Set();
    // What the render thread may spend in a quantum, all told; the models
    // get what the rest of the playing chain leaves (see planBudget).
    rig.quantumUs = (128 / rig.ctx.sampleRate) * 1e6;
    rig.renderShare = 0.8;
    rig.blockCosts = new Map(); // block JSON (non-model) → µs per quantum
    rig.input = null;
    return rig;
  }

  settle(map, m) {
    const p = map.get(m.replyTo);
    if (!p) return;
    map.delete(m.replyTo);
    if (m.error !== undefined) p.reject(new Error(m.error));
    else p.resolve(m.value);
  }

  ask(msg, transfer = []) {
    const replyTo = this.nextReply++;
    return new Promise((resolve, reject) => {
      this.replies.set(replyTo, { resolve, reject });
      this.node.port.postMessage({ ...msg, replyTo }, transfer);
    });
  }

  // ── Input ────────────────────────────────────────────────────────────

  /// 'mic' — the guitar through the default input, all processing off (a
  /// DI must arrive untouched). 'demo' — the reference DI, looped.
  async useInput(kind, demoUrl) {
    await this.ctx.resume();
    this.input?.disconnect();
    this.input = null;
    if (kind === 'mic') {
      const stream = await navigator.mediaDevices.getUserMedia({
        audio: { echoCancellation: false, noiseSuppression: false, autoGainControl: false, channelCount: 1, latency: 0 },
      });
      this.input = this.ctx.createMediaStreamSource(stream);
    } else if (kind === 'demo') {
      const bytes = await (await fetch(demoUrl ?? this.base + 'di-reference.wav')).arrayBuffer();
      const buf = await this.ctx.decodeAudioData(bytes);
      const src = this.ctx.createBufferSource();
      src.buffer = buf;
      src.loop = true;
      src.start();
      this.input = src;
    }
    this.input?.connect(this.node);
  }

  // ── Workers ──────────────────────────────────────────────────────────

  async worker(i) {
    while (this.workers.length <= i) {
      const w = new Worker(this.base + 'nam_worker.js', { type: 'module' });
      const entry = { w, sab: new SharedArrayBuffer(wasm.sabBytes()), replies: new Map(), next: 1, key: null };
      entry.ctl = new Int32Array(entry.sab, 0, 16);
      const ready = new Promise((resolve) => {
        w.onmessage = (e) => (e.data.kind === 'ready' ? resolve() : this.settle(entry.replies, e.data));
      });
      w.postMessage({ kind: 'init', wasmBytes: this.wasmBytes.slice(0) });
      await ready;
      this.workers.push(entry);
    }
    return this.workers[i];
  }

  askWorker(entry, msg) {
    const replyTo = entry.next++;
    return new Promise((resolve, reject) => {
      entry.replies.set(replyTo, { resolve, reject });
      entry.w.postMessage({ ...msg, replyTo });
      // Wake it out of serve(): raise CMD, then notify the wait.
      Atomics.store(entry.ctl, 4, 1);
      Atomics.notify(entry.ctl, 0);
    });
  }

  // ── Profiling ────────────────────────────────────────────────────────

  /// Measure whatever in `blocks` has not been measured — each model once
  /// (by key), each other block once (by its JSON) — on a worker of its
  /// own, so the render thread never runs a test signal.
  async profile(blocks, fetchBytes) {
    const blockKey = (b) => JSON.stringify({ ...b, bypassed: false });
    const todo = blocks.filter((b) => (b.nam ? !this.costs.has(b.nam) : !this.blockCosts.has(blockKey(b))));
    if (todo.length === 0) return;
    if (!this.profiler) {
      const w = new Worker(this.base + 'nam_worker.js', { type: 'module' });
      this.profiler = { w, replies: new Map(), next: 1, ctl: new Int32Array(new SharedArrayBuffer(64)) };
      const ready = new Promise((resolve) => {
        w.onmessage = (e) => (e.data.kind === 'ready' ? resolve() : this.settle(this.profiler.replies, e.data));
      });
      w.postMessage({ kind: 'init', wasmBytes: this.wasmBytes.slice(0) });
      await ready;
    }
    const assets = [];
    for (const b of todo) for (const k of [b.nam, b.ir]) if (k) assets.push([k, new Uint8Array(await fetchBytes(k))]);
    const rows = await this.askWorker(this.profiler, {
      kind: 'profile', blocksJson: JSON.stringify(todo), assets, sampleRate: this.ctx.sampleRate, quanta: 60,
    });
    todo.forEach((b, i) => {
      const us = Math.round(rows[i]?.us ?? 0);
      if (b.nam) this.costs.set(b.nam, us);
      else this.blockCosts.set(blockKey(b), us);
    });
  }

  /// The models' share of a quantum for `blocks`: the render budget less
  /// what the playing non-model blocks cost, with a margin for switching
  /// one more on.
  planBudget(blocks) {
    const blockKey = (b) => JSON.stringify({ ...b, bypassed: false });
    const fixed = blocks
      .filter((b) => !b.nam && !b.bypassed)
      .reduce((sum, b) => sum + (this.blockCosts.get(blockKey(b)) ?? 50), 0);
    return Math.max(0, Math.floor(this.quantumUs * this.renderShare - fixed * 1.15));
  }

  // ── Chains ───────────────────────────────────────────────────────────

  /// Play a bundle patch: its chain plus its trims.
  async loadPatch(patch, fetchBytes) {
    const r = await this.loadChain(patch.blocks, fetchBytes);
    this.node.port.postMessage({ kind: 'set_trims', inputDb: patch.input_trim_db ?? 0, outputDb: patch.output_trim_db ?? 0 });
    return r;
  }

  /// Play `blocks` (RigBlock objects, chain order). `fetchBytes(key)`
  /// resolves an asset key (a model or IR path) to its bytes.
  async loadChain(blocks, fetchBytes) {
    const blocksJson = JSON.stringify(blocks);
    if (this.isolated) await this.profile(blocks, fetchBytes);
    const costs = new Uint32Array(blocks.map((b) => (b.nam ? this.costs.get(b.nam) ?? 0 : 0)));
    // Without isolation there are no workers: everything inline.
    this.budgetUs = this.isolated ? Math.floor(this.planBudget(blocks) * (this.budgetScale ?? 1)) : 1e9;
    const budgetUs = this.budgetUs;
    const plan = JSON.parse(wasm.planChain(blocksJson, costs, budgetUs));

    await this.ask({ kind: 'clear_chain' });

    // Remote models first: their workers must be ready before the worklet
    // links to them.
    const remotes = [];
    let w = 0;
    for (const m of plan.models) {
      if (m.place === 'inline') continue;
      if (w >= MAX_WORKERS) throw new Error('more remote models than workers');
      const block = blocks[m.slot];
      const entry = await this.worker(w++);
      const bytes = entry.key === block.nam ? null : new Uint8Array(await fetchBytes(block.nam));
      const cost = await this.askWorker(entry, {
        kind: 'load', sab: entry.sab, key: block.nam, bytes, blockJson: JSON.stringify(block),
        sampleRate: this.ctx.sampleRate, modelSize: this.modelSize ?? 1, calibration: this.calibration ?? NaN,
      });
      entry.key = block.nam;
      this.costs.set(block.nam, cost);
      remotes.push([m.slot, entry.sab]);
    }

    // Everything that runs on the render thread: inline models and IRs.
    for (const [i, b] of blocks.entries()) {
      const remote = remotes.some(([slot]) => slot === i);
      for (const key of [remote ? null : b.nam, b.ir]) {
        if (!key || this.sentToWorklet.has(key)) continue;
        // Copied, not transferred: the caller may be caching these bytes.
        const bytes = new Uint8Array(await fetchBytes(key));
        await this.ask({ kind: 'asset', key, bytes });
        this.sentToWorklet.add(key);
      }
    }

    const latency = await this.ask({ kind: 'load_chain', blocksJson, costs, budgetUs, remotes });
    this.current = { blocks, fetchBytes, plan, latency };
    return { plan, addedLatencyFrames: latency, addedLatencyMs: (latency / this.ctx.sampleRate) * 1000 };
  }

  /// Each worker's model and what serving it costs, read straight from the
  /// shared buffers: running average and worst since the last call, µs.
  workerCosts() {
    return this.workers.map((e) => {
      const r = { key: e.key, avgUs: Atomics.load(e.ctl, 5), worstUs: Atomics.load(e.ctl, 15) };
      Atomics.store(e.ctl, 15, 0);
      return r;
    });
  }

  /// Level, misses and load; call on a timer. Re-plans once when workers
  /// keep missing: a smaller render budget moves more models off-thread.
  async stats() {
    const s = await this.ask({ kind: 'stats' });
    if (this.current && s.misses > 20 && !this.replanned) {
      this.replanned = true;
      this.budgetScale = 0.6;
      await this.loadChain(this.current.blocks, this.current.fetchBytes);
    }
    return s;
  }

  setParam(block, param, value) {
    return this.ask({ kind: 'set_param', block, param, value });
  }

  setBypass(block, bypass) {
    return this.ask({ kind: 'set_bypass', block, bypass });
  }

  /// NAM level calibration (the interface's full-scale dBu), or NaN.
  setCalibration(dbu) {
    this.calibration = dbu;
    this.node.port.postMessage({ kind: 'calibration', dbu });
  }

  /// Build A2 models at `size` (1 = full) from the next loadChain.
  setModelSize(size) {
    this.modelSize = size;
    this.node.port.postMessage({ kind: 'model_size', size });
  }

  /// What the browser adds on top of the rig: output buffering.
  get deviceLatencyMs() {
    return ((this.ctx.baseLatency ?? 0) + (this.ctx.outputLatency ?? 0)) * 1000;
  }
}
