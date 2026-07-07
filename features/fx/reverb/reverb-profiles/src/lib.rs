//! Reverb hardware profiles — parameter mappings and constraints.
//!
//! A profile defines:
//! - Which controls appear (knobs, switches, stepped selectors)
//! - How each control maps to [`reverb_dsp::ReverbChain`] parameters
//! - Constraints (room size, decay time, diffusion settings)
//!
//! Profiles are pure data + mapping functions. No GUI, no framework deps.

pub mod control;
pub mod core;
pub mod emt_140;
pub mod lexicon_480;

pub use self::core::{Constraint, ParamMapping, Profile, ProfileControl};
