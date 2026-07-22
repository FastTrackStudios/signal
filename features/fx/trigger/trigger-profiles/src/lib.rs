//! Drum trigger profiles — parameter mappings and constraints.
//!
//! A profile defines which controls appear, how they map to
//! [`trigger_dsp::TriggerChain`] parameters, and any constraints.
//!
//! Profiles are pure data + mapping functions. No GUI, no framework deps.

pub mod control;
pub mod core;
pub mod drum_replacer;
