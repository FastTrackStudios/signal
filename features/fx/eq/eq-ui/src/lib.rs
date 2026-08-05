//! EQ GUI — Dioxus components for the EQ plugin.
//!
//! Hosts the EQ-specific Dioxus root component, the EQ graph viz, the
//! nice_plug parameter tree, and the bridging glue from `nice_plug` parameters
//! to [`fts_ui_audio`] widgets. EQ-specific visualizations live here. General-
//! purpose widgets (knobs, sliders, meters) come from [`fts_ui_audio`];
//! general layout primitives come from [`fts_ui`].
//!
//! - [`control_view`]: Pro-Q style spectrum analyzer with draggable band nodes
//! - [`eq_graph`] / [`eq_graph_painter`]: vello-rendered frequency-response graph
//! - [`faces`]: one front panel per hardware model — Pultec, SSL, API, 1073 —
//!   drawn from the shared kit in [`fts_ui_audio::hardware`]
//! - [`profile_view`]: profile-driven layouts (Pultec knob layout, etc.)
//! - [`params`]: nice_plug parameter tree + shared UI state
//! - [`param_adapter`]: nice_plug `ParamPtr` → [`fts_ui_audio::ParamHandle`]

// ── Portable core (compiles for wasm; the detached remotes build on it) ──
pub mod cheatsheet;
pub mod eq_graph_interaction;
pub mod eq_graph_model;
pub mod eq_graph_response;
pub mod eq_graph_svg;

// ── The Blitz/vello plugin editor ──
#[cfg(feature = "native")]
pub mod control_view;
#[cfg(feature = "native")]
pub mod eq_graph;
#[cfg(feature = "native")]
pub mod faces;
#[cfg(feature = "native")]
pub mod eq_graph_painter;
#[cfg(feature = "native")]
pub mod eq_graph_popup;
#[cfg(feature = "native")]
pub mod param_adapter;
#[cfg(feature = "native")]
pub mod params;
#[cfg(feature = "native")]
pub mod profile_view;
