//! Safe wrappers around REAPER low-level C API calls.
//!
//! Each sub-module exposes **safe** Rust functions that internally use `unsafe`
//! to call `reaper_low::Reaper` (the raw C FFI). Service code should call these
//! wrappers instead of writing `unsafe` blocks directly.
//!
//! # Design
//!
//! Follows the reaper-rs 3-tier pattern (low → medium → high). These wrappers
//! sit between `low` and the service layer, providing safe signatures while
//! keeping all `unsafe` in one auditable location.

pub mod audio;
pub mod audio_accessor;
pub mod buffer;
pub mod cstring;
pub mod envelope;
pub mod ext_state;
pub mod fx;
pub mod item;
pub mod markers;
pub mod midi;
pub mod mouse;
pub mod peak;
pub mod routing;
pub mod ruler_lanes;
pub mod tempo;
pub mod time_map;
pub mod ui;

/// Convenience alias for the low-level REAPER FFI struct.
pub type ReaperLow = reaper_low::Reaper;
