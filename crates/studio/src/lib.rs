//! FastTrack Studio: the DAW and everything built on it.
//!
//! `studio` re-exports the [`daw`] crate — the DAW's control API, protocol
//! and its own engine — and adds, each behind a feature, what sits on top:
//! the REAPER adapter (`reaper`, `reaper-ui`), project files (`file`,
//! `convert`, the source-format parsers), the UI components (`ui`), control
//! surfaces (`csi`), the `daw` command line (`cli`) and the integration-test
//! harness (`test-harness`).

pub use daw::*;

/// Initialize DAW access from a plugin context.
///
/// Detects the host environment (REAPER via CLAP, with the `reaper`
/// feature) and installs the matching `Daw`, so `studio::get()` and
/// `studio::block_on()` work. Call once from `Plugin::initialize()`.
///
/// ```rust,ignore
/// fn initialize(&mut self, ..., context: &mut impl InitContext<Self>) -> bool {
///     studio::init(context.raw_host_context());
///     true
/// }
/// ```
pub fn init(raw_host_context: Option<*const std::ffi::c_void>) -> bool {
    if daw::init(raw_host_context) {
        return true;
    }
    #[cfg(feature = "reaper")]
    if let Some(host_ptr) = raw_host_context
        && let Some((handle, runtime)) = daw_reaper::bootstrap::create_plugin_daw(host_ptr)
    {
        // The internal timer fires the facade's user callbacks.
        daw_reaper::bootstrap::register_internal_timer(daw::_fire_timer_callbacks);
        return daw::init_from_parts(handle, runtime);
    }
    let _ = raw_host_context;
    false
}

/// Get a sync DAW handle for code running on REAPER's main thread.
///
/// Returns `None` if not on the main thread or not in REAPER.
/// This is the zero-overhead path for timer callbacks.
///
/// ```rust,ignore
/// fn my_timer() {
///     let daw = studio::main_thread_daw().unwrap();
///     let tracks = daw.track_list();
///     daw.fx_param_set("guid", 0, 2, 0.5);
/// }
/// ```
#[cfg(feature = "reaper")]
pub fn main_thread_daw() -> Option<daw_reaper::DawMainThread> {
    daw_reaper::DawMainThread::try_new()
}

// ── CSI: hardware control surfaces ──────────────────────────────────────────
#[cfg(feature = "csi")]
/// Control Surface Integration — Mackie Control / Behringer X-Touch
/// driver fed by the event bus. `studio::csi::run(daw, config)` connects
/// a surface to any backend reachable through `daw_control::Daw`.
pub mod csi {
    pub use control_surfaces::*;
}

// ── REAPER backend re-export ────────────────────────────────────────────────
// `daw-bridge`, `daw-perf-test`, and the in-process sync engine reach
// through the facade to REAPER-side helpers (event_hub, safe_wrappers,
// register_*) without taking direct `daw-reaper` deps everywhere. No
// cycle: `daw-reaper` is a feature-gated dependency of this crate.
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
    /// studio::file::convert("session.ptx", "session.rpp").unwrap();
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
    pub use formats::protools::*;
}

/// Ableton Live Set parser (`.als`).
#[cfg(feature = "ableton")]
pub mod ableton {
    pub use formats::ableton::*;
}

/// Logic Pro session parser (`.logicx`).
#[cfg(feature = "logic")]
pub mod logic {
    pub use formats::logic::*;
}

/// Advanced Authoring Format parser (`.aaf`).
#[cfg(feature = "aaf")]
pub mod aaf {
    pub use formats::aaf::*;
}

/// DAWproject parser (`.dawproject`).
#[cfg(feature = "dawproject")]
pub mod dawproject {
    pub use formats::dawproject::*;
}

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
// the `cli` feature. `studio::cli::cli_main(argv)` is the embeddable entry —
// the thin `daw` binary (src/bin/daw.rs) mounts it.
#[cfg(feature = "cli")]
pub mod cli;

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
// `studio::ui` (the `ui` feature) instead.
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
// theming tokens, widgets, and the native components family (`studio::ui::
// components` — the traced vector TCP/arrange/mixer of PR #279's main
// window; the WALTER `panels` family was deleted 2026-08-19, see
// `daw_ui::panels`' tombstone). Renderer-agnostic: drives equally under
// `dioxus-native` (Blitz/GPU) on desktop and dioxus-web in the browser. Apps
// consume the components through `studio::ui` rather than depending on
// `daw-ui` directly.
#[cfg(feature = "ui")]
pub mod ui {
    pub use daw_ui::*;
}
