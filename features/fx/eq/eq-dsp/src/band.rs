//! Single EQ band using the pro design pipeline.
//!
//! Replaces eq-dsp v2's band.rs. Uses the Pro-Q 4 architecture:
//! analog prototype -> transform -> ZPK -> biquad sections, with
//! support for all 13 filter types and variable order up to 16.

use crate::biquad::PASSTHROUGH;
use crate::design::{self, FilterType};
use crate::section::{Df1Section, Tdf2Section};

/// Maximum filter order (number of poles).
pub const MAX_ORDER: usize = 16;

/// Maximum number of cascaded 2nd-order sections.
const MAX_SECTIONS: usize = MAX_ORDER / 2 + 1; // +1 for odd-order 1st-order section

/// A single EQ band with variable order, using the pro ZPK design pipeline.
/// Which part of the stereo image a band processes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum Placement {
    /// Both channels (default).
    #[default]
    Stereo = 0,
    Left = 1,
    Right = 2,
    Mid = 3,
    Side = 4,
}

impl Placement {
    pub fn from_index(idx: u32) -> Placement {
        match idx {
            1 => Placement::Left,
            2 => Placement::Right,
            3 => Placement::Mid,
            4 => Placement::Side,
            _ => Placement::Stereo,
        }
    }
}

pub struct Band {
    pub filter_type: FilterType,
    pub freq_hz: f64,
    pub gain_db: f64,
    pub q: f64,
    pub order: usize,
    pub enabled: bool,
    /// Stereo placement: which part of the image this band filters.
    pub placement: Placement,
    /// Gain-Q interaction amount (0.0 = off, 1.0 = full). Only affects Peak.
    /// From Pro-Q 4 binary: offset 0x8c in band parameter object.
    pub gain_q_interaction: f64,
    /// Enable/bypass crossfade position (0 = dry, 1 = wet) — ~5 ms
    /// ramp so toggling a band never clicks.
    bypass_ramp: f64,

    sections: [Tdf2Section; MAX_SECTIONS],
    df1_sections: [Df1Section; MAX_SECTIONS],
    use_df1: bool,
    num_sections: usize,
    output_gain: f64,
    sample_rate: f64,
}

impl Band {
    pub fn new() -> Self {
        Self {
            filter_type: FilterType::Peak,
            freq_hz: 1000.0,
            gain_db: 0.0,
            q: 0.707,
            order: 2,
            enabled: true,
            gain_q_interaction: 0.0,
            placement: Placement::default(),
            bypass_ramp: 0.0,
            sections: std::array::from_fn(|_| Tdf2Section::new()),
            df1_sections: std::array::from_fn(|_| Df1Section::new()),
            use_df1: false,
            num_sections: 1,
            output_gain: 1.0,
            sample_rate: 48000.0,
        }
    }

    /// Recalculate coefficients using the pro ZPK design pipeline.
    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;

        if !self.enabled {
            return;
        }

        let order = self.order.clamp(0, MAX_ORDER);

        self.output_gain = 1.0;

        // Pro-Q 4 applies gain as flat output for Notch and Bandpass
        if matches!(self.filter_type, FilterType::Notch | FilterType::Bandpass)
            && self.gain_db.abs() > 0.001
        {
            self.num_sections = 0;
            self.output_gain = 10.0_f64.powf(self.gain_db / 20.0);
            return;
        }

        if order == 0 {
            self.num_sections = 0;
            return;
        }

        // Apply gain-Q interaction (only affects Peak, off by default)
        let q = if self.filter_type == FilterType::Peak && self.gain_q_interaction > 0.001 {
            design::apply_gain_q_interaction(self.q, self.gain_db, self.gain_q_interaction)
        } else {
            self.q
        };

        // Apply type-specific parameter adjustments from Pro-Q 4 binary
        let (effective_q, effective_gain) = match self.filter_type {
            FilterType::FlatTilt => {
                // Type 6: clip Q to 1.885 (from binary constant TYPE_6_CLIP)
                (q.min(1.884955592153876), self.gain_db)
            }
            FilterType::ShelfAlt => {
                // Type 12: same Q clipping as FlatTilt
                (q.min(1.884955592153876), self.gain_db)
            }
            _ => {
                // All other types: pass through raw Q and gain
                (q, self.gain_db)
            }
        };

        // Use pro design pipeline: analog prototype -> ZPK -> biquad sections
        let sos = design::design_filter(
            self.filter_type,
            self.freq_hz,
            effective_q,
            effective_gain,
            sample_rate,
            order,
        );

        self.num_sections = sos.len().min(MAX_SECTIONS);
        // Use DF1 for Peak filters (binary-exact processing form)
        self.use_df1 = self.filter_type == FilterType::Peak;

        for (i, coeffs) in sos.iter().enumerate().take(self.num_sections) {
            // Stability check
            let stable = coeffs.iter().all(|c| c.is_finite() && c.abs() < 1e12);
            let coeffs = if stable { *coeffs } else { PASSTHROUGH };
            if self.use_df1 {
                self.df1_sections[i].set_coeffs(coeffs);
            } else {
                self.sections[i].set_coeffs(coeffs);
            }
        }
    }

    /// Process a single sample through all cascaded sections.
    ///
    /// Enable/disable is crossfaded over ~5 ms (the ramp advances on
    /// channel 0 so stereo pairs stay matched).
    #[inline]
    pub fn tick(&mut self, sample: f64, ch: usize) -> f64 {
        // Fully-settled bypass: zero work (no ramp math, no cascade).
        if !self.enabled && self.bypass_ramp == 0.0 {
            return sample;
        }
        if ch == 0 {
            // 5 ms at 48 kHz ≈ coefficient 0.004; sample-rate scaling
            // here would need plumbing — the click protection is what
            // matters, not the exact ms.
            const RAMP_COEFF: f64 = 0.004;
            let target = if self.enabled { 1.0 } else { 0.0 };
            self.bypass_ramp += (target - self.bypass_ramp) * RAMP_COEFF;
            if !self.enabled && self.bypass_ramp < 1.0e-4 {
                self.bypass_ramp = 0.0;
            }
        }
        if self.bypass_ramp <= 0.0 {
            return sample;
        }
        let dry = sample;
        let wet = self.tick_inner(sample, ch);
        dry + self.bypass_ramp * (wet - dry)
    }

    /// The raw cascade (no bypass crossfade).
    #[inline]
    fn tick_inner(&mut self, sample: f64, ch: usize) -> f64 {
        let mut out = sample;
        if self.use_df1 {
            for i in 0..self.num_sections {
                out = self.df1_sections[i].tick(out, ch);
            }
        } else {
            for i in 0..self.num_sections {
                out = self.sections[i].tick(out, ch);
            }
        }
        out * self.output_gain
    }

    /// Force the bypass ramp fully open/closed (preset loads — no fade).
    pub fn snap_bypass(&mut self) {
        self.bypass_ramp = if self.enabled { 1.0 } else { 0.0 };
    }

    /// True when the band contributes nothing to the signal (disabled
    /// and the bypass ramp fully settled) — the chain skips it.
    #[inline]
    pub fn is_idle(&self) -> bool {
        !self.enabled && self.bypass_ramp == 0.0
    }

    /// Reset all section state to zero (ramp too — matches a fresh band).
    pub fn reset(&mut self) {
        for s in &mut self.sections {
            s.reset();
        }
        for s in &mut self.df1_sections {
            s.reset();
        }
        self.bypass_ramp = 0.0;
    }
}

impl Default for Band {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_band_passes_through() {
        let mut band = Band::new();
        band.gain_db = 0.0;
        band.update(48000.0);

        // With 0 dB peak, output should approximate input
        let out = band.tick(1.0, 0);
        assert!(
            (out - 1.0).abs() < 0.1,
            "Default band should pass through, got {out}"
        );
    }

    #[test]
    fn disabled_band_passes_through() {
        let mut band = Band::new();
        band.enabled = false;
        band.update(48000.0);

        let out = band.tick(0.5, 0);
        assert!(
            (out - 0.5).abs() < 1e-14,
            "Disabled band should pass through exactly"
        );
    }

    #[test]
    fn band_reset_clears_state() {
        let mut band = Band::new();
        band.filter_type = FilterType::Lowpass;
        band.freq_hz = 1000.0;
        band.order = 4;
        band.update(48000.0);

        // Process some samples to build state
        for _ in 0..100 {
            band.tick(1.0, 0);
        }

        band.reset();

        // After reset, first sample should match a fresh band
        let mut fresh = Band::new();
        fresh.filter_type = FilterType::Lowpass;
        fresh.freq_hz = 1000.0;
        fresh.order = 4;
        fresh.update(48000.0);

        let out_reset = band.tick(0.5, 0);
        let out_fresh = fresh.tick(0.5, 0);
        assert!(
            (out_reset - out_fresh).abs() < 1e-12,
            "Reset band should match fresh: {out_reset} vs {out_fresh}"
        );
    }

    #[test]
    fn bandpass_gain_applied_as_output() {
        let mut band = Band::new();
        band.filter_type = FilterType::Bandpass;
        band.gain_db = 6.0;
        band.update(48000.0);

        // With gain on bandpass, num_sections should be 0 and output_gain set
        assert_eq!(band.num_sections, 0);
        let expected_gain = 10.0_f64.powf(6.0 / 20.0);
        assert!(
            (band.output_gain - expected_gain).abs() < 1e-10,
            "Output gain should be ~{expected_gain}, got {}",
            band.output_gain
        );
    }
}
