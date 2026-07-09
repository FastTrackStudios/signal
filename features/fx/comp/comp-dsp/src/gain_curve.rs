//! Gain curve computation: threshold, ratio, knee with attack/release coefficients.
//!
//! Modeled from Pro-C 3 analysis, with explicit approximations for style
//! coefficient scaling and band-dependent response.

use crate::styles::CompressionStyle;
use std::f64::consts::PI;

/// Coefficient scaling constants used by the modeled style response.
struct CoefficientScalingConstants {
    /// Frequency limit for Style 2 special case (DAT_1802135c8)
    freq_limit_style2: f64,
    /// Scaling factor for frequency computation (DAT_180213418)
    freq_scaling: f64,
    /// Threshold value in frequency scaling (DAT_1802132b8)
    freq_threshold: f64,
    /// Blend factor for complex frequency-dependent computation
    blend_factor: f64,
}

impl Default for CoefficientScalingConstants {
    fn default() -> Self {
        Self {
            freq_limit_style2: 2.985, // DAT_1802135c8 approximation
            freq_scaling: 0.5,        // DAT_180213418
            freq_threshold: 0.785,    // DAT_1802132b8
            blend_factor: 0.789,      // DAT blend constant
        }
    }
}

/// Gain reduction curve processor with attack/release coefficients.
#[derive(Clone)]
pub struct GainCurve {
    pub threshold_db: f64,
    pub ratio: f64,
    pub knee_db: f64,
    pub range_db: f64,

    // Attack/release coefficients (linear domain)
    pub attack_coeff: f64,
    pub release_coeff: f64,
    pub other_coeff: f64,

    // Sample rate dependency
    sample_rate: f64,
    attack_ms: f64,
    release_ms: f64,

    // Compression style - affects attack/release behavior
    style: CompressionStyle,
}

impl GainCurve {
    pub fn new(sample_rate: f64) -> Self {
        let mut curve = Self {
            threshold_db: 0.0,
            ratio: 4.0,
            knee_db: 2.0,
            range_db: 60.0,
            attack_coeff: 0.01,
            release_coeff: 0.05,
            other_coeff: 0.1,
            sample_rate,
            attack_ms: 10.0,
            release_ms: 50.0,
            style: CompressionStyle::Clean,
        };
        curve.update_coefficients();
        curve
    }

    /// Set compression style (affects attack/release response)
    pub fn set_style(&mut self, style: CompressionStyle) {
        if self.style != style {
            self.style = style;
            self.update_coefficients();
        }
    }

    /// Get current compression style
    pub fn style(&self) -> CompressionStyle {
        self.style
    }

    /// Update coefficients when attack/release times change.
    /// Applies style-specific multipliers inspired by the reference analysis.
    pub fn update_coefficients(&mut self) {
        // Simplified coefficient computation from attack/release times
        // rather than the original plugin's full coefficient resolver.
        let base_attack = (-2.0 / (self.sample_rate * self.attack_ms / 1000.0)).exp();
        let base_release = (-2.0 / (self.sample_rate * self.release_ms / 1000.0)).exp();

        // Apply style-specific multipliers from the current model:
        // FET: 0.9x attack (faster), 0.95x release (slower) = more aggressive
        // VCA: 1.0x both (baseline, clean)
        // Optical: 1.15x attack (slower), 0.93x release (faster) = vintage character
        let (attack_mult, release_mult) = match self.style {
            CompressionStyle::Clean => (1.0, 1.0),
            CompressionStyle::Fet => (0.9, 0.95),
            CompressionStyle::Vca => (1.0, 1.0),
            CompressionStyle::Optical => (1.15, 0.93),
            CompressionStyle::Reserved => (1.0, 1.0),
        };

        self.attack_coeff = base_attack * attack_mult;
        self.release_coeff = base_release * release_mult;
        self.other_coeff = self.release_coeff; // Placeholder
    }

    /// Set attack time in milliseconds.
    pub fn set_attack_ms(&mut self, attack_ms: f64) {
        self.attack_ms = attack_ms.max(0.1);
        self.update_coefficients();
    }

    /// Set release time in milliseconds.
    pub fn set_release_ms(&mut self, release_ms: f64) {
        self.release_ms = release_ms.max(0.1);
        self.update_coefficients();
    }

    /// Set attack and release together without changing external parameter state.
    pub fn set_time_constants_ms(&mut self, attack_ms: f64, release_ms: f64) {
        self.attack_ms = attack_ms.max(0.1);
        self.release_ms = release_ms.max(0.1);
        self.update_coefficients();
    }

    /// Set threshold in dB.
    pub fn set_threshold(&mut self, threshold_db: f64) {
        self.threshold_db = threshold_db;
    }

    /// Set ratio (e.g., 4.0 = 4:1).
    pub fn set_ratio(&mut self, ratio: f64) {
        self.ratio = ratio.max(1.0);
    }

    /// Set knee width in dB.
    pub fn set_knee(&mut self, knee_db: f64) {
        self.knee_db = knee_db.max(0.0);
    }

    /// Update to new sample rate (recalculates coefficients).
    pub fn update(&mut self, sample_rate: f64) {
        if (sample_rate - self.sample_rate).abs() > 0.1 {
            self.sample_rate = sample_rate;
            self.update_coefficients();
        }
    }

    /// Compute gain reduction (linear) from detected level (dB).
    /// Returns linear GR where 1.0 = no reduction, 0.5 = -6 dB reduction.
    pub fn compute_gr(&self, level_db: f64) -> f64 {
        let thresh = self.threshold_db;
        let half_knee = self.knee_db / 2.0;

        if level_db < thresh - half_knee {
            1.0 // No compression
        } else if level_db > thresh + half_knee {
            // Full compression above knee
            // GR is computed as: excess * (1 - 1/ratio)
            // This gives positive dB reduction amount
            let excess = level_db - thresh;
            let gr_db_amount = excess * (1.0 - 1.0 / self.ratio);
            // Convert to linear gain (negative dB = linear < 1.0)
            db_to_linear(-gr_db_amount.min(self.range_db))
        } else {
            // Soft knee transition
            let x = (level_db - (thresh - half_knee)) / self.knee_db;
            let knee_factor = x * x;
            let excess = level_db - thresh;
            let gr_db_amount = excess * (1.0 - 1.0 / self.ratio) * knee_factor;
            db_to_linear(-gr_db_amount)
        }
    }

    /// Apply style-specific coefficient scaling to attack/release.
    /// Applies the post-curve transformations used by this modeled style layer.
    /// Returns scaled attack_coeff and release_coeff based on style.
    pub fn apply_coefficient_scaling(&self, frequency_hz: f64) -> (f64, f64) {
        let constants = CoefficientScalingConstants::default();

        match self.style {
            CompressionStyle::Fet => {
                // FET style: frequency-dependent scaling
                // For FET: scale coefficients based on frequency input
                if frequency_hz < constants.freq_limit_style2 {
                    // Frequency scaling for FET mode
                    let scaled_freq = frequency_hz * constants.freq_scaling;
                    let attack_scaled = self.attack_coeff * (1.0 + scaled_freq);
                    let release_scaled = self.release_coeff * (1.0 + scaled_freq);
                    (attack_scaled, release_scaled)
                } else {
                    (self.attack_coeff, self.release_coeff)
                }
            }
            CompressionStyle::Optical => {
                // Optical style: vintage tube response with frequency-dependent coloration
                // Optical is slower and more vintage
                // Scale inversely with frequency for vintage character
                let freq_factor =
                    (constants.freq_threshold / frequency_hz.max(0.001)).clamp(0.8, 1.2);
                let attack_scaled = self.attack_coeff * freq_factor;
                let release_scaled = self.release_coeff * freq_factor;
                (attack_scaled, release_scaled)
            }
            CompressionStyle::Vca => {
                // VCA style: pure mathematical, no frequency-dependent scaling
                // Clean, transparent response with standard coefficients
                (self.attack_coeff, self.release_coeff)
            }
            _ => {
                // Clean and other styles: direct coefficients
                (self.attack_coeff, self.release_coeff)
            }
        }
    }

    /// Compute post-curve coefficient modifications for band-specific processing.
    /// This applies style-specific transformations after gain curve computation
    /// but before final GR application.
    /// Modeled from the reference style analysis.
    pub fn compute_coefficient_adjustment(&self, input_frequency: f64) -> f64 {
        let constants = CoefficientScalingConstants::default();

        match self.style {
            CompressionStyle::Fet => {
                // FET: frequency-dependent coefficient adjustment
                // Creates the characteristic FET frequency response
                if input_frequency < constants.freq_limit_style2 {
                    // Compute frequency delta
                    let freq_base = constants.freq_threshold;
                    let freq_delta = (input_frequency - freq_base).abs();

                    // Apply frequency-dependent scaling (from binary @ Step 3-4)
                    let frequency_scale = ((freq_delta - 0.5) * 0.5).clamp(0.0, 1.0);
                    let scaled_by_pi = frequency_scale * PI;

                    // Apply to band level scaling
                    1.0 - (scaled_by_pi / 10.0).min(0.2) // Limit scaling to prevent extremes
                } else {
                    1.0
                }
            }
            CompressionStyle::Optical => {
                // Optical: vintage frequency-dependent processing
                // Creates smooth vintage coloration
                let freq_normalized = input_frequency / 1000.0; // Normalize to kHz-like scale
                let vintage_factor = (constants.blend_factor / freq_normalized).clamp(0.7, 1.3);
                vintage_factor
            }
            _ => {
                // VCA and Clean: no post-curve adjustment
                1.0
            }
        }
    }
}

#[inline]
fn db_to_linear(db: f64) -> f64 {
    10.0_f64.powf(db / 20.0)
}
