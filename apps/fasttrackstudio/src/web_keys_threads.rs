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
    /// Every pack SAB shared so far, replayed to workers spawned later
    /// (a latency-hint re-boot restarts them mid-session).
    static SHARED_PACKS: RefCell<Vec<(u32, JsValue, String)>> =
        const { RefCell::new(Vec::new()) };
}

/// Whether the streamer workers should run AT ALL.
///
/// OFF by default, and that is a measured decision rather than caution:
/// the workers cannot currently finish either kind of job. Both chunk
/// fills and zone opens read pack bytes through `__ftsPackRead`, and that
/// hook is installed in the WORKLET's scope over pack buffers on the
/// worklet's JS heap — a worker sharing the wasm heap cannot reach them
/// (see W14 in browser-keys-rig.md). Spawning them anyway costs real CPU:
/// N workers waking every few ms to drain a queue they can never satisfy
/// pushed measured render load from ~1.0 to 1.73 and failed the e2e
/// headroom check.
///
/// OFF by default — the workers still wedge the tab, and that is measured,
/// not suspected. With `fts.keys-threads = "on"` the page freezes shortly
/// after the workers come up, every time, with no error surfacing; with it
/// off the identical build is healthy (9/9 packs, 61 ms worst handler,
/// zero glitches). So the SAB pack transport ships and the workers do not.
///
/// What is known:
///   * pack bytes are no longer the blocker — [`share_pack`] gives every
///     thread the same `SharedArrayBuffer`;
///   * a worker MUST init with `thread_stack_size` or it re-runs the
///     module's main initialisation (globals, allocator) over the shared
///     heap. That is fixed in keys_streamer_worker.js and was NOT the
///     whole story.
///
/// Narrowed by isolation, and the picture is now specific:
///
///   * init ORDER was a real bug and is FIXED — workers must come up only
///     after the worklet has run main init ([`create_memory`] /
///     [`spawn_over`]). With that, workers alone no longer freeze the tab:
///     81 opens queued, zero glitches, healthy page.
///   * `thread_stack_size` was also a real bug and is fixed — without it a
///     worker re-runs main init over the shared heap.
///   * What still freezes is the COMBINATION: streamer workers running
///     while the DECODER worker is also active (the shared-pack path now
///     mirrors packs to it, which a returning visit needs). Alone, either
///     is stable; together the tab wedges.
///
/// Most likely resource pressure rather than a logic bug: that
/// configuration holds the SABs (~2.4 GB), the worklet+streamers' shared
/// heap (~684 MB of decoded PCM), AND the decoder worker's own separate
/// wasm heap with its own copies. The real resolution is probably not to
/// run both — the decoder worker exists only because the streamers could
/// not open zones, so it should RETIRE once they can. That needs
/// `zonesOpened` to actually climb, which is still unproven.
fn enabled() -> bool {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item("fts.keys-threads").ok().flatten())
        .map(|v| v == "on")
        .unwrap_or(false)
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
/// Allocate the shared memory the worklet and the workers will both use.
///
/// Separate from [`spawn_over`] because ORDER MATTERS: exactly one
/// instance may run the module's main initialisation (it sets up the
/// allocator and globals in this heap), and every other instance must come
/// up as a THREAD against an already-initialised heap. The worklet is the
/// main instance, so the memory is created here, the worklet instantiates
/// over it, and only once it reports `ready` do the workers start.
/// Spawning them earlier let a worker init against a heap with no
/// allocator — which froze the tab with no error, every time.
pub(crate) fn create_memory() -> Option<JsValue> {
    if !supported() || !enabled() {
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
    js_sys::Reflect::get(&js_sys::global(), &"WebAssembly".into())
        .ok()
        .and_then(|wasm_ns| Reflect::get(&wasm_ns, &"Memory".into()).ok())
        .and_then(|ctor| ctor.dyn_into::<js_sys::Function>().ok())
        .and_then(|ctor| js_sys::Reflect::construct(&ctor, &Array::of1(&desc)).ok())
}

/// Start the streamer workers over `memory`. Call ONLY after the worklet
/// has reported `ready` — see [`create_memory`] for why.
pub(crate) fn spawn_over(memory: &JsValue) {
    if !supported() || !enabled() {
        return;
    }
    let memory = memory.clone();

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
            // Replay the packs already shared — this worker missed them.
            SHARED_PACKS.with(|p| {
                for (id, sab, key) in p.borrow().iter() {
                    let _ = worker.post_message(&pack_msg(*id, sab, key));
                }
            });
            WORKERS.with(|w| w.borrow_mut().push(worker));
        }
    }
}

/// Hand a pack's SHARED bytes to every streamer worker.
///
/// This is the W14 unlock. Pack bytes used to live on the worklet's JS
/// heap, reachable only through a `__ftsPackRead` hook in that scope, so a
/// worker could not read them and neither of its jobs (chunk fills, zone
/// opens) could ever complete. A `SharedArrayBuffer` is visible to every
/// thread at once, costs no wasm linear memory (the 2.4 GB Worship set
/// would not fit in the 4 GB address space), and is passed by reference —
/// posting it to N workers copies nothing.
///
/// Remembered so a worker spawned later still receives every pack.
pub(crate) fn share_pack(id: u32, sab: &JsValue, key: &str) {
    SHARED_PACKS.with(|p| p.borrow_mut().push((id, sab.clone(), key.to_string())));
    WORKERS.with(|w| {
        for worker in w.borrow().iter() {
            let _ = worker.post_message(&pack_msg(id, sab, key));
        }
    });
}

fn pack_msg(id: u32, sab: &JsValue, key: &str) -> Object {
    let o = Object::new();
    let _ = Reflect::set(&o, &"kind".into(), &"pack_shared".into());
    let _ = Reflect::set(&o, &"id".into(), &id.into());
    let _ = Reflect::set(&o, &"key".into(), &key.into());
    let _ = Reflect::set(&o, &"sab".into(), sab);
    o
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
