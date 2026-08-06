//! FTS Reverb's editor.
//!
//! Seven families on a rail — IR, Hall, Plate, Room, Spring, Ambient,
//! Special — each with its own panel and its own live picture of the space.
//! What a family *is* lives in `reverb-profiles`; what it looks like lives
//! here.

pub mod faces;

#[cfg(feature = "native")]
pub mod control_view;
#[cfg(feature = "native")]
pub mod param_adapter;
#[cfg(feature = "native")]
pub mod params;
