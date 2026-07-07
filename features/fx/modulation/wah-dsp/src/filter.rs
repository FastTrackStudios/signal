//! Wah filter — Chamberlin state-variable filter with LP/BP/HP mixing.
//!
//! Uses a cascadable SVF topology for the characteristic vocal-like sweep.
//! The output is a configurable mix of lowpass, bandpass, and highpass,
//! allowing classic wah, mutron, and phaser-like tones.
//!
//! Techniques from: ZynAddSubFX (SVF topology), rkrlv2/RyanWah (mix mode,
//! variable Q tracking, exponential frequency mapping).

use std::f64::consts::PI;

/// Wah filter mode — controls the LP/BP/HP mix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WahMode {
    /// Classic wah — bandpass-dominant with some lowpass bleed.
    Classic,
    /// Mutron — bandpass-only, sharp vocal resonance.
    Mutron,
    /// Low-pass wah — darker sweep.
    Lowpass,
    /// Phase-like — mixed HP/BP for phaser-ish response.
    Phaser,
}

impl WahMode {
    /// LP, BP, HP mix weights for this mode.
    fn mix_weights(&self) -> (f64, f64, f64) {
        match self {
            WahMode::Classic => (0.2, 0.8, 0.0),
            WahMode::Mutron => (0.0, 1.0, 0.0),
            WahMode::Lowpass => (1.0, 0.2, 0.0),
            WahMode::Phaser => (0.0, 0.5, 0.5),
        }
    }
}

/// Chamberlin SVF state for one channel.
#[derive(Clone, Default)]
struct SvfState {
    low: f64,
    band: f64,
    high: f64,
}

/// Chamberlin state-variable filter with LP/BP/HP mixing.
///
/// Produces simultaneous lowpass, bandpass, and highpass outputs.
/// The wah tone is a weighted mix of these three.
///
/// Supports cascaded stages for steeper slopes.
pub struct WahFilter {
    /// Filter mode (controls LP/BP/HP mix).
    pub mode: WahMode,
    /// Base Q / resonance (1.0–20.0). Higher = more vocal.
    pub q: f64,
    /// Number of cascaded filter stages (1–4).
    pub stages: usize,
    /// Variable Q tracking (0..1). When > 0, Q varies inversely with
    /// the modulation position — emulates real inductor wah pedals.
    pub variq: f64,

    states: [[SvfState; 4]; 2], // [channel][stage]
    current_freq: f64,
    f_coeff: f64,
    q_coeff: f64,
    sample_rate: f64,
}

impl WahFilter {
    /// Minimum sweep frequency (Hz).
    pub const MIN_FREQ: f64 = 200.0;
    /// Maximum sweep frequency (Hz).
    pub const MAX_FREQ: f64 = 5000.0;
    /// Exponential mapping base (from rkrlv2).
    const FREQ_BASE: f64 = 7.0;

    pub fn new() -> Self {
        Self {
            mode: WahMode::Classic,
            q: 3.0,
            stages: 1,
            variq: 0.0,
            states: Default::default(),
            current_freq: 800.0,
            f_coeff: 0.0,
            q_coeff: 0.0,
            sample_rate: 48000.0,
        }
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.set_freq(self.current_freq);
    }

    /// Set the filter cutoff frequency directly.
    pub fn set_freq(&mut self, freq: f64) {
        self.current_freq = freq.clamp(Self::MIN_FREQ, Self::MAX_FREQ);
        // Chamberlin SVF coefficient: f = 2 * sin(pi * fc / fs)
        self.f_coeff = (2.0 * (PI * self.current_freq / self.sample_rate).sin()).clamp(0.0, 0.95); // stability clamp
        self.update_q(self.q);
    }

    /// Set cutoff from a normalized position (0..1).
    ///
    /// Uses exponential mapping (base 7.0) for perceptually uniform sweep.
    pub fn set_position(&mut self, pos: f64) {
        let pos = pos.clamp(0.0, 1.0);
        let freq =
            Self::MIN_FREQ + Self::MAX_FREQ * (Self::FREQ_BASE.powf(pos) - 1.0) / Self::FREQ_BASE;
        self.set_freq(freq);
    }

    /// Set position with variable Q tracking.
    ///
    /// When `variq > 0`, Q varies inversely with position — higher position
    /// (brighter) gets lower Q, lower position (darker) gets higher Q.
    /// This emulates the behavior of real inductor-based wah pedals.
    pub fn set_position_with_env(&mut self, pos: f64, env_level: f64) {
        self.set_position(pos);
        if self.variq > 0.0 {
            // Q varies inversely with envelope: louder = less resonant
            let q_scale = 2.0_f64.powf(2.0 * (1.0 - env_level) + 1.0);
            let varied_q = self.q * (1.0 - self.variq) + q_scale * self.variq;
            self.update_q(varied_q);
        }
    }

    fn update_q(&mut self, q: f64) {
        // HiQ mode: direct inverse for better sound (per rkrlv2)
        self.q_coeff = (1.0 / q.max(0.5)).clamp(0.01, 1.99);
    }

    /// Process one sample through the cascaded SVF.
    #[inline]
    pub fn tick(&mut self, sample: f64, ch: usize) -> f64 {
        let ch = ch.min(1);
        let (lp_gain, bp_gain, hp_gain) = self.mode.mix_weights();
        let n = self.stages.clamp(1, 4);

        let mut smp = sample;

        for stage in 0..n {
            let s = &mut self.states[ch][stage];

            // Chamberlin SVF topology
            s.low += self.f_coeff * s.band;
            s.high = smp - s.low - self.q_coeff * s.band;
            s.band += self.f_coeff * s.high;

            // Mix LP/BP/HP
            smp = lp_gain * s.low + bp_gain * s.band + hp_gain * s.high;
        }

        smp
    }

    /// Get the current cutoff frequency.
    pub fn freq(&self) -> f64 {
        self.current_freq
    }

    pub fn reset(&mut self) {
        self.states = Default::default();
    }
}

impl Default for WahFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// Triple-cascaded envelope smoother.
///
/// Three first-order lowpass smoothers in series create a 3rd-order
/// characteristic with natural, lag-free response.
/// Based on rkrlv2's RyanWah envelope smoothing (20ms time constant).
pub struct TripleSmoother {
    s1: f64,
    s2: f64,
    s3: f64,
    coeff: f64,
}

impl TripleSmoother {
    pub fn new() -> Self {
        Self {
            s1: 0.0,
            s2: 0.0,
            s3: 0.0,
            coeff: 0.01,
        }
    }

    /// Set the smoothing time constant.
    pub fn set_time_ms(&mut self, ms: f64, sample_rate: f64) {
        let time_s = ms * 0.001;
        if time_s > 0.0 && sample_rate > 0.0 {
            self.coeff = 1.0 - (-1.0 / (time_s * sample_rate)).exp();
        } else {
            self.coeff = 1.0;
        }
    }

    /// Process one sample through three cascaded smoothers.
    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        self.s1 += (input - self.s1) * self.coeff;
        self.s2 += (self.s1 - self.s2) * self.coeff;
        self.s3 += (self.s2 - self.s3) * self.coeff;
        self.s3
    }

    pub fn value(&self) -> f64 {
        self.s3
    }

    pub fn reset(&mut self, val: f64) {
        self.s1 = val;
        self.s2 = val;
        self.s3 = val;
    }
}

impl Default for TripleSmoother {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    #[test]
    fn position_maps_to_freq_range() {
        let mut f = WahFilter::new();
        f.update(SR);

        f.set_position(0.0);
        assert!(f.freq() >= WahFilter::MIN_FREQ, "Min freq: {}", f.freq());

        f.set_position(1.0);
        assert!(f.freq() >= WahFilter::MIN_FREQ, "Max freq: {}", f.freq());
        // With base-7 exponential, pos=1 should be near max
        assert!(f.freq() > 1000.0, "Pos=1 should be high: {}", f.freq());
    }

    #[test]
    fn bandpass_peaks_at_cutoff() {
        let mut f = WahFilter::new();
        f.mode = WahMode::Mutron; // Pure bandpass
        f.q = 5.0;
        f.update(SR);
        f.set_freq(1000.0);

        // Feed 1000 Hz sine — should pass well
        let mut energy_pass: f64 = 0.0;
        for i in 0..4800 {
            let s = (2.0 * PI * 1000.0 * i as f64 / SR).sin();
            let out = f.tick(s, 0);
            energy_pass += out * out;
        }

        // Feed 100 Hz sine — should be attenuated
        f.reset();
        let mut energy_reject: f64 = 0.0;
        for i in 0..4800 {
            let s = (2.0 * PI * 100.0 * i as f64 / SR).sin();
            let out = f.tick(s, 0);
            energy_reject += out * out;
        }

        assert!(
            energy_pass > energy_reject * 2.0,
            "Bandpass should favor cutoff freq: pass={energy_pass:.2}, reject={energy_reject:.2}"
        );
    }

    #[test]
    fn no_nan_across_sweep() {
        let mut f = WahFilter::new();
        f.update(SR);

        for i in 0..48000 {
            let pos = i as f64 / 48000.0;
            f.set_position(pos);
            let s = (2.0 * PI * 440.0 * i as f64 / SR).sin();
            let out = f.tick(s, 0);
            assert!(out.is_finite(), "NaN at sample {i}");
        }
    }

    #[test]
    fn svf_resonance() {
        let mut f = WahFilter::new();
        f.mode = WahMode::Mutron;
        f.q = 15.0;
        f.update(SR);
        f.set_freq(1000.0);

        // SVF should ring nicely at high Q
        let mut max_out: f64 = 0.0;
        let out = f.tick(1.0, 0);
        max_out = max_out.max(out.abs());
        for _ in 1..480 {
            let out = f.tick(0.0, 0);
            max_out = max_out.max(out.abs());
        }
        assert!(max_out > 0.1, "SVF at Q=15 should ring: max={max_out}");
    }

    #[test]
    fn variq_changes_q() {
        let mut f = WahFilter::new();
        f.mode = WahMode::Mutron;
        f.q = 8.0;
        f.variq = 1.0;
        f.update(SR);

        // At env=0 (quiet), Q should be higher → more ringing
        f.set_position_with_env(0.5, 0.0);
        let q_quiet = f.q_coeff;

        // At env=1 (loud), Q should be lower → less ringing
        f.set_position_with_env(0.5, 1.0);
        let q_loud = f.q_coeff;

        // Higher q_coeff = less resonance (it's 1/Q)
        assert!(
            q_loud > q_quiet,
            "Loud should have higher damping: loud={q_loud}, quiet={q_quiet}"
        );
    }

    #[test]
    fn triple_smoother_converges() {
        let mut s = TripleSmoother::new();
        s.set_time_ms(5.0, SR);

        for _ in 0..4800 {
            // 100ms
            s.tick(1.0);
        }
        assert!(
            (s.value() - 1.0).abs() < 0.01,
            "Should converge: {}",
            s.value()
        );
    }

    #[test]
    fn triple_smoother_is_smoother_than_single() {
        let mut single = 0.0_f64;
        let mut triple = TripleSmoother::new();
        triple.set_time_ms(5.0, SR);
        let coeff = 1.0 - (-1.0 / (0.005 * SR)).exp();

        // Step response: triple should lag more (smoother)
        single += (1.0 - single) * coeff;
        let v1 = triple.tick(1.0);
        assert!(
            v1 < single,
            "Triple should lag: triple={v1}, single={single}"
        );
    }

    #[test]
    fn cascaded_stages_steeper() {
        // 2 stages should attenuate off-frequency more than 1
        let mut f1 = WahFilter::new();
        f1.mode = WahMode::Mutron;
        f1.q = 3.0;
        f1.stages = 1;
        f1.update(SR);
        f1.set_freq(1000.0);

        let mut f2 = WahFilter::new();
        f2.mode = WahMode::Mutron;
        f2.q = 3.0;
        f2.stages = 2;
        f2.update(SR);
        f2.set_freq(1000.0);

        // Feed off-frequency signal
        let mut energy1: f64 = 0.0;
        let mut energy2: f64 = 0.0;
        for i in 0..4800 {
            let s = (2.0 * PI * 200.0 * i as f64 / SR).sin();
            let o1 = f1.tick(s, 0);
            let o2 = f2.tick(s, 0);
            energy1 += o1 * o1;
            energy2 += o2 * o2;
        }

        assert!(
            energy2 < energy1,
            "2 stages should attenuate more: 1-stage={energy1:.2}, 2-stage={energy2:.2}"
        );
    }
}
