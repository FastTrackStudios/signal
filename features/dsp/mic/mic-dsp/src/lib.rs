//! Dual-capsule microphone modelling.
//!
//! A front/rear capsule pair (a Sphere-style dual-diaphragm recording:
//! left = front capsule, right = rear) is rendered as a modelled mic, with
//! the reference's controls — pattern, axis, low cut, proximity, rear trim,
//! output, phase, capsule swap — reproduced from measurements of it rather
//! than from a physical model, so the result can be nulled against it.
//!
//! What the measurements showed the reference to be, and so what this is:
//! a static linear 2→1 filter per setting (the real-mic models add a weak
//! level-dependent distortion, not modelled yet), whose pattern is one of
//! nine measured steps, whose axis is a linear crossfade of five measured
//! anchors, and whose low cut and proximity are exact first-order moves of
//! a corner. See `signal-analyzer`'s `sphere_capture` for the measurement.

pub mod conv;
pub mod engine;
pub mod model;
pub mod section;

pub use engine::{DualAdjust, DualMic, DualSettings, LATENCY, MicChain, latency, Settings, Solo, StereoMic, StereoSettings};
pub use model::{MicModel, ModelError, ProximityLaw};
