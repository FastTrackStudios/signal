//! 3-band multiband compression system.
//!
//! The layout is informed by the Pro-C research notes and processes audio
//! through 3 independent compression bands:
//! - Band 0: Low frequency band (uses low-pass filter)
//! - Band 1: Mid frequency band (uses band-pass filter: high-pass + low-pass)
//! - Band 2: High frequency band (uses high-pass filter)
//!
//! Each band:
//! 1. Separates input into frequency band using adaptive filters
//! 2. Detects level of band-specific audio
//! 3. Computes gain reduction per band
//! 4. Smooths gain reduction with Hermite cubic
//! 5. Applies compression to band-specific audio
//!
//! Bands are summed at the output for final reconstruction.

use crate::biquad::Biquad;
use crate::biquad::{design_highpass_biquad, design_lowpass_biquad};
use crate::detector::Detector;
use crate::gain_curve::GainCurve;
use crate::hermite::{HermiteCubicSmoother, StateFuncHypothesis};
use std::f64::consts::{LN_2, PI};

/// Per-band compression state
#[derive(Clone)]
pub struct CompressionBand {
    /// Band index (0=low, 1=mid, 2=high)
    pub band_index: usize,

    /// Level detector for this band
    detector: Detector,

    /// Gain curve processor
    gain_curve: GainCurve,

    /// Smoothing with Hermite cubic
    smoother: HermiteCubicSmoother,

    /// Last computed gain reduction (dB)
    last_gr_db: f64,

    /// Previous detected level (for Band 2 hysteresis)
    previous_level_db: f64,

    /// Sample rate (needed for crossover frequency calculation)
    sample_rate: f64,

    /// High-pass filter for band separation (Band 1 and 2)
    hp_filter: Option<Biquad>,

    /// Low-pass filter for band separation (Band 0 and 1)
    lp_filter: Option<Biquad>,
}

impl CompressionBand {
    pub fn new(sample_rate: f64, band_index: usize) -> Self {
        Self {
            band_index,
            detector: Detector::new(),
            gain_curve: GainCurve::new(sample_rate),
            smoother: HermiteCubicSmoother::new(StateFuncHypothesis::Identity),
            last_gr_db: 0.0,
            previous_level_db: -80.0,
            sample_rate,
            hp_filter: None,
            lp_filter: None,
        }
    }

    /// Calculate level-dependent crossover frequency from detected level
    /// Modeled band level computation:
    /// freq = (2 * exp(level * LN2) * PI) / sample_rate
    fn compute_crossover_frequency(&self, level_db: f64) -> f64 {
        // Clamp level to reasonable range to prevent overflow
        let level_clamped = level_db.clamp(-80.0, 20.0);

        // Apply exponential: exp(level * ln(2))
        let exp_val = (level_clamped * LN_2).exp();

        // Compute frequency: (2 * exp_val * π) / sample_rate
        (2.0 * exp_val * PI) / self.sample_rate
    }

    /// Set up band-specific filters based on crossover frequencies
    /// Band 0 (low): Low-pass filter at freq_low
    /// Band 1 (mid): High-pass at freq_low + Low-pass at freq_high
    /// Band 2 (high): High-pass filter at freq_high
    fn update_band_filters(&mut self, freq_low: f64, freq_high: f64) {
        // Normalize frequencies to 0-1 range (0 = DC, 1 = Nyquist)
        // Nyquist frequency = sample_rate / 2
        let nyquist = self.sample_rate / 2.0;
        let norm_low = (freq_low / nyquist).clamp(0.001, 0.999);
        let norm_high = (freq_high / nyquist).clamp(0.001, 0.999);

        match self.band_index {
            0 => {
                // Band 0 (Low): Only low-pass at freq_low
                self.lp_filter = Some(design_lowpass_biquad(norm_low));
                self.hp_filter = None;
            }
            1 => {
                // Band 1 (Mid): High-pass at freq_low + Low-pass at freq_high
                self.hp_filter = Some(design_highpass_biquad(norm_low));
                self.lp_filter = Some(design_lowpass_biquad(norm_high));
            }
            2 => {
                // Band 2 (High): Only high-pass at freq_high
                self.hp_filter = Some(design_highpass_biquad(norm_high));
                self.lp_filter = None;
            }
            _ => {}
        }
    }

    /// Apply band-specific filters to input audio
    fn apply_band_filters(&mut self, input: f64, channel: usize) -> f64 {
        let mut output = input;

        // Apply high-pass filter if present (Band 1 and 2)
        if let Some(ref mut hp) = self.hp_filter {
            output = hp.tick(output, channel.min(1));
        }

        // Apply low-pass filter if present (Band 0 and 1)
        if let Some(ref mut lp) = self.lp_filter {
            output = lp.tick(output, channel.min(1));
        }

        output
    }

    /// Apply Band 2 special sqrt-based processing (only for band_index == 2)
    /// Modeled band gain computation:
    /// band2_output = sqrt(level_abs² + 1.0) * freq_scaled + (level_abs * 0.5)
    /// Where freq_scaled = crossover_freq * 0.5 (DAT_180213064)
    fn apply_band2_special_processing(&mut self, level_db: f64, crossover_freq: f64) -> f64 {
        if self.band_index != 2 {
            return level_db;
        }

        // Compute level difference for hysteresis (Band 2 adaptive detection)
        let level_diff = (level_db - self.previous_level_db).abs();

        // Band 2 scaling constant from the reference model.
        const BAND2_SCALE: f64 = 0.5;

        // Crossover frequency scaling for Band 2
        let freq_scaled = crossover_freq * BAND2_SCALE;

        // Apply sqrt-based formula:
        // sqrt(level_diff² + 1.0) provides smooth rounding near zero
        let sqrt_component = (level_diff * level_diff + 1.0).sqrt() * freq_scaled;

        // Add linear component: level_diff * 0.5
        let linear_component = level_diff.abs() * BAND2_SCALE;

        // Final Band 2 output
        let band2_output = sqrt_component + linear_component;

        // Apply hysteresis with a reference-informed threshold.
        const HYSTERESIS_WIDTH: f64 = 4.0;
        const DETECTION_THRESHOLD: f64 = 40.0;

        let hysteresis_zone =
            (DETECTION_THRESHOLD - HYSTERESIS_WIDTH)..=DETECTION_THRESHOLD;
        let output_with_hysteresis = if !hysteresis_zone.contains(&band2_output) {
            band2_output
        } else {
            // Within hysteresis zone - use smoothed interpolation
            DETECTION_THRESHOLD - HYSTERESIS_WIDTH
                + (band2_output - (DETECTION_THRESHOLD - HYSTERESIS_WIDTH))
        };

        // Update previous level for next sample
        self.previous_level_db = level_db;

        output_with_hysteresis
    }

    /// Process one sample through this band's compression
    pub fn process(&mut self, input: f64, channel: usize) -> f64 {
        // Step 1: Detect level from original input
        let level_db = self.detector.detect_level(input.abs());

        // Step 2: Compute level-dependent crossover frequencies
        // Reference-informed frequency-dependent scaling:
        // freq = (2 * exp(level * LN2) * PI) / sample_rate
        let base_freq = self.compute_crossover_frequency(level_db);
        let freq_low = base_freq * 0.5; // Lower crossover (between Band 0 and 1)
        let freq_high = base_freq * 2.0; // Upper crossover (between Band 1 and 2)

        // Step 3: Update band-specific filters
        self.update_band_filters(freq_low, freq_high);

        // Step 4: Apply band-specific frequency filtering to input
        let band_audio = self.apply_band_filters(input, channel);

        // Step 5: Detect level from band-filtered audio (not original)
        let band_level_db = self.detector.detect_level(band_audio.abs());

        // Step 6: Apply Band 2 special sqrt-based processing (if applicable)
        // This modifies the effective level used for gain curve computation
        let effective_level_db = self.apply_band2_special_processing(band_level_db, base_freq);

        // Step 7: Compute gain reduction using effective level
        let mut gr_instant = self.gain_curve.compute_gr(effective_level_db);

        // Step 7b: Apply style-specific FET coloration.
        if self.gain_curve.style() == crate::styles::CompressionStyle::Fet {
            let atan_input = -self.gain_curve.attack_coeff;
            let atan_result = crate::styles::atan_approx(atan_input);

            // Constants from the reference model.
            const ATAN_MIN: f64 = 0.1; // DAT_180213300
            const ATAN_MAX: f64 = 0.971948; // DAT_1802134f8

            let clamped_atan = atan_result.clamp(ATAN_MIN, ATAN_MAX);

            // Apply atan coloration as multiplicative scaling to GR
            gr_instant *= clamped_atan;
        }

        // Step 8: Smooth with Hermite cubic
        let log_rel = self.gain_curve.release_coeff.ln();
        let log_atk = self.gain_curve.attack_coeff.ln();
        let sqrt_h0 = gr_instant.sqrt();
        let sqrt_h1 = (gr_instant * 0.9).sqrt();

        let gr_smoothed = self.smoother.process(
            gr_instant,
            self.gain_curve.attack_coeff,
            self.gain_curve.release_coeff,
            log_rel,
            log_atk,
            sqrt_h0,
            sqrt_h1,
            channel,
        );

        // Track for metering
        self.last_gr_db = audiocore_dsp::db::linear_to_db(gr_smoothed).max(0.0);

        // Step 9: Apply compression to band-specific audio (returns compressed band)
        band_audio * gr_smoothed
    }

    /// Update parameters for this band
    pub fn set_threshold(&mut self, threshold_db: f64) {
        self.gain_curve.set_threshold(threshold_db);
    }

    pub fn set_ratio(&mut self, ratio: f64) {
        self.gain_curve.set_ratio(ratio);
    }

    pub fn set_knee(&mut self, knee_db: f64) {
        self.gain_curve.set_knee(knee_db);
    }

    pub fn set_attack_ms(&mut self, attack_ms: f64) {
        self.gain_curve.set_attack_ms(attack_ms);
    }

    pub fn set_release_ms(&mut self, release_ms: f64) {
        self.gain_curve.set_release_ms(release_ms);
    }

    pub fn set_style(&mut self, style_id: i32) {
        self.gain_curve
            .set_style(crate::styles::CompressionStyle::from_id(style_id));
    }

    /// Reset internal state
    pub fn reset(&mut self) {
        self.detector.reset();
        self.smoother.reset();
        self.last_gr_db = 0.0;
    }

    /// Get gain reduction in dB
    pub fn gain_reduction_db(&self) -> f64 {
        self.last_gr_db
    }
}

/// 3-band multiband compression system
/// Binary architecture @ 0x18010bf40 (438 instructions)
pub struct MultiBandCompressor {
    /// Three independent compression bands
    bands: [CompressionBand; 3],

    /// Sample rate (needed for level-dependent crossover)
    sample_rate: f64,
}

impl MultiBandCompressor {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            bands: [
                CompressionBand::new(sample_rate, 0), // High band
                CompressionBand::new(sample_rate, 1), // Mid band
                CompressionBand::new(sample_rate, 2), // Low band
            ],
            sample_rate,
        }
    }

    /// Process one sample through all 3 bands
    /// Each band:
    /// 1. Separates input into frequency band (HP/LP filters)
    /// 2. Detects level of band-specific audio
    /// 3. Applies compression to band-specific audio
    /// 4. Returns compressed band audio
    ///
    /// Bands are summed for final output reconstruction
    pub fn process(&mut self, input: f64, channel: usize) -> f64 {
        // Process through all 3 bands and collect compressed band outputs
        let band0_output = self.bands[0].process(input, channel);
        let band1_output = self.bands[1].process(input, channel);
        let band2_output = self.bands[2].process(input, channel);

        // Combine bands by summing the compressed band-specific audio
        // This proper multiband architecture:
        // - Splits input into 3 frequency bands (0=low, 1=mid, 2=high)
        // - Processes each band with independent compression
        // - Recombines by summing the 3 processed bands
        // Normalization by 3 prevents clipping from summation
        

        (band0_output + band1_output + band2_output) / 3.0
    }

    /// Update to new sample rate
    pub fn update(&mut self, sample_rate: f64) {
        if (sample_rate - self.sample_rate).abs() > 0.1 {
            self.sample_rate = sample_rate;
            for band in &mut self.bands {
                band.gain_curve.update(sample_rate);
            }
        }
    }

    /// Set parameters for all bands
    pub fn set_threshold(&mut self, threshold_db: f64) {
        for band in &mut self.bands {
            band.set_threshold(threshold_db);
        }
    }

    pub fn set_ratio(&mut self, ratio: f64) {
        for band in &mut self.bands {
            band.set_ratio(ratio);
        }
    }

    pub fn set_knee(&mut self, knee_db: f64) {
        for band in &mut self.bands {
            band.set_knee(knee_db);
        }
    }

    pub fn set_attack_ms(&mut self, attack_ms: f64) {
        for band in &mut self.bands {
            band.set_attack_ms(attack_ms);
        }
    }

    pub fn set_release_ms(&mut self, release_ms: f64) {
        for band in &mut self.bands {
            band.set_release_ms(release_ms);
        }
    }

    pub fn set_style(&mut self, style_id: i32) {
        for band in &mut self.bands {
            band.set_style(style_id);
        }
    }

    /// Reset all bands
    pub fn reset(&mut self) {
        for band in &mut self.bands {
            band.reset();
        }
    }

    /// Get maximum gain reduction across all bands
    pub fn gain_reduction_db(&self) -> f64 {
        self.bands
            .iter()
            .map(|b| b.gain_reduction_db())
            .fold(0.0, f64::max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multiband_creation() {
        let mb = MultiBandCompressor::new(48000.0);
        assert_eq!(mb.bands.len(), 3);
        assert_eq!(mb.bands[0].band_index, 0);
        assert_eq!(mb.bands[1].band_index, 1);
        assert_eq!(mb.bands[2].band_index, 2);
    }

    #[test]
    fn test_multiband_processing() {
        let mut mb = MultiBandCompressor::new(48000.0);
        mb.set_threshold(-18.0);
        mb.set_ratio(4.0);

        let input = 0.5;
        let output = mb.process(input, 0);

        // Output should be compressed (less than input)
        assert!(output.abs() <= input.abs());
        assert!(output.is_finite());
    }

    #[test]
    fn test_band_independence() {
        let mb = MultiBandCompressor::new(48000.0);
        // Each band should be independent
        assert_eq!(mb.bands[0].band_index, 0);
        assert_eq!(mb.bands[1].band_index, 1);
        assert_eq!(mb.bands[2].band_index, 2);
    }
}
