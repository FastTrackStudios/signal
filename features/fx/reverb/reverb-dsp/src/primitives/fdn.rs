//! Feedback Delay Network (FDN) — the workhorse of Room and Hall reverbs.
//!
//! N parallel delay lines mixed through a unitary feedback matrix
//! (Householder or Hadamard) with per-line damping filters.

use audiocore_dsp::dc_blocker::DcBlocker;
use audiocore_dsp::delay_line::DelayLine;
use audiocore_dsp::denormal::flush;
use audiocore_dsp::prng::XorShift32;

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

    // ── Jot per-line T60 shelf (opt-in via `set_t60`) ──────────────
    // Exact frequency-dependent decay: per line i with length Mi,
    // R0 = 10^(−3·Mi/(fs·T60_dc)), Rπ = 10^(−3·Mi/(fs·T60_nyq)),
    // pole p = (R0−Rπ)/(R0+Rπ), gain g = 2·R0·Rπ/(R0+Rπ). Replaces
    // decay_gain + damping + band_split when active, and applies Jot's
    // tonal-correction one-zero on the summed output so decay changes
    // don't recolor the wet spectrum.
    t60_mode: bool,
    shelf_g: Vec<f64>,
    shelf_p: Vec<f64>,
    shelf_state: Vec<f64>,
    tc_b: f64,
    tc_prev: f64,

    // ── Slow orthogonal rotation (opt-in via `set_rotation`) ───────
    // Post-matrix Givens rotations between line pairs with slowly
    // swept angles: animates the tail with no decay error and no
    // pitch artifacts (Schlecht's time-varying-matrix result).
    rot_depth: f64,
    rot_inc: f64,
    rot_phase: f64,
    rot_cs: Vec<(f64, f64)>,
    rot_countdown: u32,

    // ── In-loop allpasses (opt-in via `set_loop_allpass`) ──────────
    // Zita-style Schroeder allpass inside each line's feedback path,
    // coefficients alternating ±coeff: density compounds every pass.
    loop_ap: Vec<DelayLine>,
    loop_ap_len: Vec<usize>,
    loop_ap_coeff: f64,

    // ── Random-walk delay jitter (opt-in via `set_jitter`) ─────────
    // reverbsc-style per-line drift: random targets, linear glide,
    // fractional reads — huge-but-unchorused tail animation.
    jitter_depth: f64,
    jitter_cur: Vec<f64>,
    jitter_step: Vec<f64>,
    jitter_count: Vec<u32>,
    jitter_rng: XorShift32,
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
            t60_mode: false,
            shelf_g: vec![0.0; n],
            shelf_p: vec![0.0; n],
            shelf_state: vec![0.0; n],
            tc_b: 0.0,
            tc_prev: 0.0,
            rot_depth: 0.0,
            rot_inc: 0.0,
            rot_phase: 0.0,
            rot_cs: vec![(1.0, 0.0); n / 2],
            rot_countdown: 0,
            loop_ap: Vec::new(),
            loop_ap_len: vec![0; n],
            loop_ap_coeff: 0.0,
            jitter_depth: 0.0,
            jitter_cur: vec![0.0; n],
            jitter_step: vec![0.0; n],
            jitter_count: vec![0; n],
            jitter_rng: XorShift32::new(0xFD4_517E5),
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
        // Re-tune the in-loop DC blockers while we have the sample rate:
        // a 10 Hz corner settles ~3x faster after the onset burst than
        // the default ~4 Hz pole (less infrasonic relaxation drift in
        // short-room IRs) and still sits below audibility.
        for dc in &mut self.dc_blockers {
            dc.set_cutoff(10.0, sample_rate);
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

    /// Switch to exact Jot per-line T60 decay: `t60_dc` / `t60_nyq`
    /// seconds at DC and Nyquist. Replaces `set_decay` + `set_damping`
    /// + `set_band_decay` (those become inert while active); pass a
    /// huge T60 for infinite hold. Disable with `clear_t60`.
    pub fn set_t60(&mut self, t60_dc: f64, t60_nyq: f64, sample_rate: f64) {
        let t_dc = t60_dc.max(0.01);
        let t_ny = t60_nyq.max(0.01);
        for i in 0..self.num_lines {
            let mi = self.delay_samples[i] as f64;
            let r0 = 10.0f64.powf(-3.0 * mi / (sample_rate * t_dc));
            let rp = 10.0f64.powf(-3.0 * mi / (sample_rate * t_ny));
            self.shelf_p[i] = (r0 - rp) / (r0 + rp);
            self.shelf_g[i] = 2.0 * r0 * rp / (r0 + rp);
        }
        // Tonal correction: |E(ω)|² ∝ 1/T60(ω) via a one-zero.
        let alpha = (t_ny / t_dc).clamp(0.05, 20.0);
        self.tc_b = (1.0 - alpha) / (1.0 + alpha);
        self.t60_mode = true;
    }

    pub fn clear_t60(&mut self) {
        self.t60_mode = false;
    }

    /// Slow orthogonal-rotation tail animation: Givens rotations
    /// between line pairs, angles swept at `rate_hz` with peak `depth`
    /// radians. Depth 0 disables. Artifact-free: the loop stays
    /// lossless-equivalent, so decay time and pitch are untouched.
    pub fn set_rotation(&mut self, rate_hz: f64, depth: f64, sample_rate: f64) {
        self.rot_depth = depth.clamp(0.0, 0.5);
        self.rot_inc = rate_hz.max(0.0) / sample_rate;
        self.rot_countdown = 0;
    }

    /// Zita-style in-loop allpasses: one short Schroeder allpass per
    /// line, coefficients alternating `±coeff` — echo density builds
    /// every recirculation instead of only at the input diffuser.
    /// Coeff 0 disables. Allocates on first call (control path only).
    pub fn set_loop_allpass(&mut self, coeff: f64) {
        self.loop_ap_coeff = coeff.clamp(-0.9, 0.9);
        if self.loop_ap_coeff.abs() > 1e-4 && self.loop_ap.is_empty() {
            for i in 0..self.num_lines {
                // Short prime-ish lengths derived from the line length.
                let len = (self.delay_samples[i] / 7 + 19 + 26 * i) | 1;
                self.loop_ap_len[i] = len;
                self.loop_ap.push(DelayLine::new(len + 4));
            }
        }
    }

    /// reverbsc-style random-walk delay jitter: each line drifts its
    /// read position by up to `±depth_ms`, gliding linearly to freshly
    /// randomized targets. Depth 0 disables (integer reads).
    pub fn set_jitter(&mut self, depth_ms: f64, sample_rate: f64) {
        self.jitter_depth = (depth_ms * 0.001 * sample_rate).max(0.0);
    }

    /// Process one mono input sample, return the mixed output of all lines.
    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        let n = self.num_lines;

        // Read from all delay lines (fractional when jittered).
        if self.jitter_depth > 1e-9 {
            for i in 0..n {
                if self.jitter_count[i] == 0 {
                    // New random drift target, glide over 300–1500 samples.
                    let interval =
                        300 + (self.jitter_rng.next_bipolar().abs() * 1200.0) as u32;
                    let target = self.jitter_rng.next_bipolar() * self.jitter_depth;
                    self.jitter_step[i] = (target - self.jitter_cur[i]) / interval as f64;
                    self.jitter_count[i] = interval;
                }
                self.jitter_count[i] -= 1;
                self.jitter_cur[i] += self.jitter_step[i];
                let pos = (self.delay_samples[i] as f64 + self.jitter_cur[i])
                    .clamp(1.0, (self.delays[i].len() - 2) as f64);
                self.feedback[i] = self.delays[i].read_linear(pos);
            }
        } else {
            for i in 0..n {
                self.feedback[i] = self.delays[i].read(self.delay_samples[i]);
            }
        }

        // Sum output before mixing (tap from raw delay outputs).
        let mut output = 0.0;
        let output_scale = 1.0 / (n as f64).sqrt();
        for i in 0..n {
            output += self.feedback[i] * output_scale;
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

        // Slow orthogonal rotation between line pairs (tail animation).
        if self.rot_depth > 1e-9 && n >= 2 {
            if self.rot_countdown == 0 {
                self.rot_countdown = 16;
                self.rot_phase = (self.rot_phase + self.rot_inc * 16.0).fract();
                for (k, cs) in self.rot_cs.iter_mut().enumerate() {
                    let theta = self.rot_depth
                        * (core::f64::consts::TAU
                            * (self.rot_phase + k as f64 * 0.31))
                            .sin();
                    *cs = (theta.cos(), theta.sin());
                }
            }
            self.rot_countdown -= 1;
            for k in 0..n / 2 {
                let (c, sn) = self.rot_cs[k];
                let a = self.feedback[2 * k];
                let b = self.feedback[2 * k + 1];
                self.feedback[2 * k] = c * a - sn * b;
                self.feedback[2 * k + 1] = sn * a + c * b;
            }
        }

        for i in 0..n {
            // Per-line decay: exact Jot T60 shelf when engaged,
            // otherwise the legacy damping · decay · band-split path.
            let mut sig = if self.t60_mode {
                let y = self.shelf_g[i] * self.feedback[i]
                    + self.shelf_p[i] * self.shelf_state[i];
                self.shelf_state[i] = flush(y);
                y
            } else {
                let mut sig = self.damping[i].tick(self.feedback[i]) * self.decay_gain;
                if self.band_split_active {
                    let low = self.band_split[i].tick(sig);
                    let high = sig - low;
                    sig = low * self.low_decay_mult + high * self.high_decay_mult;
                }
                sig
            };

            // In-loop allpass: density compounds each recirculation.
            if self.loop_ap_coeff.abs() > 1e-4 && !self.loop_ap.is_empty() {
                let g = if i % 2 == 0 {
                    self.loop_ap_coeff
                } else {
                    -self.loop_ap_coeff
                };
                let delayed = self.loop_ap[i].read(self.loop_ap_len[i]);
                let v = sig - g * delayed;
                self.loop_ap[i].write(v);
                sig = delayed + g * v;
            }

            // Block DC in the recirculating path — long tails otherwise
            // accumulate subsonic offset (worst with pitch-shifted or
            // saturated feedback around the FDN).
            sig = self.dc_blockers[i].tick(sig);
            self.delays[i].write(flush(input + sig));
        }

        // Jot tonal correction (one-zero) so T60 changes don't recolor
        // the wet spectrum.
        if self.t60_mode && self.tc_b.abs() > 1e-9 {
            let corrected = (output - self.tc_b * self.tc_prev) / (1.0 - self.tc_b);
            self.tc_prev = output;
            corrected
        } else {
            output
        }
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
        for ap in &mut self.loop_ap {
            ap.clear();
        }
        self.shelf_state.fill(0.0);
        self.tc_prev = 0.0;
        self.jitter_cur.fill(0.0);
        self.jitter_step.fill(0.0);
        self.jitter_count.fill(0);
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

    #[test]
    fn t60_shelf_hits_the_target_decay() {
        // Flat T60 = 2 s: energy must drop ~30 dB per second.
        let mut fdn = Fdn::new(&LENGTHS, MixMatrix::Householder);
        fdn.set_t60(2.0, 2.0, 48000.0);
        let mut e_early = 0.0;
        let mut e_late = 0.0;
        for n in 0..144_000 {
            let x = if n == 0 { 1.0 } else { 0.0 };
            let y = fdn.tick(x);
            if (48_000..72_000).contains(&n) {
                e_early += y * y;
            }
            if (96_000..120_000).contains(&n) {
                e_late += y * y;
            }
        }
        // One second apart → −30 dB = 1e-3 energy ratio (±half decade).
        let ratio = e_late / e_early.max(1e-30);
        assert!(
            (3.0e-4..3.0e-3).contains(&ratio),
            "T60=2s decay ratio off: {ratio:e} (want ≈1e-3)"
        );
    }

    #[test]
    fn rotation_mod_does_not_change_decay() {
        // Near-infinite T60 with rotation engaged: the loop must stay
        // lossless-equivalent (energy holds), unlike delay modulation
        // which erodes or grows the tail.
        let run = |rot: bool| -> f64 {
            let mut fdn = Fdn::new(&LENGTHS, MixMatrix::Householder);
            fdn.set_t60(1.0e6, 1.0e6, 48000.0);
            if rot {
                fdn.set_rotation(0.7, 0.3, 48000.0);
            }
            let mut late = 0.0;
            for n in 0..240_000 {
                let x = if n < 100 { 0.5 } else { 0.0 };
                let y = fdn.tick(x);
                if n > 192_000 {
                    late += y * y;
                }
            }
            late
        };
        let still = run(false);
        let rotated = run(true);
        let ratio = rotated / still.max(1e-30);
        assert!(
            (0.5..2.0).contains(&ratio),
            "rotation must preserve loop energy: {ratio}"
        );
    }
}
