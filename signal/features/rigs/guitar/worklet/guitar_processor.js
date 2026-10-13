// AudioWorkletProcessor hosting `GuitarWorklet` (signal-guitar-worklet) —
// the browser guitar rig's render loop. The page (guitar_rig.js) talks to it
// over port messages; the NAM models it cannot afford run on Web Workers
// (nam_worker.js) reached through SharedArrayBuffers it is handed.
//
// Same constraints as the keys worklet (features/rigs/keys/worklet): the
// glue is STATICALLY imported (no dynamic import() in this scope), the
// polyfill must come first (no TextDecoder/performance/crypto here), and
// the wasm BYTES arrive in the init message (no fetch here).
//
// Messages — any carrying `replyTo` gets `{ replyTo, value | error }`:
//   init        { wasmBytes, sampleRate }                  → 'ready'
//   asset       { key, bytes }                  install a model / IR
//   calibration { dbu }                         NaN = off
//   model_size  { size }                        1 = full
//   load_chain  { blocksJson, costs, budgetUs, remotes: [[slot, sab]…] }
//                                               → added latency (frames)
//   set_trims   { inputDb, outputDb }           the patch's trims
//   clear_chain
//   set_param   { block, param, value }
//   set_bypass  { block, bypass }
//   stats       → { inPeak, outPeak, misses, addedLatencyFrames, load,
//                   quanta, glitches }

import './worklet_polyfill.js';
import init, * as wasm from './signal_guitar_worklet.js';

class GuitarRigProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this.rig = null;
    this.silence = new Float32Array(128);
    // Render load, aggregated (the clock here is Date-backed, ~1 ms): time
    // summed over a window, divided by the window's audio time.
    this.windowMs = 0;
    this.windowQuanta = 0;
    this.load = 0;
    this.quanta = 0;
    this.lastFrame = -1;
    this.glitches = 0;
    this.port.onmessage = (e) => this.handle(e.data);
  }

  reply(msg, value, error) {
    if (msg.replyTo === undefined) return;
    this.port.postMessage(error === undefined ? { replyTo: msg.replyTo, value } : { replyTo: msg.replyTo, error });
  }

  async handle(msg) {
    try {
      switch (msg.kind) {
        case 'init':
          await init({ module_or_path: msg.wasmBytes });
          this.rig = new wasm.GuitarWorklet(msg.sampleRate);
          this.port.postMessage({ kind: 'ready' });
          break;
        case 'asset':
          wasm.installAsset(msg.key, msg.bytes);
          this.reply(msg, true);
          break;
        case 'calibration':
          wasm.setCalibration(msg.dbu);
          break;
        case 'model_size':
          wasm.setModelSize(msg.size);
          break;
        case 'load_chain': {
          const latency = this.rig.loadChain(msg.blocksJson, msg.costs, msg.budgetUs, msg.remotes);
          this.reply(msg, latency);
          break;
        }
        case 'set_trims':
          this.rig?.setTrims(msg.inputDb ?? 0, msg.outputDb ?? 0);
          break;
        case 'clear_chain':
          this.rig?.clearChain();
          this.reply(msg, true);
          break;
        case 'set_param':
          this.reply(msg, this.rig?.setParam(msg.block, msg.param, msg.value) ?? false);
          break;
        case 'set_bypass':
          this.reply(msg, this.rig?.setBypass(msg.block, msg.bypass) ?? false);
          break;
        case 'stats': {
          const [inPeak, outPeak] = this.rig ? this.rig.takePeaks() : [0, 0];
          this.reply(msg, {
            inPeak, outPeak,
            misses: this.rig ? this.rig.misses() : 0,
            addedLatencyFrames: this.rig ? this.rig.addedLatency() : 0,
            load: this.load, quanta: this.quanta, glitches: this.glitches,
          });
          break;
        }
      }
    } catch (e) {
      this.reply(msg, undefined, String(e?.message ?? e));
      if (msg.replyTo === undefined) console.error('guitar worklet:', msg.kind, e);
    }
  }

  process(inputs, outputs) {
    const out = outputs[0];
    if (!this.rig || !out || out.length === 0) return true;
    // A skipped quantum is a dropout the player heard.
    if (this.lastFrame >= 0 && currentFrame !== this.lastFrame + 128) this.glitches++;
    this.lastFrame = currentFrame;

    const input = inputs[0] && inputs[0][0] ? inputs[0][0] : this.silence;
    const outL = out[0];
    const outR = out[1] || out[0];
    const t0 = Date.now();
    this.rig.process(input, outL, outR);
    this.windowMs += Date.now() - t0;
    this.quanta++;
    if (++this.windowQuanta === 250) {
      this.load = this.windowMs / ((250 * 128 / sampleRate) * 1000);
      this.windowMs = 0;
      this.windowQuanta = 0;
    }
    return true;
  }
}

registerProcessor('signal-guitar-rig', GuitarRigProcessor);
