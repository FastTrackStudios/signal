//! FTS Reverb's editor.
//!
//! Seven families on a rail — IR, Hall, Plate, Room, Spring, Ambient,
//! Special — each with its own panel and its own live picture of the space.
//! What a family *is* lives in `reverb-profiles`; what it looks like lives
//! here.

pub mod faces;

/// The live picture of what the effect is doing — one widget, two painters:
/// WGSL where the renderer hands over a device, vectors where it does not.
/// Behind a feature because it wants a Blitz host for the custom-widget
/// mechanism, which a plain wasm remote has not got.
#[cfg(feature = "viz")]
pub mod viz;

#[cfg(feature = "native")]
pub mod control_view;
#[cfg(feature = "native")]
pub mod eq_view;
#[cfg(feature = "native")]
pub use fts_plug_ui::param_adapter;
#[cfg(feature = "native")]
pub mod params;
#[cfg(feature = "native")]
pub mod preset_view;
