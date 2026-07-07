//! Delay DSP engine — time-based effects for the plugin suite.
//!
//! Based on techniques from qdelay (tiagolr) and ChowDSP's AnalogTapeModel.
//!
//! - [`tape_delay::TapeDelay`] — Tape echo with wow/flutter, feedback filtering,
//!   saturation, ducking, and diffusion
//! - [`pitch_delay::PitchDelay`] — Per-repeat pitch shifting with granular crossfade
//! - [`engine::DelayEngine`] — Unified wrapper over all delay styles
//! - [`chain::DelayChain`] — Full stereo delay with ping-pong, swing, and mix

pub mod bbd_delay;
pub mod chain;
pub mod clean_delay;
pub mod engine;
pub mod lofi_delay;
pub mod modulation;
pub mod pitch_delay;
pub mod reverse_delay;
pub mod rhythm_delay;
pub mod shimmer_delay;
pub mod tape_delay;

pub use chain::{DelayChain, HeadMode};
pub use engine::{DelayEngine, DelayStyle};
pub use modulation::WobbleShape;
pub use tape_delay::SaturationType;
