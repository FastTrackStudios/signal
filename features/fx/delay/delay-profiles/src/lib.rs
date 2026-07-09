//! Delay hardware profiles — parameter mappings and constraints.
//!
//! A profile defines:
//! - Which controls appear (knobs, switches, stepped selectors)
//! - How each control maps to [`delay_dsp::DelayChain`] parameters
//! - Constraints (tempo sync, feedback limits, filter settings)
//!
//! Profiles are pure data + mapping functions. No GUI, no framework deps.

pub mod control;
pub mod core;
pub mod echoplex;
pub mod space_echo;

pub use self::core::{Constraint, ParamMapping, Profile, ProfileControl};
