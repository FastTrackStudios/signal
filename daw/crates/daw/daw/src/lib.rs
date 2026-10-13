// Lint debt: workspace flipped dead_code/unused to warn (task cleanup);
// this crate predates that — burn down separately.
#![allow(dead_code, unused)]

//! Unified facade for DAW interaction.
//!
//! This is the single public API surface for the `daw` domain. External consumers
//! should depend only on this crate — never on internal crates directly.
//!
//! # Namespaces
//!
//! - **`rpc`** → `daw::rpc` — Async Vox control API (`Daw`, `Project`, `Transport`, `TrackHandle`,
//!   `FxChain`, `BatchBuilder`, etc.). Use from UI, remote clients, background tasks.
//! - **`service`** → `daw::service` — Raw protocol types and service clients.
//!
//! # Feature-gated modules
//!
//! - **`standalone`** → `daw::standalone` — Reference implementation for testing.
//! - **`file`** → `daw::file` — RPP file format parser.

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
/// Detects the host environment (REAPER via CLAP, standalone, etc.)
/// and creates the appropriate `Daw` instance. Call once from
/// `Plugin::initialize()`.
///
/// ```rust,ignore
/// fn initialize(&mut self, ..., context: &mut impl InitContext<Self>) -> bool {
///     daw::init(context.raw_host_context());
///     true
/// }
/// ```
pub fn init(raw_host_context: Option<*const std::ffi::c_void>) -> bool {
    if DAW_INSTANCE.get().is_some() {
        return true;
    }

    #[cfg(feature = "reaper")]
    if let Some(host_ptr) = raw_host_context
        && let Some((daw, runtime)) = daw_reaper::bootstrap::create_plugin_daw(host_ptr)
    {
        // Register internal timer that fires user callbacks
        daw_reaper::bootstrap::register_internal_timer(|| {
            _fire_timer_callbacks();
        });

        // Install the single global Daw (owned by daw-control) + the facade's
        // runtime/timers.
        let _ = rpc::Daw::init(daw.caller().clone());
        let _ = DAW_INSTANCE.set(DawInstance {
            runtime,
            _timer_callbacks: std::sync::Mutex::new(Vec::new()),
        });
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

/// Get a sync DAW handle for code running on REAPER's main thread.
///
/// Returns `None` if not on the main thread or not in REAPER.
/// This is the zero-overhead path for timer callbacks.
///
/// ```rust,ignore
/// fn my_timer() {
///     let daw = daw::main_thread_daw().unwrap();
///     let tracks = daw.track_list();
///     daw.fx_param_set("guid", 0, 2, 0.5);
/// }
/// ```
#[cfg(feature = "reaper")]
pub fn main_thread_daw() -> Option<daw_reaper::DawMainThread> {
    daw_reaper::DawMainThread::try_new()
}

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

// ── CSI: hardware control surfaces ──────────────────────────────────────────
#[cfg(feature = "csi")]
/// Control Surface Integration — Mackie Control / Behringer X-Touch
/// driver fed by the event bus. `daw::csi::run(daw, config)` connects
/// a surface to any backend reachable through `daw_control::Daw`.
pub mod csi {
    pub use daw_csi::*;
}

// ── REAPER backend re-export ────────────────────────────────────────────────
// `daw-bridge`, `daw-perf-test`, and the in-process sync engine reach
// through the facade to REAPER-side helpers (event_hub, safe_wrappers,
// register_*) without taking direct `daw-reaper` deps everywhere. No
// cycle: `daw-reaper` is a feature-gated dependency of this facade.
#[cfg(feature = "reaper")]
pub mod reaper {
    pub use daw_reaper::*;
}

// ── File: RPP file format parser ────────────────────────────────────────────
#[cfg(feature = "file")]
/// High-performance RPP (REAPER Project) file format parser, plus
/// cross-format project conversion ([`file::convert`], with the `convert`
/// feature).
pub mod file {
    pub use dawfile_reaper::*;

    /// Error from [`convert`].
    #[cfg(feature = "reaper")]
    #[derive(Debug)]
    pub enum ConvertError {
        /// The output extension isn't a supported conversion target.
        UnsupportedOutput(String),
        /// The conversion itself failed (unsupported input, parse/build error).
        Convert(String),
        /// Reading the input or writing the output failed.
        Io(std::io::Error),
    }

    #[cfg(feature = "reaper")]
    impl std::fmt::Display for ConvertError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                ConvertError::UnsupportedOutput(ext) => {
                    write!(
                        f,
                        "unsupported output format '.{ext}' (supported target: .rpp)"
                    )
                }
                ConvertError::Convert(e) => write!(f, "conversion failed: {e}"),
                ConvertError::Io(e) => write!(f, "io error: {e}"),
            }
        }
    }

    #[cfg(feature = "reaper")]
    impl std::error::Error for ConvertError {}

    /// Convert a supported project file to another supported format, dispatching
    /// on file extensions.
    ///
    /// Supported today: any of `.ptx` / `.ptf` / `.pts` (Pro Tools), `.als`
    /// (Ableton), `.aaf`, `.dawproject` **→ `.rpp` (REAPER)**. Other output
    /// formats return [`ConvertError::UnsupportedOutput`]; unsupported inputs
    /// surface as [`ConvertError::Convert`]. Requires the `reaper` feature
    /// (enable both via the `convert` feature).
    ///
    /// ```no_run
    /// daw::file::convert("session.ptx", "session.rpp").unwrap();
    /// ```
    #[cfg(feature = "reaper")]
    pub fn convert(
        input: impl AsRef<std::path::Path>,
        output: impl AsRef<std::path::Path>,
    ) -> Result<(), ConvertError> {
        let input = input.as_ref();
        let output = output.as_ref();
        let out_ext = output
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        match out_ext.as_str() {
            "rpp" => {
                let rpp = daw_reaper::project_import::convert_to_rpp(&input.to_string_lossy())
                    .map_err(|e| ConvertError::Convert(e.to_string()))?;
                std::fs::write(output, rpp).map_err(ConvertError::Io)
            }
            other => Err(ConvertError::UnsupportedOutput(other.to_string())),
        }
    }
}

// ── Source-format parsers ───────────────────────────────────────────────────
//
// Each module exposes one DAW's file-format parser (format → daw-proto
// types) and is gated by a per-format feature, so a consumer depends only
// on the formats it actually reads. None of these pull in REAPER; the
// reaper-based file→RPP conversion lives in [`file::convert`] instead.
// Every parser declares a `capability::FeatureSupport` describing which
// daw-proto domains it can read/write for that format.

/// Pro Tools session parser (`.ptx` / `.ptf` / `.pts`).
#[cfg(feature = "protools")]
pub mod protools {
    pub use dawfile_protools::*;
}

/// Ableton Live Set parser (`.als`).
#[cfg(feature = "ableton")]
pub mod ableton {
    pub use dawfile_ableton::*;
}

/// Logic Pro session parser (`.logicx`).
#[cfg(feature = "logic")]
pub mod logic {
    pub use dawfile_logic::*;
}

/// Advanced Authoring Format parser (`.aaf`).
#[cfg(feature = "aaf")]
pub mod aaf {
    pub use dawfile_aaf::*;
}

/// DAWproject parser (`.dawproject`).
#[cfg(feature = "dawproject")]
pub mod dawproject {
    pub use dawfile_dawproject::*;
}

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

// ── Integration-test harness ────────────────────────────────────────────────
//
// Enabled via `features = ["test-harness"]`. Folds the former `reaper-test`
// crate in as a feature-gated module that drives `daw-reaper` over the daw
// RPC surface. Generalized over the `DawBackend` trait (single concrete impl:
// `ReaperBackend`). Re-exports the `#[daw_test]` / `#[reaper_test]` macros.
#[cfg(feature = "test-harness")]
pub mod test;

/// Versioning a REAPER configuration — the ~350 KB that actually
/// defines "your REAPER", filtered out of a ~360 MB resource directory.
///
/// Plain file work: no DAW connection, no feature gate.
pub mod reaper_config;

// ── CLI ─────────────────────────────────────────────────────────────────────
//
// The `daw` command-line surface (former `apps/daw-cli`), folded in behind
// the `cli` feature. `daw::cli::cli_main(argv)` is the embeddable entry —
// the thin `daw` binary (src/bin/daw.rs) and the `fts daw` subcommand both
// mount it.
#[cfg(feature = "cli")]
pub mod cli;

// ── Streaming ergonomics ────────────────────────────────────────────────────
pub use stream::RxExt;
mod stream;

// ── Module system: standard interface for extension modules ─────────────────
pub use daw_module as module;
pub use daw_module::{
    ActionDef, DawModule, DockPosition, ModuleContext, PanelComponent, PanelDef, PanelRenderer,
};

// ── UI host (REAPER + Dioxus) ──────────────────────────────────────────────
//
// Enabled via `features = ["reaper-ui"]`. Re-exports the curated public
// surface of `daw-reaper-dioxus` so apps depend only on the daw facade.
// Audio-graph / WASM / embedded targets get nothing pulled in — the GPU
// / blitz / dioxus-native stack only enters the dependency graph when the
// feature is on.
//
// This is REAPER-specific UI plumbing (dock host, HWND-embedded views,
// overlays). The portable, vector-themeable component library lives under
// `daw::ui` (the `ui` feature) instead.
#[cfg(feature = "reaper-ui")]
pub mod reaper_ui {
    /// Dock host implementation backed by REAPER's docker + Dioxus.
    pub mod dock {
        pub use daw_reaper_dioxus::dock::{
            DockablePanelConfig, hide_panel, init as init_dock, is_panel_hwnd, is_panel_visible,
            panel_element_size, prewarm_panel, register_panel, register_panel_from_service,
            remount_panel, restore_dock_state, save_dock_state, set_panel_focus_on_click,
            show_panel, toggle_panel, unregister_all_panels, update_panels,
        };
        pub use daw_reaper_dioxus::service::{init as init_service, register_panel_from_def};
    }

    /// The `DockHosting` backend, for hosts that publish their own
    /// service router.
    ///
    /// `daw-bridge` mounts this itself; anything else running its own
    /// host (fts-extensions, via `host-hooks`) has to mount it too, or
    /// `DockHost` calls arrive at a router with no such service and fail
    /// to decode rather than failing cleanly.
    pub use daw_reaper_dioxus::ReaperDockHost;

    /// Embedded Dioxus view inside an existing REAPER HWND.
    pub use daw_reaper_dioxus::EmbeddedView;
    /// Transparent floating overlay (HUDs, popups).
    pub use daw_reaper_dioxus::{DioxusOverlay, DioxusOverlayBuilder, OverlayConfig};

    /// Pixel snapshot helpers — render a Dioxus root component to an
    /// off-screen GPU surface and assert against a golden byte file.
    /// Useful for component-level visual regression tests in
    /// fts-extensions or any other consumer.
    pub mod snapshot {
        pub use daw_reaper_dioxus::snapshot::{
            SnapshotError, compare_to_golden, render_panel_offscreen,
        };
    }

    /// Dioxus prelude — re-exported for component authors so they don't have
    /// to depend on `dioxus-native` directly.
    pub use daw_reaper_dioxus::prelude;
}

// ── UI components (portable, vector-themeable) ─────────────────────────────
//
// Enabled via `features = ["ui"]`. Re-exports the `daw-ui` component library —
// theming tokens, widgets, and the native components family (`daw::ui::
// components` — the traced vector TCP/arrange/mixer of PR #279's main
// window; the WALTER `panels` family was deleted 2026-08-19, see
// `daw_ui::panels`' tombstone). Renderer-agnostic: drives equally under
// `dioxus-native` (Blitz/GPU) on desktop and dioxus-web in the browser. Apps
// consume the components through `daw::ui` rather than depending on
// `daw-ui` directly.
#[cfg(feature = "ui")]
pub mod ui {
    pub use daw_ui::*;
}
