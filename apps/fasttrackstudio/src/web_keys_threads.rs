//! Shared-memory streamer threads (W13 of
//! `crates/signal/docs/browser-keys-rig.md`).
//!
//! One `WebAssembly.Memory` (shared) is created HERE and handed to both the
//! AudioWorklet and every streamer worker, so all of them instantiate the
//! same module over the same heap. That is what makes the browser rig
//! behave like the native one: a streamer worker decodes a chunk straight
//! into memory the audio thread already reads — no `postMessage`, no copy,
//! and nothing for the audio thread to do but render.
//!
//! Preconditions, both checked at runtime rather than assumed:
//!   * the page is cross-origin isolated (COOP/COEP — the engine sets them
//!     via `EngineHost::cross_origin_isolated`), else `SharedArrayBuffer`
//!     does not exist;
//!   * the staged wasm was built with `+atomics` (`just
//!     keys-worklet-wasm-threads`), else a worker reports it and we stay on
//!     the single-threaded decoder-worker path (W12).
//!
//! Everything degrades rather than breaks: no isolation or no atomics means
//! [`spawn`] returns `None`, and the caller boots exactly as before.

use std::cell::RefCell;

use js_sys::{Array, Object, Reflect};
use wasm_bindgen::JsCast as _;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;

use crate::web_keys_rig::{WORKLET_GLUE_URL, WORKLET_WASM_URL};

const STREAMER_WORKER_URL: &str = "/worklet/keys_streamer_worker.js";

/// Memory descriptor. Must match what the module's memory IMPORT declares —
/// the generated glue's own default (`initial: 40, maximum: 65536` pages,
/// i.e. 2.5 MB grown on demand up to the wasm32 ceiling of 4 GB).
const MEM_INITIAL_PAGES: u32 = 40;
const MEM_MAXIMUM_PAGES: u32 = 65_536;

/// How many streamer threads to run. The native pool sizes itself from the
/// machine (`available_parallelism / 4`, clamped 2..4); do the same from
/// `navigator.hardwareConcurrency`, leaving the audio thread and the page
/// their own cores. Decoding is the only work these do, and one is enough
/// until several voices fault at once — which is exactly when a second and
/// third stop it being audible.
fn worker_count() -> u32 {
    web_sys::window()
        .map(|w| w.navigator().hardware_concurrency() as u32)
        .map(|n| (n / 4).clamp(2, 4))
        .unwrap_or(2)
}

thread_local! {
    /// The live workers, kept alive for the page's lifetime (dropping a
    /// `Worker` terminates the thread).
    static WORKERS: RefCell<Vec<web_sys::Worker>> = const { RefCell::new(Vec::new()) };
    /// Set once a worker has reported `ready` — the proof that the threaded
    /// path is actually live rather than merely attempted.
    static READY: RefCell<u32> = const { RefCell::new(0) };
    /// A worker's error text, if any came back (module built without
    /// atomics, fetch failure, …). Surfaced in the audio panel.
    static LAST_ERROR: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Whether this page can host wasm threads at all.
pub(crate) fn supported() -> bool {
    let isolated = web_sys::window()
        .and_then(|w| Reflect::get(&w, &"crossOriginIsolated".into()).ok())
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let has_sab = web_sys::window()
        .and_then(|w| Reflect::has(&w, &"SharedArrayBuffer".into()).ok())
        .unwrap_or(false);
    isolated && has_sab
}

/// Streamer workers that have reported ready.
pub(crate) fn ready_count() -> u32 {
    READY.with(|r| *r.borrow())
}

/// The last streamer-worker error (empty when none).
pub(crate) fn last_error() -> String {
    LAST_ERROR.with(|e| e.borrow().clone())
}

/// Create the shared memory and start the streamer workers over it.
///
/// Returns the memory to hand to the worklet's `init`, or `None` when the
/// page cannot host threads — in which case the caller keeps the
/// single-threaded path. Workers report `ready` asynchronously; the
/// returned memory is usable immediately either way.
pub(crate) fn spawn() -> Option<JsValue> {
    if !supported() {
        return None;
    }
    // Terminate any previous generation (a latency-hint re-boot builds a
    // fresh AudioContext, and a worker bound to the OLD memory would be
    // decoding into a heap nobody reads).
    shutdown();

    let desc = Object::new();
    let _ = Reflect::set(&desc, &"initial".into(), &MEM_INITIAL_PAGES.into());
    let _ = Reflect::set(&desc, &"maximum".into(), &MEM_MAXIMUM_PAGES.into());
    let _ = Reflect::set(&desc, &"shared".into(), &true.into());
    let memory = js_sys::Reflect::get(&js_sys::global(), &"WebAssembly".into())
        .ok()
        .and_then(|wasm_ns| Reflect::get(&wasm_ns, &"Memory".into()).ok())
        .and_then(|ctor| ctor.dyn_into::<js_sys::Function>().ok())
        .and_then(|ctor| js_sys::Reflect::construct(&ctor, &Array::of1(&desc)).ok())?;

    let opts = web_sys::WorkerOptions::new();
    opts.set_type(web_sys::WorkerType::Module);
    for _ in 0..worker_count() {
        let Ok(worker) = web_sys::Worker::new_with_options(STREAMER_WORKER_URL, &opts) else {
            continue;
        };
        let onmessage =
            Closure::<dyn FnMut(web_sys::MessageEvent)>::new(|ev: web_sys::MessageEvent| {
                let data = ev.data();
                let kind = Reflect::get(&data, &"kind".into())
                    .ok()
                    .and_then(|k| k.as_string())
                    .unwrap_or_default();
                match kind.as_str() {
                    "ready" => READY.with(|r| *r.borrow_mut() += 1),
                    "error" => {
                        let err = Reflect::get(&data, &"error".into())
                            .ok()
                            .and_then(|v| v.as_string())
                            .unwrap_or_default();
                        tracing::warn!("keys streamer worker: {err}");
                        LAST_ERROR.with(|e| *e.borrow_mut() = err);
                    }
                    _ => {}
                }
            });
        worker.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        onmessage.forget(); // page-lifetime

        let init = Object::new();
        let _ = Reflect::set(&init, &"kind".into(), &"init".into());
        let _ = Reflect::set(&init, &"glueUrl".into(), &WORKLET_GLUE_URL.into());
        let _ = Reflect::set(&init, &"wasmUrl".into(), &WORKLET_WASM_URL.into());
        let _ = Reflect::set(&init, &"memory".into(), &memory);
        if worker.post_message(&init).is_ok() {
            WORKERS.with(|w| w.borrow_mut().push(worker));
        }
    }
    Some(memory)
}

/// Terminate every streamer worker (a re-boot, or the page going away).
pub(crate) fn shutdown() {
    WORKERS.with(|w| {
        for worker in w.borrow_mut().drain(..) {
            worker.terminate();
        }
    });
    READY.with(|r| *r.borrow_mut() = 0);
}
