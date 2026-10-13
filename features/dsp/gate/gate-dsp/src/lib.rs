//! gate-dsp — a drum gate with source modes, look-ahead, a shaped release
//! and an HF de-bleed expander, matched to a reference drum gate by null
//! test (black-box measurement only; see the module docs for what each
//! block was measured to be).
//!
//! Structure (all measured):
//!
//! - **Modes** are fixed bundles: look-ahead (5 ms, or 1.5 ms for snare
//!   top), the detector's key filter (kick: 5th-order low-pass at 1 kHz;
//!   snare/toms: plus a 5th-order high-pass at 130 Hz) and the de-bleed
//!   crossover (500 Hz kick/snare bottom, 1 kHz snare top/toms).
//! - **Main gate**: stereo-linked peak detector on the key (instant attack,
//!   10 ms release), opens instantly at threshold (Ghost lowers it 20 dB),
//!   holds 50 ms, then releases along `R + (1 − R)(1 − t/Length)⁵`.
//! - **De-bleed**: a subtractive crossover (`lo = BW5(x)`, `hi = x − lo`, so
//!   equal band gains sum to a pure delay) whose high band is expanded 2:1
//!   below `threshold^a(Debleed)` by a 60 ms peak follower on the gated
//!   signal — which is why it only bites as the gate closes.
//!
//! `no_std` + `alloc`; allocation happens in [`DrumGate::new`] only.

#![no_std]
#![deny(clippy::disallowed_methods)]

extern crate alloc;

pub mod engine;
pub mod filter;

pub use engine::{
    DEBLEED_RELEASE_MS, DETECTOR_RELEASE_MS, DrumGate, GHOST_OFFSET_DB, HOLD_MS, KEY_HIGH_PASS_HZ,
    KEY_LOW_PASS_HZ, Mode, OUTPUT_LAW_SLOPE, Settings, db_to_gain, debleed_exponent, output_gain,
};
