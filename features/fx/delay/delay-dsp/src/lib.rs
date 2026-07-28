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
pub mod drum_delay;
pub mod dual;
pub mod engine;
pub mod filter_delay;
pub mod lofi_delay;
pub mod modulation;
pub mod multitap_delay;
pub mod oilcan_delay;
pub mod pitch_delay;
pub mod reverb_delay;
pub mod reverse_delay;
pub mod rhythm_delay;
pub mod shimmer_delay;
pub mod spectral_delay;
pub mod tape_delay;
pub mod tilt;

pub use chain::{DelayChain, HeadMode, TapDivision};
pub use dual::{DualDelay, DualRouting};
pub use drum_delay::{DrumHead, DrumSpacing, HeadPlayback, GOLDEN_HEADS, SILVER_HEADS};
pub use engine::{DelayEngine, DelayStyle};
pub use filter_delay::{FilterLfoShape, FilterLocation};
pub use modulation::WobbleShape;
pub use multitap_delay::{FeedbackMode, Tap, TapFilter, TapGrid, TapPreset, MAX_TAPS};
pub use lofi_delay::LoFiFilterShape;
pub use oilcan_delay::OilCanHeads;
pub use spectral_delay::{DensityMode, GrainDirection, GrainShape};
pub use bbd_delay::BbdVoice;
pub use pitch_delay::{IceInterval, IceSlice};
pub use tape_delay::{SaturationType, TapeSpeed, TapeVoice};

/// Equal-power pan gains — see [`audiocore_dsp::stereo::pan_equal_power`]
/// (centered = unity both sides; hard pans full-power one side).
#[inline]
pub(crate) fn pan_gains(pan: f64) -> (f64, f64) {
    audiocore_dsp::stereo::pan_equal_power(pan)
}
