//! Transient detection — envelope-based and energy-based algorithms.
//!
//! Used for audio-triggered modulation. Two detection modes:
//! - Simple: fast-attack envelope follower, triggers on positive derivative
//! - Drums: RMS energy window, triggers on energy increase
//!
//! Based on tiagolr's Transient detector (gate12, filtr, time12, reevr).

use audiocore_dsp::biquad::{Biquad, FilterType};

/// Detection algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectAlgo {
    /// Envelope follower — triggers on amplitude rise.
    Simple,
    /// Energy window — triggers on RMS increase. Better for drums.
    Drums,
}

/// Transient detector for audio-triggered modulation.
pub struct TransientDetector {
    /// Detection algorithm.
    pub algo: DetectAlgo,
    /// Sensitivity threshold (0..1). Lower = more sensitive.
    pub sensitivity: f64,
    /// Amplitude threshold — signal must exceed this to trigger.
    pub threshold: f64,
    /// Highpass filter frequency for pre-filtering (0 = disabled).
    pub lowcut_freq: f64,
    /// Lowpass filter frequency for pre-filtering (0 = disabled).
    pub highcut_freq: f64,

    // Envelope follower state (Simple mode)
    envelope: f64,
    prev_envelope: f64,
    attack_coeff: f64,
    release_coeff: f64,

    // Energy window state (Drums mode)
    energy_buf: Vec<f64>,
    energy_pos: usize,
    energy_sum: f64,
    prev_rms: f64,

    // Pre-filters
    lowcut: Biquad,
    highcut: Biquad,

    // Cooldown
    cooldown_samples: usize,
    cooldown_remaining: usize,

    sample_rate: f64,
}

/// Cooldown after a trigger to prevent retriggering (50ms).
const COOLDOWN_MS: f64 = 50.0;

impl TransientDetector {
    pub fn new() -> Self {
        Self {
            algo: DetectAlgo::Simple,
            sensitivity: 0.5,
            threshold: 0.01,
            lowcut_freq: 0.0,
            highcut_freq: 0.0,
            envelope: 0.0,
            prev_envelope: 0.0,
            attack_coeff: 0.0,
            release_coeff: 0.0,
            energy_buf: Vec::new(),
            energy_pos: 0,
            energy_sum: 0.0,
            prev_rms: 0.0,
            lowcut: Biquad::new(),
            highcut: Biquad::new(),
            cooldown_samples: 0,
            cooldown_remaining: 0,
            sample_rate: 48000.0,
        }
    }

    /// Update coefficients for the current sample rate.
    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.cooldown_samples = (COOLDOWN_MS * 0.001 * sample_rate) as usize;

        // Envelope follower: fast attack (0.1ms), slow release (100ms)
        let attack_s = 0.0001;
        let release_s = 0.1;
        self.attack_coeff = (-1.0 / (sample_rate * attack_s)).exp();
        self.release_coeff = (-1.0 / (sample_rate * release_s)).exp();

        // Energy window: 20ms
        let window_size = (0.02 * sample_rate) as usize;
        if self.energy_buf.len() != window_size {
            self.energy_buf = vec![0.0; window_size];
            self.energy_pos = 0;
            self.energy_sum = 0.0;
        }

        // Pre-filters
        if self.lowcut_freq > 0.0 {
            self.lowcut
                .set(FilterType::Highpass, self.lowcut_freq, 0.707, sample_rate);
        }
        if self.highcut_freq > 0.0 {
            self.highcut
                .set(FilterType::Lowpass, self.highcut_freq, 0.707, sample_rate);
        }
    }

    /// Process one sample. Returns `true` if a transient is detected.
    pub fn tick(&mut self, sample: f64) -> bool {
        // Cooldown
        if self.cooldown_remaining > 0 {
            self.cooldown_remaining -= 1;
            // Still update state even during cooldown
            self.update_state(sample);
            return false;
        }

        let triggered = match self.algo {
            DetectAlgo::Simple => self.detect_simple(sample),
            DetectAlgo::Drums => self.detect_drums(sample),
        };

        if triggered {
            self.cooldown_remaining = self.cooldown_samples;
        }

        triggered
    }

    /// Pre-filter a sample through lowcut/highcut.
    pub fn filter(&mut self, sample: f64, ch: usize) -> f64 {
        let mut s = sample;
        if self.lowcut_freq > 0.0 {
            s = self.lowcut.tick(s, ch);
        }
        if self.highcut_freq > 0.0 {
            s = self.highcut.tick(s, ch);
        }
        s
    }

    fn update_state(&mut self, sample: f64) {
        let abs = sample.abs();
        match self.algo {
            DetectAlgo::Simple => {
                self.prev_envelope = self.envelope;
                let coeff = if abs > self.envelope {
                    self.attack_coeff
                } else {
                    self.release_coeff
                };
                self.envelope = coeff * (self.envelope - abs) + abs;
            }
            DetectAlgo::Drums => {
                let sq = sample * sample;
                if !self.energy_buf.is_empty() {
                    self.energy_sum -= self.energy_buf[self.energy_pos];
                    self.energy_buf[self.energy_pos] = sq;
                    self.energy_sum += sq;
                    self.energy_pos = (self.energy_pos + 1) % self.energy_buf.len();
                }
                self.prev_rms = (self.energy_sum / self.energy_buf.len().max(1) as f64).sqrt();
            }
        }
    }

    fn detect_simple(&mut self, sample: f64) -> bool {
        let abs = sample.abs();
        self.prev_envelope = self.envelope;
        let coeff = if abs > self.envelope {
            self.attack_coeff
        } else {
            self.release_coeff
        };
        self.envelope = coeff * (self.envelope - abs) + abs;

        let diff = (self.envelope - self.prev_envelope) * 10.0;
        diff > self.sensitivity && abs > self.threshold
    }

    fn detect_drums(&mut self, sample: f64) -> bool {
        let abs = sample.abs();
        let sq = sample * sample;

        let prev_rms = self.prev_rms;

        if !self.energy_buf.is_empty() {
            self.energy_sum -= self.energy_buf[self.energy_pos];
            self.energy_buf[self.energy_pos] = sq;
            self.energy_sum += sq;
            self.energy_pos = (self.energy_pos + 1) % self.energy_buf.len();
        }

        let rms = (self.energy_sum / self.energy_buf.len().max(1) as f64).sqrt();
        self.prev_rms = rms;

        let diff = (rms - prev_rms) * 75.0;
        diff > self.sensitivity && abs > self.threshold
    }

    pub fn reset(&mut self) {
        self.envelope = 0.0;
        self.prev_envelope = 0.0;
        self.energy_buf.fill(0.0);
        self.energy_pos = 0;
        self.energy_sum = 0.0;
        self.prev_rms = 0.0;
        self.cooldown_remaining = 0;
        self.lowcut.reset();
        self.highcut.reset();
    }
}

impl Default for TransientDetector {
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
    fn silence_no_trigger() {
        let mut d = TransientDetector::new();
        d.update(SR);

        for _ in 0..4800 {
            assert!(!d.tick(0.0));
        }
    }

    #[test]
    fn impulse_triggers_simple() {
        let mut d = TransientDetector::new();
        d.algo = DetectAlgo::Simple;
        d.sensitivity = 0.01;
        d.threshold = 0.001;
        d.update(SR);

        // Feed silence, then a loud impulse
        for _ in 0..480 {
            d.tick(0.0);
        }

        let mut triggered = false;
        for _ in 0..10 {
            if d.tick(1.0) {
                triggered = true;
                break;
            }
        }
        assert!(triggered, "Impulse should trigger simple detector");
    }

    #[test]
    fn impulse_triggers_drums() {
        let mut d = TransientDetector::new();
        d.algo = DetectAlgo::Drums;
        d.sensitivity = 0.01;
        d.threshold = 0.001;
        d.update(SR);

        // Feed silence
        for _ in 0..2400 {
            d.tick(0.0);
        }

        // Feed loud burst
        let mut triggered = false;
        for i in 0..960 {
            let s = (2.0 * PI * 100.0 * i as f64 / SR).sin() * 0.9;
            if d.tick(s) {
                triggered = true;
                break;
            }
        }
        assert!(triggered, "Burst should trigger drums detector");
    }

    #[test]
    fn cooldown_prevents_retrigger() {
        let mut d = TransientDetector::new();
        d.algo = DetectAlgo::Simple;
        d.sensitivity = 0.01;
        d.threshold = 0.001;
        d.update(SR);

        // First trigger
        for _ in 0..480 {
            d.tick(0.0);
        }
        let mut first_hit = false;
        for _ in 0..10 {
            if d.tick(1.0) {
                first_hit = true;
                break;
            }
        }
        assert!(first_hit);

        // Immediate second attempt should be blocked by cooldown
        let mut second_hit = false;
        for _ in 0..10 {
            if d.tick(1.0) {
                second_hit = true;
                break;
            }
        }
        assert!(!second_hit, "Cooldown should prevent immediate retrigger");
    }
}
