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
/// Biquads a band can hold.
///
/// The integer design needs `MAX_ORDER / 2` plus one more for an odd order's
/// first-order tail. A fractional slope adds its ladder on top of that, so the
/// budget has to cover both — otherwise the steepest bands silently drop
/// sections at the `min(MAX_SECTIONS)` clamp below and come out shallower than
/// they were asked to be.
const MAX_SECTIONS: usize = MAX_ORDER / 2 + 1 + design::fractional::SECTION_COUNT;

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
    #[must_use]
    pub const fn from_index(idx: u32) -> Self {
        match idx {
            1 => Self::Left,
            2 => Self::Right,
            3 => Self::Mid,
            4 => Self::Side,
            _ => Self::Stereo,
        }
    }
}

pub struct Band {
    pub filter_type: FilterType,
    pub freq_hz: f64,
    pub gain_db: f64,
    pub q: f64,
    pub order: usize,
    /// Slope beyond `order`, in poles, 0..1.
    ///
    /// Pro-Q's slope control is continuous, so a band can sit between two
    /// orders — 7.5 or 15.25 dB/oct. The integer part is `order`; this is the
    /// remainder, realized by a pole-zero ladder (see
    /// [`crate::design::fractional`]). Zero for every band that lands on an
    /// integer, which is most of them.
    pub fractional_order: f64,
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
    #[must_use]
    pub fn new() -> Self {
        Self {
            filter_type: FilterType::Peak,
            freq_hz: 1000.0,
            gain_db: 0.0,
            q: 0.707,
            order: 2,
            fractional_order: 0.0,
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

        // Brickwall's order is a sentinel, not a pole count — clamping it to
        // MAX_ORDER turns it back into the 96 dB/oct cascade it is meant to
        // replace, which is exactly the bug this design was written to fix.
        let order = if self.order == crate::slope::BRICKWALL_ORDER {
            self.order
        } else {
            self.order.clamp(0, MAX_ORDER)
        };

        self.output_gain = 1.0;

        // A Notch's gain field does nothing at all in the plugin, exactly as a
        // Bandpass's does not. Both used to take this path — a flat output
        // trim and no filter built — and Bandpass was fixed when it was
        // measured; the Notch case was left because it had not been. Measured
        // now, at 1 kHz Q 4, the plugin gives the *identical* curve at -6 dB
        // and at +6:
        //
        // ```text
        //      Hz    Pro-Q -6   Pro-Q +6    ours (either)
        //     794       -1.97      -1.97           -/+6.00
        //     891       -5.23      -5.23           -/+6.00
        //    1000     -126.26    -126.26           -/+6.00
        // ```
        //
        // So the gain is ignored and the notch is built, which is what the
        // rest of this function now does for every shape.

        // Order zero with a fractional remainder is a slope shallower than
        // 6 dB/oct: no integer design at all, just the ladder.
        let fractional_only = order == 0 && self.fractional_order > 1.0e-6;
        if order == 0 && !fractional_only {
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
                (q.min(1.884_955_592_153_876), self.gain_db)
            }
            FilterType::ShelfAlt => {
                // Type 12: same Q clipping as FlatTilt
                (q.min(1.884_955_592_153_876), self.gain_db)
            }
            _ => {
                // All other types: pass through raw Q and gain
                (q, self.gain_db)
            }
        };

        // Use pro design pipeline: analog prototype -> ZPK -> biquad sections
        let sos = if fractional_only {
            Vec::new()
        } else {
            design::design_filter(
                self.filter_type,
                self.freq_hz,
                effective_q,
                effective_gain,
                sample_rate,
                order,
            )
        };

        // A fractional slope adds a ladder on top of the integer design —
        // but ONLY for the two shapes whose slope is a one-sided roll-off
        // into a stop band.
        //
        // A shelf settles at a finite gain and a bell returns to unity on both
        // sides; for those, "slope" is the steepness of a *bounded* transition,
        // so a ladder that keeps falling for four octaves does not steepen
        // them, it tilts them. Applied to bells and shelves this cost 98 dB on
        // one preset — a 2.45-slope bell at 2.2 kHz picked up a high-cut that
        // was never asked for. Those shapes take the nearest integer order
        // instead, which is what the caller hands over.
        let mut sos = sos;
        if self.fractional_order > 1.0e-6 {
            if let Some(high_pass) = match self.filter_type {
                FilterType::Highpass => Some(true),
                FilterType::Lowpass => Some(false),
                _ => None,
            } {
                sos.extend(design::fractional::sections(
                    self.freq_hz,
                    self.fractional_order,
                    sample_rate,
                    high_pass,
                ));
            }
        }

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

    /// The band's magnitude response at `hz`, in dB.
    ///
    /// Read off the coefficients the band is actually running, so it costs no
    /// redesign and cannot drift from what the audio hears. A disabled band
    /// contributes nothing.
    pub fn magnitude_db(&self, hz: f64, sample_rate: f64) -> f64 {
        if !self.enabled || self.num_sections == 0 {
            return if self.enabled {
                20.0 * self.output_gain.max(1.0e-12).log10()
            } else {
                0.0
            };
        }
        let w = core::f64::consts::TAU * hz / sample_rate;
        let (cw, sw) = (w.cos(), w.sin());
        let (c2w, s2w) = ((2.0 * w).cos(), (2.0 * w).sin());
        let mut db = 20.0 * self.output_gain.max(1.0e-12).log10();
        for i in 0..self.num_sections {
            let [b0, b1, b2, a1, a2] = if self.use_df1 {
                self.df1_sections[i].coeffs()
            } else {
                self.sections[i].coeffs()
            };
            // H(e^jw) with z^-1 = cos w - j sin w.
            let num_re = b2.mul_add(c2w, b0 + b1 * cw);
            let num_im = -(b1 * sw + b2 * s2w);
            let den_re = 1.0 + a1 * cw + a2 * c2w;
            let den_im = -(a1 * sw + a2 * s2w);
            let num = (num_re * num_re + num_im * num_im).max(1.0e-30);
            let den = (den_re * den_re + den_im * den_im).max(1.0e-30);
            db += 10.0 * (num / den).log10();
        }
        db
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
    pub const fn is_idle(&self) -> bool {
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

    /// A bandpass keeps filtering when it carries gain.
    ///
    /// This test used to assert the opposite — that gain on a bandpass set
    /// `num_sections = 0` and became a flat output trim. Measured against
    /// Pro-Q 4, that is not what the plugin does: a bandpass at Q 2.5 peaks
    /// near 0 dB at its centre and is over 100 dB down two octaves away, while
    /// the flat-trim path produced a level line across the whole spectrum. On
    /// "Band Pass Narrow" the difference was 97 dB of mean error, which fell
    /// to 0.01 dB once the filter was built. A bandpass has a unity passband,
    /// so its gain field is not an output trim.
    ///
    /// Notch keeps the old behaviour: it has not been measured the same way,
    /// and it is a different filter.
    #[test]
    fn bandpass_with_gain_still_filters() {
        let mut band = Band::new();
        band.filter_type = FilterType::Bandpass;
        band.freq_hz = 1000.0;
        band.q = 2.5;
        band.gain_db = 6.0;
        band.order = 2;
        band.update(48000.0);

        assert!(
            band.num_sections > 0,
            "a bandpass with gain must still build its filter",
        );
        assert!(
            (band.output_gain - 1.0).abs() < 1e-10,
            "and must not become a flat trim (output_gain {})",
            band.output_gain,
        );
    }

    /// A Notch's gain field is inert, exactly as a Bandpass's is.
    ///
    /// Measured against the plugin at 1 kHz Q 4: the response at -6 dB and at
    /// +6 dB is the same curve to the second decimal, and it is a notch, not
    /// a trim. This used to build no filter at all and apply the gain as a
    /// flat output scale.
    #[test]
    fn notch_gain_is_ignored() {
        let mut band = Band::new();
        band.filter_type = FilterType::Notch;
        band.gain_db = 6.0;
        band.update(48000.0);

        assert!(band.num_sections > 0, "the notch must still be built");
        assert!(
            (band.output_gain - 1.0).abs() < 1e-12,
            "no output trim, got {}",
            band.output_gain
        );

        let mut plain = Band::new();
        plain.filter_type = FilterType::Notch;
        plain.gain_db = 0.0;
        plain.update(48000.0);
        assert_eq!(
            band.num_sections, plain.num_sections,
            "gain must not change how the filter is built",
        );

        // And it is a notch: a run of the band's own frequency comes out
        // essentially silent whatever the gain field says.
        let sr = 48_000.0;
        let mut rms = 0.0f64;
        for i in 0..(sr as usize) {
            let x = (core::f64::consts::TAU * band.freq_hz * i as f64 / sr).sin();
            let y = band.tick(x, 0);
            if i > sr as usize / 2 {
                rms += y * y;
            }
        }
        let db = 10.0 * (rms / (sr / 2.0)).max(1e-30).log10();
        assert!(db < -60.0, "the band's own frequency should be notched out, got {db:.1} dB");
    }
}
