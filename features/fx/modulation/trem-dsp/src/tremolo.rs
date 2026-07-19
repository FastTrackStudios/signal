//! Core tremolo engine — amplitude modulation via modulator output.
//!
//! Supports harmonic tremolo (split into low/high bands, modulate
//! out of phase) and standard tremolo.

use audiocore_dsp::biquad::{Biquad, FilterType};

/// Tremolo shape applied to the raw modulator output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TremShape {
    /// Use the modulator output directly (pattern/LFO shaped).
    Pattern,
    /// Sine wave at the modulator rate.
    Sine,
    /// Triangle wave.
    Triangle,
    /// Square wave with adjustable pulse width.
    Square,
}

/// Tremolo mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TremMode {
    /// Standard tremolo — same modulation on both channels.
    Mono,
    /// Stereo tremolo — L/R modulated with phase offset.
    Stereo,
    /// Harmonic tremolo — low and high bands modulated out of phase.
    Harmonic,
}

/// Single-channel tremolo processor.
///
/// Takes a modulation value (0..1) and applies it as amplitude modulation.
pub struct Tremolo {
    /// Modulation depth (0..1). 0 = no effect, 1 = full depth.
    pub depth: f64,
    /// Tremolo mode.
    pub mode: TremMode,
    /// Crossover frequency for harmonic tremolo (Hz).
    pub crossover_freq: f64,

    // Harmonic tremolo filters
    lp: Biquad,
    hp: Biquad,

    sample_rate: f64,
}

impl Tremolo {
    pub fn new() -> Self {
        Self {
            depth: 0.5,
            mode: TremMode::Mono,
            crossover_freq: 800.0,
            lp: Biquad::new(),
            hp: Biquad::new(),
            sample_rate: 48000.0,
        }
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.lp
            .set(FilterType::Lowpass, self.crossover_freq, 0.707, sample_rate);
        self.hp.set(
            FilterType::Highpass,
            self.crossover_freq,
            0.707,
            sample_rate,
        );
    }

    /// Apply tremolo to a sample given the modulation value (0..1).
    ///
    /// `mod_val` is the primary modulation, `mod_val_inv` is the inverted
    /// modulation for harmonic tremolo's complementary band.
    #[inline]
    pub fn tick(&mut self, sample: f64, mod_val: f64, ch: usize) -> f64 {
        // Convert 0..1 modulation to gain: at depth=1, gain ranges 0..1
        // at depth=0, gain is always 1 (no modulation)
        let gain = 1.0 - self.depth * (1.0 - mod_val);
        let gain_inv = 1.0 - self.depth * mod_val;

        match self.mode {
            TremMode::Mono | TremMode::Stereo => sample * gain,
            TremMode::Harmonic => {
                let low = self.lp.tick(sample, ch);
                let high = self.hp.tick(sample, ch);
                low * gain + high * gain_inv
            }
        }
    }

    pub fn reset(&mut self) {
        self.lp.reset();
        self.hp.reset();
    }
}

impl Default for Tremolo {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Analog Style — waveshaper saturation applied after amplitude modulation
// ---------------------------------------------------------------------------

/// Saturation character applied to the tremolo output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(Default)]
pub enum AnalogStyle {
    /// No saturation — clean digital.
    #[default]
    Clean,
    /// Gentle low-frequency warmth.
    Fat,
    /// Compressed, smooth saturation.
    Squash,
    /// Broadband soft saturation.
    Dirt,
    /// Exaggerated high-end clipping with clean lows.
    Crunch,
    /// Asymmetric hard clipping.
    Shred,
    /// Extreme pumping compression with makeup gain.
    Pump,
}


/// Stateful analog-style processor (Crunch needs a crossover filter).
pub struct AnalogProcessor {
    pub style: AnalogStyle,
    lp: Biquad,
    hp: Biquad,
    sample_rate: f64,
}

impl AnalogProcessor {
    pub fn new() -> Self {
        Self {
            style: AnalogStyle::Clean,
            lp: Biquad::new(),
            hp: Biquad::new(),
            sample_rate: 48000.0,
        }
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        // Crunch crossover at 2 kHz
        self.lp.set(FilterType::Lowpass, 2000.0, 0.707, sample_rate);
        self.hp
            .set(FilterType::Highpass, 2000.0, 0.707, sample_rate);
    }

    /// Apply the selected analog style to a single sample.
    #[inline]
    pub fn tick(&mut self, x: f64, ch: usize) -> f64 {
        match self.style {
            AnalogStyle::Clean => x,
            AnalogStyle::Fat => {
                // Gentle: tanh(x*1.5) / tanh(1.5)
                let drive = 1.5;
                (x * drive).tanh() / drive.tanh()
            }
            AnalogStyle::Squash => {
                let drive = 2.5;
                (x * drive).tanh() / drive.tanh()
            }
            AnalogStyle::Dirt => {
                let drive = 4.0;
                (x * drive).tanh() / drive.tanh()
            }
            AnalogStyle::Crunch => {
                // Saturate highs, keep lows clean
                let low = self.lp.tick(x, ch);
                let high = self.hp.tick(x, ch);
                let drive = 4.0;
                let sat_high = (high * drive).tanh() / drive.tanh();
                low + sat_high
            }
            AnalogStyle::Shred => {
                // Asymmetric: positive clips harder than negative
                if x >= 0.0 {
                    (x * 6.0).tanh() / 6.0_f64.tanh()
                } else {
                    (x * 3.0).tanh() / 3.0_f64.tanh()
                }
            }
            AnalogStyle::Pump => {
                // Heavy compression with makeup gain
                (x * 8.0).tanh() * 1.5 / 8.0_f64.tanh()
            }
        }
    }

    pub fn reset(&mut self) {
        self.lp.reset();
        self.hp.reset();
    }
}

impl Default for AnalogProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    const SR: f64 = 48000.0;

    #[test]
    fn no_depth_passes_through() {
        let mut t = Tremolo::new();
        t.depth = 0.0;
        t.update(SR);

        let input = 0.75;
        let out = t.tick(input, 0.5, 0);
        assert!(
            (out - input).abs() < 1e-10,
            "Zero depth should pass through: {out}"
        );
    }

    #[test]
    fn full_depth_at_zero_mod_silences() {
        let mut t = Tremolo::new();
        t.depth = 1.0;
        t.update(SR);

        let out = t.tick(1.0, 0.0, 0);
        assert!(
            out.abs() < 1e-10,
            "Full depth at mod=0 should silence: {out}"
        );
    }

    #[test]
    fn full_depth_at_one_mod_passes() {
        let mut t = Tremolo::new();
        t.depth = 1.0;
        t.update(SR);

        let out = t.tick(1.0, 1.0, 0);
        assert!(
            (out - 1.0).abs() < 1e-10,
            "Full depth at mod=1 should pass: {out}"
        );
    }

    #[test]
    fn harmonic_tremolo_produces_output() {
        let mut t = Tremolo::new();
        t.depth = 1.0;
        t.mode = TremMode::Harmonic;
        t.crossover_freq = 800.0;
        t.update(SR);

        let mut has_output = false;
        for i in 0..4800 {
            let s = (2.0 * PI * 440.0 * i as f64 / SR).sin();
            let mod_val = (2.0 * PI * 5.0 * i as f64 / SR).sin() * 0.5 + 0.5;
            let out = t.tick(s, mod_val, 0);
            if out.abs() > 0.01 {
                has_output = true;
            }
            assert!(out.is_finite(), "NaN at sample {i}");
        }
        assert!(has_output, "Harmonic tremolo should produce output");
    }

    #[test]
    fn modulation_range() {
        let mut t = Tremolo::new();
        t.depth = 0.5;
        t.update(SR);

        // At mod=0.5, gain = 1 - 0.5 * (1 - 0.5) = 0.75
        let out = t.tick(1.0, 0.5, 0);
        assert!((out - 0.75).abs() < 1e-10, "Half depth at mid mod: {out}");
    }

    // --- AnalogProcessor tests ---

    #[test]
    fn analog_clean_passthrough() {
        let mut ap = AnalogProcessor::new();
        ap.style = AnalogStyle::Clean;
        ap.update(SR);

        let out = ap.tick(0.5, 0);
        assert!(
            (out - 0.5).abs() < 1e-10,
            "Clean should pass through: {out}"
        );
    }

    #[test]
    fn analog_fat_saturates() {
        let mut ap = AnalogProcessor::new();
        ap.style = AnalogStyle::Fat;
        ap.update(SR);

        // Use a value above 1.0 where saturation compression is clear
        let out = ap.tick(2.0, 0);
        assert!(out < 2.0, "Fat should compress peaks: {out}");
        assert!(out > 1.0, "Fat shouldn't crush to zero: {out}");
    }

    #[test]
    fn analog_shred_asymmetric() {
        let mut ap = AnalogProcessor::new();
        ap.style = AnalogStyle::Shred;
        ap.update(SR);

        // Asymmetric clipping: positive drive=6, negative drive=3.
        // At high input levels, higher drive compresses more (output closer to 1).
        // So for the same |input|, positive output should differ from negative output.
        let pos = ap.tick(0.8, 0);
        let neg = ap.tick(-0.8, 0);
        // They should simply be different in magnitude (asymmetric)
        assert!(
            (pos.abs() - neg.abs()).abs() > 0.001,
            "Shred should be asymmetric: pos={pos}, neg={neg}"
        );
        // And both should be finite
        assert!(pos.is_finite() && neg.is_finite());
    }

    #[test]
    fn analog_styles_no_nan() {
        let styles = [
            AnalogStyle::Clean,
            AnalogStyle::Fat,
            AnalogStyle::Squash,
            AnalogStyle::Dirt,
            AnalogStyle::Crunch,
            AnalogStyle::Shred,
            AnalogStyle::Pump,
        ];

        for style in &styles {
            let mut ap = AnalogProcessor::new();
            ap.style = *style;
            ap.update(SR);

            for &x in &[0.0, 0.5, 1.0, -0.5, -1.0, 2.0, -2.0] {
                let out = ap.tick(x, 0);
                assert!(out.is_finite(), "NaN for {:?} at {x}: {out}", style);
            }
        }
    }

    #[test]
    fn analog_pump_has_makeup_gain() {
        let mut ap = AnalogProcessor::new();
        ap.style = AnalogStyle::Pump;
        ap.update(SR);

        // Small signal should be boosted
        let out = ap.tick(0.3, 0);
        assert!(
            out > 0.3,
            "Pump should boost small signals: in=0.3, out={out}"
        );
    }
}
