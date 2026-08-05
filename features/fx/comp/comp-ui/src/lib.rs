//! Compressor GUI — Dioxus components for the FTS Comp plugin.
//!
//! Hosts the comp-specific Dioxus root component, the nice_plug parameter
//! tree, and the bridging glue from `nice_plug` parameters to
//! [`fts_ui_audio`] widgets. General-purpose widgets (knobs, meters, drag
//! provider) come from [`fts_ui_audio`]; layout primitives from [`fts_ui`].
//!
//! - [`control_view`]: the plugin editor shell — header, profile picker, and
//!   the face the selected profile swaps in
//! - [`faces`]: one front panel per profile — the FTS control surface, and the
//!   LA-2A / 1176 / SSL bus faceplates
//! - [`hardware`]: the parts a faceplate is made of (VU movement, pointer
//!   knobs, panel switches, chrome) plus their portable geometry
//! - [`profile_handle`]: a profile control → [`fts_ui_audio::ParamHandle`],
//!   including the macro fanout that lets one knob write several engine params
//! - [`sections`]: the labelled-section / knob / selector layout primitives
//! - [`profile_view`]: hardware-profile data model (1176, LA-2A, SSL bus)
//! - [`params`]: nice_plug parameter tree + shared UI state
//! - [`param_adapter`]: nice_plug `ParamPtr` → [`fts_ui_audio::ParamHandle`]

// ── Portable core (no plugin framework; compiles for wasm) ──
pub mod comp_graph_svg;
pub mod profile_view;

/// The hardware-faceplate kit — VU movements, pointer knobs, panel switches,
/// panel chrome — lives in `fts-ui-audio` now that the EQ wears faceplates
/// too. Re-exported so `crate::hardware::…` still names it here.
pub use fts_ui_audio::hardware;

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
pub mod faces;
#[cfg(feature = "native")]
pub mod param_adapter;
#[cfg(feature = "native")]
pub mod param_map;
#[cfg(feature = "native")]
pub mod params;
#[cfg(feature = "native")]
pub mod profile_handle;
#[cfg(feature = "native")]
pub mod sections;
