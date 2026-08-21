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
//   open_lanes  { program, replyTo? }                → lane count
//   reload_lanes { replyTo? }                        → true/error string
//   midi        { bytes: [status, d1, d2] }
//   all_notes_off | panic
//   track_peaks { replyTo }                          → [peak, …] (0 = master)
//   set_track_volume { index, volume } | set_track_mute { index, muted }
//   play | pause | stop

import './worklet_polyfill.js'; // MUST precede the glue: TextDecoder/TextEncoder
import init, * as wasm from './signal_keys_worklet.js';

class KeysRigProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this.renderer = null;
    // Pack buffers stay HERE, on the worklet's JS heap — outside wasm's
    // 4 GB linear memory. wasm reads ranges through globalThis.__ftsPackRead
    // (installed once below); only decoded PCM enters linear memory.
    this.packs = new Map(); // id → Uint8Array over the transferred ArrayBuffer
    this.packIdsByKey = new Map(); // spec-path key → id (frees replaced buffers)
    this.nextPackId = 1;
    this.port.onmessage = (e) => this.handleMessage(e.data);
  }

  installPackRead() {
    if (globalThis.__ftsPackRead) return;
    const packs = this.packs;
    // (id, offset, len) → Uint8Array subarray (no copy — wasm copies out of
    // it), or null when the id is unknown or the range is out of bounds.
    globalThis.__ftsPackRead = (id, offset, len) => {
      const buf = packs.get(id);
      if (!buf) return null;
      if (!(offset >= 0) || !(len >= 0) || offset + len > buf.byteLength) return null;
      return buf.subarray(offset, offset + len);
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
