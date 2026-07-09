//! Limiter DSP engine — brickwall and creative limiting for the plugin suite.
//!
//! # Algorithms (from Airwindows)
//!
//! - [`adclip::AdClip`] — ADClip8 multi-stage clipper with successive
//!   approximation and anti-aliased clipping stages
//! - [`clip_softly::ClipSoftly`] — Sine-waveshaper soft clipper that
//!   rounds transients without hard edges
//! - [`block_party::BlockParty`] — Mu-law limiter with program-dependent
//!   release and minimal pumping
//! - [`loud::Loud`] — Slew-rate limiter that tames inter-sample peaks
//!   by limiting the rate of change
//! - [`chain::LimiterChain`] — Composable chain of the above limiters

pub mod adclip;
pub mod block_party;
pub mod chain;
pub mod clip_softly;
pub mod loud;

pub use chain::LimiterChain;
