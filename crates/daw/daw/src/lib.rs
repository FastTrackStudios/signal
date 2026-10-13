// Lint debt: workspace flipped dead_code/unused to warn (task cleanup);
// this crate predates that — burn down separately.
#![allow(dead_code, unused)]

//! The DAW: its control API, its service protocol and module system, and
//! (feature `standalone`) its own engine.
//!
//! Hosts and everything layered on top — the REAPER adapter, project files,
//! the UI, control surfaces, the CLI and the integration-test harness — are
//! the `studio` crate's, which re-exports this one.
//!
//! # Namespaces
//!
//! - **`rpc`** → `daw::rpc` — Async Vox control API (`Daw`, `Project`, `Transport`, `TrackHandle`,
//!   `FxChain`, `BatchBuilder`, etc.). Use from UI, remote clients, background tasks.
//! - **`service`** → `daw::service` — Raw protocol types and service clients.
//!
//! # Feature-gated modules
//!
//! - **`standalone`** → `daw::standalone` — the DAW's own engine.

// ── RPC: async Vox control API ──────────────────────────────────────────────
//
// All async client types (`Daw`, `Project`, `Transport`, `TrackHandle`, etc.)
// live under `daw::rpc`. Use from UI, remote clients, background tasks, tests.
//
// In-process extension code that needs native sync access should use the
// service traits from `daw::service` and receive a backend from its host.
pub mod rpc {
    pub use daw_control::*;
}

// Service composition primitives — re-exported from architect so apps
// depending only on `daw` can compose service bundles via `Services`,
// `Layer::merge`, and the `layers!` macro without a direct architect
// dep. Backend crates use these traits/macros to expose their service bundles.
pub use architect::{Descriptors, Layer, LayerRouter, Mounted, Services, layers};

/// Backend-agnostic main-thread affinity. Service code calls
/// `daw::main_thread::query` / `run`; REAPER installs a `TaskSupport`-backed
/// executor, standalone installs none (the work runs inline). See
/// [`daw_proto::main_thread`].
pub use daw_proto::main_thread;

// Internal alias for the bootstrap singleton.
use rpc::Daw;

// ── Plugin API: DAW-agnostic initialization ─────────────────────────────────

use std::sync::OnceLock;

static DAW_INSTANCE: OnceLock<DawInstance> = OnceLock::new();

// wasm-only global `Daw` handle. daw-control's global singleton
// (`rpc::Daw::init`/`get`/`try_get`) is native-only, so the facade holds the
// handle here instead. `rpc::Daw` wraps vox connection types that are `!Sync`
// on wasm's single-threaded runtime, so a plain `static` (which must be `Sync`)
// can't hold it directly.
#[cfg(target_arch = "wasm32")]
struct WasmDaw(rpc::Daw);
// SAFETY: wasm32 is single-threaded — there is no other thread to share with
// or send to, so the `!Send`/`!Sync` vox connection types inside `Daw` are
// never accessed across threads. This mirrors how wasm-bindgen treats
// browser-global singletons. (`OnceLock<T>: Sync` requires `T: Send + Sync`.)
#[cfg(target_arch = "wasm32")]
unsafe impl Sync for WasmDaw {}
#[cfg(target_arch = "wasm32")]
unsafe impl Send for WasmDaw {}
#[cfg(target_arch = "wasm32")]
static WASM_DAW: OnceLock<WasmDaw> = OnceLock::new();

/// Facade-only runtime state. The `Daw` itself is NOT stored here — there is a
/// single global `Daw`, owned by `daw-control` (`rpc::Daw::get()`), which
/// `daw::get()` delegates to. This holds only the things the facade adds on top:
/// the runtime for `block_on` and the timer-callback list.
struct DawInstance {
    // Native-only: the `block_on` runtime. wasm32 has no blocking runtime; the
    // browser drives the facade purely through async `daw::get()` calls.
    #[cfg(not(target_arch = "wasm32"))]
    runtime: std::sync::Arc<tokio::runtime::Runtime>,
    _timer_callbacks: std::sync::Mutex<Vec<fn()>>,
}

/// Initialize DAW access from a plugin context.
///
/// `daw` knows no host of its own: this answers whether a DAW is already
/// installed ([`init_from_parts`]). Host detection — REAPER via CLAP — is an
/// adapter's, and lives in `studio::init` (feature `reaper`).
pub fn init(raw_host_context: Option<*const std::ffi::c_void>) -> bool {
    if DAW_INSTANCE.get().is_some() {
        return true;
    }

    let _ = raw_host_context;
    false
}

/// Initialize the facade from an already constructed DAW handle.
///
/// Extension hosts that build a custom in-process DAW service graph can use this
/// to make `daw::get()` and `daw::block_on()` available to reusable modules.
#[cfg(not(target_arch = "wasm32"))]
pub fn init_from_parts(daw: rpc::Daw, runtime: std::sync::Arc<tokio::runtime::Runtime>) -> bool {
    // Install the single global Daw (owned by daw-control) so both
    // `daw::get()` and `rpc::Daw::get()` resolve to the same instance.
    let _ = rpc::Daw::init(daw.caller().clone());
    if DAW_INSTANCE.get().is_some() {
        return true;
    }
    DAW_INSTANCE
        .set(DawInstance {
            runtime,
            _timer_callbacks: std::sync::Mutex::new(Vec::new()),
        })
        .is_ok()
}

/// wasm build of [`init_from_parts`]: install the global `Daw` so `daw::get()`
/// resolves, with no `block_on` runtime (the browser has none — reusable
/// modules drive the facade through async calls). The in-browser setlist
/// engine calls this after building its in-process `daw-control` client.
#[cfg(target_arch = "wasm32")]
pub fn init_from_parts(daw: rpc::Daw) -> bool {
    // daw-control's own global too (its wasm build keeps one per thread),
    // so code that reaches the DAW as `rpc::Daw::try_get()` — the panels,
    // the studio's project read-back — finds the same handle the facade
    // hands out. `daw::get()` reads it back out of WASM_DAW.
    let _ = rpc::Daw::init(daw.caller().clone());
    WASM_DAW.set(WasmDaw(daw)).is_ok()
}

/// Get the global `Daw` handle.
///
/// Delegates to the single global owned by `daw-control` (`rpc::Daw::get()`),
/// so the facade and lower-level code always see the same instance. Returns
/// `None` if no DAW has been initialized.
pub fn get() -> Option<&'static rpc::Daw> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        rpc::Daw::try_get()
    }
    // wasm: the facade owns the handle in WASM_DAW (daw-control's global
    // singleton is native-only).
    #[cfg(target_arch = "wasm32")]
    {
        WASM_DAW.get().map(|w| &w.0)
    }
}

/// Run an async operation on the DAW runtime.
///
/// Use from sync contexts (timer callbacks, process()) to call
/// async `Daw` methods. Native-only — wasm32 has no blocking runtime; browser
/// callers must `.await` the async `daw::get()` methods directly.
#[cfg(not(target_arch = "wasm32"))]
pub fn block_on<F: std::future::Future>(f: F) -> Option<F::Output> {
    DAW_INSTANCE.get().map(|i| i.runtime.block_on(f))
}

/// Register a callback that fires at ~30Hz on the DAW's main thread.
pub fn register_timer(callback: fn()) {
    if let Some(instance) = DAW_INSTANCE.get()
        && let Ok(mut cbs) = instance._timer_callbacks.lock()
    {
        cbs.push(callback);
    }
}

/// Called internally by the timer system to fire user callbacks.
#[doc(hidden)]
pub fn _fire_timer_callbacks() {
    if let Some(instance) = DAW_INSTANCE.get()
        && let Ok(cbs) = instance._timer_callbacks.lock()
    {
        for cb in cbs.iter() {
            cb();
        }
    }
}

// ── Main Thread Sync API ─────────────────────────────────────────────────────

// ── Service: raw protocol types & service clients ───────────────────────────
/// Raw protocol types and service clients.
pub mod service {
    pub use daw_proto::*;
}

// ── Standalone: reference/mock implementation ───────────────────────────────
#[cfg(feature = "standalone")]
/// Standalone reference implementation for testing (mock data included).
pub mod standalone {
    pub use daw_standalone::*;
}

// ── Plugin hosting: format-agnostic CLAP/VST3/LV2 plugin host ───────────────
//
// The host trait + loader live in `daw-standalone::plugin`. Re-exposed here
// directly under `daw::plugin` so consumers don't have to think of plugin
// hosting as a "standalone" concept — it's a general capability the standalone
// reference implementation happens to provide. The CLAP and VST3 backends are
// each behind their own feature flag (`clap-host`, `vst3-host`).
#[cfg(feature = "standalone")]
pub mod plugin {
    pub use daw_standalone::plugin::*;
}

// ── Realtime audio engine: no rig API ───────────────────────────────────────
//
// `AudioEngine` (reachable via `daw::standalone::audio_engine::AudioEngine`,
// gated on `standalone-audio`) is a pure realtime audio processor: open it on a
// project with `AudioEngine::with_project_prefs(daw, guid, shared, &prefs)` and
// it renders every block — items, live-input stage 0 (when `prefs.want_input`
// or a track is armed `RecordInput::Audio`), per-track FX, routing, master. It
// carries no `LiveRig`/`open_live`/`set_chain`/rig concept; consumers (e.g.
// signal's `GuitarRig`) assemble any "live monitor" rig themselves out of the
// project/FX primitives + this engine.

// ── Extension runtime: in-process REAPER extension hosting ──────────────────
//
// `daw-extension-runtime` lives as a public sibling crate, not under the
// `daw` facade. The fold can't be done without a cycle:
//   daw → daw-extension-runtime → daw-reaper → keyflow → daw
// (daw-reaper uses keyflow for chord detection; keyflow uses daw::module).
// Extension authors depend on `daw` + `daw-extension-runtime` together.

// ── Sync stack: engine + network + Link ─────────────────────────────────────
//
// `daw-synchronization`, `daw-network`, and `daw-link` are also public
// sibling crates, not folded under the `daw` facade. Each depends on
// `daw` itself (synchronization on the streaming surface, network on
// `SyncEvent` from synchronization, link is standalone), so a facade
// re-export would cycle. Consumers (other domain crates that want to
// reuse the peer-mesh transport for non-sync use cases, integration
// tests, the daw CLI's `sync` subcommand) depend on these directly.
//
// - `daw-synchronization` — backend-agnostic sync engine + drift corrector
//   + heartbeat. Wraps the streaming surface from `daw::service` in
//   `SyncEvent` envelopes.
// - `daw-network`       — TCP peer mesh + handshake + clock calibration.
//   Concrete on `SyncEvent` today; library-shaped so other domains can
//   reuse the transport.
// - `daw-link`          — Ableton Link adapter (host-agnostic via the
//   `LinkCallbacks` trait).
//
// `daw-bridge` wires all three together inside its `sync` cargo feature
// (default-on), gated at runtime by `FTS_SYNC_ENABLED=1`. See
// `daw-bridge/src/sync_runtime.rs` and `docs/sync-stack-consolidation.md`.

// ── Streaming ergonomics ────────────────────────────────────────────────────
pub use stream::RxExt;
mod stream;

// ── Module system: standard interface for extension modules ─────────────────
pub use daw_module as module;
pub use daw_module::{
    ActionDef, DawModule, DockPosition, ModuleContext, PanelComponent, PanelDef, PanelRenderer,
};
