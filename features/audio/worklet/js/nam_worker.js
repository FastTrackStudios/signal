// One NAM model on its own thread, for the browser guitar rig.
//
// The worklet hands this worker a quantum through a SharedArrayBuffer and
// the worker answers in the same buffer (protocol: src/web.rs). Between
// quanta the worker sits in `Atomics.wait` inside `NamWorker.serve()`, so
// its event loop is not running — to deliver a message, the page first
// raises CMD, which makes `serve()` return, and the worker resumes serving
// after handling it.
//
// Messages — any carrying `replyTo` gets `{ replyTo, value | error }`:
//   init { wasmBytes }                                         → 'ready'
//   load { sab, key, bytes, blockJson, sampleRate, modelSize, calibration }
//        → measured cost, µs per quantum; then serves
//   profile { blocksJson, assets: [[key, bytes]…], sampleRate, quanta }
//        → [{ name, us, bypassed }…] per block (the profiler worker)

import init, * as wasm from './signal_guitar_worklet.js';

const CMD = 4;
let worker = null;
let ctl = null;

function reply(msg, value, error) {
  if (msg.replyTo === undefined) return;
  postMessage(error === undefined ? { replyTo: msg.replyTo, value } : { replyTo: msg.replyTo, error });
}

function serve() {
  if (!worker) return;
  Atomics.store(ctl, CMD, 0);
  worker.serve();
  // CMD was raised: a message is waiting. The event loop delivers it, and
  // its handler calls serve() again.
}

onmessage = async (e) => {
  const msg = e.data;
  try {
    switch (msg.kind) {
      case 'init':
        await init({ module_or_path: msg.wasmBytes });
        postMessage({ kind: 'ready' });
        break;
      case 'load': {
        if (!worker || worker.__sab !== msg.sab) {
          worker?.free();
          worker = new wasm.NamWorker(msg.sab);
          worker.__sab = msg.sab;
          ctl = new Int32Array(msg.sab, 0, 16);
        }
        if (msg.bytes) wasm.installAsset(msg.key, msg.bytes);
        wasm.setModelSize(msg.modelSize ?? 1);
        wasm.setCalibration(msg.calibration ?? NaN);
        const cost = worker.load(msg.blockJson, msg.sampleRate);
        reply(msg, cost);
        break;
      }
      case 'profile': {
        for (const [key, bytes] of msg.assets) wasm.installAsset(key, bytes);
        reply(msg, JSON.parse(wasm.profileChain(msg.blocksJson, msg.sampleRate, msg.quanta ?? 60)));
        break;
      }
      case 'resume':
        break;
    }
  } catch (err) {
    reply(msg, undefined, String(err?.message ?? err));
  }
  // Every message ends by going back to serving the worklet.
  setTimeout(serve, 0);
};
