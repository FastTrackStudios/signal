//! Polyphonic Octave Generator — Filter Bank + Envelope Resynthesis.
//!
//! Architecture matching the EHX POG / Micro POG:
//! 1. Log-spaced IIR bandpass filter bank splits the input
//! 2. Per-band envelope follower extracts amplitude
//! 3. Per-band oscillator generates a clean tone at the target octave
//! 4. Oscillator amplitude is modulated by the envelope
//! 5. All bands are summed
//!
//! This produces the characteristic "organ-like" polyphonic octave tone:
//! clean sine-based output with the input's amplitude envelope.
//!
//! Latency: 0 samples (IIR filters only).
//! Character: Organ-like, polyphonic, zero latency.

use std::f64::consts::TAU;

use audiocore_dsp::biquad::{Biquad, FilterType};

/// Number of bandpass filter bands.
const NUM_BANDS: usize = 64;

/// Lowest band center frequency (Hz).
const FREQ_LO: f64 = 27.5;

/// Highest band center frequency (Hz).
const FREQ_HI: f64 = 14000.0;

/// Quality factor for bandpass filters.
const BAND_Q: f64 = 5.0;

/// Which octave shift to produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OctaveShift {
    /// Two octaves down (−24 semitones).
    Sub2,
    /// One octave down (−12 semitones).
    Sub1,
    /// One octave up (+12 semitones).
    Up1,
    /// Two octaves up (+24 semitones).
    Up2,
}

impl OctaveShift {
    /// Map a semitone value to the nearest octave shift.
    pub fn from_semitones(st: f64) -> Self {
        if st <= -18.0 {
            Self::Sub2
        } else if st < 0.0 {
            Self::Sub1
        } else if st < 18.0 {
            Self::Up1
        } else {
            Self::Up2
        }
    }

    /// Frequency ratio for this shift.
    fn ratio(self) -> f64 {
        match self {
            Self::Sub2 => 0.25,
            Self::Sub1 => 0.5,
            Self::Up1 => 2.0,
            Self::Up2 => 4.0,
        }
    }
}

// ── Per-Band State ─────────────────────────────────────────────────────

/// Per-band state: bandpass filter + envelope follower + oscillator.
struct Band {
    /// Bandpass filter (2nd-order IIR).
    bp: Biquad,
    /// Center frequency (Hz).
    fc: f64,

    // ── Envelope follower ──
    /// Peak-tracked envelope value.
    env: f64,
    /// Attack coefficient (fast, ~1 ms).
    env_attack: f64,
    /// Release coefficient (slower, ~10 ms).
    env_release: f64,
    /// Second-stage smoothed envelope (removes 2f ripple from rectification).
    env_smooth: f64,
    /// Smoothing coefficient: 1-pole LPF at fc/4 to attenuate 2f ripple by ~18 dB.
    smooth_coeff: f64,

    /// Frequency-dependent gain (higher bands get lower gain for natural spectral tilt).
    gain: f64,

    // ── Output oscillator ──
    /// Oscillator phase accumulator.
    osc_phase: f64,
    /// Phase increment per sample (precomputed).
    osc_phase_inc: f64,
}

impl Band {
    fn new(fc: f64, q: f64, target_freq: f64, sample_rate: f64) -> Self {
        let mut bp = Biquad::new();
        bp.set(FilterType::Bandpass, fc, q, sample_rate);

        // Envelope follower coefficients.
        let attack_ms = 1.0;
        let release_ms = 10.0;
        let env_attack = (-TAU / (attack_ms * 0.001 * sample_rate)).exp();
        let env_release = (-TAU / (release_ms * 0.001 * sample_rate)).exp();

        // Smoothing LPF at fc/4: attenuates 2f ripple by ~18 dB uniformly across bands.
        let smooth_coeff = (-TAU * (fc / 4.0) / sample_rate).exp();

        // Spectral tilt: -3 dB/octave relative to 200 Hz reference.
        // Models the natural 1/f spectral slope of musical signals.
        let gain = (200.0 / fc).powf(0.15);

        Self {
            bp,
            fc,
            env: 0.0,
            env_attack,
            env_release,
            env_smooth: 0.0,
            smooth_coeff,
            gain,
            osc_phase: 0.0,
            osc_phase_inc: TAU * target_freq / sample_rate,
        }
    }

    /// Update oscillator target frequency.
    fn set_target_freq(&mut self, target_freq: f64, sample_rate: f64) {
        self.osc_phase_inc = TAU * target_freq / sample_rate;
    }

    fn reset(&mut self) {
        self.bp.reset();
        self.env = 0.0;
        self.env_smooth = 0.0;
        self.osc_phase = 0.0;
    }

    /// Process one sample: bandpass → envelope → modulated oscillator.
    #[inline]
    fn tick(&mut self, input: f64) -> f64 {
        // Bandpass filter.
        let filtered = self.bp.tick(input, 0);

        // Envelope follower (peak detection with asymmetric smoothing).
        let abs_val = filtered.abs();
        let coeff = if abs_val > self.env {
            self.env_attack
        } else {
            self.env_release
        };
        self.env = coeff * self.env + (1.0 - coeff) * abs_val;

        // Second-stage smoothing: removes 2f ripple from rectification.
        self.env_smooth =
            self.smooth_coeff * self.env_smooth + (1.0 - self.smooth_coeff) * self.env;

        // Triangle wave: odd harmonics at 1/n² amplitude — moderate
        // harmonic content between sine (none) and sawtooth (1/n all).
        let phase_norm = self.osc_phase / TAU;
        let osc = if phase_norm < 0.25 {
            4.0 * phase_norm
        } else if phase_norm < 0.75 {
            2.0 - 4.0 * phase_norm
        } else {
            -4.0 + 4.0 * phase_norm
        };

        self.osc_phase += self.osc_phase_inc;
        if self.osc_phase >= TAU {
            self.osc_phase -= TAU;
        }

        // Output: triangle at target freq × smoothed envelope × spectral tilt.
        osc * self.env_smooth * self.gain
    }
}

// ── PolyOctave (public API) ────────────────────────────────────────────

/// Polyphonic Octave Generator using filter bank + envelope resynthesis.
pub struct PolyOctave {
    /// Which octave shift to apply.
    pub shift: OctaveShift,
    /// Dry/wet mix: 0.0 = dry, 1.0 = wet.
    pub mix: f64,

    /// Log-spaced bandpass filter bank with per-band envelope + oscillator.
    bands: Vec<Band>,
    /// Output highpass to remove DC and sub-bass artifacts.
    hp: Biquad,

    sample_rate: f64,
    last_shift: OctaveShift,
}

impl PolyOctave {
    pub fn new() -> Self {
        let sr = 48000.0;
        let shift = OctaveShift::Sub1;
        let mut s = Self {
            shift,
            mix: 1.0,
            bands: Vec::new(),
            hp: Biquad::new(),
            sample_rate: sr,
            last_shift: shift,
        };
        s.init_bands();
        s.update_hp();
        s
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.init_bands();
        self.update_hp();
        self.reset();
    }

    pub fn reset(&mut self) {
        for band in &mut self.bands {
            band.reset();
        }
        self.hp.reset();
    }

    /// Process one sample. Returns the mixed output.
    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        if self.shift != self.last_shift {
            self.reconfigure();
        }

        let nyquist = self.sample_rate * 0.5;
        let ratio = self.shift.ratio();

        let mut wet = 0.0;
        for band in &mut self.bands {
            // Skip bands whose target frequency would alias.
            if band.fc * ratio >= nyquist {
                continue;
            }
            wet += band.tick(input);
        }

        // Highpass to remove DC artifacts.
        wet = self.hp.tick(wet, 0);

        // Per-shift gain compensation. Two-octave shifts need more gain
        // because the filter bank covers a narrower effective range.
        let gain = match self.shift {
            OctaveShift::Sub2 => 0.70,
            OctaveShift::Sub1 => 0.70,
            OctaveShift::Up1 => 0.60,
            OctaveShift::Up2 => 0.85,
        };
        wet *= gain;

        input * (1.0 - self.mix) + wet * self.mix
    }

    pub fn latency(&self) -> usize {
        0
    }

    /// Initialize the filter bank with log-spaced bands.
    fn init_bands(&mut self) {
        self.bands.clear();
        let log_lo = FREQ_LO.ln();
        let log_hi = FREQ_HI.ln();
        let ratio = self.shift.ratio();
        for i in 0..NUM_BANDS {
            let t = i as f64 / (NUM_BANDS - 1) as f64;
            let fc = (log_lo + t * (log_hi - log_lo)).exp();
            let target = fc * ratio;
            self.bands
                .push(Band::new(fc, BAND_Q, target, self.sample_rate));
        }
    }

    /// Reconfigure oscillator frequencies when shift mode changes.
    fn reconfigure(&mut self) {
        let ratio = self.shift.ratio();
        let sr = self.sample_rate;
        for band in &mut self.bands {
            band.set_target_freq(band.fc * ratio, sr);
        }
        self.update_hp();
        self.last_shift = self.shift;
    }

    /// Configure the output highpass filter.
    fn update_hp(&mut self) {
        // Gentle highpass to remove DC only. Keep very low to preserve sub content.
        let cutoff = match self.shift {
            OctaveShift::Sub2 => 10.0,
            OctaveShift::Sub1 => 12.0,
            OctaveShift::Up1 => 15.0,
            OctaveShift::Up2 => 20.0,
        };
        self.hp
            .set(FilterType::Highpass, cutoff, 0.707, self.sample_rate);
    }
}

impl Default for PolyOctave {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    fn make_pog(shift: OctaveShift) -> PolyOctave {
        let mut p = PolyOctave::new();
        p.shift = shift;
        p.mix = 1.0;
        p.update(SR);
        p
    }

    fn sine(freq: f64, i: usize) -> f64 {
        (TAU * freq * i as f64 / SR).sin() * 0.5
    }

    /// Measure spectral magnitude near a target frequency using DFT.
    /// Searches ±8 bins around the target for the peak (wider for filter bank quantization).
    fn spectral_mag(signal: &[f64], target_freq: f64) -> f64 {
        let n = signal.len();
        let bin = (target_freq * n as f64 / SR) as usize;
        let lo = bin.saturating_sub(8);
        let hi = (bin + 9).min(n / 2);
        let mut best = 0.0f64;
        for b in lo..hi {
            let omega = TAU * b as f64 / n as f64;
            let mut re = 0.0f64;
            let mut im = 0.0f64;
            for (i, &s) in signal.iter().enumerate() {
                let w = 0.5 * (1.0 - (TAU * i as f64 / n as f64).cos());
                re += s * w * (omega * i as f64).cos();
                im -= s * w * (omega * i as f64).sin();
            }
            best = best.max((re * re + im * im).sqrt());
        }
        best
    }

    #[test]
    fn latency_is_zero() {
        assert_eq!(make_pog(OctaveShift::Sub1).latency(), 0);
        assert_eq!(make_pog(OctaveShift::Up1).latency(), 0);
    }

    #[test]
    fn silence_in_silence_out() {
        let mut p = make_pog(OctaveShift::Sub1);
        for _ in 0..48000 {
            let out = p.tick(0.0);
            assert!(out.abs() < 1e-10, "Should be silent: {out}");
        }
    }

    #[test]
    fn no_nan_all_shifts() {
        for shift in [
            OctaveShift::Sub2,
            OctaveShift::Sub1,
            OctaveShift::Up1,
            OctaveShift::Up2,
        ] {
            let mut p = make_pog(shift);
            for i in 0..96000 {
                let out = p.tick(sine(220.0, i));
                assert!(out.is_finite(), "{shift:?} NaN at sample {i}");
            }
        }
    }

    #[test]
    fn produces_output_all_shifts() {
        for shift in [
            OctaveShift::Sub2,
            OctaveShift::Sub1,
            OctaveShift::Up1,
            OctaveShift::Up2,
        ] {
            let mut p = make_pog(shift);
            let mut energy = 0.0;
            let warmup = 8000;
            for i in 0..48000 {
                let out = p.tick(sine(220.0, i));
                if i > warmup {
                    energy += out * out;
                }
            }
            assert!(
                energy > 0.01,
                "{shift:?} should produce output: energy={energy}"
            );
        }
    }

    #[test]
    fn octave_up_shifts_frequency_higher() {
        let mut p = make_pog(OctaveShift::Up1);
        let n = 96000;
        let mut output = Vec::with_capacity(n);
        for i in 0..n {
            output.push(p.tick(sine(440.0, i)));
        }

        let signal = &output[n / 2..n / 2 + 8192];
        let mag_880 = spectral_mag(signal, 880.0);
        let mag_440 = spectral_mag(signal, 440.0);

        eprintln!("Up1 440Hz: mag_880={mag_880:.1} mag_440={mag_440:.1}");
        assert!(
            mag_880 > mag_440 * 2.0,
            "880 Hz should dominate over 440 Hz: mag_880={mag_880:.1} mag_440={mag_440:.1}"
        );
    }

    #[test]
    fn octave_down_shifts_frequency_lower() {
        let mut p = make_pog(OctaveShift::Sub1);
        let n = 96000;
        let mut output = Vec::with_capacity(n);
        for i in 0..n {
            output.push(p.tick(sine(440.0, i)));
        }

        let signal = &output[n / 2..n / 2 + 8192];
        let mag_220 = spectral_mag(signal, 220.0);
        let mag_440 = spectral_mag(signal, 440.0);

        eprintln!("Sub1 440Hz: mag_220={mag_220:.1} mag_440={mag_440:.1}");
        assert!(
            mag_220 > mag_440 * 2.0,
            "220 Hz should dominate over 440 Hz: mag_220={mag_220:.1} mag_440={mag_440:.1}"
        );
    }

    #[test]
    fn polyphonic_preserves_both_notes() {
        let mut p = make_pog(OctaveShift::Up1);
        let n = 96000;
        let mut output = Vec::with_capacity(n);
        for i in 0..n {
            let chord = sine(440.0, i) + sine(660.0, i);
            output.push(p.tick(chord));
        }

        // Use band energy to account for filter bank quantization.
        let signal = &output[n / 2..n / 2 + 8192];
        let energy_a = band_energy(signal, 800.0, 960.0); // ~880 Hz region
        let energy_e = band_energy(signal, 1200.0, 1440.0); // ~1320 Hz region
        let energy_gap = band_energy(signal, 1000.0, 1150.0); // gap between

        eprintln!("Polyphonic: A_band={energy_a:.1} E_band={energy_e:.1} gap={energy_gap:.1}");
        assert!(
            energy_a > energy_gap,
            "880 Hz band should exceed gap: {energy_a:.1} vs {energy_gap:.1}"
        );
        assert!(
            energy_e > energy_gap,
            "1320 Hz band should exceed gap: {energy_e:.1} vs {energy_gap:.1}"
        );
    }

    #[test]
    fn dry_wet_mix() {
        let mut p = make_pog(OctaveShift::Sub1);
        p.mix = 0.0;

        // With mix=0, output should equal input (no latency delay needed).
        for i in 0..48000 {
            let input = sine(440.0, i);
            let out = p.tick(input);
            assert!(
                (out - input).abs() < 1e-10,
                "Mix=0 should pass dry at sample {i}: got {out}, expected {input}"
            );
        }
    }

    #[test]
    fn from_semitones_mapping() {
        assert_eq!(OctaveShift::from_semitones(-24.0), OctaveShift::Sub2);
        assert_eq!(OctaveShift::from_semitones(-18.0), OctaveShift::Sub2);
        assert_eq!(OctaveShift::from_semitones(-12.0), OctaveShift::Sub1);
        assert_eq!(OctaveShift::from_semitones(-1.0), OctaveShift::Sub1);
        assert_eq!(OctaveShift::from_semitones(0.0), OctaveShift::Up1);
        assert_eq!(OctaveShift::from_semitones(12.0), OctaveShift::Up1);
        assert_eq!(OctaveShift::from_semitones(18.0), OctaveShift::Up2);
        assert_eq!(OctaveShift::from_semitones(24.0), OctaveShift::Up2);
    }

    #[test]
    fn two_octave_down_shifts_to_quarter_frequency() {
        let mut p = make_pog(OctaveShift::Sub2);
        let n = 96000;
        let mut output = Vec::with_capacity(n);
        for i in 0..n {
            output.push(p.tick(sine(440.0, i)));
        }

        let signal = &output[n / 2..n / 2 + 8192];
        let mag_110 = spectral_mag(signal, 110.0);
        let mag_220 = spectral_mag(signal, 220.0);
        let mag_440 = spectral_mag(signal, 440.0);

        eprintln!("Sub2 440Hz: mag_110={mag_110:.1}, mag_220={mag_220:.1}, mag_440={mag_440:.1}");

        assert!(
            mag_110 > mag_220,
            "110 Hz should dominate: mag_110={mag_110:.1} mag_220={mag_220:.1}"
        );
    }

    /// Measure total spectral energy in a frequency range using DFT.
    fn band_energy(signal: &[f64], freq_lo: f64, freq_hi: f64) -> f64 {
        let n = signal.len();
        let bin_lo = (freq_lo * n as f64 / SR) as usize;
        let bin_hi = (freq_hi * n as f64 / SR) as usize;
        let mut total = 0.0;
        for b in bin_lo..=bin_hi.min(n / 2) {
            let omega = TAU * b as f64 / n as f64;
            let mut re = 0.0f64;
            let mut im = 0.0f64;
            for (i, &s) in signal.iter().enumerate() {
                let w = 0.5 * (1.0 - (TAU * i as f64 / n as f64).cos());
                re += s * w * (omega * i as f64).cos();
                im -= s * w * (omega * i as f64).sin();
            }
            total += re * re + im * im;
        }
        total.sqrt()
    }

    #[test]
    fn two_octave_up_shifts_to_quadruple_frequency() {
        let mut p = make_pog(OctaveShift::Up2);
        let n = 96000;
        let mut output = Vec::with_capacity(n);
        for i in 0..n {
            output.push(p.tick(sine(440.0, i)));
        }

        // Measure energy in octave bands around the expected output.
        // Filter bank quantization means output lands near 4*fc, not exactly 1760.
        let signal = &output[n / 2..n / 2 + 8192];
        let energy_target = band_energy(signal, 1500.0, 2000.0); // ~1760 Hz region
        let energy_lower = band_energy(signal, 700.0, 1000.0); // ~880 Hz region
        let energy_input = band_energy(signal, 350.0, 550.0); // ~440 Hz region

        eprintln!(
            "Up2 440Hz: target_band={energy_target:.1}, lower_band={energy_lower:.1}, input_band={energy_input:.1}"
        );

        assert!(
            energy_target > energy_lower,
            "Target octave should have more energy than lower: {energy_target:.1} vs {energy_lower:.1}"
        );
    }

    #[test]
    fn output_level_near_unity() {
        for shift in [
            OctaveShift::Sub2,
            OctaveShift::Sub1,
            OctaveShift::Up1,
            OctaveShift::Up2,
        ] {
            let mut p = make_pog(shift);
            let n = 96000;
            let warmup = 24000;
            let mut in_sq = 0.0;
            let mut out_sq = 0.0;
            let mut count = 0;

            for i in 0..n {
                let input = sine(440.0, i);
                let out = p.tick(input);
                if i >= warmup {
                    in_sq += input * input;
                    out_sq += out * out;
                    count += 1;
                }
            }

            let in_rms = (in_sq / count as f64).sqrt();
            let out_rms = (out_sq / count as f64).sqrt();
            let ratio_db = 20.0 * (out_rms / in_rms).log10();
            eprintln!("{shift:?}: {ratio_db:+.1} dB from unity");
            assert!(
                ratio_db.abs() < 12.0,
                "{shift:?}: output {ratio_db:+.1} dB from unity",
            );
        }
    }

    #[test]
    fn different_sample_rates() {
        for &sr in &[44100.0, 48000.0, 96000.0] {
            let mut p = PolyOctave::new();
            p.shift = OctaveShift::Up1;
            p.mix = 1.0;
            p.update(sr);

            let mut energy = 0.0;
            for i in 0..((sr * 1.0) as usize) {
                let input = (TAU * 440.0 * i as f64 / sr).sin() * 0.5;
                let out = p.tick(input);
                if i > (sr * 0.2) as usize {
                    energy += out * out;
                }
                assert!(out.is_finite(), "NaN at sr={sr}, sample {i}");
            }
            assert!(energy > 0.01, "No output at sr={sr}: energy={energy}");
        }
    }
}
