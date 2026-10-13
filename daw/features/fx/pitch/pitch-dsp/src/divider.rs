//! Frequency divider — analog-style octave generation (Boss OC-2 / EHX OC-3).
//!
//! Tracks zero crossings of the input waveform and toggles a flip-flop at
//! half the rate to produce a sub-octave square wave. The square wave is
//! shaped by the input's envelope for dynamic tracking.
//!
//! Latency: 0 samples.
//! Character: Synthy, square-wave. The classic "OC-2 sound".

use serde::{Deserialize, Serialize};

/// Octave division ratio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DivideRatio {
    /// One octave down (divide by 2).
    Oct1,
    /// Two octaves down (divide by 4).
    Oct2,
}

/// Analog-style frequency divider for sub-octave generation.
pub struct FreqDivider {
    /// Division ratio.
    pub ratio: DivideRatio,
    /// Mix: 0.0 = dry only, 1.0 = wet only.
    pub mix: f64,

    // Flip-flop state for octave 1 (÷2).
    flip1: bool,
    // Flip-flop state for octave 2 (÷4, driven by flip1 edges).
    flip2: bool,
    flip1_prev: bool,

    // Zero-crossing detection.
    prev_sample: f64,
    // Whether previous sample was positive (for hysteresis).
    was_positive: bool,

    // Envelope follower for shaping the sub output.
    envelope: f64,
    attack_coeff: f64,
    release_coeff: f64,

    // DC blocker (1-pole high-pass) to remove DC from the square wave.
    dc_x1: f64,
    dc_y1: f64,
    dc_coeff: f64,

    // Hysteresis threshold to avoid false triggers from noise.
    hysteresis: f64,
}

impl FreqDivider {
    pub fn new() -> Self {
        Self {
            ratio: DivideRatio::Oct1,
            mix: 1.0,
            flip1: false,
            flip2: false,
            flip1_prev: false,
            prev_sample: 0.0,
            was_positive: false,
            envelope: 0.0,
            attack_coeff: 0.0,
            release_coeff: 0.0,
            dc_x1: 0.0,
            dc_y1: 0.0,
            dc_coeff: 0.997,
            hysteresis: 0.01,
        }
    }

    pub fn update(&mut self, sample_rate: f64) {
        // Fast attack (~0.5ms), slower release (~10ms) for envelope.
        self.attack_coeff = (-1.0 / (0.0005 * sample_rate)).exp();
        self.release_coeff = (-1.0 / (0.01 * sample_rate)).exp();
        // DC blocker: ~10 Hz cutoff.
        self.dc_coeff = 1.0 - (std::f64::consts::TAU * 10.0 / sample_rate);
    }

    pub fn reset(&mut self) {
        self.flip1 = false;
        self.flip2 = false;
        self.flip1_prev = false;
        self.prev_sample = 0.0;
        self.was_positive = false;
        self.envelope = 0.0;
        self.dc_x1 = 0.0;
        self.dc_y1 = 0.0;
    }

    /// Process one sample. Returns the mixed output.
    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        // Envelope follower.
        let abs_in = input.abs();
        let coeff = if abs_in > self.envelope {
            self.attack_coeff
        } else {
            self.release_coeff
        };
        self.envelope = coeff * self.envelope + (1.0 - coeff) * abs_in;

        // Zero-crossing detection with hysteresis.
        let is_positive = if self.was_positive {
            input > -self.hysteresis
        } else {
            input > self.hysteresis
        };

        // Detect negative→positive zero crossing.
        if is_positive && !self.was_positive {
            self.flip1 = !self.flip1;

            // Octave 2: toggle on flip1 rising edges.
            if self.flip1 && !self.flip1_prev {
                self.flip2 = !self.flip2;
            }
            self.flip1_prev = self.flip1;
        }
        self.was_positive = is_positive;
        self.prev_sample = input;

        // Generate sub signal: square wave shaped by envelope.
        let sub_raw = match self.ratio {
            DivideRatio::Oct1 => {
                if self.flip1 {
                    1.0
                } else {
                    -1.0
                }
            }
            DivideRatio::Oct2 => {
                if self.flip2 {
                    1.0
                } else {
                    -1.0
                }
            }
        };
        let sub_shaped = sub_raw * self.envelope;

        // DC blocker on sub signal.
        let dc_out = sub_shaped - self.dc_x1 + self.dc_coeff * self.dc_y1;
        self.dc_x1 = sub_shaped;
        self.dc_y1 = dc_out;

        // Mix dry/wet.
        input * (1.0 - self.mix) + dc_out * self.mix
    }

    pub fn latency(&self) -> usize {
        0
    }
}

impl Default for FreqDivider {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    const SR: f64 = 48000.0;

    fn make_divider() -> FreqDivider {
        let mut d = FreqDivider::new();
        d.ratio = DivideRatio::Oct1;
        d.mix = 1.0;
        d.update(SR);
        d
    }

    #[test]
    fn zero_latency() {
        let d = make_divider();
        assert_eq!(d.latency(), 0);
    }

    #[test]
    fn silence_in_silence_out() {
        let mut d = make_divider();
        for _ in 0..4800 {
            let out = d.tick(0.0);
            assert!(out.abs() < 0.001, "Should be silent: {out}");
        }
    }

    #[test]
    fn produces_sub_octave() {
        let mut d = make_divider();
        let freq = 440.0;
        let n = (SR / freq * 20.0) as usize; // 20 cycles

        let mut output = Vec::with_capacity(n);
        for i in 0..n {
            let input = (2.0 * PI * freq * i as f64 / SR).sin() * 0.8;
            output.push(d.tick(input));
        }

        // Count zero crossings in output (should be roughly half the input's).
        let mut crossings = 0usize;
        for w in output.windows(2) {
            if (w[0] >= 0.0) != (w[1] >= 0.0) {
                crossings += 1;
            }
        }

        // Input has ~40 zero crossings (20 cycles * 2).
        // Sub-octave should have ~20 (10 cycles * 2).
        assert!(
            crossings > 10 && crossings < 30,
            "Expected ~20 crossings for sub-octave, got {crossings}"
        );
    }

    #[test]
    fn no_nan() {
        let mut d = make_divider();
        for i in 0..48000 {
            let input = (2.0 * PI * 82.0 * i as f64 / SR).sin() * 0.9;
            let out = d.tick(input);
            assert!(out.is_finite(), "NaN at sample {i}");
        }
    }

    #[test]
    fn envelope_tracks_dynamics() {
        let mut d = make_divider();

        // Feed loud signal, then silence.
        let mut last_loud = 0.0f64;
        for i in 0..4800 {
            let input = (2.0 * PI * 200.0 * i as f64 / SR).sin() * 0.8;
            last_loud = d.tick(input).abs().max(last_loud);
        }

        // After extended silence, output should decay.
        let mut max_quiet = 0.0f64;
        for _ in 0..48000 {
            max_quiet = d.tick(0.0).abs().max(max_quiet);
        }

        // The DC blocker holds some energy briefly, so just verify decay happened.
        let mut tail_energy = 0.0f64;
        for _ in 0..4800 {
            tail_energy += d.tick(0.0).abs();
        }
        tail_energy /= 4800.0;

        assert!(
            tail_energy < 0.01,
            "Envelope should decay to near zero: tail_avg={tail_energy}"
        );
    }

    #[test]
    fn oct2_produces_two_octaves_down() {
        let mut d = make_divider();
        d.ratio = DivideRatio::Oct2;

        let freq = 440.0;
        let n = (SR / freq * 40.0) as usize;

        let mut output = Vec::with_capacity(n);
        for i in 0..n {
            let input = (2.0 * PI * freq * i as f64 / SR).sin() * 0.8;
            output.push(d.tick(input));
        }

        // Count zero crossings: 2 octaves down = 1/4 frequency.
        let mut crossings = 0usize;
        for w in output.windows(2) {
            if (w[0] >= 0.0) != (w[1] >= 0.0) {
                crossings += 1;
            }
        }

        // Input has ~80 crossings (40 cycles * 2).
        // Two octaves down should have ~20 (10 cycles * 2).
        assert!(
            crossings > 8 && crossings < 35,
            "Expected ~20 crossings for 2-octave sub, got {crossings}"
        );
    }

    #[test]
    fn dry_wet_mix() {
        let mut dry_only = make_divider();
        dry_only.mix = 0.0;

        for i in 0..4800 {
            let input = (2.0 * PI * 440.0 * i as f64 / SR).sin() * 0.5;
            let out = dry_only.tick(input);
            assert!(
                (out - input).abs() < 1e-10,
                "Mix=0 should pass dry: diff={}",
                (out - input).abs()
            );
        }
    }
}
