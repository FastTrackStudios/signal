//! FTS Modulation's editor.
//!
//! Five circuits on a rail — Chorus, Flanger, Vibrato, Tremolo, Wah — each
//! with its own panel and its own drawing of what it moves. What a circuit
//! *is* lives in `modulation-profiles`; what it looks like lives here.

pub mod faces;

#[cfg(feature = "native")]
pub mod control_view;
#[cfg(feature = "native")]
pub mod param_adapter;
#[cfg(feature = "native")]
pub mod params;
