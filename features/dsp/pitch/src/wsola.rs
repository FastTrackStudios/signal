//! WSOLA — SoundTouch-style pitch shifter.
//!
//! Pitch shifting without tempo change via two-stage processing:
//!
//! 1. **Rate transposer** — cubic-interpolation resampling with anti-alias FIR
//! 2. **TDStretch** — WSOLA time stretching with cross-correlation splice search
//!
//! Processing order minimises intermediate data volume:
//! - Pitch down (ratio ≤ 1): rate-transpose first (fewer samples), then stretch
//! - Pitch up (ratio > 1): stretch first (fewer samples), then rate-transpose
//!
//! Key SoundTouch techniques implemented:
//! - Normalised cross-correlation with centre-bias for optimal splice points
//! - Anti-alias FIR filtering on the rate-transposition stage
//! - Block-based sequence processing with linear overlap-add crossfade
//! - Pending-skip accumulator for drift-free tempo tracking
//!
//! Latency: `base_grain_size` samples (default 1024).
//! Character: Smooth, low-artefact, works well on complex material.

use std::collections::VecDeque;

// ═══════════════════════════════════════════════════════════════════
// Anti-Alias FIR
// ═══════════════════════════════════════════════════════════════════

const AA_LEN: usize = 32;

/// Windowed-sinc low-pass FIR for anti-aliasing the rate transposer.
struct AaFilter {
    coeffs: [f64; AA_LEN],
    buf: [f64; AA_LEN],
    pos: usize,
    bypass: bool,
}

impl AaFilter {
    fn new() -> Self {
        Self {
            coeffs: [0.0; AA_LEN],
            buf: [0.0; AA_LEN],
            pos: 0,
            bypass: true,
        }
    }

    /// Design a low-pass filter with given normalised cutoff (0–0.5).
    fn design(&mut self, cutoff: f64, enable: bool) {
        self.bypass = !enable;
        if !enable {
            return;
        }
        let c = cutoff.clamp(0.01, 0.49);
        let mid = (AA_LEN - 1) as f64 * 0.5;
        let mut sum = 0.0;
        for i in 0..AA_LEN {
            let x = i as f64 - mid;
            let sinc = if x.abs() < 1e-10 {
                2.0 * c
            } else {
                (std::f64::consts::TAU * c * x).sin() / (std::f64::consts::PI * x)
            };
            // Hann window.
            let w = 0.5 * (1.0 - (std::f64::consts::TAU * i as f64 / (AA_LEN - 1) as f64).cos());
            self.coeffs[i] = sinc * w;
            sum += self.coeffs[i];
        }
        if sum.abs() > 1e-12 {
            for c in &mut self.coeffs {
                *c /= sum;
            }
        }
    }

    #[inline(always)]
    fn tick(&mut self, s: f64) -> f64 {
        if self.bypass {
            return s;
        }
        self.buf[self.pos] = s;
        self.pos = (self.pos + 1) % AA_LEN;
        let mut out = 0.0;
        for i in 0..AA_LEN {
            out += self.coeffs[i] * self.buf[(self.pos + i) % AA_LEN];
        }
        out
    }

    fn reset(&mut self) {
        self.buf = [0.0; AA_LEN];
        self.pos = 0;
    }
}

// ═══════════════════════════════════════════════════════════════════
// Rate Transposer
// ═══════════════════════════════════════════════════════════════════

/// Cubic-interpolation resampler with optional anti-alias filtering.
///
/// `rate` > 1 produces fewer output samples (pitch up + speed up).
/// `rate` < 1 produces more output samples (pitch down + speed down).
struct RateTransposer {
    rate: f64,
    frac: f64,
    hist: [f64; 4],
    count: usize,
    aa_pre: AaFilter,
    aa_post: AaFilter,
    output: VecDeque<f64>,
}

impl RateTransposer {
    fn new() -> Self {
        Self {
            rate: 1.0,
            frac: 0.0,
            hist: [0.0; 4],
            count: 0,
            aa_pre: AaFilter::new(),
            aa_post: AaFilter::new(),
            output: VecDeque::with_capacity(4096),
        }
    }

    fn set_rate(&mut self, rate: f64) {
        self.rate = rate;
        // Don't filter when rate is near unity — avoids unnecessary colouring.
        if (rate - 1.0).abs() < 0.05 {
            self.aa_pre.design(0.49, false);
            self.aa_post.design(0.49, false);
        } else if rate > 1.0 {
            // Pitch up: low-pass input to prevent fold-over aliasing.
            self.aa_pre.design(0.5 / rate, true);
            self.aa_post.design(0.49, false);
        } else {
            // Pitch down: low-pass output to remove interpolation aliases.
            self.aa_pre.design(0.49, false);
            self.aa_post.design(0.5 * rate, true);
        }
    }

    /// Push one input sample; may produce zero or more output samples.
    #[inline]
    fn push(&mut self, sample: f64) {
        let s = self.aa_pre.tick(sample);
        self.hist[0] = self.hist[1];
        self.hist[1] = self.hist[2];
        self.hist[2] = self.hist[3];
        self.hist[3] = s;
        self.count += 1;
        if self.count < 4 {
            return;
        }
        // Generate interpolated output at each fractional position within
        // the current input interval.
        while self.frac < 1.0 {
            let t = self.frac;
            let [y0, y1, y2, y3] = self.hist;
            // Catmull-Rom cubic interpolation.
            let out = y1
                + 0.5
                    * t
                    * ((y2 - y0)
                        + t * ((2.0 * y0 - 5.0 * y1 + 4.0 * y2 - y3)
                            + t * (3.0 * (y1 - y2) + y3 - y0)));
            self.output.push_back(self.aa_post.tick(out));
            self.frac += self.rate;
        }
        self.frac -= 1.0;
    }

    fn reset(&mut self) {
        self.frac = 0.0;
        self.hist = [0.0; 4];
        self.count = 0;
        self.aa_pre.reset();
        self.aa_post.reset();
        self.output.clear();
    }
}

// ═══════════════════════════════════════════════════════════════════
// TDStretch (WSOLA core)
// ═══════════════════════════════════════════════════════════════════

/// WSOLA time stretcher with cross-correlation splice search.
///
/// `tempo` > 1 compresses time (fewer output samples per input).
/// `tempo` < 1 stretches time (more output samples per input).
struct TdStretch {
    tempo: f64,

    // Sequence parameters (derived from grain size).
    seq_len: usize,
    seek_len: usize,
    overlap_len: usize,

    // Input accumulation buffer.
    input: Vec<f64>,

    // Previous overlap tail for crossfade with next sequence.
    prev_overlap: Vec<f64>,

    // Output FIFO.
    output: VecDeque<f64>,

    // Fractional skip accumulator — tracks how many input samples to
    // consume between sequences, including any deficit from previous
    // cycles where there wasn't enough input to drain fully.
    pending_skip: f64,

    is_first: bool,
}

impl TdStretch {
    fn new() -> Self {
        Self {
            tempo: 1.0,
            seq_len: 1024,
            seek_len: 256,
            overlap_len: 128,
            input: Vec::with_capacity(8192),
            prev_overlap: vec![0.0; 128],
            output: VecDeque::with_capacity(8192),
            pending_skip: 0.0,
            is_first: true,
        }
    }

    /// Configure sequence parameters from a base grain size.
    /// `seq_len` = grain, `seek_len` = grain/4, `overlap_len` = grain/8.
    fn configure(&mut self, grain: usize) {
        self.seq_len = grain.max(32);
        self.seek_len = (grain / 4).max(8);
        self.overlap_len = (grain / 8).max(4);
        self.prev_overlap.resize(self.overlap_len, 0.0);
        self.prev_overlap.fill(0.0);
    }

    #[inline]
    fn push(&mut self, s: f64) {
        self.input.push(s);
    }

    /// Process accumulated input, emitting WSOLA output sequences.
    fn process(&mut self) {
        let min_input = self.seq_len + self.seek_len;

        loop {
            // Drain any pending skip carried over from a previous cycle
            // where the desired skip exceeded available input.
            if self.pending_skip >= 1.0 {
                let skip = self.pending_skip as usize;
                let drain = skip.min(self.input.len());
                if drain > 0 {
                    self.input.drain(..drain);
                }
                self.pending_skip -= drain as f64;
            }

            if self.input.len() < min_input {
                break;
            }

            if self.is_first {
                // First sequence: copy directly (no previous overlap to crossfade).
                let copy_end = self.seq_len - self.overlap_len;
                self.output.extend(&self.input[..copy_end]);
                self.prev_overlap
                    .copy_from_slice(&self.input[copy_end..self.seq_len]);
                self.is_first = false;
            } else {
                // Find best splice point via centre-biased cross-correlation.
                let best = self.find_best_offset();

                // Linear crossfade: prev overlap tail → new sequence start.
                let inv = 1.0 / self.overlap_len as f64;
                for i in 0..self.overlap_len {
                    let t = i as f64 * inv;
                    self.output
                        .push_back(self.prev_overlap[i] * (1.0 - t) + self.input[best + i] * t);
                }

                // Copy middle (non-overlapping) section directly.
                let mid_start = best + self.overlap_len;
                let mid_end = best + self.seq_len - self.overlap_len;
                self.output.extend(&self.input[mid_start..mid_end]);

                // Save new tail for next crossfade.
                let tail_start = best + self.seq_len - self.overlap_len;
                self.prev_overlap
                    .copy_from_slice(&self.input[tail_start..tail_start + self.overlap_len]);
            }

            // Accumulate tempo-scaled skip for input advancement.
            self.pending_skip += self.tempo * (self.seq_len - self.overlap_len) as f64;
        }
    }

    /// Normalised cross-correlation search with centre bias (SoundTouch-style).
    ///
    /// Scans `seek_len` candidate positions and returns the offset that best
    /// matches `prev_overlap`, with a slight preference for positions near the
    /// centre of the search window to reduce drift.
    fn find_best_offset(&self) -> usize {
        best_offset(&self.prev_overlap, &self.input, self.seek_len)
    }

    fn reset(&mut self) {
        self.input.clear();
        self.prev_overlap.fill(0.0);
        self.output.clear();
        self.pending_skip = 0.0;
        self.is_first = true;
    }
}

/// Normalised cross-correlation search with centre bias (SoundTouch-style).
///
/// Scans `seek_len` candidate positions in `input` and returns the offset
/// whose first `reference.len()` samples best match `reference`, with a
/// slight preference for positions near the centre of the search window
/// to reduce drift.
fn best_offset(reference: &[f64], input: &[f64], seek_len: usize) -> usize {
    let overlap_len = reference.len();
    // Reference energy is constant across all candidates.
    let e_ref: f64 = reference.iter().map(|s| s * s).sum();
    if e_ref < 1e-12 {
        return seek_len / 2;
    }

    let mid = seek_len as f64 * 0.5;
    let inv_mid = 1.0 / mid.max(1.0);
    let mut best_corr = f64::NEG_INFINITY;
    let mut best_pos = 0;

    for pos in 0..seek_len {
        if pos + overlap_len > input.len() {
            break;
        }
        let mut corr = 0.0f64;
        let mut e_cand = 0.0f64;

        for i in 0..overlap_len {
            let c = input[pos + i];
            corr += reference[i] * c;
            e_cand += c * c;
        }

        let denom = (e_cand * e_ref).sqrt();
        let mut norm = if denom > 1e-12 { corr / denom } else { 0.0 };

        // Centre bias: prefer mid-window positions to reduce drift.
        let d = (pos as f64 - mid) * inv_mid;
        norm *= 1.0 - 0.25 * d * d;

        if norm > best_corr {
            best_corr = norm;
            best_pos = pos;
        }
    }

    best_pos
}

// ═══════════════════════════════════════════════════════════════════
// Offline time-stretch (multichannel)
// ═══════════════════════════════════════════════════════════════════

/// Time-stretch planar audio offline with WSOLA — the [`TdStretch`] core
/// (sequences, centre-biased splice search, linear crossfade), on every
/// channel at once: the splice points are chosen once, from the channels'
/// mix, so a stereo pair stays in phase with itself.
///
/// `channels` holds the input, `start` the frame the output begins at,
/// `tempo` how many input frames each output frame advances (0.5 plays
/// the input at half speed, its pitch kept), `out_len` how many frames to
/// make. `grain` is the sequence length in frames (1024 at 48 kHz in the
/// shifter); it shrinks to fit a short input, which only has so much to
/// splice from. Reads only inside the input.
pub fn stretch_planar(
    channels: &[Vec<f32>],
    start: usize,
    tempo: f64,
    out_len: usize,
    grain: usize,
) -> Vec<Vec<f32>> {
    let mut out: Vec<Vec<f32>> = channels.iter().map(|_| vec![0.0; out_len]).collect();
    let in_len = channels.iter().map(Vec::len).min().unwrap_or(0);
    if in_len == 0 || out_len == 0 {
        return out;
    }
    let start = start.min(in_len - 1);
    // Sequence, search and overlap lengths, as `TdStretch::configure`
    // sets them — shrunk so a sequence and its search fit in the input.
    let seq_len = grain.min(in_len * 4 / 5).max(8);
    let seek_len = (seq_len / 4).max(2);
    let overlap_len = (seq_len / 8).max(2);
    let hop = seq_len - overlap_len;
    let mix: Vec<f64> = (0..in_len)
        .map(|i| channels.iter().map(|c| f64::from(c[i])).sum::<f64>())
        .collect();
    let last_start = in_len.saturating_sub(seq_len);

    // The first sequence as it is.
    let mut at = start.min(last_start);
    let mut written = 0;
    let copy = |out: &mut Vec<Vec<f32>>, from: usize, to_out: usize, n: usize| {
        for (o, c) in out.iter_mut().zip(channels) {
            for i in 0..n {
                if let (Some(slot), Some(&s)) = (o.get_mut(to_out + i), c.get(from + i)) {
                    *slot = s;
                }
            }
        }
    };
    copy(&mut out, at, 0, hop);
    written += hop;
    let mut k = 1usize;
    while written < out_len {
        // Where the input continues naturally from the last sequence, and
        // where the tempo says it should be by now.
        let natural = at + hop;
        let ideal = start as f64 + (k * hop) as f64 * tempo;
        let lo = (ideal - seek_len as f64 / 2.0).round().max(0.0) as usize;
        let lo = lo.min(last_start);
        let reference: Vec<f64> = (0..overlap_len)
            .map(|i| mix.get(natural + i).copied().unwrap_or(0.0))
            .collect();
        let window = &mix[lo..in_len];
        let seek = seek_len.min(last_start - lo + 1);
        let best = lo + best_offset(&reference, window, seek);
        // Crossfade the last sequence's own continuation into the new one,
        // then the new one's middle as it is.
        let inv = 1.0 / overlap_len as f64;
        for (o, c) in out.iter_mut().zip(channels) {
            for i in 0..overlap_len {
                let t = (i as f64 * inv) as f32;
                let from = c.get(natural + i).copied().unwrap_or(0.0);
                let to = c.get(best + i).copied().unwrap_or(0.0);
                if let Some(slot) = o.get_mut(written + i) {
                    *slot = from * (1.0 - t) + to * t;
                }
            }
        }
        copy(
            &mut out,
            best + overlap_len,
            written + overlap_len,
            hop - overlap_len,
        );
        written += hop;
        at = best;
        k += 1;
    }
    out
}

// ═══════════════════════════════════════════════════════════════════
// Public API
// ═══════════════════════════════════════════════════════════════════

pub struct WsolaShifter {
    /// Pitch ratio: 0.5 = octave down, 2.0 = octave up.
    pub speed: f64,
    /// Dry/wet mix (0.0–1.0).
    pub mix: f64,
    /// Base sequence length in samples (at 48 kHz). Scaled to actual sample rate.
    pub base_grain_size: usize,

    transposer: RateTransposer,
    stretcher: TdStretch,

    /// Final output FIFO, consumed one sample per `tick()`.
    output: VecDeque<f64>,

    sample_rate: f64,
    prev_speed: f64,
}

impl WsolaShifter {
    const DEFAULT_GRAIN: usize = 1024;
    // Kept for API compatibility with chain.rs.
    #[allow(dead_code)]
    const DEFAULT_TOLERANCE: usize = 128;
    #[allow(dead_code)]
    const BUF_SECONDS: f64 = 2.0;

    pub fn new() -> Self {
        Self {
            speed: 0.5,
            mix: 1.0,
            base_grain_size: Self::DEFAULT_GRAIN,
            transposer: RateTransposer::new(),
            stretcher: TdStretch::new(),
            output: VecDeque::with_capacity(8192),
            sample_rate: 48000.0,
            prev_speed: 0.0,
        }
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;

        // Scale grain size to actual sample rate.
        let mut grain = ((self.base_grain_size as f64 * sample_rate) / 48000.0) as usize;
        if !grain.is_multiple_of(2) {
            grain += 1;
        }
        grain = grain.max(32);

        self.stretcher.configure(grain);

        let speed = self.speed.clamp(0.25, 4.0);
        self.transposer.set_rate(speed);
        self.stretcher.tempo = 1.0 / speed;
        self.prev_speed = self.speed;
    }

    pub fn reset(&mut self) {
        self.transposer.reset();
        self.stretcher.reset();
        self.output.clear();
        self.prev_speed = 0.0;
    }

    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        // Lazily update rate/tempo when speed changes between ticks.
        if (self.speed - self.prev_speed).abs() > 1e-10 {
            let speed = self.speed.clamp(0.25, 4.0);
            self.transposer.set_rate(speed);
            self.stretcher.tempo = 1.0 / speed;
            self.prev_speed = self.speed;
        }

        if self.speed > 1.0 {
            // Pitch up: stretch first (compresses data), then rate-transpose.
            self.stretcher.push(input);
            self.stretcher.process();
            while let Some(s) = self.stretcher.output.pop_front() {
                self.transposer.push(s);
            }
            while let Some(s) = self.transposer.output.pop_front() {
                self.output.push_back(s);
            }
        } else {
            // Pitch down or unity: rate-transpose first (reduces data), then stretch.
            self.transposer.push(input);
            while let Some(s) = self.transposer.output.pop_front() {
                self.stretcher.push(s);
            }
            self.stretcher.process();
            while let Some(s) = self.stretcher.output.pop_front() {
                self.output.push_back(s);
            }
        }

        // Pop one output sample per input (1:1 steady-state rate).
        // During warmup, returns 0.0 until the pipeline starts producing.
        let wet = self.output.pop_front().unwrap_or(0.0);
        input * (1.0 - self.mix) + wet * self.mix
    }

    pub fn latency(&self) -> usize {
        let mut grain = ((self.base_grain_size as f64 * self.sample_rate) / 48000.0) as usize;
        if !grain.is_multiple_of(2) {
            grain += 1;
        }
        grain.max(32)
    }
}

impl Default for WsolaShifter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    const SR: f64 = 48000.0;

    fn make_wsola() -> WsolaShifter {
        let mut w = WsolaShifter::new();
        w.speed = 0.5;
        w.mix = 1.0;
        w.update(SR);
        w
    }

    #[test]
    fn silence_in_silence_out() {
        let mut w = make_wsola();
        for _ in 0..4800 {
            let out = w.tick(0.0);
            assert!(out.abs() < 1e-6, "Should be silent: {out}");
        }
    }

    #[test]
    fn produces_output_on_sine() {
        let mut w = make_wsola();
        let freq = 220.0;
        let mut energy = 0.0;
        let n = 48000;
        for i in 0..n {
            let input = (2.0 * PI * freq * i as f64 / SR).sin() * 0.5;
            let out = w.tick(input);
            if i > 4096 {
                energy += out * out;
            }
        }
        assert!(energy > 0.1, "Should produce output: energy={energy}");
    }

    #[test]
    fn no_nan() {
        let mut w = make_wsola();
        for i in 0..48000 {
            let input = (2.0 * PI * 82.0 * i as f64 / SR).sin() * 0.9;
            let out = w.tick(input);
            assert!(out.is_finite(), "NaN/Inf at sample {i}");
        }
    }

    #[test]
    fn different_speeds_differ() {
        let freq = 220.0;
        let n = 9600;
        let collect = |speed: f64| -> Vec<f64> {
            let mut w = make_wsola();
            w.speed = speed;
            (0..n)
                .map(|i| {
                    let s = (2.0 * PI * freq * i as f64 / SR).sin() * 0.5;
                    w.tick(s)
                })
                .collect()
        };
        let down = collect(0.5);
        let up = collect(2.0);
        let diff: f64 = down
            .iter()
            .zip(up.iter())
            .map(|(a, b)| (a - b).abs())
            .sum::<f64>()
            / n as f64;
        assert!(diff > 0.001, "Different speeds should differ: {diff}");
    }

    #[test]
    fn dry_wet_mix() {
        let mut w = make_wsola();
        w.mix = 0.0;
        for i in 0..4800 {
            let input = (2.0 * PI * 440.0 * i as f64 / SR).sin() * 0.5;
            let out = w.tick(input);
            assert!((out - input).abs() < 1e-10, "Mix=0 should pass dry");
        }
    }

    /// Measure pitch using YIN CMND (finds first local minimum below threshold).
    fn measure_pitch(signal: &[f64], sample_rate: f64) -> f64 {
        let min_period = (sample_rate / 2000.0) as usize;
        let max_period = (sample_rate / 30.0) as usize;
        let window = signal.len().min(max_period * 10);
        let max_lag = max_period.min(window / 2);
        if max_lag <= min_period {
            return 0.0;
        }
        let threshold = 0.2;
        let mut cmnd = vec![1.0f64; max_lag + 1];
        let mut running_sum = 0.0;
        for lag in 1..=max_lag {
            let mut diff = 0.0;
            let n = window - lag;
            for i in 0..n {
                let d = signal[i] - signal[i + lag];
                diff += d * d;
            }
            running_sum += diff;
            cmnd[lag] = if running_sum > 1e-12 {
                diff * lag as f64 / running_sum
            } else {
                0.0
            };
        }
        for lag in min_period..max_lag {
            if cmnd[lag] < threshold && cmnd[lag + 1] > cmnd[lag] {
                return sample_rate / lag as f64;
            }
        }
        let mut best_lag = min_period;
        let mut best_val = f64::MAX;
        for (lag, &val) in cmnd.iter().enumerate().take(max_lag + 1).skip(min_period) {
            if val < best_val {
                best_val = val;
                best_lag = lag;
            }
        }
        if best_val < 0.5 {
            sample_rate / best_lag as f64
        } else {
            0.0
        }
    }

    #[test]
    fn octave_down_pitch_accuracy() {
        let mut w = make_wsola();
        w.speed = 0.5;
        w.update(SR);
        let freq = 440.0;
        let n = 96000;
        let mut output = Vec::with_capacity(n);
        for i in 0..n {
            let input = (2.0 * PI * freq * i as f64 / SR).sin() * 0.5;
            output.push(w.tick(input));
        }

        // Measure pitch using autocorrelation on the second half (after warmup).
        let start = n / 2;
        let measured_freq = measure_pitch(&output[start..], SR);
        let expected = freq * 0.5;
        let error_cents = 1200.0 * (measured_freq / expected).log2().abs();
        assert!(
            error_cents < 100.0,
            "Octave down should be ~{expected}Hz, got {measured_freq:.1}Hz ({error_cents:.0}c error)"
        );
    }
}

#[cfg(test)]
mod offline_tests {
    use super::stretch_planar;

    /// Zero crossings a second of `x` at `sr`.
    fn crossings(x: &[f32], sr: f64) -> f64 {
        let n = x
            .windows(2)
            .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
            .count();
        n as f64 / (x.len() as f64 / sr)
    }

    #[test]
    fn a_slowed_stretch_keeps_its_pitch_and_its_stereo() {
        let sr = 48_000.0;
        let tone = |f: f64, n: usize| -> Vec<f32> {
            (0..n)
                .map(|i| (std::f64::consts::TAU * f * i as f64 / sr).sin() as f32)
                .collect()
        };
        let input = vec![tone(440.0, 9_600), tone(440.0, 9_600)];
        // 100 ms of source over 250 ms: 0.4 of its speed.
        let out = stretch_planar(&input, 0, 0.4, 12_000, 1024);
        assert_eq!(out[0].len(), 12_000, "as long as asked");
        let rate = crossings(&out[0], sr);
        assert!(
            (rate - 880.0).abs() < 880.0 * 0.03,
            "440 Hz still: {rate:.0} crossings a second"
        );
        // The same splices on both channels: still one signal.
        assert_eq!(out[0], out[1], "the pair stays in phase");
    }

    #[test]
    fn a_short_input_is_stretched_from_what_it_has() {
        // 10 ms of a decay drawn over 50 ms: never reads past it.
        let input = vec![vec![0.5_f32; 480]];
        let out = stretch_planar(&input, 0, 0.2, 2_400, 1024);
        assert_eq!(out[0].len(), 2_400);
        assert!(
            out[0].iter().all(|s| (s - 0.5).abs() < 1e-6),
            "only the input's own audio"
        );
    }
}
