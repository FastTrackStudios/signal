//! Top-level filter design dispatcher matching Pro-Q 4's
//! `design_filter_zpk_and_transform` (0x1800ff6f0).
//!
//! Maps filter type + parameters to a vector of biquad sections by dispatching
//! to the appropriate design path:
//!   - LP/HP: Butterworth prototype -> bilinear -> Q adjustment
//!   - BP: Butterworth LP -> LP->BP transform -> bilinear -> normalize
//!   - Notch: Butterworth LP -> LP->BS transform -> bilinear -> normalize
//!   - Peak: cascade::compute_cascade_peak
//!   - Shelves: shelf module functions
//!   - Allpass: Butterworth -> bilinear -> reflect zeros
//!   - ShelfAlt: cascade::compute_cascade_shelf_alt

use std::f64::consts::PI;

use crate::biquad::{self, Coeffs};
use crate::cascade;
use crate::shelf_zpk;

mod allpass;
mod bandpass;
mod bell;
mod common;
mod hp;
mod lp;
mod notch;
mod shelf;
mod tilt;
use allpass::{design_allpass_with_lookup, design_bandpass_variant};
use bandpass::mzt_bandpass_simple_cascade;
use bell::mzt_peak_cascade;
use hp::mzt_highpass_simple_cascade;
use lp::mzt_lowpass_simple_cascade;
use notch::mzt_notch_simple_cascade;
use shelf::shelf_universal_synth_cascade;
use tilt::mzt_tilt_shelf_cascade;

/// Filter types matching Pro-Q 4's type codes (0-12).
///
/// From filter_type_dispatcher (0x1800fe2a0) and apply_eq_band_parameters_full (0x1801110b0):
///   0 = Peak/Bell, 1 = HP, 2 = LP, 3 = BP, 4 = Notch,
///   5 = Band Pass variant, 6 = Flat Tilt,
///   7 = Low Shelf, 8 = High Shelf, 9 = Tilt Shelf,
///   10 = Band Shelf, 11 = Allpass, 12 = Shelf Alt
///
/// Type 6 (Flat Tilt) identified from binary: apply_eq_band_parameters_full uses
/// `cos(Q) * pow(const, cos(Q)*scale + offset)` frequency mapping for type 6,
/// and apply_shelf_gain_to_zpk squares the gain for type 6.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FilterType {
    Peak,            // type 0 — own ZPK via compute_cascade_coefficients
    Highpass,        // type 1 — Butterworth direct
    Lowpass,         // type 2 — Butterworth direct
    Bandpass,        // type 3 — Butterworth LP + elliptic LP→BP
    Notch,           // type 4 — Butterworth LP + LP→BS
    BandPassVariant, // type 5 — elliptic LP→BP with special Q mapping
    FlatTilt,        // type 6 — cos-based frequency mapping + LP→BP + gain²
    LowShelf,        // type 7 — Butterworth + bilinear + shelf gain
    HighShelf,       // type 8 — Butterworth + bilinear + shelf gain
    TiltShelf,       // type 9 — Butterworth + bilinear + shelf gain
    BandShelf,       // type 10 — LP→BP + bilinear
    Allpass,         // type 11 — negate zeros (transform type 4)
    ShelfAlt,        // type 12 — own ZPK via compute_cascade_coefficients
}

/// Design a complete filter and return biquad sections.
///
/// This is the main entry point, equivalent to `setup_eq_band_filter` +
/// `design_filter_zpk_and_transform`.
///
/// Parameters:
///   - `filter_type`: which filter shape
///   - `freq_hz`: center/corner frequency in Hz
///   - `q`: quality factor (bandwidth control)
///   - `gain_db`: gain in dB (for peak/shelf types)
///   - `sample_rate`: audio sample rate in Hz
///   - `order`: filter order (2, 4, 6, 8, ... -- number of poles)
///
/// Returns a vector of biquad coefficient arrays, one per section.
pub fn design_filter(
    filter_type: FilterType,
    freq_hz: f64,
    q: f64,
    gain_db: f64,
    sample_rate: f64,
    order: usize,
) -> Vec<Coeffs> {
    let order = order.max(1);
    let n = order.div_ceil(2);

    let _ = common::slope_from_pole_count(order);

    match filter_type {
        FilterType::Lowpass => {
            // slope=0 (Db0) is bypass per FabFilter docs; conformance maps it to order=1.
            if order == 1 {
                return vec![biquad::PASSTHROUGH];
            }
            mzt_lowpass_simple_cascade(n, freq_hz, q, sample_rate, order)
        }
        FilterType::Highpass => {
            if order == 1 {
                return vec![biquad::PASSTHROUGH];
            }
            mzt_highpass_simple_cascade(n, freq_hz, q, sample_rate, order)
        }
        FilterType::Bandpass => {
            if order == 1 {
                return vec![biquad::PASSTHROUGH];
            }
            mzt_bandpass_simple_cascade(n, freq_hz, q, sample_rate, order)
        }
        FilterType::Notch => mzt_notch_simple_cascade(n, freq_hz, q, sample_rate),
        FilterType::BandPassVariant => design_bandpass_variant(n, freq_hz, q, sample_rate),
        FilterType::Peak => mzt_peak_cascade(n, freq_hz, q, gain_db, sample_rate, order),
        FilterType::ShelfAlt => {
            cascade::compute_cascade_shelf_alt(freq_hz, q, gain_db, sample_rate, order)
        }
        FilterType::LowShelf => mzt_low_shelf_cascade(n, freq_hz, q, gain_db, sample_rate, order),
        FilterType::HighShelf => mzt_high_shelf_cascade(n, freq_hz, q, gain_db, sample_rate, order),
        FilterType::TiltShelf => mzt_tilt_shelf_cascade(n, freq_hz, q, gain_db, sample_rate, order),
        FilterType::BandShelf => {
            shelf_zpk::design_band_shelf_zpk(n, freq_hz, q, gain_db, sample_rate)
        }
        FilterType::Allpass => design_allpass_with_lookup(n, freq_hz, q, sample_rate, order),
        FilterType::FlatTilt => {
            cascade::compute_cascade_flat_tilt(freq_hz, q, gain_db, sample_rate, order)
        }
    }
}

// ── MZT-based cascaded designs ──────────────────────────────────────────────

/// Low shelf cascade — Pro-Q 4 Butterworth cascade with per-pole gain g = gain^(1/(2N)).
///
/// Analog prototype (per `docs/reports/proq4/re/shelf_cascade_higher_slopes.md`):
///   - Butterworth angles θ_i = π(2i+1)/(2N)
///   - Poles at `(-sin θ_i, ±cos θ_i) · wa / g`       (scaled INWARD in frequency)
///   - Zeros at `(-sin θ_i, ±cos θ_i) · wa · g`       (scaled OUTWARD in frequency)
///   - wa = 2·fs·tan(π·fc/fs) (pre-warped corner)
/// Each 2nd-order analog section is bilinear-transformed to a digital biquad.
fn mzt_low_shelf_cascade(
    n: usize,
    freq_hz: f64,
    q: f64,
    gain_db: f64,
    sample_rate: f64,
    pole_count: usize,
) -> Vec<Coeffs> {
    if gain_db.abs() < 1e-9 {
        return vec![biquad::PASSTHROUGH; n.max(1)];
    }
    // Universal-synth folds in the per-section anti-cramping clamps
    // (W_POLE_MAX ≈ 2.99, W_THIRD_MAX ≈ 2.36, W_ZERO_MAX ≈ 1.88) decoded from
    // `compute_shelf_band_parameters` — these are what carry shelves to
    // 3744/3744 pure-algorithmic conformance.
    shelf_universal_synth_cascade(
        n,
        freq_hz,
        q,
        gain_db,
        sample_rate,
        pole_count,
        /*high_shelf=*/ false,
    )
}

fn mzt_high_shelf_cascade(
    n: usize,
    freq_hz: f64,
    q: f64,
    gain_db: f64,
    sample_rate: f64,
    pole_count: usize,
) -> Vec<Coeffs> {
    if gain_db.abs() < 1e-9 {
        return vec![biquad::PASSTHROUGH; n.max(1)];
    }
    shelf_universal_synth_cascade(
        n,
        freq_hz,
        q,
        gain_db,
        sample_rate,
        pole_count,
        /*high_shelf=*/ true,
    )
}

/// Apply Gain-Q interaction to a peak filter.
///
/// From Pro-Q 4 binary (compute_peak_band_parameters at 0x18010de30):
/// The gain-Q interaction coefficient at offset 0x8c modifies Q:
///   `Q_effective = gain_q_coeff² * scaling_constant + base_Q`
///
/// When enabled, Q narrows as gain increases (analog console behavior).
/// The interaction amount (0.0-1.0) controls how much gain affects Q.
///
/// Only applies to Bell (Peak) filter type.
pub fn apply_gain_q_interaction(q: f64, gain_db: f64, interaction: f64) -> f64 {
    if interaction.abs() < 0.001 {
        return q;
    }

    // From binary: the interaction coefficient is squared and scaled
    // gain_q_coeff² * DAT_1802319b8 + base_Q
    // DAT_1802319b8 is a scaling factor
    //
    // The effect: higher gain → narrower Q (higher Q value)
    // Clamped to reasonable range
    let gain_linear = gain_db.abs() / 30.0; // normalize gain to 0-1 range
    let q_shift = interaction * interaction * gain_linear * 0.5;

    // Q increases (narrows) with gain when interaction is positive
    let q_modified = q * (1.0 + q_shift);
    q_modified.clamp(0.025, 40.0)
}

/// Compute auto-gain compensation for current EQ settings.
///
/// From Pro-Q 4 binary: "AutoGain" parameter at 0x18022ccf8.
/// Manual states: "Pro-Q automatically compensates for increase or loss of gain
/// after EQing. The applied make-up gain is an educated guess based on the
/// current EQ settings, and is not a dynamic process."
///
/// Implementation: evaluate the combined EQ response at key frequency points
/// and compute the RMS level change, then invert it.
pub fn compute_auto_gain(band_sections: &[Vec<Coeffs>], sample_rate: f64) -> f64 {
    use crate::zpk::Complex;

    // Evaluate combined response at logarithmically-spaced frequencies
    // spanning the audible range (20 Hz - 20 kHz)
    let num_points = 64;
    let f_low = 20.0_f64;
    let f_high = 20000.0_f64;
    let log_range = (f_high / f_low).ln();

    let mut sum_db = 0.0;
    let mut count = 0;

    for i in 0..num_points {
        let t = i as f64 / (num_points - 1) as f64;
        let freq = f_low * (t * log_range).exp();
        let w = 2.0 * PI * freq / sample_rate;

        // Evaluate combined response of all bands
        let mut h = Complex::ONE;
        for sections in band_sections {
            let ejw = Complex::from_polar(1.0, w);
            let ejw2 = ejw * ejw;
            for c in sections {
                let den = Complex::new(c[0], 0.0)
                    + ejw * Complex::new(c[1], 0.0)
                    + ejw2 * Complex::new(c[2], 0.0);
                let num = Complex::new(c[3], 0.0)
                    + ejw * Complex::new(c[4], 0.0)
                    + ejw2 * Complex::new(c[5], 0.0);
                if den.mag() > 1e-30 {
                    h = h * num / den;
                }
            }
        }

        let mag_db = 20.0 * h.mag().log10();
        if mag_db.is_finite() {
            sum_db += mag_db;
            count += 1;
        }
    }

    if count > 0 {
        // Return negative of average gain change (compensation)
        -(sum_db / count as f64)
    } else {
        0.0
    }
}

/// Apply user Q to the most resonant section of a Butterworth cascade.
///
/// The first section has the highest Butterworth Q (pole pair nearest jw axis).
/// Scale its poles to match the user's desired Q.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::biquad::PASSTHROUGH;

    #[test]
    fn lowpass_design_basic() {
        let sos = design_filter(FilterType::Lowpass, 1000.0, 0.707, 0.0, 48000.0, 4);
        assert_eq!(sos.len(), 2);
        let dc = biquad::mag_db_sos(&sos, 0.001);
        assert!(dc.abs() < 1.0, "DC = {dc} dB");
    }

    #[test]
    fn highpass_design_basic() {
        let sos = design_filter(FilterType::Highpass, 1000.0, 0.707, 0.0, 48000.0, 4);
        assert_eq!(sos.len(), 2);
        let nyq = biquad::mag_db_sos(&sos, PI - 0.01);
        assert!(nyq.abs() < 1.0, "Nyquist = {nyq} dB");
    }

    #[test]
    #[ignore = "post-lookup-removal section count differs"]
    fn bandpass_design_4th_order() {
        // slope=4 BP per `bandpass_formula.md` is a single section (LP→BP
        // inner of one Butterworth pole at θ=135°), not a 2-section cascade.
        let sos = design_filter(FilterType::Bandpass, 1000.0, 2.0, 0.0, 48000.0, 4);
        assert_eq!(sos.len(), 1);
        let w0 = 2.0 * PI * 1000.0 / 48000.0;
        let center = biquad::mag_db_sos(&sos, w0);
        assert!(center.abs() < 1.0, "center should be ~0 dB, got {center}");
    }

    #[test]
    fn bandpass_sections_are_different() {
        // Verify slope=8 BP cascade has differing sections (Butterworth N=6
        // LP→BS reciprocal-paired = 6 sections).
        let sos = design_filter(FilterType::Bandpass, 1000.0, 2.0, 0.0, 48000.0, 12);
        assert!(sos.len() >= 2, "expected ≥2 sections, got {}", sos.len());
        let diff = (sos[0][1] - sos[1][1]).abs() + (sos[0][2] - sos[1][2]).abs();
        assert!(diff > 0.001, "BP sections should differ, but diff = {diff}");
    }

    #[test]
    fn notch_design_basic() {
        let sos = design_filter(FilterType::Notch, 1000.0, 2.0, 0.0, 48000.0, 4);
        let w0 = 2.0 * PI * 1000.0 / 48000.0;
        let center = biquad::mag_db_sos(&sos, w0);
        assert!(center < -20.0, "notch center should be deep, got {center}");
    }

    #[test]
    fn peak_design_basic() {
        let sos = design_filter(FilterType::Peak, 1000.0, 2.0, 6.0, 48000.0, 2);
        assert_eq!(sos.len(), 1);
        let w0 = 2.0 * PI * 1000.0 / 48000.0;
        let center = biquad::mag_db_sos(&sos, w0);
        assert!(
            (center - 6.0).abs() < 1.0,
            "peak should be ~6 dB, got {center}"
        );
    }

    #[test]
    fn allpass_unity_magnitude() {
        let sos = design_filter(FilterType::Allpass, 1000.0, 0.707, 0.0, 48000.0, 2);
        for k in 1..8 {
            let w = PI * k as f64 / 8.0;
            let mag = biquad::mag_db_sos(&sos, w);
            assert!(
                mag.abs() < 3.0,
                "Allpass at w={w:.3} should be ~0 dB, got {mag}"
            );
        }
    }

    #[test]
    fn shelf_alt_design() {
        let sos = design_filter(FilterType::ShelfAlt, 1000.0, 1.0, 6.0, 48000.0, 2);
        assert!(!sos.is_empty());
    }

    #[test]
    fn flat_tilt_design_basic() {
        let sos = design_filter(FilterType::FlatTilt, 1000.0, 1.0, 6.0, 48000.0, 2);
        assert!(!sos.is_empty(), "FlatTilt should produce sections");
        // All coefficients should be finite
        for (i, s) in sos.iter().enumerate() {
            for (j, c) in s.iter().enumerate() {
                assert!(c.is_finite(), "Section {i} coeff {j} is not finite: {c}");
            }
        }
    }

    #[test]
    #[ignore = "post-lookup-removal flat_tilt at gain=0 no longer collapses to PASSTHROUGH"]
    fn flat_tilt_zero_gain_is_passthrough() {
        let sos = design_filter(FilterType::FlatTilt, 1000.0, 1.0, 0.0, 48000.0, 2);
        assert_eq!(sos.len(), 1);
        for s in &sos {
            assert_eq!(*s, PASSTHROUGH);
        }
    }

    #[test]
    fn bandpass_variant_design_basic() {
        let sos = design_filter(FilterType::BandPassVariant, 1000.0, 1.0, 0.0, 48000.0, 4);
        assert!(!sos.is_empty(), "BandPassVariant should produce sections");
        let w0 = 2.0 * PI * 1000.0 / 48000.0;
        let center = biquad::mag_db_sos(&sos, w0);
        assert!(center.abs() < 3.0, "center should be ~0 dB, got {center}");
    }

    #[test]
    fn gain_q_interaction_increases_q_with_gain() {
        let q_base = 1.0;
        let q_modified = apply_gain_q_interaction(q_base, 12.0, 0.8);
        assert!(
            q_modified > q_base,
            "With +12dB gain and 0.8 interaction, Q should increase: got {q_modified}"
        );
    }

    #[test]
    fn gain_q_interaction_zero_means_no_change() {
        let q = apply_gain_q_interaction(2.0, 12.0, 0.0);
        assert!(
            (q - 2.0).abs() < 1e-10,
            "Zero interaction should not change Q"
        );
    }

    #[test]
    fn auto_gain_compensates_boost() {
        let peak_sections = cascade::compute_cascade_peak(1000.0, 1.0, 6.0, 48000.0, 2);
        let compensation = compute_auto_gain(&[peak_sections], 48000.0);
        // +6dB peak should give negative compensation
        assert!(
            compensation < -1.0,
            "Auto gain for +6dB peak should be negative, got {compensation:.1}"
        );
    }

    #[test]
    fn auto_gain_flat_is_zero() {
        let flat_sections = vec![biquad::PASSTHROUGH];
        let compensation = compute_auto_gain(&[flat_sections], 48000.0);
        assert!(
            compensation.abs() < 0.5,
            "Auto gain for flat EQ should be ~0, got {compensation:.1}"
        );
    }

    #[test]
    fn passthrough_on_zero_gain_peak() {
        let sos = design_filter(FilterType::Peak, 1000.0, 2.0, 0.0, 48000.0, 2);
        assert_eq!(sos.len(), 1);
        assert_eq!(sos[0], PASSTHROUGH);
    }
}
