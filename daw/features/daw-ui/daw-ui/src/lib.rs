// Lint debt: workspace flipped dead_code/unused to warn (task cleanup);
// this crate predates that — burn down separately.
#![allow(dead_code, unused)]

//! DAW UI — Dioxus components for DAW integration.
//!
//! Provides panels for interacting with a connected DAW:
//! - MixerPanel — horizontal track strips with volume/pan/mute/solo
//! - TrackControlPanel (TCP) — vertical track list with folder hierarchy
//! - ArrangementView — native arrange: ruler, lanes on the TCP's pitch, items
//! - MainWindowPreview — the whole REAPER shape: transport, TCP | arrange, docked mixer
//! - FxParameterBrowser — live FX parameter browser with bidirectional control
//! - FxBrowserDockPanel — dock-ready wrapper for the FX browser
//!
//! **Before writing a component, read `features/daw-ui/BLITZ.md`** — the
//! rendering contract for the Blitz renderer REAPER embeds. Every rule in
//! it was a multi-hour bug first. The measured screenshots the layout is
//! converged against live in `features/daw-ui/reference/`.

// ── DAW-wired panels (poll a live `daw_control::Daw`) ──
pub mod components;
/// The thin wrappers that make `daw_theme_art`'s pure art clickable — see
/// [`controls`] for why the two layers are separate.
#[cfg(feature = "web")]
pub mod controls;
pub mod hooks;
pub mod layouts;
pub mod panel_registration;
pub mod prelude;
pub mod signals;

// ── Reusable, vector-themeable component library (merged from the former
// `audio-controls` crate). Low-level widgets + the token-based theming model.
// The WALTER-driven top-level panels that used to live beside them were
// deleted 2026-08-19 — see `panels/mod.rs` for the tombstone; the native
// `components::*` family above is the one UI. ──
pub mod core;
/// The native transport bar (require the `web`/dioxus feature). The rest
/// of this module is a tombstone — see its docs.
#[cfg(feature = "web")]
pub mod panels;
pub mod theming;
pub mod widgets;

#[cfg(feature = "reaper-test-panels")]
pub mod test_panels;

/// The compiled Tailwind chrome classes some `components` panels use
/// (mixer, TCP). Exported so a host window can embed it with
/// `document::Style` — a cross-crate `include_str!` of the asset would
/// break at the repo boundary, and a linked stylesheet never loads
/// under Blitz.
pub const TAILWIND_CSS: &str = include_str!("../assets/tailwind.css");

// Re-exports for desktop app
pub use components::arrangement_view::ArrangementView;
pub use components::fx_chain_tree::FxChainTree;
pub use components::fx_parameter_browser::FxParameterBrowser;
pub use components::mixer::MixerPanel;
#[cfg(feature = "web")]
pub use components::toolbars::{KeybindProfile, ProfilePicker};
pub use components::toolbars::{
    LeftToolbar, ModeDropdown, ModeIndicator, ModeOption, RightToolbar, ToolbarAction, TopToolbar,
};
pub use components::track_control_panel::TrackControlPanel;
pub use layouts::daw_panels::{DawApplication, FxBrowserDockPanel};
pub use panel_registration::register_panels;
