//! MultiTapDelay — up to 16 user-editable taps.
//!
//! TimeLine MX "MultiTap" machine parity: a configurable tap pattern
//! where each tap has its own position (as a fraction of the base delay
//! time), level, pan, and enable. Built-in presets cover the common
//! rhythmic shapes; `RhythmDelay` remains as the fixed-pattern legacy
//! style.
//!
//! Per-tap `pan` is stored for API parity but not applied in the mono
//! per-channel engine; stereo tap placement lands with the deep pass.

use audiocore_dsp::biquad::{Biquad, FilterType};
use audiocore_dsp::delay_line::DelayLine;
use audiocore_dsp::smoothing::ParamSmoother;

pub const MAX_TAPS: usize = 16;

/// One tap in the pattern.
#[derive(Debug, Clone, Copy)]
pub struct Tap {
    pub enabled: bool,
    /// Position as a fraction of the base delay time (0.0–1.0].
    pub position: f64,
    /// Output level (0.0–1.0).
    pub level: f64,
    /// Stereo pan (-1.0–1.0). Stored for parity; applied in the deep pass.
    pub pan: f64,
}

impl Tap {
    pub const fn off() -> Self {
        Self {
            enabled: false,
            position: 1.0,
            level: 0.0,
            pan: 0.0,
        }
    }

    pub const fn at(position: f64, level: f64) -> Self {
        Self {
            enabled: true,
            position,
            level,
            pan: 0.0,
        }
    }
}

/// Built-in tap-pattern presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TapPreset {
    /// Four even quarters, decaying.
    Quarters,
    /// Dotted-eighth + quarter "U2" figure.
    DottedEighth,
    /// Golden-ratio cascade (Echorec-ish).
    Golden,
    /// Dense 8-tap early-reflection cluster.
    EarlyReflections,
    /// Accelerando: taps bunch up toward the delay time.
    Accelerando,
}

impl TapPreset {
    pub fn taps(self) -> [Tap; MAX_TAPS] {
        let mut taps = [Tap::off(); MAX_TAPS];
        match self {
            TapPreset::Quarters => {
                for (i, t) in [0.25, 0.5, 0.75, 1.0].iter().enumerate() {
                    taps[i] = Tap::at(*t, 1.0 - i as f64 * 0.2);
                }
            }
            TapPreset::DottedEighth => {
                taps[0] = Tap::at(0.375, 0.8);
                taps[1] = Tap::at(0.75, 0.6);
                taps[2] = Tap::at(1.0, 1.0);
            }
            TapPreset::Golden => {
                for (i, t) in [0.146, 0.236, 0.382, 0.618, 1.0].iter().enumerate() {
                    taps[i] = Tap::at(*t, 0.5 + 0.5 * t);
                }
            }
            TapPreset::EarlyReflections => {
                let positions = [0.06, 0.11, 0.17, 0.25, 0.36, 0.5, 0.71, 1.0];
                for (i, t) in positions.iter().enumerate() {
                    taps[i] = Tap::at(*t, 0.9 - i as f64 * 0.09);
                }
            }
            TapPreset::Accelerando => {
                let positions = [0.5, 0.75, 0.875, 0.9375, 0.96875, 1.0];
                for (i, t) in positions.iter().enumerate() {
                    taps[i] = Tap::at(*t, 0.5 + i as f64 * 0.1);
                }
            }
        }
        taps
    }
}

pub struct MultiTapDelay {
    /// Base delay time in ms (clamped to 60–2500).
    pub time_ms: f64,
    /// Feedback amount (0.0–1.0), fed from the tap at the greatest position.
    pub feedback: f64,
    /// The tap pattern.
    pub taps: [Tap; MAX_TAPS],
    /// High-cut in the feedback path (0 = off).
    pub hicut_freq: f64,
    /// Low-cut in the feedback path (0 = off).
    pub locut_freq: f64,
    /// Decay EQ tilt (shared engine param).
    pub decay_tilt: f64,

    delay: DelayLine,
    hicut: Biquad,
    locut: Biquad,
    decay_eq: Biquad,
    feedback_sample: f64,
    sample_rate: f64,
    smoother: ParamSmoother,
}

impl MultiTapDelay {
    pub const MIN_TIME_MS: f64 = 60.0;
    pub const MAX_TIME_MS: f64 = 2500.0;
    const MAX_DELAY_S: f64 = 3.0;

    pub fn new() -> Self {
        Self {
            time_ms: 500.0,
            feedback: 0.3,
            taps: TapPreset::Quarters.taps(),
            hicut_freq: 0.0,
            locut_freq: 0.0,
            decay_tilt: 0.0,
            delay: DelayLine::new(48000 * 3 + 1024),
            hicut: Biquad::new(),
            locut: Biquad::new(),
            decay_eq: Biquad::new(),
            feedback_sample: 0.0,
            sample_rate: 48000.0,
            smoother: ParamSmoother::new(0.0),
        }
    }

    pub fn set_preset(&mut self, preset: TapPreset) {
        self.taps = preset.taps();
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.time_ms = self.time_ms.clamp(Self::MIN_TIME_MS, Self::MAX_TIME_MS);

        let max_len = (sample_rate * Self::MAX_DELAY_S) as usize + 1024;
        if self.delay.len() < max_len {
            self.delay = DelayLine::new(max_len);
        }

        if self.hicut_freq > 0.0 {
            self.hicut
                .set(FilterType::Lowpass, self.hicut_freq, 0.707, sample_rate);
        }
        if self.locut_freq > 0.0 {
            self.locut
                .set(FilterType::Highpass, self.locut_freq, 0.707, sample_rate);
        }
        if self.decay_tilt.abs() > 0.01 {
            if self.decay_tilt < 0.0 {
                let freq = 20000.0 * (1.0 + self.decay_tilt).max(0.05);
                self.decay_eq
                    .set(FilterType::Lowpass, freq, 0.707, sample_rate);
            } else {
                let freq = 20.0 + self.decay_tilt * 2000.0;
                self.decay_eq
                    .set(FilterType::Highpass, freq, 0.707, sample_rate);
            }
        }

        self.smoother.set_time(0.15, sample_rate);
        let target = self.time_ms * 0.001 * sample_rate;
        if self.smoother.value() == 0.0 {
            self.smoother.set_immediate(target);
        }
    }

    pub fn tick(&mut self, input: f64, ch: usize) -> f64 {
        let target_delay = self.time_ms * 0.001 * self.sample_rate;
        self.smoother.set_target(target_delay);
        let smooth_delay = self.smoother.tick();
        let max_read = self.delay.len() as f64 - 4.0;

        let mut output = 0.0;
        let mut fb_pos = 0.0f64;
        for tap in &self.taps {
            if !tap.enabled || tap.level <= 0.0 {
                continue;
            }
            let pos = (smooth_delay * tap.position).clamp(1.0, max_read);
            output += self.delay.read_cubic(pos) * tap.level;
            fb_pos = fb_pos.max(tap.position);
        }

        // Regenerate from the latest active tap so the pattern repeats
        // as a whole (TimeLine behavior).
        let mut fb = if fb_pos > 0.0 {
            self.delay
                .read_cubic((smooth_delay * fb_pos).clamp(1.0, max_read))
                * self.feedback
        } else {
            0.0
        };

        if self.hicut_freq > 0.0 {
            fb = self.hicut.tick(fb, ch);
        }
        if self.locut_freq > 0.0 {
            fb = self.locut.tick(fb, ch);
        }
        if self.decay_tilt.abs() > 0.01 {
            fb = self.decay_eq.tick(fb, ch);
        }
        fb = fb.clamp(-1.5, 1.5);

        self.delay.write(input + fb);
        self.feedback_sample = fb;

        output
    }

    pub fn last_feedback(&self) -> f64 {
        self.feedback_sample
    }

    pub fn reset(&mut self) {
        self.delay.clear();
        self.hicut.reset();
        self.locut.reset();
        self.decay_eq.reset();
        self.feedback_sample = 0.0;
        self.smoother.reset(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    #[test]
    fn taps_land_at_configured_positions() {
        let mut d = MultiTapDelay::new();
        d.time_ms = 800.0;
        d.feedback = 0.0;
        d.taps = [Tap::off(); MAX_TAPS];
        d.taps[0] = Tap::at(0.25, 1.0);
        d.taps[1] = Tap::at(1.0, 1.0);
        d.update(SR);

        let mut hits = Vec::new();
        for i in 0..96000 {
            let input = if i == 0 { 1.0 } else { 0.0 };
            if d.tick(input, 0).abs() > 0.3 {
                hits.push(i as i64);
            }
        }
        let t1 = (200.0 * SR / 1000.0) as i64;
        let t2 = (800.0 * SR / 1000.0) as i64;
        assert!(hits.iter().any(|&h| (h - t1).abs() < 100), "{hits:?}");
        assert!(hits.iter().any(|&h| (h - t2).abs() < 100), "{hits:?}");
    }

    #[test]
    fn disabled_taps_are_silent() {
        let mut d = MultiTapDelay::new();
        d.time_ms = 200.0;
        d.feedback = 0.0;
        d.taps = [Tap::off(); MAX_TAPS];
        d.update(SR);

        for i in 0..24000 {
            let input = if i == 0 { 1.0 } else { 0.0 };
            assert!(d.tick(input, 0).abs() < 1e-12);
        }
    }

    #[test]
    fn presets_produce_output_and_no_nan() {
        for preset in [
            TapPreset::Quarters,
            TapPreset::DottedEighth,
            TapPreset::Golden,
            TapPreset::EarlyReflections,
            TapPreset::Accelerando,
        ] {
            let mut d = MultiTapDelay::new();
            d.time_ms = 400.0;
            d.feedback = 0.5;
            d.set_preset(preset);
            d.update(SR);

            let mut energy = 0.0;
            for i in 0..48000 {
                let input = if i < 50 { 0.8 } else { 0.0 };
                let out = d.tick(input, 0);
                assert!(out.is_finite(), "{preset:?} NaN at {i}");
                energy += out * out;
            }
            assert!(energy > 0.001, "{preset:?} should produce output");
        }
    }
}
