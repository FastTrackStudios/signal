//! Spectral dynamics engine — per-bin resonance suppression /
//! isolation (ANINA / soothe class, and the per-band "Pro-Q Spectral"
//! behavior when driven with a band mask).
//!
//! STFT (Hann analysis + synthesis, 4× overlap) → per-bin level in dB
//! → compare against a **spectrally smoothed reference** of the same
//! spectrum (relative mode: a bin only triggers when it sticks out of
//! its own spectral neighborhood — that's what makes it a resonance
//! suppressor rather than an EQ) or an absolute threshold → per-bin
//! gain reduction with attack/release smoothing in time → gains applied
//! to both channels' spectra → overlap-add resynthesis.
//!
//! Clean-room design from published behavior only (see
//! `spec/eq-suite-plan.md`): Density sharpens per-bin selectivity,
//! Tilt applies +3 dB/oct to the trigger spectrum (pink-noise
//! normalization, Pro-Q 4's published default), Freeze locks the gain
//! curve, Gate skips near-silent bins, Delta morphs the output from
//! suppression to isolation (listen to what's being removed).

use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpectralParams {
    /// Reduction depth (0..1): scales the per-bin gain reduction.
    pub amount: f64,
    /// Selectivity (0..1): sharpens the over-threshold mask; low =
    /// broad gentle ranges, high = narrow surgical notches.
    pub density: f64,
    /// +3 dB/oct trigger tilt (highs trigger slightly more).
    pub tilt: bool,
    /// Per-bin attack/release in ms.
    pub attack_ms: f64,
    pub release_ms: f64,
    /// Work range: bins outside are untouched.
    pub lo_hz: f64,
    pub hi_hz: f64,
    /// Bins below this absolute level never trigger (keeps the noise
    /// floor from being "suppressed").
    pub gate_db: f64,
    /// Relative mode (default): trigger on bin − smoothed-neighborhood.
    /// Absolute mode: trigger on bin level vs `threshold_db`.
    pub relative: bool,
    /// Threshold: dB **over the smoothed reference** in relative mode
    /// (0 = any prominence triggers; 6 = only strong resonances);
    /// absolute bin dB threshold otherwise.
    pub threshold_db: f64,
    /// Lock the current gain curve (ANINA Freeze).
    pub freeze: bool,
    /// Output morph: 0 = suppression, 1 = isolation (the removed part).
    pub delta: f64,
}

impl Default for SpectralParams {
    fn default() -> Self {
        Self {
            amount: 0.5,
            density: 0.5,
            tilt: true,
            attack_ms: 3.0,
            release_ms: 80.0,
            lo_hz: 80.0,
            hi_hz: 16000.0,
            gate_db: -70.0,
            relative: true,
            threshold_db: 4.0,
            freeze: false,
            delta: 0.0,
        }
    }
}

/// Spectral-neighborhood smoothing width in octaves (each side) for the
/// relative reference — roughly a third-octave view of "expected" level.
const SMOOTH_OCTAVES: f64 = 0.33;

pub struct SpectralEngine {
    pub params: SpectralParams,
    fft: Arc<dyn RealToComplex<f64>>,
    ifft: Arc<dyn ComplexToReal<f64>>,
    block: usize,
    hop: usize,
    window: Vec<f64>,
    /// Input rings (per channel) + pending fill.
    in_buf: [Vec<f64>; 2],
    /// Overlap-add accumulators (per channel).
    ola: [Vec<f64>; 2],
    /// Ready output queue (per channel).
    out_buf: [Vec<f64>; 2],
    fill: usize,
    // Scratch (preallocated).
    frame: Vec<f64>,
    spec: [Vec<Complex<f64>>; 2],
    mag_db: Vec<f64>,
    ref_db: Vec<f64>,
    gr_db: Vec<f64>,
    gain: Vec<f64>,
    attack_coeff: f64,
    release_coeff: f64,
    sample_rate: f64,
    primed: bool,
}

impl SpectralEngine {
    /// `block` must be a power of two (512 / 1024 / 2048).
    pub fn new(sample_rate: f64, block: usize) -> Self {
        let mut planner = RealFftPlanner::<f64>::new();
        let fft = planner.plan_fft_forward(block);
        let ifft = planner.plan_fft_inverse(block);
        let hop = block / 4;
        let window: Vec<f64> = (0..block)
            .map(|i| {
                let t = i as f64 / block as f64;
                0.5 - 0.5 * (core::f64::consts::TAU * t).cos()
            })
            .collect();
        let bins = block / 2 + 1;
        let mut e = Self {
            params: SpectralParams::default(),
            fft,
            ifft,
            block,
            hop,
            window,
            in_buf: [vec![0.0; block], vec![0.0; block]],
            ola: [vec![0.0; block], vec![0.0; block]],
            out_buf: [Vec::with_capacity(block * 2), Vec::with_capacity(block * 2)],
            fill: 0,
            frame: vec![0.0; block],
            spec: [
                vec![Complex::new(0.0, 0.0); bins],
                vec![Complex::new(0.0, 0.0); bins],
            ],
            mag_db: vec![-120.0; bins],
            ref_db: vec![-120.0; bins],
            gr_db: vec![0.0; bins],
            gain: vec![1.0; bins],
            attack_coeff: 1.0,
            release_coeff: 1.0,
            sample_rate,
            primed: false,
        };
        e.update(sample_rate);
        e
    }

    /// Latency in samples (one analysis block minus the sample that
    /// completes it — verified by impulse: a spike at t returns at
    /// t + block − 1).
    pub fn latency(&self) -> usize {
        self.block - 1
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        // Frame-rate ballistics: coefficients per HOP, not per sample.
        let hop_s = self.hop as f64 / sample_rate;
        let c = |ms: f64| -> f64 {
            if ms <= 0.0 {
                1.0
            } else {
                1.0 - (-hop_s / (ms * 0.001)).exp()
            }
        };
        self.attack_coeff = c(self.params.attack_ms);
        self.release_coeff = c(self.params.release_ms);
    }

    /// Process one stereo sample; returns the (delayed) processed pair.
    #[inline]
    pub fn tick(&mut self, left: f64, right: f64) -> (f64, f64) {
        let pos = self.fill;
        self.in_buf[0][pos] = left;
        self.in_buf[1][pos] = right;
        self.fill += 1;
        if self.fill == self.block {
            self.process_frame();
            // Slide the input ring left by one hop.
            for ch in 0..2 {
                self.in_buf[ch].copy_within(self.hop.., 0);
            }
            self.fill = self.block - self.hop;
        }
        let (l, r) = if self.out_buf[0].is_empty() {
            (0.0, 0.0)
        } else {
            (self.out_buf[0].remove(0), self.out_buf[1].remove(0))
        };
        (l, r)
    }

    fn process_frame(&mut self) {
        let bins = self.mag_db.len();
        // Forward FFT both channels (windowed).
        for ch in 0..2 {
            for i in 0..self.block {
                self.frame[i] = self.in_buf[ch][i] * self.window[i];
            }
            let _ = self.fft.process(&mut self.frame, &mut self.spec[ch]);
        }

        if !self.params.freeze {
            // Trigger spectrum: average channel magnitude in dB + tilt.
            let bin_hz = self.sample_rate / self.block as f64;
            for i in 0..bins {
                let m = 0.5 * (self.spec[0][i].norm() + self.spec[1][i].norm())
                    / (self.block as f64 * 0.25);
                let mut db = 20.0 * m.max(1.0e-10).log10();
                if self.params.tilt && i > 0 {
                    let f = i as f64 * bin_hz;
                    db += 3.0 * (f / 1000.0).log2();
                }
                self.mag_db[i] = db;
            }

            // Smoothed spectral reference: two-pass (up + down) one-pole
            // across bins with an octave-proportional coefficient —
            // cheap constant-Q-ish neighborhood average.
            let mut acc = self.mag_db[0];
            for i in 0..bins {
                let f = (i.max(1)) as f64 * bin_hz;
                let neighbors = f * (2.0f64.powf(SMOOTH_OCTAVES) - 1.0) / bin_hz;
                let c = 1.0 / (1.0 + neighbors.max(1.0));
                acc += (self.mag_db[i] - acc) * c;
                self.ref_db[i] = acc;
            }
            let mut acc = self.mag_db[bins - 1];
            for i in (0..bins).rev() {
                let f = (i.max(1)) as f64 * bin_hz;
                let neighbors = f * (2.0f64.powf(SMOOTH_OCTAVES) - 1.0) / bin_hz;
                let c = 1.0 / (1.0 + neighbors.max(1.0));
                acc += (self.mag_db[i] - acc) * c;
                self.ref_db[i] = 0.5 * (self.ref_db[i] + acc);
            }

            // Per-bin target gain reduction.
            let sharp = 1.0 + self.params.density.clamp(0.0, 1.0) * 3.0;
            for i in 0..bins {
                let f = i as f64 * bin_hz;
                let in_range = f >= self.params.lo_hz && f <= self.params.hi_hz;
                let gated = self.mag_db[i] < self.params.gate_db;
                let over = if self.params.relative {
                    self.mag_db[i] - self.ref_db[i] - self.params.threshold_db
                } else {
                    self.mag_db[i] - self.params.threshold_db
                };
                let target = if in_range && !gated && over > 0.0 {
                    // Density sharpens: prominence^sharp scaling, capped.
                    (over * sharp * self.params.amount.clamp(0.0, 1.0)).min(24.0)
                } else {
                    0.0
                };
                // Frame-rate attack/release per bin.
                let c = if target > self.gr_db[i] {
                    self.attack_coeff
                } else {
                    self.release_coeff
                };
                self.gr_db[i] += (target - self.gr_db[i]) * c;
                self.gain[i] = 10.0f64.powf(-self.gr_db[i] / 20.0);
            }
        }

        // Apply gains; delta morphs suppressed ↔ removed.
        let delta = self.params.delta.clamp(0.0, 1.0);
        for ch in 0..2 {
            for i in 0..bins {
                let g_keep = self.gain[i];
                let g = g_keep * (1.0 - delta) + (1.0 - g_keep) * delta;
                self.spec[ch][i] *= g;
            }
            let mut spec = self.spec[ch].clone();
            let _ = self.ifft.process(&mut spec, &mut self.frame);
            // Overlap-add with synthesis window; Hann² at 75% overlap
            // sums to 1.5·block, folded into the normalization.
            let norm = 1.0 / (self.block as f64 * 1.5);
            for i in 0..self.block {
                self.ola[ch][i] += self.frame[i] * self.window[i] * norm;
            }
            // Emit one hop of finished samples.
            for i in 0..self.hop {
                self.out_buf[ch].push(self.ola[ch][i]);
            }
            self.ola[ch].copy_within(self.hop.., 0);
            for i in (self.block - self.hop)..self.block {
                self.ola[ch][i] = 0.0;
            }
        }
        self.primed = true;
    }

    pub fn reset(&mut self) {
        for ch in 0..2 {
            self.in_buf[ch].fill(0.0);
            self.ola[ch].fill(0.0);
            self.out_buf[ch].clear();
        }
        self.fill = 0;
        self.gr_db.fill(0.0);
        self.gain.fill(1.0);
        self.primed = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    /// Deterministic noise.
    fn noise(seed: &mut u64) -> f64 {
        *seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((*seed >> 33) as f64 / (1u64 << 31) as f64) - 1.0
    }

    /// Band energy of a buffer via Goertzel-ish correlation.
    fn tone_energy(buf: &[f64], freq: f64) -> f64 {
        let mut re = 0.0;
        let mut im = 0.0;
        for (i, &x) in buf.iter().enumerate() {
            let ph = core::f64::consts::TAU * freq * i as f64 / SR;
            re += x * ph.cos();
            im += x * ph.sin();
        }
        (re * re + im * im) / buf.len() as f64
    }

    #[test]
    fn suppresses_a_resonance_but_not_the_bed() {
        let mut e = SpectralEngine::new(SR, 1024);
        e.params.amount = 1.0;
        e.params.threshold_db = 3.0;
        e.params.attack_ms = 1.0;
        e.update(SR);
        let n = 96_000;
        let mut seed = 7u64;
        let mut out = vec![0.0; n];
        let mut inp = vec![0.0; n];
        for i in 0..n {
            // Noise bed at low level + screaming 2 kHz resonance.
            let x = 0.02 * noise(&mut seed)
                + 0.5 * (core::f64::consts::TAU * 2000.0 * i as f64 / SR).sin();
            inp[i] = x;
            let (l, _) = e.tick(x, x);
            out[i] = l;
        }
        let late_in = &inp[n / 2..];
        let late_out = &out[n / 2..];
        let res_in = tone_energy(late_in, 2000.0);
        let res_out = tone_energy(late_out, 2000.0);
        let red_db = 10.0 * (res_out / res_in).log10();
        assert!(
            red_db < -6.0,
            "resonance should be pulled down: {red_db:.1} dB"
        );
    }

    #[test]
    fn amount_zero_is_transparent() {
        let mut e = SpectralEngine::new(SR, 1024);
        e.params.amount = 0.0;
        e.update(SR);
        let n = 48_000;
        let lat = e.latency();
        let mut seed = 3u64;
        let mut inp = vec![0.0; n];
        let mut out = vec![0.0; n];
        for i in 0..n {
            let x = 0.3 * noise(&mut seed);
            inp[i] = x;
            let (l, _) = e.tick(x, x);
            out[i] = l;
        }
        // Compare aligned by latency, skip warmup.
        let mut err = 0.0;
        let mut sig = 0.0;
        for i in 8000..(n - lat) {
            let d = out[i + lat] - inp[i];
            err += d * d;
            sig += inp[i] * inp[i];
        }
        let err_db = 10.0 * (err / sig).log10();
        assert!(err_db < -30.0, "amount 0 should be near-null: {err_db:.1} dB");
    }

    #[test]
    fn delta_isolates_the_removed_part() {
        // suppressed + isolated must reconstruct the processed-off
        // signal: out(δ=0) + out(δ=1) = passthrough (per-frame linear).
        let run = |delta: f64| -> Vec<f64> {
            let mut e = SpectralEngine::new(SR, 1024);
            e.params.amount = 1.0;
            e.params.delta = delta;
            e.params.attack_ms = 1.0;
            e.update(SR);
            let mut seed = 11u64;
            let n = 48_000;
            let mut out = vec![0.0; n];
            for i in 0..n {
                let x = 0.02 * noise(&mut seed)
                    + 0.4 * (core::f64::consts::TAU * 3000.0 * i as f64 / SR).sin();
                let (l, _) = e.tick(x, x);
                out[i] = l;
            }
            out
        };
        let kept = run(0.0);
        let removed = run(1.0);
        // The removed part must carry the resonance far more than the
        // kept part carries it.
        let res_kept = tone_energy(&kept[24_000..], 3000.0);
        let res_removed = tone_energy(&removed[24_000..], 3000.0);
        assert!(
            res_removed > res_kept * 2.0,
            "delta output should isolate the resonance: kept={res_kept:.5} removed={res_removed:.5}"
        );
    }
}
