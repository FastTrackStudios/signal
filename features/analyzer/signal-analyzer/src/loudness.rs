//! Loudness-weighted comparison (ITU-R BS.1770).
//!
//! The middle metric between a null test and a decay fit. A null says
//! "bit-identical or not", a decay fit says "rings for the same time"; this
//! says "sits at the same loudness, with the same weight in each band" — which
//! is what decides whether a translated preset drops into a mix at the level
//! the engineer expected.

use crate::filters::{OCTAVE_CENTRES_HZ, k_weight, octave_bands};
use crate::null::rms;

/// Offset from the BS.1770 mean-square sum to LUFS.
const LUFS_OFFSET: f64 = -0.691;

/// Integrated K-weighted loudness of a mono buffer, in LUFS.
///
/// This is the ungated measurement — appropriate here because the analyzer
/// feeds continuous, deliberately-chosen stimulus rather than programme
/// material, so BS.1770's gating (which exists to ignore silence between
/// dialogue) would only add variance.
#[must_use]
pub fn loudness_lufs(x: &[f32], sample_rate: f64) -> f64 {
    if x.is_empty() {
        return f64::NEG_INFINITY;
    }
    let weighted = k_weight(x, sample_rate);
    let mean_square = weighted
        .iter()
        .map(|&s| (s as f64) * (s as f64))
        .sum::<f64>()
        / weighted.len() as f64;
    if mean_square <= 0.0 {
        return f64::NEG_INFINITY;
    }
    LUFS_OFFSET + 10.0 * mean_square.log10()
}

/// Per-octave-band energy of a buffer, in dB.
///
/// Bands follow [`OCTAVE_CENTRES_HZ`]. A silent band reports `-inf`.
#[must_use]
pub fn band_levels_db(x: &[f32], sample_rate: f64) -> Vec<(f64, f64)> {
    octave_bands(x, sample_rate)
        .into_iter()
        .enumerate()
        .map(|(i, band)| {
            let r = rms(&band);
            let db = if r <= 0.0 {
                f64::NEG_INFINITY
            } else {
                20.0 * r.log10()
            };
            (OCTAVE_CENTRES_HZ[i], db)
        })
        .collect()
}

/// One octave band of a loudness comparison.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BandDifference {
    pub centre_hz: f64,
    pub reference_db: f64,
    pub candidate_db: f64,
    /// Candidate − reference, in dB. `None` when either side is silent, since
    /// a difference against `-inf` is not a number worth reporting.
    pub difference_db: Option<f64>,
}

/// A loudness-weighted comparison of two renders.
#[derive(Debug, Clone, PartialEq)]
pub struct LoudnessComparison {
    pub reference_lufs: f64,
    pub candidate_lufs: f64,
    /// Candidate − reference, in LU.
    pub loudness_difference_lu: f64,
    pub bands: Vec<BandDifference>,
    /// Largest absolute band difference, over bands where both sides had
    /// signal. `None` if no band could be compared.
    pub worst_band_difference_db: Option<f64>,
}

impl LoudnessComparison {
    /// Whether overall loudness and every comparable band sit within
    /// `tolerance_db`.
    #[must_use]
    pub fn passes(&self, tolerance_db: f64) -> bool {
        if !self.loudness_difference_lu.is_finite()
            || self.loudness_difference_lu.abs() > tolerance_db
        {
            return false;
        }
        match self.worst_band_difference_db {
            Some(w) => w <= tolerance_db,
            // Nothing comparable means nothing verified — do not call that a pass.
            None => false,
        }
    }
}

/// Compare two renders by loudness and per-band balance.
pub fn compare_loudness(
    reference: &[f32],
    candidate: &[f32],
    sample_rate: f64,
) -> LoudnessComparison {
    let reference_lufs = loudness_lufs(reference, sample_rate);
    let candidate_lufs = loudness_lufs(candidate, sample_rate);

    let bands: Vec<BandDifference> = band_levels_db(reference, sample_rate)
        .into_iter()
        .zip(band_levels_db(candidate, sample_rate))
        .map(|((centre_hz, r), (_, c))| BandDifference {
            centre_hz,
            reference_db: r,
            candidate_db: c,
            difference_db: (r.is_finite() && c.is_finite()).then_some(c - r),
        })
        .collect();

    let worst_band_difference_db = bands
        .iter()
        .filter_map(|b| b.difference_db)
        .map(f64::abs)
        .fold(None, |acc: Option<f64>, d| {
            Some(acc.map_or(d, |a| a.max(d)))
        });

    LoudnessComparison {
        reference_lufs,
        candidate_lufs,
        loudness_difference_lu: candidate_lufs - reference_lufs,
        bands,
        worst_band_difference_db,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generators::{sine, white_noise};

    const SR: f64 = 48_000.0;

    #[test]
    fn a_full_scale_1k_sine_reads_near_minus_three_lufs() {
        // BS.1770 weighting is ~0 dB at 1 kHz, so a 0 dBFS sine (-3.01 dBFS
        // RMS) reads about -3.01 LUFS on a mono measurement.
        let l = loudness_lufs(&sine(1000.0, SR, 96_000), SR);
        assert!((l + 3.01).abs() < 0.6, "got {l} LUFS");
    }

    #[test]
    fn halving_amplitude_drops_loudness_by_six_lu() {
        let x = sine(1000.0, SR, 48_000);
        let quiet: Vec<f32> = x.iter().map(|&s| s * 0.5).collect();
        let d = loudness_lufs(&quiet, SR) - loudness_lufs(&x, SR);
        assert!((d + 6.02).abs() < 0.1, "got {d} LU");
    }

    #[test]
    fn high_frequencies_are_weighted_up_relative_to_low() {
        // K-weighting lifts the top and cuts the bottom.
        let low = loudness_lufs(&sine(60.0, SR, 96_000), SR);
        let mid = loudness_lufs(&sine(1000.0, SR, 96_000), SR);
        let high = loudness_lufs(&sine(10_000.0, SR, 96_000), SR);
        assert!(low < mid, "60 Hz {low} should read below 1 kHz {mid}");
        assert!(high > mid, "10 kHz {high} should read above 1 kHz {mid}");
    }

    #[test]
    fn silence_and_empty_are_negative_infinity() {
        assert!(loudness_lufs(&vec![0.0; 4800], SR).is_infinite());
        assert!(loudness_lufs(&[], SR).is_infinite());
    }

    #[test]
    fn band_levels_track_a_tone_into_its_own_band() {
        let levels = band_levels_db(&sine(1000.0, SR, 48_000), SR);
        let loudest = levels
            .iter()
            .filter(|(_, db)| db.is_finite())
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap();
        assert_eq!(loudest.0, 1000.0, "1 kHz tone should peak the 1 kHz band");
    }

    #[test]
    fn identical_renders_compare_as_no_difference() {
        let x = white_noise(48_000, 31);
        let c = compare_loudness(&x, &x, SR);
        assert!(c.loudness_difference_lu.abs() < 1e-9);
        assert!(c.worst_band_difference_db.unwrap() < 1e-9);
        assert!(c.passes(0.1));
    }

    #[test]
    fn a_level_offset_shows_up_in_loudness_and_every_band() {
        let x = white_noise(48_000, 37);
        // +3 dB across the board.
        let hot: Vec<f32> = x.iter().map(|&s| s * 1.4125).collect();
        let c = compare_loudness(&x, &hot, SR);

        assert!((c.loudness_difference_lu - 3.0).abs() < 0.1);
        assert!((c.worst_band_difference_db.unwrap() - 3.0).abs() < 0.2);
        assert!(!c.passes(1.0));
        assert!(c.passes(4.0));
    }

    #[test]
    fn a_silent_candidate_does_not_pass() {
        let x = white_noise(4800, 41);
        let c = compare_loudness(&x, &vec![0.0; 4800], SR);
        assert!(!c.passes(100.0), "silence must never score as a match");
        assert_eq!(c.worst_band_difference_db, None);
    }

    #[test]
    fn a_tilted_candidate_fails_on_bands_even_at_matched_loudness() {
        // This is the case overall loudness alone would miss: same LUFS,
        // wrong spectral balance.
        let x = white_noise(96_000, 43);
        let [shelf, _] = crate::filters::k_weighting(SR);
        let tilted = shelf.apply(&x); // lifts the top only
        let c = compare_loudness(&x, &tilted, SR);
        assert!(
            c.worst_band_difference_db.unwrap() > 1.0,
            "a spectral tilt must register, got {:?}",
            c.worst_band_difference_db
        );
    }
}
