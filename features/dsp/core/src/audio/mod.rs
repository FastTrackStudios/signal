//! FTS DSP — Low-level audio processing primitives.
//!
//! Ported from Airwindows (MIT, Chris Johnson) via airwin2rack.
//! Zero framework dependencies — usable from nih-plug, LV2, JACK,
//! standalone, embedded, or WASM targets.
//!
//! This crate provides the building blocks that plugin-specific DSP
//! crates (`eq-dsp`, `comp-dsp`, etc.) compose into full processors.

pub mod biquad;
pub mod db;
pub mod dc_blocker;
pub mod delay_line;
pub mod denormal;
pub mod dither;
pub mod envelope;
pub mod gain_curve;
pub mod grain_pitch;
pub mod lfo;
pub mod loudness;
pub mod note_sync;
pub mod one_pole;
pub mod oversampling;
pub mod prng;
pub mod slew;
pub mod smoothing;
pub mod soft_clip;
pub mod stereo;
pub mod window;

/// Sample rate and buffer context passed to processors.
#[derive(Debug, Clone, Copy)]
pub struct AudioConfig {
    pub sample_rate: f64,
    pub max_buffer_size: usize,
}

// r[impl dsp.processor.trait]
// r[impl dsp.processor.send]
/// Uniform interface for all DSP processors.
///
/// Implemented by both primitives (individual filters) and composite
/// processors (full EQ chain, compressor engine, etc.).
pub trait Processor: Send {
    /// Reset all internal state.
    fn reset(&mut self);

    /// Recalculate coefficients after parameter changes.
    fn update(&mut self, config: AudioConfig);

    /// Process stereo audio in-place.
    fn process(&mut self, left: &mut [f64], right: &mut [f64]);
}
