//! FFT-based onset detection with multiple detection functions.
//!
//! Implements six onset detection functions (ODFs):
//!
//! - **Spectral Flux**: Sum of positive magnitude differences.
//!   Reference: Dixon (2006) "Onset Detection Revisited"
//!
//! - **SuperFlux**: Spectral flux with maximum filter vibrato suppression.
//!   Reference: Böck & Widmer (2013) "Maximum Filter Vibrato Suppression
//!   for Onset Detection", DAFx-13.
//!
//! - **HFC** (High Frequency Content): Frequency-weighted energy sum.
//!   Emphasizes broadband transients over tonal content.
//!   Reference: Bello et al. (2005) "A Tutorial on Onset Detection"
//!
//! - **Complex Domain**: Combines magnitude and phase prediction error.
//!   Catches soft/tonal onsets that magnitude-only methods miss.
//!   Reference: Bello & Duxbury (2003); Bello et al. (2005)
//!
//! - **Rectified Complex Domain**: Complex domain with half-wave rectification.
//!   Generally the best single classical ODF across mixed material.
//!   Reference: Bello et al. (2005)
//!
//! - **Modified KL Divergence**: Logarithmic relative spectral change.
//!   Sensitive to quiet onsets in loud contexts.
//!   Reference: Bello et al. (2005)

use rustfft::{num_complex::Complex, FftPlanner};

/// Onset detection function selection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FluxMode {
    /// Standard spectral flux: positive magnitude differences.
    SpectralFlux,
    /// SuperFlux: spectral flux with maximum filter vibrato suppression
    /// and log-filtered spectrogram (24 bands/octave).
    SuperFlux,
    /// High frequency content: frequency-weighted spectral energy.
    /// Particularly effective for percussive onsets.
    Hfc,
    /// Complex domain: magnitude + phase prediction error.
    /// Good for both percussive and tonal onsets.
    ComplexDomain,
    /// Rectified complex domain: complex domain with half-wave rectification.
    /// Only counts bins where magnitude increased.
    RectifiedComplexDomain,
    /// Modified Kullback-Leibler divergence: logarithmic relative change.
    /// Sensitive to quiet onsets in loud contexts.
    ModifiedKl,
}

/// FFT-based onset detector supporting multiple detection functions.
///
/// Accumulates samples into hops, computes spectra via FFT,
/// and outputs an onset detection function value per hop.
pub struct SpectralFluxDetector {
    pub mode: FluxMode,

    // FFT
    fft_size: usize,
    hop_size: usize,
    window: Vec<f64>,
    fft_buf: Vec<Complex<f64>>,
    magnitude: Vec<f64>,
    prev_magnitude: Vec<f64>,

    // Phase storage (for complex domain methods)
    phase: Vec<f64>,
    prev_phase: Vec<f64>,
    prev_prev_phase: Vec<f64>,

    // Hop accumulation
    input_buf: Vec<f64>,
    input_pos: usize,

    // Filterbank (SuperFlux)
    filterbank: Vec<Vec<(usize, f64)>>,
    filtered_spec: Vec<f64>,
    prev_filtered_spec: Vec<f64>,
    max_filtered_spec: Vec<f64>,
    num_bands: usize,

    // Peak picking state
    odf_ring: Vec<f64>,
    odf_ring_pos: usize,
    odf_ring_len: usize,

    // Config
    sample_rate: f64,
    /// Maximum filter width in frequency bins (SuperFlux).
    pub max_filter_size: usize,
}

/// Default FFT size for onset detection.
pub const DEFAULT_FFT_SIZE: usize = 2048;
/// Default hop size (determines latency and temporal resolution).
pub const DEFAULT_HOP_SIZE: usize = 441; // ~10ms at 44.1kHz, ~9.2ms at 48kHz

// SuperFlux filterbank parameters
const BANDS_PER_OCTAVE: usize = 24;
const FMIN: f64 = 30.0;
const FMAX: f64 = 17000.0;
// Peak picking ring buffer for adaptive threshold
const ODF_RING_LEN: usize = 31; // ~150ms at 200fps

impl SpectralFluxDetector {
    pub fn new(mode: FluxMode, fft_size: usize, hop_size: usize, sample_rate: f64) -> Self {
        let num_bins = fft_size / 2 + 1;

        // Hanning window
        let window: Vec<f64> = (0..fft_size)
            .map(|i| 0.5 * (1.0 - (2.0 * std::f64::consts::PI * i as f64 / fft_size as f64).cos()))
            .collect();

        // Build filterbank for SuperFlux
        let (filterbank, num_bands) = if mode == FluxMode::SuperFlux {
            build_filterbank(num_bins, sample_rate, BANDS_PER_OCTAVE, FMIN, FMAX)
        } else {
            (Vec::new(), 0)
        };

        let spec_len = if mode == FluxMode::SuperFlux {
            num_bands
        } else {
            num_bins
        };

        Self {
            mode,
            fft_size,
            hop_size,
            window,
            fft_buf: vec![Complex::new(0.0, 0.0); fft_size],
            magnitude: vec![0.0; num_bins],
            prev_magnitude: vec![0.0; num_bins],
            phase: vec![0.0; num_bins],
            prev_phase: vec![0.0; num_bins],
            prev_prev_phase: vec![0.0; num_bins],
            input_buf: vec![0.0; fft_size],
            input_pos: 0,
            filterbank,
            filtered_spec: vec![0.0; spec_len],
            prev_filtered_spec: vec![0.0; spec_len],
            max_filtered_spec: vec![0.0; spec_len],
            num_bands,
            odf_ring: vec![0.0; ODF_RING_LEN],
            odf_ring_pos: 0,
            odf_ring_len: ODF_RING_LEN,
            sample_rate,
            max_filter_size: 3,
        }
    }

    /// Update sample rate (reconstructs internal state).
    pub fn update(&mut self, sample_rate: f64) {
        if (self.sample_rate - sample_rate).abs() > 0.1 {
            *self = Self::new(self.mode, self.fft_size, self.hop_size, sample_rate);
        }
    }

    /// Reset all internal state.
    pub fn reset(&mut self) {
        self.magnitude.fill(0.0);
        self.prev_magnitude.fill(0.0);
        self.phase.fill(0.0);
        self.prev_phase.fill(0.0);
        self.prev_prev_phase.fill(0.0);
        self.input_buf.fill(0.0);
        self.input_pos = 0;
        self.filtered_spec.fill(0.0);
        self.prev_filtered_spec.fill(0.0);
        self.max_filtered_spec.fill(0.0);
        self.odf_ring.fill(0.0);
        self.odf_ring_pos = 0;
    }

    /// Feed one sample. Returns `Some(odf_value)` when a hop completes.
    #[inline]
    pub fn tick(&mut self, sample: f64) -> Option<f64> {
        self.input_buf[self.input_pos] = sample;
        self.input_pos += 1;

        if self.input_pos >= self.hop_size {
            self.input_pos = 0;
            let odf = self.compute_hop();
            let keep = self.fft_size - self.hop_size;
            self.input_buf.copy_within(self.hop_size..self.fft_size, 0);
            self.input_buf[keep..].fill(0.0);
            Some(odf)
        } else {
            None
        }
    }

    /// Returns the latency in samples introduced by this detector.
    pub fn latency_samples(&self) -> usize {
        self.fft_size
    }

    /// Returns the hop size in samples.
    pub fn hop_size(&self) -> usize {
        self.hop_size
    }

    /// Compute ODF for the current hop.
    fn compute_hop(&mut self) -> f64 {
        // Apply window and fill FFT buffer
        for i in 0..self.fft_size {
            self.fft_buf[i] = Complex::new(self.input_buf[i] * self.window[i], 0.0);
        }

        // In-place FFT
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(self.fft_size);
        fft.process(&mut self.fft_buf);

        // Extract magnitude and phase
        let num_bins = self.fft_size / 2 + 1;
        for i in 0..num_bins {
            self.magnitude[i] = self.fft_buf[i].norm();
            self.phase[i] = self.fft_buf[i].arg();
        }

        match self.mode {
            FluxMode::SpectralFlux => self.compute_spectral_flux(),
            FluxMode::SuperFlux => self.compute_superflux(),
            FluxMode::Hfc => self.compute_hfc(),
            FluxMode::ComplexDomain => self.compute_complex_domain(false),
            FluxMode::RectifiedComplexDomain => self.compute_complex_domain(true),
            FluxMode::ModifiedKl => self.compute_modified_kl(),
        }
    }

    /// Standard spectral flux: sum of positive magnitude differences.
    fn compute_spectral_flux(&mut self) -> f64 {
        let num_bins = self.fft_size / 2 + 1;
        let mut flux = 0.0;

        for i in 0..num_bins {
            let diff = self.magnitude[i] - self.prev_magnitude[i];
            if diff > 0.0 {
                flux += diff;
            }
        }

        self.prev_magnitude[..num_bins].copy_from_slice(&self.magnitude[..num_bins]);
        flux
    }

    /// SuperFlux: log-filtered spectral flux with maximum filter vibrato suppression.
    fn compute_superflux(&mut self) -> f64 {
        let num_bins = self.fft_size / 2 + 1;

        // Apply triangular filterbank
        for b in 0..self.num_bands {
            let mut sum = 0.0;
            for &(bin, weight) in &self.filterbank[b] {
                if bin < num_bins {
                    sum += self.magnitude[bin] * weight;
                }
            }
            self.filtered_spec[b] = (sum + 1.0).log10();
        }

        // Maximum filter on previous frame
        let half = self.max_filter_size / 2;
        for b in 0..self.num_bands {
            let lo = if b >= half { b - half } else { 0 };
            let hi = (b + half + 1).min(self.num_bands);
            let mut max_val = self.prev_filtered_spec[lo];
            for j in (lo + 1)..hi {
                if self.prev_filtered_spec[j] > max_val {
                    max_val = self.prev_filtered_spec[j];
                }
            }
            self.max_filtered_spec[b] = max_val;
        }

        // Positive differences against max-filtered previous
        let mut flux = 0.0;
        for b in 0..self.num_bands {
            let diff = self.filtered_spec[b] - self.max_filtered_spec[b];
            if diff > 0.0 {
                flux += diff;
            }
        }

        self.prev_filtered_spec[..self.num_bands]
            .copy_from_slice(&self.filtered_spec[..self.num_bands]);
        flux
    }

    /// HFC: frequency-weighted spectral energy.
    /// sum_k(k * |X(k)|²) — emphasizes high-frequency transient energy.
    fn compute_hfc(&mut self) -> f64 {
        let num_bins = self.fft_size / 2 + 1;
        let mut hfc = 0.0;

        for k in 0..num_bins {
            hfc += (k as f64) * self.magnitude[k] * self.magnitude[k];
        }

        // Half-wave rectified difference from previous HFC
        let prev_hfc = {
            let mut prev = 0.0;
            for k in 0..num_bins {
                prev += (k as f64) * self.prev_magnitude[k] * self.prev_magnitude[k];
            }
            prev
        };

        self.prev_magnitude[..num_bins].copy_from_slice(&self.magnitude[..num_bins]);

        let diff = hfc - prev_hfc;
        if diff > 0.0 {
            diff
        } else {
            0.0
        }
    }

    /// Complex domain: magnitude + phase prediction error.
    /// When `rectified` is true, only counts bins where magnitude increased
    /// (rectified complex domain — generally the best single classical ODF).
    fn compute_complex_domain(&mut self, rectified: bool) -> f64 {
        let num_bins = self.fft_size / 2 + 1;
        let mut cd = 0.0;

        for k in 0..num_bins {
            // Predict phase by linear extrapolation from two previous frames
            let predicted_phase = 2.0 * self.prev_phase[k] - self.prev_prev_phase[k];

            // Predicted complex value: previous magnitude at predicted phase
            let predicted = Complex::new(
                self.prev_magnitude[k] * predicted_phase.cos(),
                self.prev_magnitude[k] * predicted_phase.sin(),
            );

            // Actual current value
            let actual = self.fft_buf[k];

            // Distance between predicted and actual
            let distance = (actual - predicted).norm();

            if rectified {
                // Only count if magnitude increased (half-wave rectification)
                if self.magnitude[k] > self.prev_magnitude[k] {
                    cd += distance;
                }
            } else {
                cd += distance;
            }
        }

        // Rotate phase history
        self.prev_prev_phase[..num_bins].copy_from_slice(&self.prev_phase[..num_bins]);
        self.prev_phase[..num_bins].copy_from_slice(&self.phase[..num_bins]);
        self.prev_magnitude[..num_bins].copy_from_slice(&self.magnitude[..num_bins]);

        cd
    }

    /// Modified KL divergence: log(1 + |X(n,k)| / (|X(n-1,k)| + eps))
    /// Sensitive to relative rather than absolute spectral changes.
    fn compute_modified_kl(&mut self) -> f64 {
        let num_bins = self.fft_size / 2 + 1;
        let eps = 1e-10;
        let mut mkl = 0.0;

        for k in 0..num_bins {
            let ratio = self.magnitude[k] / (self.prev_magnitude[k] + eps);
            mkl += (1.0 + ratio).ln();
        }

        // Subtract the baseline (when spectra are identical, each bin contributes ln(2))
        let baseline = num_bins as f64 * 2.0_f64.ln();
        let result = mkl - baseline;

        self.prev_magnitude[..num_bins].copy_from_slice(&self.magnitude[..num_bins]);

        if result > 0.0 {
            result
        } else {
            0.0
        }
    }

    /// Adaptive peak picking on ODF values.
    ///
    /// Returns true if the given ODF value represents a local peak
    /// above the adaptive threshold (moving average + delta).
    pub fn is_peak(&mut self, odf: f64, threshold_delta: f64) -> bool {
        self.odf_ring[self.odf_ring_pos % self.odf_ring_len] = odf;
        self.odf_ring_pos += 1;

        if self.odf_ring_pos < self.odf_ring_len {
            return false;
        }

        let avg: f64 = self.odf_ring.iter().sum::<f64>() / self.odf_ring_len as f64;

        let max_window = 5;
        let mut is_local_max = true;
        for i in 1..=max_window / 2 {
            if self.odf_ring_pos > i {
                let prev_idx = (self.odf_ring_pos - 1 - i) % self.odf_ring_len;
                if self.odf_ring[prev_idx] >= odf {
                    is_local_max = false;
                    break;
                }
            }
        }

        is_local_max && odf >= avg + threshold_delta && odf > 0.0
    }
}

/// Build a triangular filterbank with `bands_per_octave` bands from `fmin` to `fmax`.
fn build_filterbank(
    num_bins: usize,
    sample_rate: f64,
    bands_per_octave: usize,
    fmin: f64,
    fmax: f64,
) -> (Vec<Vec<(usize, f64)>>, usize) {
    let fmin_log = fmin.ln();
    let fmax_log = fmax.ln();
    let num_octaves = (fmax / fmin).log2();
    let num_bands = (num_octaves * bands_per_octave as f64).ceil() as usize;

    let mut center_freqs: Vec<f64> = Vec::with_capacity(num_bands + 2);
    for i in 0..=num_bands + 1 {
        let freq = (fmin_log + (fmax_log - fmin_log) * i as f64 / (num_bands + 1) as f64).exp();
        center_freqs.push(freq);
    }

    let bin_hz = sample_rate / (2.0 * (num_bins - 1) as f64);
    let mut filterbank = Vec::with_capacity(num_bands);

    for b in 0..num_bands {
        let f_lo = center_freqs[b];
        let f_center = center_freqs[b + 1];
        let f_hi = center_freqs[b + 2];

        let mut band = Vec::new();
        let bin_lo = (f_lo / bin_hz).floor() as usize;
        let bin_hi = ((f_hi / bin_hz).ceil() as usize).min(num_bins - 1);

        for bin in bin_lo..=bin_hi {
            let freq = bin as f64 * bin_hz;
            let weight = if freq <= f_center {
                if f_center > f_lo {
                    (freq - f_lo) / (f_center - f_lo)
                } else {
                    1.0
                }
            } else if f_hi > f_center {
                (f_hi - freq) / (f_hi - f_center)
            } else {
                1.0
            };

            if weight > 0.0 {
                band.push((bin, weight));
            }
        }

        if band.is_empty() {
            let center_bin = ((f_center / bin_hz).round() as usize).min(num_bins - 1);
            band.push((center_bin, 1.0));
        }

        filterbank.push(band);
    }

    (filterbank, num_bands)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;
    const FFT: usize = 1024;
    const HOP: usize = 512;

    fn silence_then_transient(
        det: &mut SpectralFluxDetector,
        silence: usize,
        tone_hz: f64,
    ) -> Vec<f64> {
        let mut odfs = Vec::new();
        for _ in 0..silence {
            if let Some(odf) = det.tick(0.0) {
                odfs.push(odf);
            }
        }
        for i in 0..HOP {
            let sample = (i as f64 * tone_hz * std::f64::consts::TAU / SR).sin() * 0.9;
            if let Some(odf) = det.tick(sample) {
                odfs.push(odf);
            }
        }
        odfs
    }

    fn assert_silent_is_zero(mode: FluxMode) {
        let mut det = SpectralFluxDetector::new(mode, FFT, HOP, SR);
        let mut last_odf = None;
        for _ in 0..FFT * 2 {
            if let Some(odf) = det.tick(0.0) {
                last_odf = Some(odf);
            }
        }
        assert!(last_odf.is_some());
        assert_eq!(
            last_odf.unwrap(),
            0.0,
            "mode {:?} should be zero on silence",
            mode
        );
    }

    fn assert_detects_transient(mode: FluxMode) {
        let mut det = SpectralFluxDetector::new(mode, FFT, HOP, SR);
        let odfs = silence_then_transient(&mut det, FFT * 2, 440.0);
        let max_odf = odfs.iter().cloned().fold(0.0_f64, f64::max);
        assert!(
            max_odf > 0.01,
            "mode {:?}: expected ODF spike on transient, got max={}",
            mode,
            max_odf
        );
    }

    #[test]
    fn spectral_flux_silent() {
        assert_silent_is_zero(FluxMode::SpectralFlux);
    }
    #[test]
    fn spectral_flux_transient() {
        assert_detects_transient(FluxMode::SpectralFlux);
    }

    #[test]
    fn superflux_silent() {
        let mut det = SpectralFluxDetector::new(FluxMode::SuperFlux, 2048, 441, 44100.0);
        let mut last_odf = None;
        for _ in 0..4096 {
            if let Some(odf) = det.tick(0.0) {
                last_odf = Some(odf);
            }
        }
        assert!(last_odf.is_some());
        assert_eq!(last_odf.unwrap(), 0.0);
    }
    #[test]
    fn superflux_transient() {
        let mut det = SpectralFluxDetector::new(FluxMode::SuperFlux, 2048, 441, 44100.0);
        let mut odfs = Vec::new();
        for _ in 0..4096 {
            if let Some(odf) = det.tick(0.0) {
                odfs.push(odf);
            }
        }
        for i in 0..441 {
            let s = (i as f64 * 200.0 * std::f64::consts::TAU / 44100.0).sin() * 0.9;
            if let Some(odf) = det.tick(s) {
                odfs.push(odf);
            }
        }
        let max_odf = odfs.iter().cloned().fold(0.0_f64, f64::max);
        assert!(max_odf > 0.01, "SuperFlux: expected spike, got {}", max_odf);
    }

    #[test]
    fn hfc_silent() {
        assert_silent_is_zero(FluxMode::Hfc);
    }
    #[test]
    fn hfc_transient() {
        assert_detects_transient(FluxMode::Hfc);
    }

    #[test]
    fn complex_domain_silent() {
        assert_silent_is_zero(FluxMode::ComplexDomain);
    }
    #[test]
    fn complex_domain_transient() {
        assert_detects_transient(FluxMode::ComplexDomain);
    }

    #[test]
    fn rectified_complex_domain_silent() {
        assert_silent_is_zero(FluxMode::RectifiedComplexDomain);
    }
    #[test]
    fn rectified_complex_domain_transient() {
        assert_detects_transient(FluxMode::RectifiedComplexDomain);
    }

    #[test]
    fn modified_kl_silent() {
        assert_silent_is_zero(FluxMode::ModifiedKl);
    }
    #[test]
    fn modified_kl_transient() {
        assert_detects_transient(FluxMode::ModifiedKl);
    }

    #[test]
    fn filterbank_covers_range() {
        let (fb, num_bands) = build_filterbank(1025, 44100.0, 24, 30.0, 17000.0);
        assert!(
            num_bands > 100,
            "Expected >100 bands at 24/octave over 30-17kHz"
        );
        assert_eq!(fb.len(), num_bands);
        for (i, band) in fb.iter().enumerate() {
            assert!(!band.is_empty(), "Band {} is empty", i);
        }
    }

    #[test]
    fn peak_picker_detects_spike() {
        let mut det = SpectralFluxDetector::new(FluxMode::SpectralFlux, FFT, HOP, SR);
        for _ in 0..ODF_RING_LEN {
            det.is_peak(0.1, 0.5);
        }
        let result = det.is_peak(5.0, 0.5);
        assert!(result, "Expected peak detection on spike");
    }
}
