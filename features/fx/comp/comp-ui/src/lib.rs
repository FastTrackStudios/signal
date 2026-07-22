//! Compressor GUI — Dioxus components for the FTS Comp plugin.
//!
//! Hosts the comp-specific Dioxus root component, the nice_plug parameter
//! tree, and the bridging glue from `nice_plug` parameters to
//! [`fts_ui_audio`] widgets. General-purpose widgets (knobs, meters, drag
//! provider) come from [`fts_ui_audio`]; layout primitives from [`fts_ui`].
//!
//! - [`control_view`]: the plugin editor — classic comp knob row + GR meter
//! - [`profile_view`]: hardware-profile data model (1176, LA-2A, SSL bus)
//! - [`params`]: nice_plug parameter tree + shared UI state
//! - [`param_adapter`]: nice_plug `ParamPtr` → [`fts_ui_audio::ParamHandle`]

// ── Portable core (no plugin framework; compiles for wasm) ──
pub mod comp_graph_svg;
pub mod profile_view;

pub use profile_view::{
    profile_skin, ProfileControlGroup, ProfileControlKind, ProfileControlView, ProfileParamWrite,
    ProfileSkin, ProfileSkinGroup, ProfileView,
};

// ── The Blitz/vello plugin editor ──
#[cfg(feature = "native")]
pub mod comp_graph;
#[cfg(feature = "native")]
pub mod control_view;
#[cfg(feature = "native")]
pub mod param_adapter;
#[cfg(feature = "native")]
pub mod params;
