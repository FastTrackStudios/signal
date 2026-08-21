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
//   set_track_volume { index, volume } | set_track_mute { index, muted }
//   play | pause | stop
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
    this.port.onmessage = (e) => this.handleMessage(e.data);
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
    // Merge with the previous / next neighbour when contiguous.
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
          await init(msg.wasmBytes);
          this.renderer = new wasm.KeysWorklet(msg.sampleRate);
          this.port.postMessage({ kind: 'ready' });
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
            this.renderer.reloadLanes();
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
        case 'all_notes_off':
          this.renderer?.allNotesOff();
          break;
        case 'panic':
          this.renderer?.panic();
          break;
        case 'track_peaks':
          this.reply(msg, this.renderer ? Array.from(this.renderer.trackPeaks()) : []);
          break;
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
    const out = outputs[0];
    this.renderer.render(out[0], out[1] ?? out[0]);
    return true;
  }
}

registerProcessor('fts-keys-rig', KeysRigProcessor);
