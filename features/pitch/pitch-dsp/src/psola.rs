//! PSOLA — Pitch-Synchronous Overlap-Add for monophonic pitch shifting.
//!
//! Detects pitch via YIN, then uses dual crossfading read heads with
//! pitch-aware cross-correlation alignment at grain boundaries. Uses smaller
//! grains than WSOLA (512 vs 1024) and adapts xcorr tolerance and tail
//! length to the detected pitch period.
//!
//! Latency: `grain_size` samples (default 1024).
//! Character: Most natural for monophonic sources, preserves formants.

use audiocore_dsp::delay_line::DelayLine;

pub struct PsolaShifter {
    pub speed: f64,
    pub mix: f64,
    pub base_window_size: usize,

    analysis_buf: DelayLine,

    pub(crate) detected_period: usize,
    min_period: usize,
    max_period: usize,

    /// Read offset for grain A (samples behind write head).
    offset_a: f64,
    /// Read offset for grain B (offset by half a grain).
    offset_b: f64,
    /// Phase within the current grain cycle (0.0–1.0).
    grain_phase: f64,
    /// Phase increment per sample.
    phase_inc: f64,
    /// Grain size in samples.
    grain_size: usize,

    /// Previous grain tail for cross-correlation matching.
    prev_tail_a: Vec<f64>,
    prev_tail_b: Vec<f64>,

    write_count: usize,
    detect_countdown: usize,
    detect_interval: usize,

    sample_rate: f64,
    window_size: usize,
}

impl PsolaShifter {
    const DEFAULT_WINDOW: usize = 2048;
    const DEFAULT_GRAIN: usize = 1024;
    const DEFAULT_TOLERANCE: usize = 128;
    const BUF_SECONDS: f64 = 2.0;

    pub fn new() -> Self {
        let grain_size = Self::DEFAULT_GRAIN;
        // As `update` sizes it at 48 kHz, so preparing there keeps this one
        // rather than allocating a second.
        let buf_len = 48000 * 2 + grain_size + Self::DEFAULT_TOLERANCE * 2 + 64;
        Self {
            speed: 0.5,
            mix: 1.0,
            base_window_size: Self::DEFAULT_WINDOW,
            analysis_buf: DelayLine::new(buf_len),
            detected_period: 0,
            min_period: 24,
            max_period: 1200,
            offset_a: grain_size as f64,
            offset_b: grain_size as f64 * 1.5,
            grain_phase: 0.0,
            phase_inc: 1.0 / grain_size as f64,
            grain_size,
            prev_tail_a: vec![0.0; grain_size / 4],
            prev_tail_b: vec![0.0; grain_size / 4],
            write_count: 0,
            detect_countdown: 0,
            detect_interval: 512,
            sample_rate: 48000.0,
            window_size: Self::DEFAULT_WINDOW,
        }
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.min_period = (sample_rate / 2000.0).max(2.0) as usize;
        self.max_period = (sample_rate / 40.0) as usize;

        // Scale grain size to sample rate.
        self.grain_size = ((Self::DEFAULT_GRAIN as f64 * sample_rate) / 48000.0) as usize;
        if !self.grain_size.is_multiple_of(2) {
            self.grain_size += 1;
        }

        let buf_len = (sample_rate * Self::BUF_SECONDS) as usize
            + self.grain_size
            + Self::DEFAULT_TOLERANCE * 2
            + 64;
        if self.analysis_buf.len() < buf_len {
            self.analysis_buf = DelayLine::new(buf_len);
        }

        self.phase_inc = 1.0 / self.grain_size as f64;
        self.offset_a = self.grain_size as f64;
        self.offset_b = self.grain_size as f64 * 1.5;
        self.detect_interval = 512.min(self.max_period);
        self.window_size = self.base_window_size;

        let tail_len = self.grain_size / 4;
        self.prev_tail_a.resize(tail_len, 0.0);
        self.prev_tail_b.resize(tail_len, 0.0);
    }

    pub fn reset(&mut self) {
        self.analysis_buf.clear();
        self.detected_period = 0;
        self.offset_a = self.grain_size as f64;
        self.offset_b = self.grain_size as f64 * 1.5;
        self.grain_phase = 0.0;
        self.prev_tail_a.fill(0.0);
        self.prev_tail_b.fill(0.0);
        self.write_count = 0;
        self.detect_countdown = 0;
    }

    /// sin²(π * phase) crossfade.
    #[inline]
    fn crossfade(phase: f64) -> f64 {
        let s = (std::f64::consts::PI * phase).sin();
        s * s
    }

    /// YIN pitch detection. Returns period in samples, or 0 if unvoiced.
    fn detect_pitch(&self) -> usize {
        let window = self.window_size.min(self.max_period * 2 + self.min_period);
        if self.write_count < window {
            return 0;
        }

        let max_lag = self.max_period.min(window / 2);
        let threshold = 0.15;

        let mut cmnd_vals = vec![1.0f64; max_lag + 1];
        let mut running_sum = 0.0f64;

        #[allow(clippy::needless_range_loop)]
        for lag in 1..=max_lag {
            let mut diff = 0.0f64;
            let n = window - lag;
            for i in 0..n {
                let a = self.analysis_buf.read(i + 1);
                let b = self.analysis_buf.read(i + lag + 1);
                let d = a - b;
                diff += d * d;
            }
            running_sum += diff;
            cmnd_vals[lag] = if running_sum > 1e-12 {
                diff * lag as f64 / running_sum
            } else {
                0.0
            };
        }

        let mut best_lag = 0usize;
        let mut best_val = f64::MAX;

        for lag in self.min_period..max_lag {
            let val = cmnd_vals[lag];
            if val < best_val {
                best_val = val;
                best_lag = lag;
            }
            if val < threshold && cmnd_vals[lag + 1] > val {
                return lag;
            }
        }

        if best_val < 0.5 && best_lag >= self.min_period {
            best_lag
        } else {
            0
        }
    }

    /// Find best read offset near `nominal` using cross-correlation with `tail`.
    /// When pitch is detected, searches over a full pitch period for optimal
    /// pitch-mark alignment — the key differentiator from WSOLA.
    fn best_offset(&self, nominal: f64, tail: &[f64]) -> f64 {
        let tolerance = if self.detected_period > 0 {
            self.detected_period as isize
        } else {
            Self::DEFAULT_TOLERANCE as isize
        };
        let tail_len = tail.len();
        let buf_len = self.analysis_buf.len();

        let mut energy_tail = 0.0f64;
        for s in tail {
            energy_tail += s * s;
        }
        if energy_tail < 1e-12 {
            return nominal;
        }

        let mut best_corr = f64::NEG_INFINITY;
        let mut best_delta: isize = 0;

        for delta in -tolerance..=tolerance {
            let candidate = nominal + delta as f64;
            if candidate < 1.0 || (candidate as usize + tail_len) >= buf_len {
                continue;
            }

            let mut correlation = 0.0f64;
            let mut energy_cand = 0.0f64;

            #[allow(clippy::needless_range_loop)]
            for i in 0..tail_len {
                let s = self.analysis_buf.read(candidate as usize + i);
                correlation += tail[i] * s;
                energy_cand += s * s;
            }

            let denom = (energy_tail * energy_cand).sqrt();
            let norm_corr = if denom > 1e-12 {
                correlation / denom
            } else {
                0.0
            };

            if norm_corr > best_corr {
                best_corr = norm_corr;
                best_delta = delta;
            }
        }

        nominal + best_delta as f64
    }

    /// Save a short tail of audio at `offset` for future cross-correlation.
    fn save_tail(&self, offset: f64, tail: &mut [f64]) {
        let buf_len = self.analysis_buf.len();
        for (i, t) in tail.iter_mut().enumerate() {
            let pos = offset as usize + i;
            if pos > 0 && pos < buf_len {
                *t = self.analysis_buf.read(pos);
            } else {
                *t = 0.0;
            }
        }
    }

    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        self.analysis_buf.write(input);
        self.write_count += 1;

        // Periodic pitch detection.
        if self.detect_countdown == 0 {
            self.detected_period = self.detect_pitch();
            self.detect_countdown = self.detect_interval;
        }
        self.detect_countdown -= 1;

        let max_offset = self.analysis_buf.len() as f64 - 4.0;
        let drift = 1.0 - self.speed;

        // Advance read heads.
        self.offset_a += drift;
        self.offset_b += drift;

        // Advance phase.
        let prev_phase = self.grain_phase;
        self.grain_phase += self.phase_inc;

        // Pitch-adaptive tail length: one full pitch period when detected,
        // otherwise grain_size/4 (same as WSOLA default).
        let tail_len = if self.detected_period > 0 {
            self.detected_period
        } else {
            self.grain_size / 4
        };

        // At each grain boundary, RECENTER the head: the per-sample
        // drift IS the pitch shift, so the boundary jump must move by
        // a whole number of pitch periods (phase-neutral) back toward
        // the nominal offset — otherwise the head saturates at the
        // buffer edge and the shift collapses (the pre-fix bug:
        // correlating a stale tail against a constant nominal never
        // recentered; 452 Hz at −0.47 st came out at 469 Hz).
        // Phase reference = a tail saved from the CURRENT drifted
        // position; the correlation then fine-aligns around the
        // period-multiple recenter target. Tail buffers are reused in
        // place — steady-state boundaries are allocation-free.
        if self.grain_phase >= 1.0 {
            self.grain_phase -= 1.0;
            let mut tail = core::mem::take(&mut self.prev_tail_a);
            tail.resize(tail_len, 0.0);
            self.save_tail(self.offset_a, &mut tail);
            let nominal = self.grain_size as f64;
            let period = if self.detected_period > 0 {
                self.detected_period as f64
            } else {
                tail_len.max(1) as f64
            };
            let jump = ((self.offset_a - nominal) / period).round() * period;
            let target = (self.offset_a - jump).clamp(1.0, max_offset);
            let best = self.best_offset(target, &tail);
            self.offset_a = best.clamp(1.0, max_offset);
            self.prev_tail_a = tail;
        }

        // Head B: same, half a grain later.
        if prev_phase < 0.5 && self.grain_phase >= 0.5 {
            let mut tail = core::mem::take(&mut self.prev_tail_b);
            tail.resize(tail_len, 0.0);
            self.save_tail(self.offset_b, &mut tail);
            let nominal = self.grain_size as f64;
            let period = if self.detected_period > 0 {
                self.detected_period as f64
            } else {
                tail_len.max(1) as f64
            };
            let jump = ((self.offset_b - nominal) / period).round() * period;
            let target = (self.offset_b - jump).clamp(1.0, max_offset);
            let best = self.best_offset(target, &tail);
            self.offset_b = best.clamp(1.0, max_offset);
            self.prev_tail_b = tail;
        }

        // Clamp offsets.
        self.offset_a = self.offset_a.clamp(1.0, max_offset);
        self.offset_b = self.offset_b.clamp(1.0, max_offset);

        // Read grains with cubic interpolation.
        let a = self.analysis_buf.read_cubic(self.offset_a);
        let b = self.analysis_buf.read_cubic(self.offset_b);

        // Crossfade.
        let win_a = Self::crossfade(self.grain_phase);
        let win_b = Self::crossfade((self.grain_phase + 0.5).fract());

        let wet = a * win_a + b * win_b;

        input * (1.0 - self.mix) + wet * self.mix
    }

    pub fn latency(&self) -> usize {
        self.grain_size
    }
}

impl Default for PsolaShifter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    const SR: f64 = 48000.0;

    fn make_psola() -> PsolaShifter {
        let mut p = PsolaShifter::new();
        p.speed = 0.5;
        p.mix = 1.0;
        p.update(SR);
        p
    }

    /// Measure pitch using YIN CMND over a large window.
    #[allow(dead_code)]
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
    fn silence_in_silence_out() {
        let mut p = make_psola();
        for _ in 0..4800 {
            let out = p.tick(0.0);
            assert!(out.abs() < 1e-6, "Should be silent: {out}");
        }
    }

    #[test]
    fn produces_output_on_sine() {
        let mut p = make_psola();
        let freq = 220.0;
        let mut energy = 0.0;
        let n = 48000;
        for i in 0..n {
            let input = (2.0 * PI * freq * i as f64 / SR).sin() * 0.5;
            let out = p.tick(input);
            if i > 4096 {
                energy += out * out;
            }
        }
        assert!(energy > 0.1, "Should produce output: energy={energy}");
    }

    #[test]
    fn no_nan() {
        let mut p = make_psola();
        for i in 0..48000 {
            let input = (2.0 * PI * 82.0 * i as f64 / SR).sin() * 0.9;
            let out = p.tick(input);
            assert!(out.is_finite(), "NaN at sample {i}");
        }
    }

    #[test]
    fn detects_pitch() {
        let mut p = make_psola();
        let freq = 440.0;
        for i in 0..4800 {
            let input = (2.0 * PI * freq * i as f64 / SR).sin() * 0.8;
            p.tick(input);
        }
        let expected_period = (SR / freq).round() as usize;
        assert!(p.detected_period > 0, "Should detect pitch");
        assert!(
            (p.detected_period as f64 - expected_period as f64).abs() < 5.0,
            "Period should be ~{expected_period}, got {}",
            p.detected_period
        );
    }

    #[test]
    fn unvoiced_detection_for_noise() {
        let mut p = make_psola();
        let mut rng = 12345u64;
        for _ in 0..9600 {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
            let noise = (rng as f64 / u64::MAX as f64) * 2.0 - 1.0;
            p.tick(noise * 0.5);
        }
    }

    #[test]
    fn different_speeds_differ() {
        let freq = 220.0;
        let n = 9600;
        let collect = |speed: f64| -> Vec<f64> {
            let mut p = make_psola();
            p.speed = speed;
            (0..n)
                .map(|i| {
                    let s = (2.0 * PI * freq * i as f64 / SR).sin() * 0.5;
                    p.tick(s)
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
        let mut p = make_psola();
        p.mix = 0.0;
        for i in 0..4800 {
            let input = (2.0 * PI * 440.0 * i as f64 / SR).sin() * 0.5;
            let out = p.tick(input);
            assert!((out - input).abs() < 1e-10, "Mix=0 should pass dry");
        }
    }

    #[test]
    fn octave_down_pitch_accuracy() {
        let mut p = make_psola();
        p.speed = 0.5;
        p.update(SR);
        let freq = 440.0;
        let n = 96000;
        let mut output = Vec::with_capacity(n);
        for i in 0..n {
            let input = (2.0 * PI * freq * i as f64 / SR).sin() * 0.5;
            output.push(p.tick(input));
        }

        let start = n / 2;
        // Zero-crossing measure: the CMND autocorrelation is fooled by
        // faint grain-rate AM into a subharmonic lag (reported 44 Hz on
        // audio whose actual fundamental measures a clean 220 Hz).
        let late = &output[start..];
        let mut crossings = 0;
        for i in 1..late.len() {
            if late[i - 1] < 0.0 && late[i] >= 0.0 {
                crossings += 1;
            }
        }
        let measured_freq = crossings as f64 * SR / late.len() as f64;
        let expected = freq * 0.5;
        let error_cents = 1200.0 * (measured_freq / expected).log2().abs();
        assert!(
            error_cents < 100.0,
            "Octave down should be ~{expected}Hz, got {measured_freq:.1}Hz ({error_cents:.0}c error)"
        );
    }
}
