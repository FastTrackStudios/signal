//! Feedback Delay Network (FDN) — the workhorse of Room and Hall reverbs.
//!
//! N parallel delay lines mixed through a unitary feedback matrix
//! (Householder or Hadamard) with per-line damping filters.

use dsp_core::num;

use audiocore_dsp::dc_blocker::DcBlocker;
use audiocore_dsp::delay_line::DelayLine;
use audiocore_dsp::denormal::flush;
use audiocore_dsp::prng::XorShift32;

use super::householder;
use super::one_pole::Lp1;

/// Mixing matrix type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixMatrix {
    Householder,
    Hadamard,
}

/// Generic FDN with N delay lines.
/// Everything one delay line of the network owns.
///
/// This replaced sixteen parallel `Vec`s indexed by line number. The network
/// is the core primitive under most of the algorithms in this crate, and the
/// parallel form meant that adding a per-line term — a shelf, a jitter
/// counter, a decay EQ — took sixteen edits to keep aligned and gave the
/// compiler no way to notice when one was missed.
struct Line {
    delay: DelayLine,
    /// Read distance in samples, clamped to the line's own length.
    delay_samples: usize,
    damping: Lp1,
    dc_blocker: DcBlocker,
    band_split: Lp1,
    /// One-pole decay shelf: gain, pole, and its running state.
    shelf_g: f64,
    shelf_p: f64,
    shelf_state: f64,
    decay_eq: [audiocore_dsp::biquad::Biquad; crate::algorithm::DECAY_BANDS],
    /// Length of this line's in-loop allpass, when one is fitted.
    loop_ap_len: usize,
    eq_low_lp: Lp1,
    eq_high_lp: Lp1,
    /// Random-walk delay modulation: current offset, step, and samples left
    /// before a new step is drawn.
    jitter_cur: f64,
    jitter_step: f64,
    jitter_count: u32,
}

impl Line {
    fn new(len: usize) -> Self {
        Self {
            delay: DelayLine::new(len.saturating_add(1)),
            delay_samples: len,
            damping: Lp1::new(),
            dc_blocker: DcBlocker::new(),
            band_split: Lp1::new(),
            shelf_g: 0.0,
            shelf_p: 0.0,
            shelf_state: 0.0,
            decay_eq: core::array::from_fn(|_| audiocore_dsp::biquad::Biquad::new()),
            loop_ap_len: 0,
            eq_low_lp: Lp1::new(),
            eq_high_lp: Lp1::new(),
            jitter_cur: 0.0,
            jitter_step: 0.0,
            jitter_count: 0,
        }
    }
}

/// Which optional in-loop stages are active. One struct rather than
/// three loose `*_active` bools spread down the field list.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
struct ActiveStages {
    /// Split decay: low and high bands decay at different rates.
    band_split: bool,
    /// The drawn per-band decay-rate curve.
    decay_eq: bool,
    /// The per-line in-loop shelving EQ.
    loop_eq: bool,
}

pub struct Fdn {
    /// Which optional in-loop stages are engaged.
    active: ActiveStages,
    /// One entry per delay line.
    lines: Vec<Line>,
    /// Each line's last output, kept as its own vector because the mixing
    /// matrix operates on it as a contiguous slice — that is the one piece of
    /// per-line state the network treats as a vector rather than per line.
    feedback: Vec<f64>,
    decay_gain: f64,    // Overall decay multiplier
    mix_matrix: MixMatrix,
    num_lines: usize,
    // 2-band decay control: split feedback into low/high via one-pole
    // crossover, multiply each by its own decay coefficient. Default
    // (1.0, 1.0) is a no-op.
    low_decay_mult: f64,
    high_decay_mult: f64,

    // ── Jot per-line T60 shelf (opt-in via `set_t60`) ──────────────
    // Exact frequency-dependent decay: per line i with length Mi,
    // R0 = 10^(−3·Mi/(fs·T60_dc)), Rπ = 10^(−3·Mi/(fs·T60_nyq)),
    // pole p = (R0−Rπ)/(R0+Rπ), gain g = 2·R0·Rπ/(R0+Rπ). Replaces
    // decay_gain + damping + band_split when active, and applies Jot's
    // tonal-correction one-zero on the summed output so decay changes
    // don't recolor the wet spectrum.
    t60_mode: bool,
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
    decay_eq_on: [bool; crate::algorithm::DECAY_BANDS],

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
    loop_ap_coeff: f64,

    // ── Per-line in-loop shelving EQ (opt-in, CloudSeed-style) ─────
    // Cheap one-pole-based low + high shelf inside every feedback
    // path: tonal color compounds per recirculation (the CloudSeed
    // per-line EQ trick). Boosts are clamped small — a shelf gain in
    // the loop multiplies the per-band loop gain.
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
    jitter_rng: XorShift32,
}

impl Fdn {
    /// Create an FDN with the given delay lengths (in samples).
    #[must_use]
    pub fn new(delay_lengths: &[usize], matrix: MixMatrix) -> Self {
        let n = delay_lengths.len();
        let lines = delay_lengths.iter().map(|&len| Line::new(len)).collect();
        let feedback = vec![0.0; n];

        Self {
            active: ActiveStages::default(),
            lines,
            feedback,
            decay_gain: 0.85,
            mix_matrix: matrix,
            num_lines: n,
            low_decay_mult: 1.0,
            high_decay_mult: 1.0,
            t60_mode: false,
            tc_b: 0.0,
            tc_prev: 0.0,
            decay_eq_on: [false; crate::algorithm::DECAY_BANDS],
            rot_depth: 0.0,
            rot_inc: 0.0,
            rot_phase: 0.0,
            rot_cs: vec![(1.0, 0.0); n / 2],
            rot_countdown: 0,
            loop_ap: Vec::new(),
            loop_ap_coeff: 0.0,
            eq_low_gain: 1.0,
            eq_high_gain: 1.0,
            vintage: false,
            vintage_phase: 0.0,
            vintage_inc: 0.9 / 48_000.0,
            jitter_depth: 0.0,
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
        self.active.band_split = (low_mult - 1.0).abs() > 1e-4 || (high_mult - 1.0).abs() > 1e-4;
        for line in &mut self.lines {
            let b = &mut line.band_split;
            b.set_freq(crossover_hz, sample_rate);
        }
    }

    /// Set all delay lengths (in samples). Must match the number of lines.
    pub fn set_delays(&mut self, lengths: &[usize]) {
        for (line, &len) in self.lines.iter_mut().zip(lengths).take(self.num_lines) {
            line.delay_samples = len.min(line.delay.len().saturating_sub(1));
        }
    }

    /// Set the damping filter cutoff for all lines.
    pub fn set_damping(&mut self, freq_hz: f64, sample_rate: f64) {
        for line in &mut self.lines {
            let d = &mut line.damping;
            d.set_freq(freq_hz, sample_rate);
        }
        // Re-tune the in-loop DC blockers while we have the sample rate.
        //
        // The corner has to be far lower than audibility suggests, because
        // this filter sits INSIDE the feedback loop and its attenuation
        // compounds once per recirculation. A 10 Hz corner costs only 0.11 dB
        // at 62 Hz — inaudible in isolation — but a 3 s tail on ~10 ms delay
        // lines recirculates nearly 300 times, so the bottom octave arrives
        // some 30 dB down on the rest of the spectrum. That showed up as our
        // 62 Hz band decaying *shorter* than 125 Hz even with the Decay Rate
        // EQ boosting it as hard as it could.
        //
        // 3 Hz costs 0.018 dB a pass, about 5 dB over the same tail, and still
        // blocks the subsonic offset that long feedback paths accumulate.
        for line in &mut self.lines {
            let dc = &mut line.dc_blocker;
            dc.set_cutoff(3.0, sample_rate);
        }
    }

    /// Set the damping coefficient directly (0.0 = no damping, 1.0 = max).
    pub fn set_damping_coeff(&mut self, g: f64) {
        for line in &mut self.lines {
            let d = &mut line.damping;
            d.set_coeff(g);
        }
    }

    /// Set the overall decay gain (0.0 = no feedback, 1.0 = infinite).
    pub const fn set_decay(&mut self, gain: f64) {
        self.decay_gain = gain.clamp(0.0, 0.999);
    }

    /// Switch to exact Jot per-line T60 decay: `t60_dc` / `t60_nyq`
    /// seconds at DC and Nyquist. Replaces `set_decay` + `set_damping`
    /// + `set_band_decay` (those become inert while active); pass a
    ///   huge T60 for infinite hold. Disable with `clear_t60`.
    pub fn set_t60(&mut self, t60_dc: f64, t60_nyq: f64, sample_rate: f64) {
        let t_dc = t60_dc.max(0.01);
        let t_ny = t60_nyq.max(0.01);
        for line in self.lines.iter_mut().take(self.num_lines) {
            // The loop length is the delay line PLUS any in-loop allpass:
            // that allpass sits inside the feedback path, so it lengthens
            // every recirculation. Ignoring it makes the tail run long —
            // measurably so, ~1.16x for Hall's 0.6 coefficient and ~2x for
            // the Room engines once they were given the same diffusion.
            let total_len = line.delay_samples.saturating_add(line.loop_ap_len);
            let mi = num::count_to_f64(total_len);
            let r0 = 10.0f64.powf(-3.0 * mi / (sample_rate * t_dc));
            let rp = 10.0f64.powf(-3.0 * mi / (sample_rate * t_ny));
            line.shelf_p = (r0 - rp) / (r0 + rp);
            line.shelf_g = 2.0 * r0 * rp / (r0 + rp);
        }
        // Tonal correction: |E(ω)|² ∝ 1/T60(ω) via a one-zero.
        let alpha = (t_ny / t_dc).clamp(0.05, 20.0);
        self.tc_b = (1.0 - alpha) / (1.0 + alpha);
        self.t60_mode = true;
    }

    pub const fn clear_t60(&mut self) {
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
        const PROBE_POINTS: usize = 48;
        use audiocore_dsp::biquad::FilterType;
        let any = bands.iter().any(crate::algorithm::DecayBand::is_active);
        self.active.decay_eq = any;
        if !any {
            return;
        }
        let t60 = t60_mid.max(0.01);
        let decay_eq_on = &mut self.decay_eq_on;
        for line in self.lines.iter_mut().take(self.num_lines) {
            let mi = num::count_to_f64(line.delay_samples);
            let gmid_db = -60.0 * mi / (sample_rate * t60);
            // First pass: per-band target gains at this line.
            let mut gains = [0.0f64; crate::algorithm::DECAY_BANDS];

            for (on, (band, gain)) in decay_eq_on.iter_mut().zip(bands.iter().zip(&mut gains)) {
                *on = band.is_active();
                if !band.is_active() {
                    continue;
                }
                let r = band.rate.clamp(
                    crate::algorithm::DECAY_RATE_MIN,
                    crate::algorithm::DECAY_RATE_MAX,
                );
                let g = gmid_db * (1.0 / r - 1.0);
                *gain = g;
            }
            // Keep >=5 % of the base attenuation however boosts overlap.
            //
            // The constraint is on the loop gain at each FREQUENCY, so what
            // matters is the largest combined boost anywhere in the spectrum —
            // not the sum of every band's peak. Summing peaks treats bands an
            // octave apart as though they stacked, which they do not: a curve
            // lifting six separate bands got scaled down to a fraction of what
            // it asked for, and a bass-heavy chamber could not be matched at
            // any length because its low bands were pinned at the ceiling.
            //
            // Evaluating the real response over a log grid costs a few hundred
            // multiplies per parameter change, off the audio thread.
            let headroom = -gmid_db * 0.95;
            let mut peak_boost = 0.0f64;
            let f_lo = 20.0f64;
            let f_hi = (sample_rate * 0.45).max(f_lo * 2.0);
            let ratio = (f_hi / f_lo).powf(1.0 / num::count_to_f64(PROBE_POINTS - 1));
            let mut f = f_lo;
            for _ in 0..PROBE_POINTS {
                let mut total = 0.0f64;
                for (band, gain) in bands.iter().zip(&gains) {
                    if band.is_active() && gain > &0.0 {
                        total += gain * band.shape_weight_at(f);
                    }
                }
                if total > peak_boost {
                    peak_boost = total;
                }
                f *= ratio;
            }
            let scale = if peak_boost > headroom && peak_boost > 0.0 {
                headroom / peak_boost
            } else {
                1.0
            };
            let curves = line.decay_eq.iter_mut().zip(bands.iter());
            for ((filter, band), gain) in curves.zip(&gains) {
                if !band.is_active() {
                    continue;
                }
                let gain_db = if gain > &0.0 {
                    gain * scale
                } else {
                    *gain
                };
                let q = band.q.clamp(0.1, 18.0);
                let f = band.freq_hz.clamp(20.0, sample_rate * 0.45);
                let ftype = match band.shape {
                    1 => FilterType::LowShelf { gain_db },
                    2 => FilterType::HighShelf { gain_db },
                    _ => FilterType::Peak { gain_db },
                };
                filter.set(ftype, f, q, sample_rate);
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
            let loop_ap = &mut self.loop_ap;
            for (i, line) in self.lines.iter_mut().take(self.num_lines).enumerate() {
                // Short prime-ish lengths derived from the line length.
                let len = (line.delay_samples.saturating_div(7))
                    .saturating_add(19)
                    .saturating_add(i.saturating_mul(26))
                    | 1;
                line.loop_ap_len = len;
                loop_ap.push(DelayLine::new(len.saturating_add(4)));
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
        self.active.loop_eq =
            (self.eq_low_gain - 1.0).abs() > 1e-3 || (self.eq_high_gain - 1.0).abs() > 1e-3;
        for line in &mut self.lines {
            let lp = &mut line.eq_low_lp;
            lp.set_freq(low_hz.clamp(40.0, 2000.0), sample_rate);
        }
        for line in &mut self.lines {
            let lp = &mut line.eq_high_lp;
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
    ///
    /// Every walk over the lines pairs `lines` with `feedback` through a
    /// `zip`, so `num_lines` running ahead of either vector shortens the
    /// walk instead of panicking.
    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        let n = self.num_lines;

        self.read_lines(n);

        // Sum output before mixing (tap from raw delay outputs).
        let output_scale = 1.0 / num::count_to_f64(n).sqrt();
        let mut output = 0.0;
        for fb in self.feedback.iter().take(n) {
            output += *fb * output_scale;
        }

        self.mix(n);
        self.recirculate(n, input);

        // Jot tonal correction (one-zero) so T60 changes don't recolor
        // the wet spectrum.
        if self.t60_mode && self.tc_b.abs() > 1e-9 {
            let corrected = self.tc_b.mul_add(-self.tc_prev, output) / (1.0 - self.tc_b);
            self.tc_prev = output;
            corrected
        } else {
            output
        }
    }

    pub fn reset(&mut self) {
        for line in &mut self.lines {
            let d = &mut line.delay;
            d.clear();
        }
        for line in &mut self.lines {
            let d = &mut line.damping;
            d.reset();
        }
        for line in &mut self.lines {
            let b = &mut line.band_split;
            b.reset();
        }
        for line in &mut self.lines {
            let dc = &mut line.dc_blocker;
            dc.reset();
        }
        for ap in &mut self.loop_ap {
            ap.clear();
        }
        for line in &mut self.lines {
            let line = &mut line.decay_eq;
            for bq in line.iter_mut() {
                bq.reset();
            }
        }
        for line in &mut self.lines {
            let lp = &mut line.eq_low_lp;
            lp.reset();
        }
        for line in &mut self.lines {
            let lp = &mut line.eq_high_lp;
            lp.reset();
        }
        for line in &mut self.lines {
            line.shelf_state = 0.0;
            line.jitter_cur = 0.0;
            line.jitter_step = 0.0;
            line.jitter_count = 0;
        }
        self.tc_prev = 0.0;
        self.feedback.fill(0.0);
    }

    /// Fill `feedback` from each line's delay output — a fractional read
    /// when the vintage sweep or the jitter walk is moving it.
    fn read_lines(&mut self, n: usize) {
        if self.vintage {
            // Classic voice: one shared sine sweeps every line (common-
            // mode = audible chorus), truncated reads grind the sweep.
            self.vintage_phase += self.vintage_inc;
            if self.vintage_phase >= 1.0 {
                self.vintage_phase -= 1.0;
            }
            let sweep = (self.vintage_phase * core::f64::consts::TAU).sin() * 3.5;
            for (line, fb) in self.lines.iter_mut().zip(&mut self.feedback).take(n) {
                let pos = (num::count_to_f64(line.delay_samples) + sweep)
                    .clamp(1.0, num::count_to_f64(line.delay.len().saturating_sub(2)));
                *fb = line.delay.read(num::f64_to_index(pos));
            }
        } else if self.jitter_depth > 1e-9 {
            let rng = &mut self.jitter_rng;
            let depth = self.jitter_depth;
            for (line, fb) in self.lines.iter_mut().zip(&mut self.feedback).take(n) {
                if line.jitter_count == 0 {
                    // New random drift target, glide over 300–1500 samples.
                    let interval =
                        300u32.saturating_add(num::f64_to_u32(rng.next_bipolar().abs() * 1200.0));
                    let target = rng.next_bipolar() * depth;
                    line.jitter_step = (target - line.jitter_cur) / f64::from(interval);
                    line.jitter_count = interval;
                }
                line.jitter_count = line.jitter_count.saturating_sub(1);
                line.jitter_cur += line.jitter_step;
                let pos = (num::count_to_f64(line.delay_samples) + line.jitter_cur)
                    .clamp(1.0, num::count_to_f64(line.delay.len().saturating_sub(2)));
                *fb = line.delay.read_linear(pos);
            }
        } else {
            for (line, fb) in self.lines.iter_mut().zip(&mut self.feedback).take(n) {
                *fb = line.delay.read(line.delay_samples);
            }
        }
    }

    /// Apply the mixing matrix, then the slow Givens rotation between
    /// line pairs.
    fn mix(&mut self, n: usize) {
    // Apply mixing matrix. `n <= feedback.len()` by construction, so
        // the `else` never runs; it just keeps the mix off a panic.
        let matrix = self.mix_matrix;
        if let Some(bus) = self.feedback.get_mut(..n) {
            match matrix {
                // Hadamard requires power of 2 — if not, fall back to Householder
                MixMatrix::Hadamard if n.is_power_of_two() => super::hadamard::mix(bus),
                MixMatrix::Householder | MixMatrix::Hadamard => householder::mix(bus),
            }
        }

        // Slow orthogonal rotation between line pairs (tail animation).
        if self.rot_depth > 1e-9 && n >= 2 {
            if self.rot_countdown == 0 {
                self.rot_countdown = 16;
                self.rot_phase = self.rot_inc.mul_add(16.0, self.rot_phase).fract();
                for (k, cs) in self.rot_cs.iter_mut().enumerate() {
                    let theta = self.rot_depth
                        * (core::f64::consts::TAU * num::count_to_f64(k).mul_add(0.31, self.rot_phase)).sin();
                    *cs = (theta.cos(), theta.sin());
                }
            }
            self.rot_countdown = self.rot_countdown.saturating_sub(1);
            // Chunks of two are the (2k, 2k + 1) pairs the rotation used
            // to address by hand.
            for (pair, &(c, sn)) in self
                .feedback
                .chunks_exact_mut(2)
                .zip(&self.rot_cs)
                .take(n / 2)
            {
                let [first, second] = pair else {
                    continue;
                };
                let (a, b) = (*first, *second);
                *first = c.mul_add(a, -(sn * b));
                *second = sn.mul_add(a, c * b);
            }
        }
    }

    /// Per-line decay, EQ and diffusion, back into each delay line.
    fn recirculate(&mut self, n: usize, input: f64) {
    let Self {
            lines,
            feedback,
            loop_ap,
            decay_eq_on,
            ..
        } = self;
        for (i, (line, fb)) in lines.iter_mut().zip(feedback.iter()).take(n).enumerate() {
            // Per-line decay: exact Jot T60 shelf when engaged,
            // otherwise the legacy damping · decay · band-split path.
            let mut sig = if self.t60_mode {
                let y = line.shelf_g.mul_add(*fb, line.shelf_p * line.shelf_state);
                line.shelf_state = flush(y);
                y
            } else {
                let mut sig = line.damping.tick(*fb) * self.decay_gain;
                if self.active.band_split {
                    let low = line.band_split.tick(sig);
                    let high = sig - low;
                    sig = low.mul_add(self.low_decay_mult, high * self.high_decay_mult);
                }
                sig
            };

            // Decay Rate EQ: the per-line curve filters, multiplying the
            // loop response so decay time follows the drawn curve
            // (`fx.reverb.decay-eq`).
            if self.active.decay_eq {
                for (on, band) in decay_eq_on.iter().zip(line.decay_eq.iter_mut()) {
                    if *on {
                        sig = band.tick(sig, 0);
                    }
                }
            }

            // In-loop allpass: density compounds each recirculation.
            if self.loop_ap_coeff.abs() > 1e-4 {
                let g = if i % 2 == 0 {
                    self.loop_ap_coeff
                } else {
                    -self.loop_ap_coeff
                };
                // `loop_ap` is empty until sized, which is why this is a
                // lookup rather than a fourth arm of the zip.
                if let Some(ap) = loop_ap.get_mut(i) {
                    let delayed = ap.read(line.loop_ap_len);
                    let v = sig - g * delayed;
                    ap.write(v);
                    sig = delayed + g * v;
                }
            }

            // Per-line loop shelving EQ (color compounds per pass).
            if self.active.loop_eq {
                let low = line.eq_low_lp.tick(sig);
                sig += (self.eq_low_gain - 1.0) * low;
                let lp2 = line.eq_high_lp.tick(sig);
                sig += (self.eq_high_gain - 1.0) * (sig - lp2);
            }

            // Block DC in the recirculating path — long tails otherwise
            // accumulate subsonic offset (worst with pitch-shifted or
            // saturated feedback around the FDN).
            sig = line.dc_blocker.tick(sig);
            line.delay.write(flush(input + sig));
        }
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