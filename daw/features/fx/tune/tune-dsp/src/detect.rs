//! Monophonic fundamental-frequency detection (YIN).
//!
//! Clean-room implementation of the YIN pitch estimator (de Cheveigné &
//! Kawahara, 2002): difference function → cumulative-mean normalisation →
//! absolute-threshold pick → parabolic interpolation. This is the front end of
//! the `tune` FX — it produces the per-frame pitch track that [`crate::note`]
//! groups into editable notes and [`crate::correct`] retunes.
//!
//! Monophonic only for now (one f0 per frame); polyphonic detection is the
//! larger follow-up that makes `tune` a full Melodyne competitor. Allocation is
//! confined to [`YinDetector::new`]; [`YinDetector::detect`] is alloc-free.

/// Detection tuning.
#[derive(Clone, Copy, Debug)]
pub struct YinConfig {
    /// Analysis window length in samples (also the max lag + 1).
    pub window: usize,
    /// Absolute threshold for the cumulative-mean difference (0.10–0.15 typical).
    pub threshold: f64,
    /// Lowest detectable frequency, Hz (bounds the max lag searched).
    pub min_hz: f64,
    /// Highest detectable frequency, Hz (bounds the min lag searched).
    pub max_hz: f64,
}

impl Default for YinConfig {
    fn default() -> Self {
        Self {
            window: 2048,
            threshold: 0.12,
            min_hz: 65.0,   // ~C2
            max_hz: 1200.0, // ~D6
        }
    }
}

/// A single-frame detection result.
#[derive(Clone, Copy, Debug)]
pub struct PitchFrame {
    /// Estimated fundamental in Hz, or `None` when unvoiced/unreliable.
    pub f0_hz: Option<f64>,
    /// Aperiodicity at the chosen lag (lower = more confidently pitched).
    pub aperiodicity: f64,
    /// Frame RMS (linear) — handy for voiced/unvoiced gating downstream.
    pub rms: f64,
}

/// Reusable YIN detector.
pub struct YinDetector {
    cfg: YinConfig,
    sample_rate: f64,
    diff: Vec<f64>,
    cmnd: Vec<f64>,
    min_lag: usize,
    max_lag: usize,
    // FFT-accelerated difference function: d(τ) = E0 + E(τ) − 2·c(τ)
    // with the cross term via one forward/inverse FFT pair — O(W log W)
    // instead of the naive O(W²/4). All buffers preallocated.
    fft_fwd: std::sync::Arc<dyn realfft::RealToComplex<f64>>,
    fft_inv: std::sync::Arc<dyn realfft::ComplexToReal<f64>>,
    fft_len: usize,
    pad_a: Vec<f64>,
    pad_b: Vec<f64>,
    spec_a: Vec<realfft::num_complex::Complex<f64>>,
    spec_b: Vec<realfft::num_complex::Complex<f64>>,
    xcorr: Vec<f64>,
    cumsq: Vec<f64>,
}

impl YinDetector {
    /// Build a detector for a sample rate.
    pub fn new(sample_rate: f64, cfg: YinConfig) -> Self {
        let sr = sample_rate.max(1.0);
        let half = cfg.window / 2;
        let min_lag = ((sr / cfg.max_hz) as usize).max(2);
        let max_lag = ((sr / cfg.min_hz) as usize)
            .min(half.saturating_sub(1))
            .max(min_lag + 1);
        let fft_len = (cfg.window + half).next_power_of_two();
        let mut planner = realfft::RealFftPlanner::<f64>::new();
        let fft_fwd = planner.plan_fft_forward(fft_len);
        let fft_inv = planner.plan_fft_inverse(fft_len);
        let bins = fft_len / 2 + 1;
        Self {
            cfg,
            sample_rate: sr,
            diff: vec![0.0; half],
            cmnd: vec![0.0; half],
            min_lag,
            max_lag,
            fft_fwd,
            fft_inv,
            fft_len,
            pad_a: vec![0.0; fft_len],
            pad_b: vec![0.0; fft_len],
            spec_a: vec![realfft::num_complex::Complex::new(0.0, 0.0); bins],
            spec_b: vec![realfft::num_complex::Complex::new(0.0, 0.0); bins],
            xcorr: vec![0.0; fft_len],
            cumsq: vec![0.0; cfg.window + 1],
        }
    }

    /// Window length expected by [`YinDetector::detect`].
    #[inline]
    pub fn window(&self) -> usize {
        self.cfg.window
    }

    /// Estimate f0 for one frame. `frame.len()` should be at least
    /// [`YinDetector::window`]; extra samples are ignored.
    pub fn detect(&mut self, frame: &[f64]) -> PitchFrame {
        let w = self.cfg.window.min(frame.len());
        let half = w / 2;
        let rms = (frame[..w].iter().map(|s| s * s).sum::<f64>() / w as f64).sqrt();

        // Difference function d(τ) = Σ_{j<H}(x_j − x_{j+τ})²
        //                          = E[0..H] + E[τ..τ+H] − 2·c(τ),
        // c(τ) = Σ_{j<H} x_j·x_{j+τ} via FFT cross-correlation of the
        // first half against the whole window.
        self.cumsq[0] = 0.0;
        for (j, &x) in frame.iter().enumerate().take(w) {
            self.cumsq[j + 1] = self.cumsq[j] + x * x;
        }
        let e0 = self.cumsq[half];
        self.pad_a[..w].copy_from_slice(&frame[..w]);
        self.pad_a[w..].fill(0.0);
        self.pad_b[..half].copy_from_slice(&frame[..half]);
        self.pad_b[half..].fill(0.0);
        let _ = self.fft_fwd.process(&mut self.pad_a, &mut self.spec_a);
        let _ = self.fft_fwd.process(&mut self.pad_b, &mut self.spec_b);
        // conj(B)·A → correlation c(τ) at non-negative lags.
        for (a, b) in self.spec_a.iter_mut().zip(&self.spec_b) {
            *a *= b.conj();
        }
        let _ = self.fft_inv.process(&mut self.spec_a, &mut self.xcorr);
        let norm = 1.0 / self.fft_len as f64;
        self.diff[0] = 0.0;
        for tau in 1..half.min(self.max_lag + 1) {
            let e_tau = self.cumsq[tau + half] - self.cumsq[tau];
            let c = self.xcorr[tau] * norm;
            self.diff[tau] = (e0 + e_tau - 2.0 * c).max(0.0);
        }

        // Cumulative mean normalised difference.
        self.cmnd[0] = 1.0;
        let mut running = 0.0;
        for tau in 1..half.min(self.max_lag + 1) {
            running += self.diff[tau];
            self.cmnd[tau] = if running > 0.0 {
                self.diff[tau] * tau as f64 / running
            } else {
                1.0
            };
        }

        // Absolute-threshold pick: first local min below threshold.
        let mut tau_est = None;
        let mut tau = self.min_lag;
        while tau < self.max_lag {
            if self.cmnd[tau] < self.cfg.threshold {
                while tau + 1 < self.max_lag && self.cmnd[tau + 1] < self.cmnd[tau] {
                    tau += 1;
                }
                tau_est = Some(tau);
                break;
            }
            tau += 1;
        }

        // Fall back to the global minimum lag if nothing crossed threshold.
        let (tau_best, aper) = match tau_est {
            Some(t) => (t, self.cmnd[t]),
            None => {
                let mut best = self.min_lag;
                for t in (self.min_lag + 1)..self.max_lag {
                    if self.cmnd[t] < self.cmnd[best] {
                        best = t;
                    }
                }
                (best, self.cmnd[best])
            }
        };

        // Parabolic interpolation around tau_best for sub-sample accuracy.
        let refined = self.parabolic(tau_best);
        let voiced = tau_est.is_some() && rms > 1e-4;
        let f0 = if voiced && refined > 0.0 {
            Some(self.sample_rate / refined)
        } else {
            None
        };

        PitchFrame {
            f0_hz: f0,
            aperiodicity: aper,
            rms,
        }
    }

    fn parabolic(&self, tau: usize) -> f64 {
        if tau <= self.min_lag || tau + 1 >= self.max_lag {
            return tau as f64;
        }
        let s0 = self.cmnd[tau - 1];
        let s1 = self.cmnd[tau];
        let s2 = self.cmnd[tau + 1];
        let denom = 2.0 * (2.0 * s1 - s2 - s0);
        if denom.abs() < 1e-12 {
            tau as f64
        } else {
            tau as f64 + (s2 - s0) / denom
        }
    }
}

/// MIDI note number (float) for a frequency, A4 = 69 = 440 Hz.
pub fn hz_to_midi(hz: f64) -> f64 {
    69.0 + 12.0 * (hz / 440.0).log2()
}

/// Frequency for a (float) MIDI note number.
pub fn midi_to_hz(midi: f64) -> f64 {
    440.0 * 2f64.powf((midi - 69.0) / 12.0)
}
