//! Audio metering DSP — all computations run on the audio thread.
//!
//! Each module exposes a processor struct that ingests audio samples and
//! writes results into shared `Arc<RwLock<>>` state for the UI thread.
//!
//! # Architecture
//!
//! The pattern throughout this crate:
//! - DSP structs own an `Arc<*State>` that they write to on the audio thread.
//! - UI painters receive a cloned `Arc<*State>` and read from it during rendering.
//! - All mutable fields inside `*State` are behind `parking_lot::RwLock`.

pub mod bit_depth;
pub mod k_meter;
pub mod lufs;
pub mod phase;
pub mod spectrum;
pub mod true_peak;
