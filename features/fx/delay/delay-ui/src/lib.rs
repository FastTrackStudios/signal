//! FTS Delay's editor.
//!
//! Six families on a rail — Digital, Tape, Analog, Pitch, Rhythmic, Special —
//! each with its own panel and its own live picture of what the repeats are
//! doing. What a family *is* lives in `delay-profiles`; what it looks like
//! lives here.

pub mod faces;

#[cfg(feature = "native")]
pub mod control_view;
#[cfg(feature = "native")]
pub use fts_plug_ui::param_adapter;
#[cfg(feature = "native")]
pub mod params;
