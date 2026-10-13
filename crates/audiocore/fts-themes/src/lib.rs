//! Theme engine — loads and renders visual presentations for profiles.
//!
//! A theme defines layout, colors, knob style, assets, and fonts.
//! Multiple themes can reference the same profile (e.g., a skeuomorphic
//! Pultec theme and a minimal Pultec theme both use the Pultec profile).

pub mod engine;
pub mod fasttrack;
