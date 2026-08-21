// A STREAMER WORKER (W13) — the browser's version of one thread in the
// native engine's streamer pool.
//
// It instantiates the same `signal-keys-worklet` module over the SAME shared
// WebAssembly.Memory the AudioWorklet renders from. That is the whole point:
// when this worker decodes a chunk, it writes it into the heap the audio
// thread already reads, so there is nothing to send and nothing for the
// audio thread to do. Contrast the single-threaded fallback
// (keys_decoder_worker.js), which must decode into its own memory and copy
// the PCM across a MessagePort.
//
// Requires cross-origin isolation (COOP/COEP) for SharedArrayBuffer, and a
// module built with +atomics (see `just keys-worklet-wasm-threads`).
//
// Protocol:
//   page → worker : init { glueUrl, wasmUrl, memory }   → ready | error
// After `ready` the worker parks inside wasm on the streamer queue's futex
// and never returns — it is a thread, not a request handler.

// Pack bytes this worker can read: id → Uint8Array over a SHARED buffer
// the page allocated (W14). Before this existed the worker had no way to
// read pack bytes at all — they lived on the worklet's JS heap — so every
// job it dequeued failed and the whole streamer path was dead weight.
const packs = new Map();

// The reader fts-sample calls for External packs. Same contract as the
// processor's: return a view for a covered range, or null.
function installPackRead() {
  if (globalThis.__ftsPackRead) return;
  globalThis.__ftsPackRead = (id, offset, len) => {
    const buf = packs.get(id);
    if (!buf) return null;
    if (!(offset >= 0) || !(len >= 0) || offset + len > buf.byteLength) return null;
    // A subarray over a SharedArrayBuffer is a view, not a copy; wasm
    // copies out of it exactly as it does on the audio thread.
    return buf.subarray(offset, offset + len);
  };
}

self.onmessage = async (e) => {
  const msg = e.data;
  // Pack shares arrive before AND after init (the page replays them to a
  // worker spawned later), so handle them regardless of readiness.
  if (msg?.kind === 'pack_shared') {
    packs.set(msg.id, new Uint8Array(msg.sab));
    installPackRead();
    return;
  }
  if (msg?.kind !== 'init') return;
  try {
    const glue = await import(msg.glueUrl);
    // Pass the SHARED memory so this instance maps the same heap as the
    // worklet's. Without it wasm-bindgen would allocate a fresh memory and
    // the worker would decode into a heap nobody reads.
    const bytes = await (await fetch(msg.wasmUrl)).arrayBuffer();
    // `thread_stack_size` is what makes this a THREAD rather than a second
    // main. Without it the glue calls `__wbindgen_start(undefined)`, which
    // runs the module's MAIN initialisation — globals, allocator state —
    // and every worker doing that over the same shared heap corrupts it.
    // Symptom: the whole tab freezes shortly after the workers come up,
    // with no error anywhere. With a stack size the glue allocates a fresh
    // stack + TLS block for this thread instead.
    //
    // Must be a non-zero multiple of 64 KiB (the glue asserts it).
    await glue.default({
      module_or_path: bytes,
      memory: msg.memory,
      thread_stack_size: 2 * 1024 * 1024,
    });

    if (typeof glue.threadsAvailable === 'function' && !glue.threadsAvailable()) {
      self.postMessage({
        kind: 'error',
        error: 'module built without atomics — streamer workers cannot run',
      });
      return;
    }
    installPackRead();
    self.postMessage({ kind: 'ready' });

    // The streamer loop. Parking happens HERE rather than in Rust because
    // the wasm wait/notify intrinsics are nightly-only and fts-sample
    // compiles on stable for native — see stream_wasm's module docs.
    //
    // `Atomics.wait` is legal in a worker (it traps on the audio thread and
    // the main thread, which is exactly why this work lives over here). The
    // short timeout means no notify is ever required: worst case a queued
    // sample waits WAIT_MS before a decoder picks it up, and a chunk is
    // hundreds of ms of audio, so that is far inside the lead time.
    const WAIT_MS = 4;
    const i32 = new Int32Array(msg.memory.buffer);
    const addr = glue.streamerWakeAddr() >>> 2; // byte address → i32 index
    for (;;) {
      const before = glue.streamerWakeValue() | 0;
      // Returns 'not-equal' immediately when work was queued between the
      // read and the wait — the standard futex race guard.
      Atomics.wait(i32, addr, before, WAIT_MS);
      glue.streamerDrain();
    }
  } catch (err) {
    self.postMessage({ kind: 'error', error: String(err) });
  }
};
