//! Per-block vocal classification.
//!
//! Clean-room reimplementation of the ZCR + spectral-centroid + spectral-flux
//! scheme popularised by Melodyne-style volume-riding macros. Each ~10 ms block
//! of mono audio is reduced to four cheap features and sorted into one of three
//! classes that the rest of the crate keys off:
//!
//! - [`BlockClass::Tonal`] — voiced/pitched material (the thing we ride toward a
//!   target level).
//! - [`BlockClass::Consonant`] — unvoiced/sibilant/plosive energy (fed to the
//!   de-esser, held out of the level target).
//! - [`BlockClass::Silence`] — below the adaptive noise floor (gate territory).
//!
//! The spectral features use a partial naïve DFT over a down-sampled, Hann
//! windowed block — identical maths to a `spectrum-analyzer` bin sweep but with
//! a handful of low bins, which is all the centroid/flux need. Trig and window
//! tables are allocated once in [`Classifier::new`]; `analyze_block` performs no
//! allocation, so the classifier is reusable on the realtime path.

use audiocore_dsp::db::linear_to_db;

/// Feature-extraction configuration. Defaults mirror the reference macro.
#[derive(Clone, Copy, Debug)]
pub struct ClassifyConfig {
    /// Block length in milliseconds (~10 ms is a good vocal compromise).
    pub block_ms: f64,
    /// ZCR at or above this is treated as unvoiced/noisy.
    pub zcr_tonal_max: f64,
    /// Spectral centroid (Hz) at or above this is treated as sibilant.
    pub sc_tonal_max_hz: f64,
    /// Spectral flux at or above this marks an onset (excluded from tonal).
    pub flux_onset_thresh: f64,
    /// Temporal smoothing half-window, in blocks (majority vote).
    pub smooth_frames: usize,
    /// DFT down-sample factor (4 → analyse every 4th sample).
    pub dft_downsample: usize,
    /// Number of DFT bins swept for the centroid/flux.
    pub n_bins: usize,
    /// Low bins ignored (DC / rumble).
    pub low_cut_bins: usize,
    /// One-pole smoothing on the flux estimate (0..1, higher = smoother).
    pub flux_alpha: f64,
}

impl Default for ClassifyConfig {
    fn default() -> Self {
        Self {
            block_ms: 10.0,
            zcr_tonal_max: 0.15,
            sc_tonal_max_hz: 2500.0,
            flux_onset_thresh: 0.30,
            smooth_frames: 3,
            dft_downsample: 4,
            n_bins: 50,
            low_cut_bins: 2,
            flux_alpha: 0.7,
        }
    }
}

/// Coarse class of a single analysis block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockClass {
    /// Voiced / pitched — the material we ride toward the target.
    Tonal,
    /// Unvoiced / sibilant / transient consonant.
    Consonant,
    /// Below the adaptive noise floor.
    Silence,
}

/// Cheap per-block features.
#[derive(Clone, Copy, Debug, Default)]
pub struct BlockFeatures {
    /// Linear RMS of the block.
    pub rms: f64,
    /// RMS in dBFS (floored, never `-inf`).
    pub rms_db: f64,
    /// Zero-crossing rate in [0, 1].
    pub zcr: f64,
    /// Spectral centroid in Hz.
    pub centroid_hz: f64,
    /// Smoothed spectral flux (rectified, magnitude-normalised).
    pub flux: f64,
}

/// Stateful block classifier. Holds the cached window/trig tables and the
/// previous-block spectrum needed for flux; otherwise allocation-free.
pub struct Classifier {
    cfg: ClassifyConfig,
    sample_rate: f64,
    block_samples: usize,
    n_eff: usize,
    n_bins: usize,
    hann: Vec<f64>,
    // Row-major [bin][n] trig tables over the down-sampled block.
    cos_tab: Vec<f64>,
    sin_tab: Vec<f64>,
    // Scratch, allocated once.
    mono: Vec<f64>,
    ds_buf: Vec<f64>,
    mags: Vec<f64>,
    prev_mags: Vec<f64>,
    have_prev: bool,
    flux_smooth: f64,
}

impl Classifier {
    /// Build a classifier for the given sample rate. Panics only if the block
    /// would be degenerately short (< 4 samples), which cannot happen for sane
    /// sample rates and the default 10 ms block.
    pub fn new(sample_rate: f64, cfg: ClassifyConfig) -> Self {
        let sample_rate = sample_rate.max(1.0);
        let block_samples = ((sample_rate * cfg.block_ms / 1000.0) as usize).max(4);
        let ds = cfg.dft_downsample.max(1);
        let n_eff = (block_samples - 1) / ds + 1;
        let n_bins = cfg.n_bins.min(n_eff / 2).max(1);

        // Hann window over the down-sampled length.
        let denom = (n_eff.max(2) - 1) as f64;
        let hann = (0..n_eff)
            .map(|n| 0.5 * (1.0 - (core::f64::consts::TAU * n as f64 / denom).cos()))
            .collect();

        // Trig tables: angle = 2π·k·n / n_eff, k in [1, n_bins].
        let mut cos_tab = vec![0.0; n_bins * n_eff];
        let mut sin_tab = vec![0.0; n_bins * n_eff];
        for k in 0..n_bins {
            for n in 0..n_eff {
                let angle = core::f64::consts::TAU * ((k + 1) * n) as f64 / n_eff as f64;
                cos_tab[k * n_eff + n] = angle.cos();
                sin_tab[k * n_eff + n] = angle.sin();
            }
        }

        Self {
            cfg,
            sample_rate,
            block_samples,
            n_eff,
            n_bins,
            hann,
            cos_tab,
            sin_tab,
            mono: vec![0.0; block_samples],
            ds_buf: vec![0.0; n_eff],
            mags: vec![0.0; n_bins],
            prev_mags: vec![0.0; n_bins],
            have_prev: false,
            flux_smooth: 0.0,
        }
    }

    /// Samples per analysis block for this configuration.
    #[inline]
    pub fn block_samples(&self) -> usize {
        self.block_samples
    }

    /// Reset the flux history (call between disjoint items).
    pub fn reset(&mut self) {
        self.have_prev = false;
        self.flux_smooth = 0.0;
    }

    /// Analyse one mono block. `block.len()` should equal
    /// [`Classifier::block_samples`]; shorter slices are zero-padded, longer are
    /// truncated. Returns the block's features. No allocation.
    pub fn analyze_block(&mut self, block: &[f64]) -> BlockFeatures {
        let bs = self.block_samples;
        // Copy + DC-block into scratch.
        let mut dc = 0.0;
        for i in 0..bs {
            let s = block.get(i).copied().unwrap_or(0.0);
            self.mono[i] = s;
            dc += s;
        }
        dc /= bs as f64;
        if dc.abs() > 1e-6 {
            for i in 0..bs {
                self.mono[i] -= dc;
            }
        }

        // RMS.
        let mut sum_sq = 0.0;
        for i in 0..bs {
            sum_sq += self.mono[i] * self.mono[i];
        }
        let rms = (sum_sq / bs as f64).sqrt();

        // Zero-crossing rate.
        let mut zc = 0usize;
        for i in 1..bs {
            if (self.mono[i] >= 0.0) != (self.mono[i - 1] >= 0.0) {
                zc += 1;
            }
        }
        let zcr = if bs > 1 {
            zc as f64 / (bs - 1) as f64
        } else {
            0.0
        };

        // Down-sample + window.
        let ds = self.cfg.dft_downsample.max(1);
        let mut idx = 0usize;
        let mut i = 0usize;
        while i < bs && idx < self.n_eff {
            self.ds_buf[idx] = self.mono[i] * self.hann[idx];
            idx += 1;
            i += ds;
        }
        let used = idx.min(self.n_eff);

        // Partial DFT → magnitudes, centroid, flux accumulator.
        let sr_eff = self.sample_rate / ds as f64;
        let freq_step = sr_eff / self.n_eff as f64;
        let k_start = self.cfg.low_cut_bins.min(self.n_bins);

        let mut sum_mag = 0.0;
        let mut sum_weighted = 0.0;
        for k in 0..self.n_bins {
            let mag = if k < k_start {
                0.0
            } else {
                let base = k * self.n_eff;
                let mut re = 0.0;
                let mut im = 0.0;
                for n in 0..used {
                    let s = self.ds_buf[n];
                    re += s * self.cos_tab[base + n];
                    im -= s * self.sin_tab[base + n];
                }
                let m = (re * re + im * im).sqrt();
                sum_mag += m;
                sum_weighted += m * ((k + 1) as f64 * freq_step);
                m
            };
            self.mags[k] = mag;
        }
        let centroid_hz = if sum_mag > 0.0 {
            sum_weighted / sum_mag
        } else {
            0.0
        };

        // Spectral flux (rectified positive difference, normalised).
        let mut flux = 0.0;
        if self.have_prev && sum_mag > 0.0 {
            for k in 0..self.n_bins {
                let diff = self.mags[k] - self.prev_mags[k];
                if diff > 0.0 {
                    flux += diff;
                }
            }
            flux /= sum_mag;
        }
        self.flux_smooth =
            self.cfg.flux_alpha * self.flux_smooth + (1.0 - self.cfg.flux_alpha) * flux;
        self.prev_mags.copy_from_slice(&self.mags);
        self.have_prev = true;

        BlockFeatures {
            rms,
            rms_db: linear_to_db(rms),
            zcr,
            centroid_hz,
            flux: self.flux_smooth,
        }
    }

    /// Classify a block's features against an (already computed) silence
    /// threshold. Ordering matters: silence wins first, then the tonal test;
    /// everything audible-but-not-tonal is a consonant.
    pub fn classify(&self, f: &BlockFeatures, silence_db: f64) -> BlockClass {
        if f.rms_db <= silence_db {
            return BlockClass::Silence;
        }
        let tonal = f.zcr < self.cfg.zcr_tonal_max
            && f.centroid_hz < self.cfg.sc_tonal_max_hz
            && f.flux < self.cfg.flux_onset_thresh;
        if tonal {
            BlockClass::Tonal
        } else {
            BlockClass::Consonant
        }
    }

    /// Config accessor.
    #[inline]
    pub fn config(&self) -> &ClassifyConfig {
        &self.cfg
    }
}

/// Estimate an adaptive silence threshold (dB) from a set of block RMS values.
///
/// Takes the 10th-percentile RMS as the noise floor and sits `+6 dB` above it,
/// clamped to a sane window. Mirrors the reference macro's adaptive gate.
pub fn adaptive_silence_db(rms_values: &[f64]) -> f64 {
    if rms_values.is_empty() {
        return -50.0;
    }
    let mut v: Vec<f64> = rms_values.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
    let p10 = ((v.len() as f64 * 0.10) as usize).min(v.len() - 1);
    let noise_db = linear_to_db(v[p10].max(1e-9));
    (noise_db + 6.0).clamp(-60.0, -30.0)
}

/// Majority-vote temporal smoothing of a tonal-mask, half-window `smooth_frames`.
/// Returns a new mask; input/output are `is_tonal` booleans per block.
pub fn smooth_tonal_mask(raw: &[bool], smooth_frames: usize) -> Vec<bool> {
    let n = raw.len();
    let sf = smooth_frames;
    (0..n)
        .map(|i| {
            let lo = i.saturating_sub(sf);
            let hi = (i + sf + 1).min(n);
            let votes = raw[lo..hi].iter().filter(|b| **b).count();
            let total = hi - lo;
            votes * 2 > total
        })
        .collect()
}
