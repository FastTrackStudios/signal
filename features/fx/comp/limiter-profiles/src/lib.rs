//! Limiter hardware profiles — parameter mappings and constraints.
//!
//! A profile defines:
//! - Which controls appear (knobs, switches, stepped selectors)
//! - How each control maps to [`limiter_dsp::LimiterChain`] parameters
//! - Constraints (ceiling, release shape, lookahead settings)
//!
//! Profiles are pure data + mapping functions. No GUI, no framework deps.

pub mod control;
pub mod core;
pub mod l2_style;
pub mod mastering;

pub use self::core::{Constraint, ParamMapping, Profile, ProfileControl};
