//! Feedback Delay Network (FDN) — the workhorse of Room and Hall reverbs.
//!
//! N parallel delay lines mixed through a unitary feedback matrix
//! (Householder or Hadamard) with per-line damping filters.

use audiocore_dsp::dc_blocker::DcBlocker;
use audiocore_dsp::delay_line::DelayLine;
use audiocore_dsp::denormal::flush;

use super::householder;
use super::one_pole::Lp1;

/// Mixing matrix type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MixMatrix {
    Householder,
    Hadamard,
}

/// Generic FDN with N delay lines.
pub struct Fdn {
    delays: Vec<DelayLine>,
    delay_samples: Vec<usize>,
    damping: Vec<Lp1>,
    dc_blockers: Vec<DcBlocker>,
    feedback: Vec<f64>, // Per-line state (output of delay -> matrix input)
    decay_gain: f64,    // Overall decay multiplier
    mix_matrix: MixMatrix,
    num_lines: usize,
    // 2-band decay control: split feedback into low/high via one-pole
    // crossover, multiply each by its own decay coefficient. Default
    // (1.0, 1.0) is a no-op.
    band_split: Vec<Lp1>,
    low_decay_mult: f64,
    high_decay_mult: f64,
    band_split_active: bool,
}

impl Fdn {
    /// Create an FDN with the given delay lengths (in samples).
    pub fn new(delay_lengths: &[usize], matrix: MixMatrix) -> Self {
        let n = delay_lengths.len();
        let delays = delay_lengths
            .iter()
            .map(|&len| DelayLine::new(len + 1))
            .collect();
        let delay_samples = delay_lengths.to_vec();
        let damping = (0..n).map(|_| Lp1::new()).collect();
        let dc_blockers = (0..n).map(|_| DcBlocker::new()).collect();
        let band_split = (0..n).map(|_| Lp1::new()).collect();
        let feedback = vec![0.0; n];

        Self {
            delays,
            delay_samples,
            damping,
            dc_blockers,
            feedback,
            decay_gain: 0.85,
            mix_matrix: matrix,
            num_lines: n,
            band_split,
            low_decay_mult: 1.0,
            high_decay_mult: 1.0,
            band_split_active: false,
        }
    }

    /// Configure per-band feedback decay. Splits feedback at `crossover_hz`
    /// into low/high parts and scales each. `low_mult` > 1 lengthens the
    /// low-frequency tail (warmer rooms), `< 1` shortens it. Same for
    /// `high_mult`. Both = 1.0 disables the split entirely.
    pub fn set_band_decay(
        &mut self,
        crossover_hz: f64,
        low_mult: f64,
        high_mult: f64,
        sample_rate: f64,
    ) {
        self.low_decay_mult = low_mult.clamp(0.0, 2.0);
        self.high_decay_mult = high_mult.clamp(0.0, 2.0);
        self.band_split_active = (low_mult - 1.0).abs() > 1e-4 || (high_mult - 1.0).abs() > 1e-4;
        for b in &mut self.band_split {
            b.set_freq(crossover_hz, sample_rate);
        }
    }

    /// Set all delay lengths (in samples). Must match the number of lines.
    pub fn set_delays(&mut self, lengths: &[usize]) {
        for (i, &len) in lengths.iter().enumerate().take(self.num_lines) {
            self.delay_samples[i] = len.min(self.delays[i].len() - 1);
        }
    }

    /// Set the damping filter cutoff for all lines.
    pub fn set_damping(&mut self, freq_hz: f64, sample_rate: f64) {
        for d in &mut self.damping {
            d.set_freq(freq_hz, sample_rate);
        }
    }

    /// Set the damping coefficient directly (0.0 = no damping, 1.0 = max).
    pub fn set_damping_coeff(&mut self, g: f64) {
        for d in &mut self.damping {
            d.set_coeff(g);
        }
    }

    /// Set the overall decay gain (0.0 = no feedback, 1.0 = infinite).
    pub fn set_decay(&mut self, gain: f64) {
        self.decay_gain = gain.clamp(0.0, 0.999);
    }

    /// Process one mono input sample, return the mixed output of all lines.
    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        let n = self.num_lines;

        // Read from all delay lines
        for i in 0..n {
            self.feedback[i] = self.delays[i].read(self.delay_samples[i]);
        }

        // Apply mixing matrix
        match self.mix_matrix {
            MixMatrix::Householder => householder::mix(&mut self.feedback[..n]),
            MixMatrix::Hadamard => {
                // Hadamard requires power of 2 — if not, fall back to Householder
                if n.is_power_of_two() {
                    super::hadamard::mix(&mut self.feedback[..n]);
                } else {
                    householder::mix(&mut self.feedback[..n]);
                }
            }
        }

        // Sum output before feeding back (tap from delay outputs)
        let mut output = 0.0;
        let output_scale = 1.0 / (n as f64).sqrt();

        for i in 0..n {
            output += self.delays[i].read(self.delay_samples[i]) * output_scale;

            // Apply damping and decay, then feed back with input injection
            let mut sig = self.damping[i].tick(self.feedback[i]) * self.decay_gain;
            if self.band_split_active {
                let low = self.band_split[i].tick(sig);
                let high = sig - low;
                sig = low * self.low_decay_mult + high * self.high_decay_mult;
            }
            // Block DC in the recirculating path — long tails otherwise
            // accumulate subsonic offset (worst with pitch-shifted or
            // saturated feedback around the FDN).
            sig = self.dc_blockers[i].tick(sig);
            self.delays[i].write(flush(input + sig));
        }

        output
    }

    pub fn reset(&mut self) {
        for d in &mut self.delays {
            d.clear();
        }
        for d in &mut self.damping {
            d.reset();
        }
        for b in &mut self.band_split {
            b.reset();
        }
        for dc in &mut self.dc_blockers {
            dc.reset();
        }
        self.feedback.fill(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LENGTHS: [usize; 4] = [1049, 1327, 1559, 1801];

    #[test]
    fn impulse_decays_to_silence() {
        let mut fdn = Fdn::new(&LENGTHS, MixMatrix::Householder);
        fdn.set_decay(0.8);
        fdn.set_damping(6000.0, 48000.0);

        let mut late = 0.0f64;
        for n in 0..480_000 {
            let x = if n == 0 { 1.0 } else { 0.0 };
            let y = fdn.tick(x);
            assert!(y.is_finite(), "NaN at {n}");
            if n > 400_000 {
                late = late.max(y.abs());
            }
        }
        assert!(late < 1e-6, "10s tail should have decayed: {late}");
    }

    #[test]
    fn dc_input_does_not_accumulate() {
        // Constant DC into a high-feedback FDN: without the in-loop DC
        // blockers the tail integrates toward a large offset. With them
        // the output must stay bounded and near-zero-mean.
        let mut fdn = Fdn::new(&LENGTHS, MixMatrix::Householder);
        fdn.set_decay(0.98);

        for n in 0..240_000 {
            let y = fdn.tick(0.5);
            assert!(y.is_finite());
            // Direct-path DC (input reaches the output tap of every line)
            // is expected; unblocked feedback accumulation is not.
            assert!(y.abs() < 10.0, "output blew up at {n}: {y}");
        }

        // Once input stops, no offset may remain stored in the loop.
        let mut sum = 0.0;
        let mut count = 0.0;
        for n in 0..480_000 {
            let y = fdn.tick(0.0);
            assert!(y.is_finite());
            if n > 240_000 {
                sum += y;
                count += 1.0;
            }
        }
        let mean: f64 = sum / count;
        assert!(
            mean.abs() < 1e-4,
            "loop should hold no DC after input stops: {mean}"
        );
    }
}
