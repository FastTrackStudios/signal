//! EQ GUI — Dioxus components for the EQ plugin.
//!
//! Hosts the EQ-specific Dioxus root component, the EQ graph viz, the
//! `nice_plug` parameter tree, and the bridging glue from `nice_plug` parameters
//! to [`fts_audio_ui`] widgets. EQ-specific visualizations live here. General-
//! purpose widgets (knobs, sliders, meters) come from [`fts_audio_ui`];
//! general layout primitives come from [`architect_ui`].
//!
//! - [`control_view`]: Pro-Q style spectrum analyzer with draggable band nodes
//! - [`eq_graph`] / [`eq_graph_painter`]: vello-rendered frequency-response graph
//! - [`faces`]: one front panel per hardware model — Pultec, SSL, API, 1073 —
//!   drawn from the shared kit in [`fts_audio_ui::hardware`]
//! - [`profile_view`]: profile-driven layouts (Pultec knob layout, etc.)
//! - [`params`]: `nice_plug` parameter tree + shared UI state
//! - [`param_adapter`]: `nice_plug` `ParamPtr` → [`fts_audio_ui::ParamHandle`]

// ── Portable core (compiles for wasm; the detached remotes build on it) ──
pub mod cheatsheet;
/// Dynamic + spectral EQ: the ring, the badge, and the band panel.
///
/// Portable, and gated behind `native` until 2026-09 only because it arrived
/// alongside the vello editor. It needs nothing that editor needs — dioxus
/// and `fts_audio_ui`'s `Knob`/`ParamHandle`, both unconditional deps — and a
/// detached remote has the same eleven per-band parameters to reach as the
/// plugin does. Keeping it native-only meant the wire-driven surfaces could
/// draw a band's frequency and gain but never its dynamics.
pub mod dynamics;
pub mod eq_graph_interaction;
pub mod eq_graph_model;
pub mod eq_graph_response;
pub mod eq_graph_svg;

// ── The Blitz/vello graph, embeddable on its own (`graph`) ──
#[cfg(feature = "native")]
pub mod control_view;
#[cfg(feature = "graph")]
pub mod eq_graph;
/// The custom-widget mechanism the graph is drawn with — Blitz's own types
/// (`nice_plug_dioxus::widget` re-exports the same), so the graph needs no
/// plugin host.
#[cfg(all(feature = "graph", not(target_arch = "wasm32")))]
pub(crate) mod widget {
    pub use blitz_dom::node::ComputedStyles;
    pub use blitz_traits::events::UiEvent;
    pub use dioxus_native::{CustomWidgetAttr, Widget};
}
#[cfg(feature = "native")]
pub mod preset_view;

/// The compiled utilities + theme tokens the EQ surface's DOM parts
/// (band popup, context menus, selectors) style themselves with.
///
/// An editor that EMBEDS [`eq_graph::EqGraph`] (`fx.embed-eq.one-surface` —
/// the saturator, the reverb) must inject this via `document::Style`, exactly
/// as the EQ plugin does. Without it the popup's layout classes are undefined
/// and collapse.
///
/// Exported as bytes because `include_str!` cannot cross a crate boundary.
#[cfg(feature = "graph")]
pub const TAILWIND_CSS: &str = include_str!("../assets/tailwind.css");
/// The graph's shader layer — the analyser as a field, a bloom under every
/// band. Painted under the vector graph; see [`eq_glow`].
#[cfg(feature = "graph")]
pub mod eq_glow;
/// The graph's painters — portable, so a host without a plugin stack (a
/// browser's canvas) can draw the same picture.
#[cfg(feature = "graph-paint")]
pub mod eq_graph_painter;
#[cfg(feature = "graph")]
pub mod eq_graph_popup;
#[cfg(feature = "native")]
pub mod faces;
#[cfg(feature = "native")]
pub use fts_plug_ui::param_adapter;
#[cfg(feature = "native")]
pub mod params;
#[cfg(feature = "native")]
pub mod profile_view;
