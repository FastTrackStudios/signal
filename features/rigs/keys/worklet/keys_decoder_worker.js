// The keys rig's DECODER WORKER (W12) — a second instance of the
// signal-keys-worklet wasm module in a plain module Worker, so sample
// decode happens OFF the audio thread. The AudioWorkletGlobalScope is
// single-threaded: its message handlers share the rendering thread, and
// the old decode-on-note-on there starved process() and dropped every
// sounding voice. This worker decodes; the worklet only memcpys.
//
// Data flow:
//   page  → worker : init { glueUrl, wasmUrl, sampleRate, warmPort }
//                    attach_pack { key, id, opfs, len }          (whole file)
//                    attach_pack_progressive { key, id, opfs, len }
//                    pack_ranges { id, ranges: [[start,len],…] } (committed)
//                    open_lanes { program } | reload_lanes
//                    coverage { center }                          (re)start fill
//   worker → page  : ready | lanes { count } | error { error, during }
//                    net_miss { id, ranges: [{start,len},…] }     (bump queue)
//                    coverage_done | coverage_paused (budget full)
//   worklet ⇄ worker (warmPort, direct — the page main thread is not in
//   this loop):
//     in :  warm { requests: [{layer, path}, …] }
//     out:  pcm  { layer, path, channels, sampleRate, pcm, chargePast }
//     in :  pcm_ack { path, layer, accepted, chargePast }
//
// Pack bytes are NOT resident here: __ftsPackRead misses are recorded,
// the needed ranges are read from the same OPFS files the page maintains
// (packs/<name>.<variant>.signalpack, or .sparse with page-pushed
// committed ranges), cached in a bounded LRU, and the decode retried.
// A range OPFS cannot serve yet is reported to the page as net_miss —
// the page bumps it to the front of the network queue, exactly like the
// worklet's old miss path.

let wasm = null;
let renderer = null;
let warmPort = null;
let sampleRate = 48000;

// id → { opfs, len, valid: [[start,len],…] sorted, whole } — what OPFS can
// legitimately serve per pack. Whole files are valid end to end; sparse
// files only where the page says a segment committed (an OPFS hole reads
// as zeros, which would "decode" garbage rather than fail).
const packFiles = new Map();
// Bounded byte cache of fetched ranges: id → { segs: [{start, bytes}] }.
const rangeCache = new Map();
let rangeCacheBytes = 0;
const RANGE_CACHE_CAP = 256 * 1024 * 1024;
// __ftsPackRead misses since the last drain: [{id, start, len}].
let misses = [];
const missKeys = new Set();

// Work queues: note-driven warms preempt background coverage.
const warmQ = [];
let covQ = [];
const delivered = new Set(); // `${layer} ${path}` already shipped
let coveragePaused = false;  // budget ceiling reached
let pumping = false;
// Items whose bytes OPFS cannot serve yet, keyed like `delivered`; retried
// when new ranges/segments arrive.
const blocked = new Map(); // key → item
// Sent-pcm ack resolvers, keyed like `delivered` — decode pacing.
const ackWaiters = new Map();

function installPackRead() {
  if (globalThis.__ftsPackRead) return;
  globalThis.__ftsPackRead = (id, offset, len) => {
    const cached = rangeCache.get(id);
    if (cached) {
      const segs = cached.segs;
      let lo = 0, hi = segs.length;
      while (lo < hi) {
        const mid = (lo + hi) >> 1;
        if (segs[mid].start <= offset) lo = mid + 1; else hi = mid;
      }
      const i = lo - 1;
      if (i >= 0) {
        const s = segs[i];
        const within = offset - s.start;
        if (within + len <= s.bytes.byteLength) {
          return s.bytes.subarray(within, within + len);
        }
      }
    }
    const key = `${id}:${offset}`;
    if (!missKeys.has(key) && misses.length < 256) {
      missKeys.add(key);
      misses.push({ id, start: offset, len });
    }
    return null;
  };
}

function drainMisses() {
  const taken = misses;
  misses = [];
  missKeys.clear();
  return taken;
}

// Insert fetched bytes into the LRU'd range cache (sorted, merged when
// exactly adjacent — same shape as the worklet's segment store).
function cacheRange(id, start, bytes) {
  let store = rangeCache.get(id);
  if (!store) {
    store = { segs: [] };
    rangeCache.set(id, store);
  }
  const segs = store.segs;
  let lo = 0, hi = segs.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (segs[mid].start < start) lo = mid + 1; else hi = mid;
  }
  segs.splice(lo, 0, { start, bytes });
  rangeCacheBytes += bytes.byteLength;
  const mergeAt = (i) => {
    const a = segs[i], b = segs[i + 1];
    if (!a || !b) return false;
    if (a.start + a.bytes.byteLength !== b.start) return false;
    const joined = new Uint8Array(a.bytes.byteLength + b.bytes.byteLength);
    joined.set(a.bytes, 0);
    joined.set(b.bytes, a.bytes.byteLength);
    segs.splice(i, 2, { start: a.start, bytes: joined });
    return true;
  };
  if (lo > 0 && mergeAt(lo - 1)) lo -= 1;
  mergeAt(lo);
  // Evict WHOLE packs' caches oldest-first when past the cap — coarse but
  // simple; a decode re-fetches what it still needs from OPFS.
  while (rangeCacheBytes > RANGE_CACHE_CAP && rangeCache.size > 1) {
    const [oldId, oldStore] = rangeCache.entries().next().value;
    if (oldId === id) break; // never evict the pack being decoded
    const freed = oldStore.segs.reduce((n, s) => n + s.bytes.byteLength, 0);
    rangeCache.delete(oldId);
    rangeCacheBytes -= freed;
  }
}

// Clamp [start,len) to the pack's valid (committed) ranges. Returns the
// covered sub-ranges and whether anything was left out.
function validCover(entry, start, len) {
  if (entry.whole) return { covered: [[start, len]], holes: false };
  const covered = [];
  let holes = false;
  let at = start;
  const end = start + len;
  for (const [vs, vl] of entry.valid) {
    const ve = vs + vl;
    if (ve <= at) continue;
    if (vs >= end) break;
    if (vs > at) { holes = true; at = vs; }
    const take = Math.min(ve, end) - at;
    if (take > 0) covered.push([at, take]);
    at += take;
    if (at >= end) break;
  }
  if (at < end) holes = true;
  return { covered, holes };
}

async function opfsFile(name) {
  const root = await navigator.storage.getDirectory();
  const dir = await root.getDirectoryHandle('packs');
  const handle = await dir.getFileHandle(name);
  return handle.getFile();
}

// Try to satisfy a miss list from OPFS. Returns the ranges no file can
// serve yet (uncommitted / undownloaded), grouped by pack id.
async function fetchMisses(taken) {
  const unserved = new Map(); // id → [{start,len}]
  for (const m of taken) {
    const entry = packFiles.get(m.id);
    if (!entry) continue;
    const { covered, holes } = validCover(entry, m.start, m.len);
    if (holes) {
      const list = unserved.get(m.id) ?? [];
      list.push({ start: m.start, len: m.len });
      unserved.set(m.id, list);
    }
    for (const [cs, cl] of covered) {
      try {
        const file = await opfsFile(entry.opfs);
        const buf = await file.slice(cs, cs + cl).arrayBuffer();
        cacheRange(m.id, cs, new Uint8Array(buf));
      } catch (e) {
        const list = unserved.get(m.id) ?? [];
        list.push({ start: cs, len: cl });
        unserved.set(m.id, list);
      }
    }
  }
  return unserved;
}

function reportUnserved(unserved) {
  for (const [id, ranges] of unserved) {
    self.postMessage({ kind: 'net_miss', id, ranges });
  }
}

// Decode one item, fetching byte ranges from OPFS as the reads miss.
// Returns 'sent' | 'blocked' | 'failed'.
async function decodeItem(item) {
  for (let attempt = 0; attempt < 6; attempt += 1) {
    let res;
    try {
      res = renderer.decodePathPcm(item.layer, item.path);
    } catch (e) {
      return 'failed';
    }
    if (res !== undefined && res !== null) {
      const pcm = res.pcm;
      const itemKey = `${item.layer} ${item.path}`;
      // Await the worklet's ack before the next decode: each pcm message
      // costs the audio thread a memcpy between render quanta, so sends
      // must be PACED — an unpaced coverage flood would itself glitch.
      const acked = new Promise((resolve) => {
        ackWaiters.set(itemKey, resolve);
        setTimeout(() => {
          if (ackWaiters.delete(itemKey)) resolve(null);
        }, 5000);
      });
      // Ship in BOUNDED PIECES. One decoded sample can be tens of MB, and
      // copying that into the worklet in a single message measured 28-34 ms
      // ON THE AUDIO THREAD (ten-plus render quanta) — the dropout this
      // whole design exists to avoid. ~1 MB per message keeps each handler
      // call to a fraction of a quantum; only the last piece publishes the
      // sample. Each slice is a COPY so it can be transferred without
      // detaching the rest.
      const CHUNK = 262_144; // f32 samples ≈ 1 MB
      for (let off = 0; off < pcm.length; off += CHUNK) {
        const end = Math.min(off + CHUNK, pcm.length);
        const piece = pcm.slice(off, end);
        warmPort.postMessage(
          {
            kind: 'pcm_chunk',
            layer: item.layer,
            path: item.path,
            channels: res.channels,
            sampleRate: res.sampleRate,
            offset: off,
            pcm: piece,
            last: end >= pcm.length,
            chargePast: !!item.chargePast,
          },
          [piece.buffer],
        );
      }
      delivered.add(itemKey);
      await acked;
      return 'sent';
    }
    const taken = drainMisses();
    if (taken.length === 0) {
      return 'failed'; // unresolvable path — not a byte problem
    }
    const unserved = await fetchMisses(taken);
    if (unserved.size > 0) {
      reportUnserved(unserved);
      return 'blocked';
    }
  }
  return 'failed';
}

// The single-flight pump: warms first, then coverage. Budget acks pause
// coverage; warms always run (they charge past the ceiling and the engine
// sheds afterwards).
async function pump() {
  if (pumping || !renderer || !warmPort) return;
  pumping = true;
  try {
    for (;;) {
      let item = warmQ.shift();
      if (!item && !coveragePaused) item = covQ.shift();
      if (!item) break;
      const key = `${item.layer} ${item.path}`;
      if (delivered.has(key)) continue;
      const outcome = await decodeItem(item);
      if (outcome === 'blocked') {
        blocked.set(key, item);
      }
      // pcm_ack pacing/budget arrives on warmPort (below) — coverage
      // pauses when an un-charged insert is refused.
    }
    if (covQ.length === 0 && !coveragePaused) {
      self.postMessage({ kind: 'coverage_done' });
    }
  } finally {
    pumping = false;
  }
}

function retryBlocked() {
  if (blocked.size === 0) return;
  for (const [key, item] of blocked) {
    blocked.delete(key);
    (item.chargePast ? warmQ : covQ).push(item);
  }
  void pump();
}

function onWarmMessage(msg) {
  switch (msg.kind) {
    case 'warm': {
      for (const r of msg.requests ?? []) {
        const key = `${r.layer} ${r.path}`;
        if (delivered.has(key)) continue;
        if (!warmQ.some((q) => q.layer === r.layer && q.path === r.path)) {
          warmQ.push({ layer: r.layer, path: r.path, chargePast: true });
        }
      }
      void pump();
      break;
    }
    case 'pcm_ack': {
      const ackKey = `${msg.layer} ${msg.path}`;
      const resolve = ackWaiters.get(ackKey);
      if (resolve) {
        ackWaiters.delete(ackKey);
        resolve(msg);
      }
      if (!msg.accepted && !msg.chargePast) {
        // The decoded-PCM budget is full: stop background fill. Warm
        // inserts keep flowing (they charge past the ceiling).
        coveragePaused = true;
        covQ = [];
        self.postMessage({ kind: 'coverage_paused' });
      }
      if (!msg.accepted) {
        delivered.delete(`${msg.layer} ${msg.path}`);
      }
      break;
    }
    default:
      break;
  }
}

// Messages are handled STRICTLY IN ORDER behind a promise chain — the
// handler is async (init compiles wasm; attaches read OPFS), and without
// the chain an attach could run before init finished.
let chain = Promise.resolve();
self.onmessage = (e) => {
  chain = chain.then(() => handleMessage(e.data)).catch(() => {});
};

async function handleMessage(msg) {
  try {
    switch (msg.kind) {
      case 'init': {
        sampleRate = msg.sampleRate ?? 48000;
        warmPort = msg.warmPort;
        warmPort.onmessage = (ev) => onWarmMessage(ev.data);
        const glue = await import(msg.glueUrl);
        wasm = glue;
        const bytes = await (await fetch(msg.wasmUrl)).arrayBuffer();
        await glue.default(bytes);
        renderer = new glue.KeysWorklet(sampleRate);
        installPackRead();
        // Blocked items retry on new committed ranges; a slow safety tick
        // catches anything a notification raced past.
        setInterval(retryBlocked, 2000);
        self.postMessage({ kind: 'ready' });
        break;
      }
      case 'attach_pack':
      case 'attach_pack_progressive': {
        packFiles.set(msg.id, {
          opfs: msg.opfs,
          len: msg.len,
          whole: msg.kind === 'attach_pack',
          valid: msg.kind === 'attach_pack' ? [[0, msg.len]] : (msg.ranges ?? []),
        });
        // The wasm-side open parses header + index through __ftsPackRead —
        // misses fetch from OPFS and the attach retries, same as decode.
        let lastErr = null;
        for (let attempt = 0; attempt < 6; attempt += 1) {
          try {
            renderer.attachPackExternal(msg.key, msg.id, msg.len);
            lastErr = null;
            break;
          } catch (err) {
            lastErr = err;
            const taken = drainMisses();
            if (taken.length === 0) break;
            const unserved = await fetchMisses(taken);
            if (unserved.size > 0) {
              reportUnserved(unserved);
              break;
            }
          }
        }
        if (msg.replyTo !== undefined) {
          self.postMessage({ replyTo: msg.replyTo, value: lastErr ? String(lastErr) : true });
        }
        break;
      }
      case 'pack_ranges': {
        const entry = packFiles.get(msg.id);
        if (entry && !entry.whole) {
          entry.valid = msg.ranges ?? [];
          retryBlocked();
        }
        break;
      }
      case 'open_lanes': {
        const count = renderer.openLanes(msg.program);
        if (msg.replyTo !== undefined) self.postMessage({ replyTo: msg.replyTo, value: count });
        break;
      }
      case 'reload_lanes': {
        renderer.reloadLanes();
        // New packs → the same path can resolve now where it couldn't
        // before; let coverage and blocked items try again.
        coveragePaused = false;
        retryBlocked();
        if (msg.replyTo !== undefined) self.postMessage({ replyTo: msg.replyTo, value: true });
        break;
      }
      case 'coverage': {
        // BOUNDED: `limit` zones from the middle-out list, not the whole
        // library. Unbounded coverage decoded gigabytes, saturated this
        // worker and thrashed the audio side's PCM budget.
        const list = renderer.coveragePaths(msg.center ?? 60);
        const limit = msg.limit ?? 48;
        covQ = [];
        for (const r of list) {
          if (covQ.length >= limit) break;
          const key = `${r.layer} ${r.path}`;
          if (!delivered.has(key)) covQ.push({ layer: r.layer, path: r.path, chargePast: false });
        }
        coveragePaused = false;
        void pump();
        break;
      }
      default:
        break;
    }
  } catch (err) {
    self.postMessage({ kind: 'error', error: String(err), during: msg?.kind });
    if (msg?.replyTo !== undefined) {
      self.postMessage({ replyTo: msg.replyTo, value: String(err) });
    }
  }
}
