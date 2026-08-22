// AudioWorkletProcessor hosting `KeysWorklet` (signal-keys-worklet) — the
// browser keys rig's audio thread. Main thread talks via port messages.
//
// The wasm glue is STATICALLY imported: AudioWorkletGlobalScope disallows
// dynamic `import()` (and has no `fetch`/`importScripts`), which is why the
// generic daw-standalone processor's `import(msg.glueUrl)` can never work
// here — its init throws and 'ready' never arrives. addModule loads this
// file as a module script, where static imports resolve relative to this
// URL, so the glue must be staged alongside (Justfile keys-worklet-wasm).
// The wasm BYTES still arrive in the init message — the worklet scope
// cannot fetch them itself.
//
// Message kinds (all with the `replyTo` convention — a message carrying
// `replyTo` gets `{ replyTo, value }` back):
//   init        { wasmBytes, sampleRate }            → 'ready'
//   attach_pack { key, bytes, replyTo? }             → true/error string
//   pack_segment { id, start, bytes }                  (fire-and-forget)
//   attach_pack_progressive { key, id, len, replyTo? } → true/error string
//   take_misses { replyTo }                          → [{id,start,len}, …]
//   open_lanes  { program, replyTo? }                → lane count
//   reload_lanes { replyTo? }                        → true/error string
//   midi        { bytes: [status, d1, d2] }
//   all_notes_off | panic
//   track_peaks { replyTo }                          → [peak, …] (0 = master)
//   audio_stats { replyTo }                          → { load, worstMs, quanta,
//                                                        sampleRate, voices,
//                                                        glitches, glitchFrames,
//                                                        worstHandlerMs,
//                                                        worstHandlerKind,
//                                                        warmDepth, pcmInserts,
//                                                        pcmRefused }
//   reset_audio_stats                                  (page-caused suspend)
//   set_track_volume { index, volume } | set_track_mute { index, muted }
//   play | pause | stop
//
// W12 — the decoder-worker port (init.warmPort, a transferred MessagePort
// wired straight to the page's decoder worker):
//   out: warm { requests: [{layer, path}, …] }   cold note-on misses
//   in:  pcm  { layer, path, channels, sampleRate, pcm: Float32Array,
//               chargePast }                     → insertPcm; replies
//        pcm_ack { path, layer, accepted, chargePast }
// The worklet NEVER decodes — message handlers share the audio thread, and
// a synchronous ogg decode here starved process() (the field bug W12
// fixed). All decode happens in the worker; only a bounded memcpy lands
// here, and the glitch counters above prove the budget is respected.
//
// Progressive packs (W7 range streaming): the PAGE allocates the id (its
// ids start at 2^20 so they never collide with this processor's own
// counter), pushes the plan's rank-0 segments via `pack_segment`, THEN
// sends `attach_pack_progressive` — the wasm-side open parses header +
// index through __ftsPackRead, so those bytes must already be present.
// Later segments stream in fire-and-forget; a read that lands in a hole
// records a miss (drained by `take_misses`, ~1 Hz from the page) and
// returns null — the engine drops that voice silently and retries on the
// next press, so NO reload is needed when the segment arrives.

import './worklet_polyfill.js'; // MUST precede the glue: TextDecoder/TextEncoder
import init, * as wasm from './signal_keys_worklet.js';

class KeysRigProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this.renderer = null;
    // Pack buffers stay HERE, on the worklet's JS heap — outside wasm's
    // 4 GB linear memory. wasm reads ranges through globalThis.__ftsPackRead
    // (installed once below); only decoded PCM enters linear memory.
    // Values are either a Uint8Array (whole pack, attach_pack) or a SPARSE
    // store { len, segs: [{start, bytes}] sorted by start } filled by
    // pack_segment messages (progressive packs).
    this.packs = new Map(); // id → Uint8Array | { len, segs }
    this.packIdsByKey = new Map(); // spec-path key → id (frees replaced buffers)
    this.nextPackId = 1;
    // Reads that fell in a sparse hole, for the page's take_misses poll.
    // Bounded + deduped by (id, start) — a held chord retrying a cold key
    // must not grow this without limit.
    this.misses = [];
    this.missKeys = new Set();
    // ── Render-load tracing (W8) ───────────────────────────────────────
    // The worklet clock is the polyfill's Date.now-backed performance
    // (~1 ms resolution) against a ~2.67 ms quantum, so a single render's
    // measurement is mostly 0-or-1 quantization noise. AGGREGATE instead:
    // sum measured render ms over a 250-quantum window (~0.7 s @ 48 kHz)
    // and divide by the window's AUDIO time (quanta × quantum-ms) — never
    // by wall time between process() calls, which includes the browser's
    // own callback pacing and would understate load. Per-window worst
    // single-quantum cost is kept too (coarse, but a spike that decodes or
    // GCs mid-render shows up as a multi-ms outlier). All preallocated
    // numbers — no per-quantum allocation.
    this.statWindowQuanta = 250;
    this.statQuantumMs = (128 / sampleRate) * 1000; // sampleRate: worklet global
    this.statRenderMs = 0;   // accumulating window
    this.statQuanta = 0;
    this.statWorstMs = 0;
    this.statLoad = 0;       // last COMPLETED window: render ms / audio ms
    this.statLoadWorstMs = 0;
    this.statTotalQuanta = 0; // monotonic, never reset
    // ── Underrun + handler tracing (W12) ──────────────────────────────
    // `currentFrame` (worklet global) advances by 128 per process() call;
    // a jump means the browser SKIPPED quanta — the output underran and
    // the player heard a dropout. This is the hard number that proves (or
    // disproves) glitch-free playback: it must stay 0 while playing.
    this.lastFrame = -1;
    this.glitches = 0;       // discontinuity episodes
    this.glitchFrames = 0;   // total frames skipped across them
    // Message handlers share the audio thread — a slow one starves
    // process() exactly like a slow render. Track the worst offender.
    this.worstHandlerMs = 0;
    this.worstHandlerKind = '';
    // The decoder worker's direct line (init.warmPort): warm requests out,
    // decoded PCM in — the page main thread never sits in this loop.
    this.warmPort = null;
    this.port.onmessage = (e) => this.timedHandle(e.data);
  }

  // Time every handler with the coarse worklet clock — slow handlers on
  // this thread ARE audio glitches, so they get first-class telemetry.
  // Only the synchronous span is measured (handleMessage awaits only in
  // 'init', whose wasm compile predates playback and would drown the
  // numbers that matter).
  timedHandle(msg) {
    const t0 = Date.now();
    const done = this.handleMessage(msg);
    if (msg?.kind !== 'init') {
      const dt = Date.now() - t0;
      if (dt > this.worstHandlerMs) {
        this.worstHandlerMs = dt;
        this.worstHandlerKind = msg?.kind ?? '?';
      }
    }
    return done;
  }

  // Insert one streamed segment into a sparse pack store, keeping `segs`
  // sorted by start and merging EXACTLY adjacent/overlapping neighbours so
  // whole-entry reads stay single-segment hits. Segments arrive in plan
  // rank order (not file order), so merges are rare and cheap.
  insertSegment(store, start, bytes) {
    const segs = store.segs;
    // Binary search for the insert position.
    let lo = 0, hi = segs.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (segs[mid].start < start) lo = mid + 1; else hi = mid;
    }
    segs.splice(lo, 0, { start, bytes });
    // Merge with the previous / next neighbour when contiguous — but ONLY
    // while the result stays small. A merge allocates and copies the joined
    // buffer, and this runs in a message handler, i.e. ON THE AUDIO THREAD:
    // with a half-streamed piano the joins grew into the tens of MB and this
    // handler measured 90 ms (~34 render quanta). Reads stitch across
    // segments anyway, so an unmerged store costs a little read work instead
    // of a render stall.
    const MERGE_CAP = 4 * 1024 * 1024;
    const mergeAt = (i) => {
      const a = segs[i], b = segs[i + 1];
      if (!a || !b) return false;
      if (a.start + a.bytes.byteLength !== b.start) return false;
      if (a.bytes.byteLength + b.bytes.byteLength > MERGE_CAP) return false;
      const joined = new Uint8Array(a.bytes.byteLength + b.bytes.byteLength);
      joined.set(a.bytes, 0);
      joined.set(b.bytes, a.bytes.byteLength);
      segs.splice(i, 2, { start: a.start, bytes: joined });
      return true;
    };
    if (lo > 0 && mergeAt(lo - 1)) lo -= 1;
    mergeAt(lo);
  }

  // Serve [offset, offset+len) out of a pack store. Whole-buffer packs
  // subarray directly; sparse packs return the single covering segment's
  // subarray (the common case — plan segments are whole entries), stitch
  // across contiguous segments when needed, and return null on any hole.
  readRange(id, buf, offset, len) {
    if (buf instanceof Uint8Array) {
      if (!(offset >= 0) || !(len >= 0) || offset + len > buf.byteLength) return null;
      return buf.subarray(offset, offset + len);
    }
    // Sparse store.
    if (!(offset >= 0) || !(len >= 0) || offset + len > buf.len) return null;
    const segs = buf.segs;
    // Last segment with start <= offset.
    let lo = 0, hi = segs.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (segs[mid].start <= offset) lo = mid + 1; else hi = mid;
    }
    let i = lo - 1;
    if (i < 0) return this.recordMiss(id, offset, len);
    const s = segs[i];
    const within = offset - s.start;
    if (within + len <= s.bytes.byteLength) {
      return s.bytes.subarray(within, within + len); // single-segment hit
    }
    // Stitch across contiguous segments into a scratch buffer.
    const out = new Uint8Array(len);
    let at = offset;
    const end = offset + len;
    while (at < end) {
      if (i >= segs.length) return this.recordMiss(id, offset, len);
      const seg = segs[i];
      if (seg.start > at || at >= seg.start + seg.bytes.byteLength) {
        return this.recordMiss(id, offset, len);
      }
      const from = at - seg.start;
      const take = Math.min(seg.bytes.byteLength - from, end - at);
      out.set(seg.bytes.subarray(from, from + take), at - offset);
      at += take;
      i += 1;
    }
    return out;
  }

  recordMiss(id, start, len) {
    const key = `${id}:${start}`;
    if (!this.missKeys.has(key) && this.misses.length < 128) {
      this.missKeys.add(key);
      this.misses.push({ id, start, len });
    }
    return null;
  }

  installPackRead() {
    if (globalThis.__ftsPackRead) return;
    // (id, offset, len) → Uint8Array (no copy on the fast paths — wasm
    // copies out of it), or null when the id is unknown, the range is out
    // of bounds, or a sparse pack has a hole there (recorded as a miss).
    globalThis.__ftsPackRead = (id, offset, len) => {
      const buf = this.packs.get(id);
      if (!buf) return null;
      return this.readRange(id, buf, offset, len);
    };
  }

  reply(msg, value) {
    if (msg.replyTo !== undefined) {
      this.port.postMessage({ replyTo: msg.replyTo, value });
    }
  }

  async handleMessage(msg) {
    try {
      switch (msg.kind) {
        case 'init': {
          // With a shared memory (W13) the worklet must instantiate over
          // THAT heap, not its own — otherwise the streamer workers decode
          // into memory this thread never reads.
          await init(msg.memory
            ? { module_or_path: msg.wasmBytes, memory: msg.memory }
            : msg.wasmBytes);
          this.renderer = new wasm.KeysWorklet(msg.sampleRate);
          // BEFORE anything decodes: the page's decoded-PCM ceiling. The
          // browser has no environment for FTS_PRELOAD_BUDGET_MB, so it
          // rides the init message.
          if (msg.pcmBudgetMb && typeof this.renderer.setPcmBudgetMb === 'function') {
            this.renderer.setPcmBudgetMb(msg.pcmBudgetMb);
          }
          // The decoder worker's MessagePort (W12): warm requests flow out,
          // decoded PCM flows back in — worklet ↔ worker directly.
          if (msg.warmPort) {
            this.warmPort = msg.warmPort;
            this.warmPort.onmessage = (e) => this.timedHandle(e.data);
          }
          this.port.postMessage({ kind: 'ready' });
          break;
        }
        case 'pcm_chunk': {
          // One bounded piece of a decoded sample (see insertPcmChunk).
          // Only the last piece publishes it and gets acked.
          if (!this.renderer) break;
          const ok = this.renderer.insertPcmChunk(
            msg.layer, msg.path, msg.channels, msg.sampleRate,
            msg.offset, msg.pcm, !!msg.last, !!msg.chargePast,
          );
          if (msg.last || !ok) {
            this.warmPort?.postMessage({
              kind: 'pcm_ack', path: msg.path, layer: msg.layer,
              accepted: ok, chargePast: !!msg.chargePast,
            });
          }
          break;
        }
        case 'pcm': {
          // Decoded PCM from the worker: one memcpy into wasm + a map
          // insert — never a decode. `chargePast` marks a note-driven warm
          // (beats the budget ceiling; the engine sheds afterwards);
          // coverage fill sends false and pauses when refused.
          if (!this.renderer) break;
          const accepted = this.renderer.insertPcm(
            msg.layer, msg.path, msg.channels, msg.sampleRate,
            msg.pcm, !!msg.chargePast,
          );
          this.warmPort?.postMessage({
            kind: 'pcm_ack', path: msg.path, layer: msg.layer,
            accepted, chargePast: !!msg.chargePast,
          });
          break;
        }
        case 'attach_pack': {
          let value = true;
          try {
            // Attach BY HANDLE: the transferred ArrayBuffer never enters
            // wasm linear memory. Keep it in this.packs and hand wasm only
            // an id + length; ranged reads come back through __ftsPackRead.
            const bytes = new Uint8Array(msg.bytes);
            const id = this.nextPackId++;
            this.packs.set(id, bytes);
            this.installPackRead();
            try {
              this.renderer.attachPackExternal(msg.key, id, bytes.byteLength);
            } catch (e) {
              this.packs.delete(id); // don't strand the buffer on failure
              throw e;
            }
            // A re-attach under the same key replaced the registry entry —
            // free the superseded buffer (engines already built over it keep
            // reading it until reload_lanes swaps them; brief overlap only).
            const prev = this.packIdsByKey.get(msg.key);
            if (prev !== undefined && prev !== id) this.packs.delete(prev);
            this.packIdsByKey.set(msg.key, id);
          } catch (e) {
            // A Rust panic traps as a bare 'unreachable' — after it, the
            // whole wasm instance traps, so the hook stashes the message on
            // the worklet scope's globalThis where JS can still read it.
            const panic = globalThis.__ftsPanic ?? '';
            value = String(e) + (panic ? ` :: ${panic}` : '');
          }
          this.reply(msg, value);
          break;
        }
        case 'attach_pack_shared': {
          // W14: bytes in a SharedArrayBuffer the page also gave to the
          // streamer workers. Identical to attach_pack from this thread's
          // point of view — a Uint8Array over the buffer, served by
          // __ftsPackRead — except the SAME bytes are now readable by
          // every thread, which is what lets the workers do their jobs.
          let value = true;
          try {
            const bytes = new Uint8Array(msg.sab);
            this.packs.set(msg.id, bytes);
            this.installPackRead();
            try {
              this.renderer.attachPackExternal(msg.key, msg.id, bytes.byteLength);
            } catch (e) {
              this.packs.delete(msg.id);
              throw e;
            }
            const prev = this.packIdsByKey.get(msg.key);
            if (prev !== undefined && prev !== msg.id) this.packs.delete(prev);
            this.packIdsByKey.set(msg.key, msg.id);
          } catch (e) {
            const panic = globalThis.__ftsPanic ?? '';
            value = String(e) + (panic ? ` :: ${panic}` : '');
          }
          this.reply(msg, value);
          break;
        }
        case 'pack_segment': {
          // Fire-and-forget: one streamed plan segment for a progressive
          // pack. Auto-creates the sparse store so the page can push
          // rank-0 segments BEFORE attach_pack_progressive.
          let store = this.packs.get(msg.id);
          if (!(store instanceof Object) || store instanceof Uint8Array) {
            store = { len: 0, segs: [] };
            this.packs.set(msg.id, store);
          }
          this.insertSegment(store, msg.start, new Uint8Array(msg.bytes));
          break;
        }
        case 'attach_pack_progressive': {
          let value = true;
          try {
            const store = this.packs.get(msg.id);
            if (!store || store instanceof Uint8Array) {
              throw new Error(`no sparse pack ${msg.id} — push its rank-0 segments first`);
            }
            store.len = msg.len;
            this.installPackRead();
            // The wasm open parses header + index through __ftsPackRead —
            // it fails cleanly (error string back) if rank-0 is missing.
            this.renderer.attachPackExternal(msg.key, msg.id, msg.len);
            const prev = this.packIdsByKey.get(msg.key);
            if (prev !== undefined && prev !== msg.id) this.packs.delete(prev);
            this.packIdsByKey.set(msg.key, msg.id);
          } catch (e) {
            const panic = globalThis.__ftsPanic ?? '';
            value = String(e) + (panic ? ` :: ${panic}` : '');
          }
          this.reply(msg, value);
          break;
        }
        case 'take_misses': {
          const taken = this.misses;
          this.misses = [];
          this.missKeys.clear();
          this.reply(msg, taken);
          break;
        }
        case 'open_lanes': {
          let value;
          try {
            value = this.renderer.openLanes(msg.program);
          } catch (e) {
            value = String(e);
          }
          this.reply(msg, value);
          break;
        }
        case 'reload_lanes': {
          let value = true;
          try {
            // `key` present → rebuild ONLY the lanes that play that pack.
            // The whole-program reload measured >500 ms on this (audio)
            // thread with the full Worship set; scoping it to the pack that
            // just arrived keeps the stall bounded and leaves every other
            // lane sounding. Without a key (or if the export is missing) it
            // falls back to the full reload.
            if (msg.key && typeof this.renderer.reloadLanesForPack === 'function') {
              this.renderer.reloadLanesForPack(msg.key);
            } else {
              this.renderer.reloadLanes();
            }
          } catch (e) {
            value = String(e);
          }
          this.reply(msg, value);
          break;
        }
        case 'midi': {
          const [s, d1, d2] = msg.bytes;
          this.renderer?.midi(s, d1, d2);
          break;
        }
        case 'reset_audio_stats':
          // The page sends this after a context suspend/resume it caused —
          // the currentFrame jump across a suspension is not an underrun.
          this.lastFrame = -1;
          this.glitches = 0;
          this.glitchFrames = 0;
          this.worstHandlerMs = 0;
          this.worstHandlerKind = '';
          break;
        case 'all_notes_off':
          this.renderer?.allNotesOff();
          break;
        case 'panic':
          this.renderer?.panic();
          break;
        case 'track_peaks':
          this.reply(msg, this.renderer ? Array.from(this.renderer.trackPeaks()) : []);
          break;
        case 'audio_stats': {
          // `load` / `worstMs` are the last COMPLETED window's numbers —
          // stable for ~0.7 s at a time, which is exactly the cadence a
          // panel wants. `voices` is -1 when the renderer (or the export)
          // is not there yet.
          let voices = -1;
          try {
            if (this.renderer && typeof this.renderer.activeVoices === 'function') {
              voices = this.renderer.activeVoices();
            }
          } catch (_e) {
            // A trapped wasm instance must not take the stats reply down.
          }
          let warmDepth = 0;
          let pcmInserts = 0;
          let pcmRefused = 0;
          let pcmUsedMb = 0;
          let pcmLimitMb = 0;
          let reloadLanes = 0;
          let reloadFull = 0;
          let zonesOpened = 0;
          let streamerDepth = 0;
          let openFailed = 0;
          let openDepth = 0;
          let streamerDropped = 0;
          let opensQueued = 0;
          try {
            if (this.renderer) {
              if (typeof this.renderer.warmQueueDepth === 'function') {
                warmDepth = this.renderer.warmQueueDepth();
              }
              if (typeof this.renderer.pcmInsertCount === 'function') {
                pcmInserts = this.renderer.pcmInsertCount();
                pcmRefused = this.renderer.pcmRefusedCount();
              }
              if (typeof this.renderer.pcmUsedMb === 'function') {
                pcmUsedMb = this.renderer.pcmUsedMb();
                pcmLimitMb = this.renderer.pcmLimitMb();
              }
              if (typeof this.renderer.reloadFullCount === 'function') {
                reloadLanes = this.renderer.reloadLaneCount();
                reloadFull = this.renderer.reloadFullCount();
              }
            }
            // W13 shared-memory path: zones opened by the streamer
            // workers straight into the caches this thread reads.
            if (typeof wasm.streamerOpened === 'function') {
              zonesOpened = wasm.streamerOpened();
              streamerDepth = wasm.streamerDepth();
              streamerDropped = wasm.streamerDropped();
              if (typeof wasm.streamerOpenFailed === 'function') {
                openFailed = wasm.streamerOpenFailed();
                openDepth = wasm.streamerOpenDepth();
              }
            }
            // Enqueued BY this thread — separates "nothing was queued"
            // from "queued but nobody drained it".
            if (typeof this.renderer.opensQueued === 'function') {
              opensQueued = this.renderer.opensQueued();
            }
          } catch (_e) { /* trapped wasm must not take the reply down */ }
          this.reply(msg, {
            load: this.statLoad,
            worstMs: this.statLoadWorstMs,
            quanta: this.statTotalQuanta,
            sampleRate,
            voices,
            // ── W12 realtime-safety proof ──
            glitches: this.glitches,
            glitchFrames: this.glitchFrames,
            worstHandlerMs: this.worstHandlerMs,
            worstHandlerKind: this.worstHandlerKind,
            warmDepth,
            pcmInserts,
            pcmRefused,
            pcmUsedMb,
            pcmLimitMb,
            reloadLanes,
            reloadFull,
            zonesOpened,
            streamerDepth,
            streamerDropped,
            openFailed,
            openDepth,
            opensQueued,
          });
          break;
        }
        case 'set_track_volume':
          this.renderer?.setTrackVolume(msg.index, msg.volume);
          break;
        case 'set_track_mute':
          this.renderer?.setTrackMute(msg.index, msg.muted);
          break;
        case 'play':
          this.renderer?.play();
          break;
        case 'pause':
          this.renderer?.pause();
          break;
        case 'stop':
          this.renderer?.stop();
          break;
      }
    } catch (e) {
      // Surface worklet-side failures to the page — the worklet scope has
      // no visible console in some browsers, and a swallowed init error
      // reads as an eternal 'starting'.
      this.port.postMessage({ kind: 'error', error: String(e), during: msg.kind });
      if (msg.replyTo !== undefined) {
        this.reply(msg, String(e));
      }
    }
  }

  process(_inputs, outputs) {
    if (!this.renderer) {
      return true;
    }
    // Underrun detection: the browser only calls process() for quanta it
    // actually plays; a currentFrame jump = skipped quanta = an audible
    // dropout. (A suspended context resuming also jumps — the page resets
    // these counters on state transitions it causes.)
    if (this.lastFrame >= 0) {
      const skipped = currentFrame - this.lastFrame - 128;
      if (skipped > 0) {
        this.glitches += 1;
        this.glitchFrames += skipped;
      }
    }
    this.lastFrame = currentFrame;
    const out = outputs[0];
    // Time the render with the coarse (~1 ms) worklet clock and aggregate —
    // see the constructor's stats comment for why raw per-call numbers lie.
    const t0 = Date.now();
    this.renderer.render(out[0], out[1] ?? out[0]);
    const dt = Date.now() - t0;
    // Ship queued warm requests to the decoder worker (cheap boolean per
    // quantum; the queue only fills on a note-on that found cold samples).
    if (this.warmPort && this.renderer.hasWarmRequests()) {
      this.warmPort.postMessage({ kind: 'warm', requests: this.renderer.takeWarmRequests() });
    }
    this.statRenderMs += dt;
    if (dt > this.statWorstMs) this.statWorstMs = dt;
    this.statQuanta += 1;
    this.statTotalQuanta += 1;
    if (this.statQuanta >= this.statWindowQuanta) {
      this.statLoad = this.statRenderMs / (this.statQuanta * this.statQuantumMs);
      this.statLoadWorstMs = this.statWorstMs;
      this.statRenderMs = 0;
      this.statWorstMs = 0;
      this.statQuanta = 0;
    }
    return true;
  }
}

registerProcessor('fts-keys-rig', KeysRigProcessor);
