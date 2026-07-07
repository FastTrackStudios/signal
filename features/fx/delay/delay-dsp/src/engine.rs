//! DelayEngine — unified wrapper for all delay styles.
//!
//! Provides a common interface over TapeDelay, CleanDelay, BbdDelay, LoFiDelay,
//! ShimmerDelay, ReverseDelay, and PitchDelay. The chain uses this instead of
//! a concrete delay type, enabling runtime style switching.

use crate::bbd_delay::BbdDelay;
use crate::clean_delay::CleanDelay;
use crate::lofi_delay::LoFiDelay;
use crate::modulation::WobbleShape;
use crate::pitch_delay::PitchDelay;
use crate::reverse_delay::ReverseDelay;
use crate::rhythm_delay::RhythmDelay;
use crate::shimmer_delay::ShimmerDelay;
use crate::tape_delay::{SaturationType, TapeDelay};

/// Available delay styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelayStyle {
    Tape,
    Clean,
    Bbd,
    LoFi,
    Shimmer,
    Reverse,
    Pitch,
    Rhythm,
}

impl DelayStyle {
    pub const COUNT: usize = 8;

    pub fn from_index(i: usize) -> Self {
        match i {
            0 => Self::Tape,
            1 => Self::Clean,
            2 => Self::Bbd,
            3 => Self::LoFi,
            4 => Self::Shimmer,
            5 => Self::Reverse,
            6 => Self::Pitch,
            7 => Self::Rhythm,
            _ => Self::Tape,
        }
    }

    pub fn to_index(self) -> usize {
        match self {
            Self::Tape => 0,
            Self::Clean => 1,
            Self::Bbd => 2,
            Self::LoFi => 3,
            Self::Shimmer => 4,
            Self::Reverse => 5,
            Self::Pitch => 6,
            Self::Rhythm => 7,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Tape => "Tape",
            Self::Clean => "Digital",
            Self::Bbd => "BBD",
            Self::LoFi => "Lo-Fi",
            Self::Shimmer => "Shimmer",
            Self::Reverse => "Reverse",
            Self::Pitch => "Pitch",
            Self::Rhythm => "Rhythm",
        }
    }
}

enum EngineInner {
    Tape(TapeDelay),
    Clean(CleanDelay),
    Bbd(BbdDelay),
    LoFi(LoFiDelay),
    Shimmer(ShimmerDelay),
    Reverse(ReverseDelay),
    Pitch(PitchDelay),
    Rhythm(RhythmDelay),
}

/// Unified delay engine wrapping all delay styles.
///
/// Shared parameters are stored here and synced to the active inner engine
/// on `update()`. Style-specific parameters are set via dedicated methods.
pub struct DelayEngine {
    inner: EngineInner,
    style: DelayStyle,

    // ── Shared parameters (used by all styles) ─────────────────────
    /// Delay time in milliseconds.
    pub time_ms: f64,
    /// Feedback amount (0.0–1.0).
    pub feedback: f64,
    /// High-cut filter frequency in Hz (0 = disabled).
    pub hicut_freq: f64,
    /// Low-cut filter frequency in Hz (0 = disabled).
    pub locut_freq: f64,

    // ── Tape-specific parameters ───────────────────────────────────
    /// Saturation drive (0.0–1.0). Tape only.
    pub drive: f64,
    /// Wow depth (0.0–1.0). Tape only.
    pub wow_depth: f64,
    /// Wow rate in Hz. Tape only.
    pub wow_rate: f64,
    /// Wow drift amount (0.0–1.0). Tape only.
    pub wow_drift: f64,
    /// Flutter depth (0.0–1.0). Tape only.
    pub flutter_depth: f64,
    /// Flutter rate in Hz. Tape only.
    pub flutter_rate: f64,
    /// Saturation type. Tape only.
    pub saturation_type: SaturationType,

    // ── Multi-head (Tape only) ─────────────────────────────────────
    pub head1_enabled: bool,
    pub head2_enabled: bool,
    pub head3_enabled: bool,

    // ── BBD-specific ───────────────────────────────────────────────
    /// LFO modulation depth (0.0–1.0). BBD only.
    pub bbd_mod_depth: f64,
    /// LFO modulation rate in Hz. BBD only.
    pub bbd_mod_rate: f64,
    /// Tone / low-pass cutoff. BBD only.
    pub bbd_tone: f64,
    /// Clock jitter amount (0.0–1.0). BBD only.
    pub bbd_clock_jitter: f64,

    // ── LoFi-specific ──────────────────────────────────────────────
    /// Bit depth for quantization (4–32). LoFi only.
    pub lofi_bit_depth: f64,
    /// Sample rate divisor (1–64). LoFi only.
    pub lofi_sr_div: f64,
    /// Noise floor injection (0.0–1.0). LoFi only.
    pub lofi_noise: f64,

    // ── Shimmer-specific ───────────────────────────────────────────
    /// Pitch ratio (0.5–4.0). Shimmer only.
    pub shimmer_pitch: f64,
    /// Shimmer mix (0.0–1.0). Shimmer only.
    pub shimmer_mix: f64,

    // ── Reverse-specific ───────────────────────────────────────────
    /// Crossfade overlap (0.0–0.5). Reverse only.
    pub reverse_crossfade: f64,

    // ── Pitch-specific ─────────────────────────────────────────────
    /// Playback speed ratio. Pitch only.
    pub pitch_speed: f64,

    // ── Rhythm-specific ──────────────────────────────────────────
    /// Tap levels for rhythm mode (8 taps at 1x–8x base time).
    pub rhythm_taps: [f64; 8],

    // ── Shared new parameters ────────────────────────────────────
    /// Decay EQ tilt (-1.0 = darken repeats, 0 = neutral, +1.0 = brighten).
    pub decay_tilt: f64,
    /// Wobble LFO shape. Tape only.
    pub wow_shape: WobbleShape,
    /// Wobble phase offset (0.0–1.0). Tape only.
    pub wow_phase_offset: f64,
}

impl DelayEngine {
    pub fn new() -> Self {
        Self {
            inner: EngineInner::Tape(TapeDelay::new()),
            style: DelayStyle::Tape,
            time_ms: 250.0,
            feedback: 0.4,
            hicut_freq: 8000.0,
            locut_freq: 0.0,
            drive: 0.0,
            wow_depth: 0.0,
            wow_rate: 0.5,
            wow_drift: 0.3,
            flutter_depth: 0.0,
            flutter_rate: 6.0,
            saturation_type: SaturationType::Tape,
            head1_enabled: true,
            head2_enabled: false,
            head3_enabled: false,
            bbd_mod_depth: 0.3,
            bbd_mod_rate: 1.0,
            bbd_tone: 4000.0,
            bbd_clock_jitter: 0.3,
            lofi_bit_depth: 12.0,
            lofi_sr_div: 4.0,
            lofi_noise: 0.0,
            shimmer_pitch: 2.0,
            shimmer_mix: 0.5,
            reverse_crossfade: 0.1,
            pitch_speed: 1.0,
            rhythm_taps: [1.0, 0.7, 0.5, 0.35, 0.25, 0.18, 0.12, 0.08],
            decay_tilt: 0.0,
            wow_shape: WobbleShape::Sine,
            wow_phase_offset: 0.0,
        }
    }

    pub fn style(&self) -> DelayStyle {
        self.style
    }

    /// Switch to a new delay style. Resets internal state.
    pub fn set_style(&mut self, style: DelayStyle) {
        if self.style == style {
            return;
        }
        self.style = style;
        self.inner = match style {
            DelayStyle::Tape => EngineInner::Tape(TapeDelay::new()),
            DelayStyle::Clean => EngineInner::Clean(CleanDelay::new()),
            DelayStyle::Bbd => EngineInner::Bbd(BbdDelay::new()),
            DelayStyle::LoFi => EngineInner::LoFi(LoFiDelay::new()),
            DelayStyle::Shimmer => EngineInner::Shimmer(ShimmerDelay::new()),
            DelayStyle::Reverse => EngineInner::Reverse(ReverseDelay::new()),
            DelayStyle::Pitch => EngineInner::Pitch(PitchDelay::new()),
            DelayStyle::Rhythm => EngineInner::Rhythm(RhythmDelay::new()),
        };
    }

    /// Sync parameters to the active engine and update coefficients.
    pub fn update(&mut self, sample_rate: f64) {
        match &mut self.inner {
            EngineInner::Tape(d) => {
                d.time_ms = self.time_ms;
                d.feedback = self.feedback;
                d.hicut_freq = self.hicut_freq;
                d.locut_freq = self.locut_freq;
                d.drive = self.drive;
                d.wow_depth = self.wow_depth;
                d.wow_rate = self.wow_rate;
                d.wow_drift = self.wow_drift;
                d.flutter_depth = self.flutter_depth;
                d.flutter_rate = self.flutter_rate;
                d.head1_enabled = self.head1_enabled;
                d.head2_enabled = self.head2_enabled;
                d.head3_enabled = self.head3_enabled;
                d.saturation_type = self.saturation_type;
                d.decay_tilt = self.decay_tilt;
                d.wow_shape = self.wow_shape;
                d.wow_phase_offset = self.wow_phase_offset;
                d.update(sample_rate);
            }
            EngineInner::Clean(d) => {
                d.time_ms = self.time_ms;
                d.feedback = self.feedback;
                d.hicut_freq = self.hicut_freq;
                d.locut_freq = self.locut_freq;
                d.decay_tilt = self.decay_tilt;
                d.update(sample_rate);
            }
            EngineInner::Bbd(d) => {
                d.time_ms = self.time_ms;
                d.feedback = self.feedback;
                d.mod_depth = self.bbd_mod_depth;
                d.mod_rate = self.bbd_mod_rate;
                d.tone = self.bbd_tone;
                d.clock_jitter = self.bbd_clock_jitter;
                d.decay_tilt = self.decay_tilt;
                d.update(sample_rate);
            }
            EngineInner::LoFi(d) => {
                d.time_ms = self.time_ms;
                d.feedback = self.feedback;
                d.hicut_freq = self.hicut_freq;
                d.locut_freq = self.locut_freq;
                d.bit_depth = self.lofi_bit_depth;
                d.sample_rate_div = self.lofi_sr_div;
                d.noise = self.lofi_noise;
                d.decay_tilt = self.decay_tilt;
                d.update(sample_rate);
            }
            EngineInner::Shimmer(d) => {
                d.time_ms = self.time_ms;
                d.feedback = self.feedback;
                d.hicut_freq = self.hicut_freq;
                d.pitch_ratio = self.shimmer_pitch;
                d.shimmer_mix = self.shimmer_mix;
                d.decay_tilt = self.decay_tilt;
                d.update(sample_rate);
            }
            EngineInner::Reverse(d) => {
                d.time_ms = self.time_ms;
                d.feedback = self.feedback;
                d.hicut_freq = self.hicut_freq;
                d.grain_crossfade = self.reverse_crossfade;
                d.decay_tilt = self.decay_tilt;
                d.update(sample_rate);
            }
            EngineInner::Pitch(d) => {
                d.time_ms = self.time_ms;
                d.feedback = self.feedback;
                d.speed = self.pitch_speed;
                d.decay_tilt = self.decay_tilt;
                d.update(sample_rate);
            }
            EngineInner::Rhythm(d) => {
                d.time_ms = self.time_ms;
                d.feedback = self.feedback;
                d.hicut_freq = self.hicut_freq;
                d.locut_freq = self.locut_freq;
                d.tap_levels = self.rhythm_taps;
                d.decay_tilt = self.decay_tilt;
                d.update(sample_rate);
            }
        }
    }

    /// Process one sample.
    pub fn tick(&mut self, input: f64, ch: usize) -> f64 {
        match &mut self.inner {
            EngineInner::Tape(d) => d.tick(input, ch),
            EngineInner::Clean(d) => d.tick(input, ch),
            EngineInner::Bbd(d) => d.tick(input, ch),
            EngineInner::LoFi(d) => d.tick(input, ch),
            EngineInner::Shimmer(d) => d.tick(input, ch),
            EngineInner::Reverse(d) => d.tick(input, ch),
            EngineInner::Pitch(d) => d.tick(input),
            EngineInner::Rhythm(d) => d.tick(input, ch),
        }
    }

    /// Get the last feedback sample for ping-pong cross-feeding.
    pub fn last_feedback(&self) -> f64 {
        match &self.inner {
            EngineInner::Tape(d) => d.last_feedback(),
            EngineInner::Clean(d) => d.last_feedback(),
            EngineInner::Bbd(d) => d.last_feedback(),
            EngineInner::LoFi(d) => d.last_feedback(),
            EngineInner::Shimmer(d) => d.last_feedback(),
            EngineInner::Reverse(d) => d.last_feedback(),
            EngineInner::Pitch(d) => d.last_feedback(),
            EngineInner::Rhythm(d) => d.last_feedback(),
        }
    }

    pub fn reset(&mut self) {
        match &mut self.inner {
            EngineInner::Tape(d) => d.reset(),
            EngineInner::Clean(d) => d.reset(),
            EngineInner::Bbd(d) => d.reset(),
            EngineInner::LoFi(d) => d.reset(),
            EngineInner::Shimmer(d) => d.reset(),
            EngineInner::Reverse(d) => d.reset(),
            EngineInner::Pitch(d) => d.reset(),
            EngineInner::Rhythm(d) => d.reset(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    #[test]
    fn all_styles_produce_delayed_output() {
        for i in 0..DelayStyle::COUNT {
            let style = DelayStyle::from_index(i);
            let mut e = DelayEngine::new();
            e.set_style(style);
            e.time_ms = 100.0;
            e.feedback = 0.0;
            e.update(SR);

            let mut has_output = false;
            for s in 0..48000 {
                let input = if s < 100 { 0.8 } else { 0.0 };
                let out = e.tick(input, 0);
                if out.abs() > 0.01 {
                    has_output = true;
                }
            }

            assert!(has_output, "{:?} style should produce output", style);
        }
    }

    #[test]
    fn all_styles_no_nan() {
        for i in 0..DelayStyle::COUNT {
            let style = DelayStyle::from_index(i);
            let mut e = DelayEngine::new();
            e.set_style(style);
            e.time_ms = 200.0;
            e.feedback = 0.6;
            e.update(SR);

            for s in 0..96000 {
                let input = (std::f64::consts::TAU * 440.0 * s as f64 / SR).sin() * 0.5;
                let out = e.tick(input, 0);
                assert!(
                    out.is_finite(),
                    "{:?} produced NaN/Inf at sample {s}",
                    style
                );
            }
        }
    }

    #[test]
    fn style_switch_resets() {
        let mut e = DelayEngine::new();
        e.time_ms = 100.0;
        e.feedback = 0.5;
        e.update(SR);

        // Feed some signal in Tape mode
        for s in 0..4800 {
            let input = if s < 100 { 1.0 } else { 0.0 };
            e.tick(input, 0);
        }

        // Switch to Clean — should reset state
        e.set_style(DelayStyle::Clean);
        e.update(SR);

        // First sample should be near zero (no residual from tape engine)
        let out = e.tick(0.0, 0);
        assert!(out.abs() < 0.01, "Style switch should reset: got {out}");
    }

    #[test]
    fn style_roundtrip() {
        for i in 0..DelayStyle::COUNT {
            let style = DelayStyle::from_index(i);
            assert_eq!(style.to_index(), i);
        }
    }
}
