//! FTS Saturate's editor.
//!
//! Five circuits on a rail — Tube, Tape, Transformer, Transistor, Digital —
//! each with its own panel and its own drawing of what it does to a waveform.
//! What a circuit *is* lives in `saturate-profiles`; what it looks like lives
//! here.

pub mod faces;

#[cfg(feature = "native")]
pub mod control_view;
#[cfg(feature = "native")]
pub use fts_plug_ui::param_adapter;
#[cfg(feature = "native")]
pub mod params;
