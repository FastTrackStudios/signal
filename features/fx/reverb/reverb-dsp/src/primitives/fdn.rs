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

    // ── Decay Rate EQ (opt-in via `set_decay_curve`) ───────────────
    // The Pro-R-style generalization of the T60 shelf
    // (`fx.reverb.decay-eq`): per line i, each active band adds a
    // biquad to the feedback path whose centre gain is
    // Gmid_dB(i)·(1/rate − 1) — i.e. the per-pass loop attenuation the
    // band's decay-time multiplier demands. Boost totals are scaled
    // down per line so the loop gain always keeps a safety margin
    // below unity.
    decay_eq_active: bool,
    decay_eq_on: [bool; crate::algorithm::DECAY_BANDS],
    decay_eq: Vec<[audiocore_dsp::biquad::Biquad; crate::algorithm::DECAY_BANDS]>,

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

    // ── Per-line in-loop shelving EQ (opt-in, CloudSeed-style) ─────
    // Cheap one-pole-based low + high shelf inside every feedback
    // path: tonal color compounds per recirculation (the CloudSeed
    // per-line EQ trick). Boosts are clamped small — a shelf gain in
    // the loop multiplies the per-band loop gain.
    loop_eq_active: bool,
    eq_low_lp: Vec<Lp1>,
    eq_high_lp: Vec<Lp1>,
    eq_low_gain: f64,
    eq_high_gain: f64,

    // ── Vintage reads (opt-in via `set_vintage_reads`) ─────────────
    // Early-'80s texture: a common-mode single-sine chorus on every
    // line (audible pitch undulation — the Classic-voice signature)
    // read with TRUNCATED (non-interpolated) positions, so the sweep
    // grinds out the era's interpolation grain instead of gliding.
    vintage: bool,
    vintage_phase: f64,
    vintage_inc: f64,

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
            decay_eq_active: false,
            decay_eq_on: [false; crate::algorithm::DECAY_BANDS],
            decay_eq: (0..n)
                .map(|_| core::array::from_fn(|_| audiocore_dsp::biquad::Biquad::new()))
                .collect(),
            rot_depth: 0.0,
            rot_inc: 0.0,
            rot_phase: 0.0,
            rot_cs: vec![(1.0, 0.0); n / 2],
            rot_countdown: 0,
            loop_ap: Vec::new(),
            loop_ap_len: vec![0; n],
            loop_ap_coeff: 0.0,
            loop_eq_active: false,
            eq_low_lp: (0..n).map(|_| Lp1::new()).collect(),
            eq_high_lp: (0..n).map(|_| Lp1::new()).collect(),
            eq_low_gain: 1.0,
            eq_high_gain: 1.0,
            vintage: false,
            vintage_phase: 0.0,
            vintage_inc: 0.9 / 48_000.0,
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
    ///   huge T60 for infinite hold. Disable with `clear_t60`.
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

    /// The Decay Rate EQ (`fx.reverb.decay-eq`): shape decay time per
    /// frequency with up to six Bell/Shelf curves of T60 multipliers,
    /// realized as per-line biquads in the feedback path.
    ///
    /// For line i, the per-pass loop attenuation at the reference decay is
    /// `Gmid_dB(i) = −60·Mi/(fs·t60_mid)`; a band whose multiplier is `r`
    /// needs the loop gain at its frequency moved to `Gmid_dB/r`, i.e. a
    /// filter of `Gmid_dB·(1/r − 1)` dB there. Longer decays are boosts
    /// toward (never past) unity: each line's total boost is scaled to keep
    /// a ≥5 % margin of its base attenuation, so the loop cannot run away
    /// however the bands overlap.
    ///
    /// Layered ON TOP of `set_t60` / the legacy path (it multiplies the
    /// loop response); flat bands cost nothing. Disable by passing a curve
    /// with no active band.
    // r[impl fx.reverb.decay-eq]
    pub fn set_decay_curve(
        &mut self,
        t60_mid: f64,
        bands: &[crate::algorithm::DecayBand; crate::algorithm::DECAY_BANDS],
        sample_rate: f64,
    ) {
        use audiocore_dsp::biquad::FilterType;
        let any = bands.iter().any(|b| b.is_active());
        self.decay_eq_active = any;
        if !any {
            return;
        }
        let t60 = t60_mid.max(0.01);
        for i in 0..self.num_lines {
            let mi = self.delay_samples[i] as f64;
            let gmid_db = -60.0 * mi / (sample_rate * t60);
            // First pass: per-band target gains at this line.
            let mut gains = [0.0f64; crate::algorithm::DECAY_BANDS];
            let mut boost_sum = 0.0f64;
            for (b, band) in bands.iter().enumerate() {
                self.decay_eq_on[b] = band.is_active();
                if !band.is_active() {
                    continue;
                }
                let r = band.rate.clamp(0.25, 4.0);
                let g = gmid_db * (1.0 / r - 1.0);
                gains[b] = g;
                if g > 0.0 {
                    boost_sum += g;
                }
            }
            // Keep ≥5 % of the base attenuation however boosts overlap.
            let headroom = -gmid_db * 0.95;
            let scale = if boost_sum > headroom && boost_sum > 0.0 {
                headroom / boost_sum
            } else {
                1.0
            };
            for (b, band) in bands.iter().enumerate() {
                if !band.is_active() {
                    continue;
                }
                let gain_db = if gains[b] > 0.0 {
                    gains[b] * scale
                } else {
                    gains[b]
                };
                let q = band.q.clamp(0.1, 18.0);
                let f = band.freq_hz.clamp(20.0, sample_rate * 0.45);
                let ftype = match band.shape {
                    1 => FilterType::LowShelf { gain_db },
                    2 => FilterType::HighShelf { gain_db },
                    _ => FilterType::Peak { gain_db },
                };
                self.decay_eq[i][b].set(ftype, f, q, sample_rate);
            }
        }
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

    /// Per-line in-loop shelving EQ: `low/high_gain_db` applied below
    /// `low_hz` / above `high_hz` INSIDE every feedback path, so the
    /// color deepens with each pass. Boosts clamp to +2 dB (loop-gain
    /// safety); cuts are free. Both gains 0 dB disables.
    pub fn set_loop_shelves(
        &mut self,
        low_hz: f64,
        low_gain_db: f64,
        high_hz: f64,
        high_gain_db: f64,
        sample_rate: f64,
    ) {
        self.eq_low_gain = 10.0f64.powf(low_gain_db.min(2.0) / 20.0);
        self.eq_high_gain = 10.0f64.powf(high_gain_db.min(2.0) / 20.0);
        self.loop_eq_active =
            (self.eq_low_gain - 1.0).abs() > 1e-3 || (self.eq_high_gain - 1.0).abs() > 1e-3;
        for lp in &mut self.eq_low_lp {
            lp.set_freq(low_hz.clamp(40.0, 2000.0), sample_rate);
        }
        for lp in &mut self.eq_high_lp {
            lp.set_freq(high_hz.clamp(800.0, 12000.0), sample_rate);
        }
    }

    /// Vintage (Classic-voice) read texture: common-mode ~0.9 Hz sine
    /// chorus with truncated reads. Off = clean modern reads.
    pub fn set_vintage_reads(&mut self, on: bool, sample_rate: f64) {
        self.vintage = on;
        self.vintage_inc = 0.9 / sample_rate.max(1.0);
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
        if self.vintage {
            // Classic voice: one shared sine sweeps every line (common-
            // mode = audible chorus), truncated reads grind the sweep.
            self.vintage_phase += self.vintage_inc;
            if self.vintage_phase >= 1.0 {
                self.vintage_phase -= 1.0;
            }
            let sweep = (self.vintage_phase * core::f64::consts::TAU).sin() * 3.5;
            for i in 0..n {
                let pos = (self.delay_samples[i] as f64 + sweep)
                    .clamp(1.0, (self.delays[i].len() - 2) as f64);
                self.feedback[i] = self.delays[i].read(pos as usize);
            }
        } else if self.jitter_depth > 1e-9 {
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

            // Decay Rate EQ: the per-line curve filters, multiplying the
            // loop response so decay time follows the drawn curve
            // (`fx.reverb.decay-eq`).
            if self.decay_eq_active {
                for b in 0..crate::algorithm::DECAY_BANDS {
                    if self.decay_eq_on[b] {
                        sig = self.decay_eq[i][b].tick(sig, 0);
                    }
                }
            }

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

            // Per-line loop shelving EQ (color compounds per pass).
            if self.loop_eq_active {
                let low = self.eq_low_lp[i].tick(sig);
                sig += (self.eq_low_gain - 1.0) * low;
                let lp2 = self.eq_high_lp[i].tick(sig);
                sig += (self.eq_high_gain - 1.0) * (sig - lp2);
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
        for line in &mut self.decay_eq {
            for bq in line.iter_mut() {
                bq.reset();
            }
        }
        for lp in &mut self.eq_low_lp {
            lp.reset();
        }
        for lp in &mut self.eq_high_lp {
            lp.reset();
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
