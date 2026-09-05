//! Energy-decay and reverberation-time measurement.
//!
//! Two reverbs built on different algorithms will never null against each
//! other, so "does ours match theirs" has to be answered with the properties
//! that make a space sound like itself: how long it rings, and how that time
//! varies with frequency. This module measures both from an impulse response.
//!
//! Method is the standard one (ISO 3382): integrate the squared IR backwards
//! to get the Schroeder energy-decay curve, then fit a line to a segment of it
//! and extrapolate to −60 dB.

use crate::filters::{octave_bands, OCTAVE_CENTRES_HZ};

/// Which segment of the decay curve the fit uses.
///
/// A full −60 dB fit is rarely usable — the tail disappears into the noise
/// floor first — so the convention is to fit a shallower segment and
/// extrapolate. Both start 5 dB down, to skip the direct sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecayFit {
    /// Fit −5 dB → −25 dB, extrapolate ×3. Tolerates a high noise floor.
    T20,
    /// Fit −5 dB → −35 dB, extrapolate ×2. More accurate when the tail is clean.
    T30,
    /// Fit −5 dB → −15 dB, extrapolate ×6. The last resort for short or noisy
    /// tails that never present 20 dB of straight decay — a large
    /// extrapolation, so only worth using when the others cannot fit at all.
    T10,
}

impl DecayFit {
    /// The dB range fitted, and the factor that extrapolates it to 60 dB.
    fn range(self) -> (f64, f64, f64) {
        match self {
            Self::T20 => (-5.0, -25.0, 3.0),
            Self::T30 => (-5.0, -35.0, 2.0),
            Self::T10 => (-5.0, -15.0, 6.0),
        }
    }
}

/// The Schroeder energy-decay curve, in dB relative to total energy.
///
/// `edc[i]` is the energy remaining from sample `i` onward. Monotonically
/// decreasing by construction, which is what makes the level crossings
/// unambiguous.
#[must_use]
pub fn energy_decay_curve(ir: &[f32]) -> Vec<f64> {
    let mut running = 0.0f64;
    // Backwards cumulative sum of squares.
    let mut tail: Vec<f64> = ir
        .iter()
        .rev()
        .map(|&s| {
            running += (s as f64) * (s as f64);
            running
        })
        .collect();
    tail.reverse();

    let total = tail.first().copied().unwrap_or(0.0);
    if total <= 0.0 {
        return vec![f64::NEG_INFINITY; ir.len()];
    }
    tail.iter()
        .map(|&e| {
            if e <= 0.0 {
                f64::NEG_INFINITY
            } else {
                10.0 * (e / total).log10()
            }
        })
        .collect()
}

/// The first sample index at or below `level_db` on a decay curve.
fn crossing(edc: &[f64], level_db: f64) -> Option<usize> {
    edc.iter().position(|&v| v <= level_db)
}

/// Minimum coefficient of determination for a decay fit to be trusted.
///
/// The Schroeder integral is taken over a *finite* buffer, so its curve always
/// plunges toward `-inf` at the end no matter what the signal was — steady
/// noise included. That means "did it cross −25 dB?" is not on its own
/// evidence of a decay, and an unguarded fit happily returns an RT60 for a
/// constant tone.
///
/// What separates a real decay is that its curve is *straight*: an
/// exponentially decaying tail is linear in dB, while the truncation artifact
/// is the sharply-curved `10·log10(1 − t/N)`. Requiring a near-linear fit
/// rejects the artifact. ISO 3382 makes the same distinction with its
/// non-linearity parameter.
const MIN_FIT_R_SQUARED: f64 = 0.98;

/// Reverberation time in seconds, from an impulse response.
///
/// Returns `None` when the response never decays far enough to fit, or when
/// what it does is not a straight decay — a tail that is too short, too
/// noisy, or simply not a decaying response at all. That is a real outcome,
/// not an error: an honest "not measurable" beats a number extrapolated from
/// a truncation artifact.
#[must_use]
pub fn reverb_time(ir: &[f32], sample_rate: f64, fit: DecayFit) -> Option<f64> {
    let edc = energy_decay_curve(ir);
    let (from_db, to_db, factor) = fit.range();

    let start = crossing(&edc, from_db)?;
    let end = crossing(&edc, to_db)?;
    if end <= start {
        return None;
    }

    // Least-squares slope over the fitted segment, in dB per sample.
    let seg = &edc[start..=end];
    if seg.len() < 3 || seg.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let n = seg.len() as f64;
    let mean_x = (n - 1.0) / 2.0;
    let mean_y = seg.iter().sum::<f64>() / n;
    let (mut num, mut den, mut total_ss) = (0.0, 0.0, 0.0);
    for (i, &y) in seg.iter().enumerate() {
        let dx = i as f64 - mean_x;
        let dy = y - mean_y;
        num += dx * dy;
        den += dx * dx;
        total_ss += dy * dy;
    }
    if den == 0.0 || total_ss == 0.0 {
        return None;
    }
    let slope_db_per_sample = num / den;
    if slope_db_per_sample >= 0.0 {
        return None; // not decaying
    }

    // Reject anything that is not a straight line in dB — see
    // MIN_FIT_R_SQUARED. r² = explained variance / total variance.
    let r_squared = (num * num / den) / total_ss;
    if r_squared < MIN_FIT_R_SQUARED {
        return None;
    }

    // Time to fall the fitted range (both terms negative, so this is
    // positive), scaled up to a full 60 dB.
    let samples = (to_db - from_db) / slope_db_per_sample;
    Some(samples * factor / sample_rate)
}

/// Reverberation time, falling back to a shallower fit when the preferred one
/// cannot be made.
///
/// Short reverbs are the reason this exists: a 0.3 s tail may never present a
/// straight 20 dB of decay, and returning `None` for it means the comparison
/// silently declines to measure a perfectly good reverb. Trying `T20` and then
/// `T10` keeps those in scope, at the cost of a larger extrapolation on the
/// ones that need the fallback.
#[must_use]
pub fn reverb_time_best_effort(ir: &[f32], sample_rate: f64, preferred: DecayFit) -> Option<f64> {
    let mut order = vec![preferred];
    for f in [DecayFit::T20, DecayFit::T10] {
        if !order.contains(&f) {
            order.push(f);
        }
    }
    order
        .into_iter()
        .find_map(|fit| reverb_time(ir, sample_rate, fit))
}

/// Reverb time per octave band.
///
/// Index matches [`OCTAVE_CENTRES_HZ`]; a band that cannot be fitted is
/// `None` rather than a fabricated number.
#[must_use]
pub fn reverb_time_per_band(
    ir: &[f32],
    sample_rate: f64,
    fit: DecayFit,
) -> Vec<(f64, Option<f64>)> {
    octave_bands(ir, sample_rate)
        .into_iter()
        .enumerate()
        .map(|(i, band)| {
            (
                OCTAVE_CENTRES_HZ[i],
                reverb_time_best_effort(&band, sample_rate, fit),
            )
        })
        .collect()
}

/// How closely a candidate's decay matches a reference's, per octave band.
#[derive(Debug, Clone, PartialEq)]
pub struct DecayComparison {
    /// `(centre_hz, reference_rt60, candidate_rt60, ratio)` per band.
    ///
    /// `ratio` is candidate ÷ reference — 1.0 is a perfect match. `None`
    /// where either side could not be fitted.
    pub bands: Vec<DecayBand>,
    /// Largest absolute deviation of `ratio` from 1.0, over the bands that
    /// could be compared. `None` if no band could be.
    pub worst_ratio_error: Option<f64>,
}

/// One octave band of a decay comparison.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecayBand {
    pub centre_hz: f64,
    pub reference_s: Option<f64>,
    pub candidate_s: Option<f64>,
    pub ratio: Option<f64>,
}

/// Compare two impulse responses band by band.
#[must_use]
pub fn compare_decay(
    reference_ir: &[f32],
    candidate_ir: &[f32],
    sample_rate: f64,
    fit: DecayFit,
) -> DecayComparison {
    let r = reverb_time_per_band(reference_ir, sample_rate, fit);
    compare_decay_against(&r, candidate_ir, sample_rate, fit)
}

/// As [`compare_decay`], but against a reference whose bands were measured
/// earlier.
///
/// Band-splitting a buffer is the expensive half of a comparison — eight
/// bands, three biquad sections each, over the whole tail. A tuning loop
/// compares against the same reference dozens of times, so measuring it once
/// and reusing it removes half the work from every iteration.
#[must_use]
pub fn compare_decay_against(
    reference_bands: &[(f64, Option<f64>)],
    candidate_ir: &[f32],
    sample_rate: f64,
    fit: DecayFit,
) -> DecayComparison {
    let r = reference_bands.to_vec();
    let c = reverb_time_per_band(candidate_ir, sample_rate, fit);

    let bands: Vec<DecayBand> = r
        .into_iter()
        .zip(c)
        .map(|((centre_hz, rs), (_, cs))| {
            let ratio = match (rs, cs) {
                (Some(rv), Some(cv)) if rv > 0.0 => Some(cv / rv),
                _ => None,
            };
            DecayBand {
                centre_hz,
                reference_s: rs,
                candidate_s: cs,
                ratio,
            }
        })
        .collect();

    let worst_ratio_error = bands
        .iter()
        .filter_map(|b| b.ratio)
        .map(|r| (r - 1.0).abs())
        .fold(None, |acc: Option<f64>, e| {
            Some(acc.map_or(e, |a| a.max(e)))
        });

    DecayComparison {
        bands,
        worst_ratio_error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48_000.0;

    /// A synthetic exponentially-decaying noise burst with a known RT60.
    fn synthetic_ir(rt60_s: f64, len_s: f64, seed: u64) -> Vec<f32> {
        let n = (len_s * SR) as usize;
        let noise = crate::generators::white_noise(n, seed);
        // Amplitude decays 60 dB over rt60 → factor 10^(-3/rt60) per second.
        (0..n)
            .map(|i| {
                let t = i as f64 / SR;
                let a = 10.0f64.powf(-3.0 * t / rt60_s);
                noise[i] * a as f32
            })
            .collect()
    }

    #[test]
    fn edc_is_monotonic_and_starts_at_zero_db() {
        let edc = energy_decay_curve(&synthetic_ir(1.0, 2.0, 7));
        assert!((edc[0] - 0.0).abs() < 1e-9);
        assert!(edc.windows(2).all(|w| w[1] <= w[0] + 1e-9), "must decrease");
    }

    #[test]
    fn edc_of_silence_is_negative_infinity() {
        let edc = energy_decay_curve(&vec![0.0; 128]);
        assert!(edc.iter().all(|v| v.is_infinite() && v.is_sign_negative()));
        assert!(energy_decay_curve(&[]).is_empty());
    }

    #[test]
    fn recovers_a_known_rt60() {
        for target in [0.4, 1.0, 2.0] {
            let ir = synthetic_ir(target, target * 3.0, 11);
            let measured = reverb_time(&ir, SR, DecayFit::T20).expect("fittable");
            let err = (measured - target).abs() / target;
            assert!(
                err < 0.1,
                "RT60 {target}: measured {measured}, off by {err:.3}"
            );
        }
    }

    #[test]
    fn t20_and_t30_agree_on_a_clean_tail() {
        let ir = synthetic_ir(1.5, 5.0, 3);
        let t20 = reverb_time(&ir, SR, DecayFit::T20).unwrap();
        let t30 = reverb_time(&ir, SR, DecayFit::T30).unwrap();
        assert!((t20 - t30).abs() / t30 < 0.1, "T20 {t20} vs T30 {t30}");
    }

    #[test]
    fn the_fallback_measures_a_short_tail_the_preferred_fit_cannot() {
        // 0.25 s RT60 rendered for only 0.6 s: T20 has no straight 20 dB to
        // fit, T10 does.
        let ir = synthetic_ir(0.25, 0.6, 29);
        let strict = reverb_time(&ir, SR, DecayFit::T30);
        let best = reverb_time_best_effort(&ir, SR, DecayFit::T30);
        assert!(best.is_some(), "the fallback should find a fit");
        if strict.is_none() {
            let v = best.unwrap();
            assert!((v - 0.25).abs() / 0.25 < 0.25, "got {v}");
        }
    }

    #[test]
    fn the_fallback_still_refuses_a_non_decaying_signal() {
        // Falling back must not turn "not a decay" into a number.
        let steady = crate::generators::white_noise(48_000, 33);
        assert_eq!(reverb_time_best_effort(&steady, SR, DecayFit::T20), None);
        assert_eq!(
            reverb_time_best_effort(&vec![0.0; 4800], SR, DecayFit::T20),
            None
        );
    }

    #[test]
    fn a_non_decaying_signal_is_not_measurable() {
        // Steady noise DOES cross -25 dB on a finite Schroeder curve (the
        // truncation artifact), so this is exactly the case the linearity
        // guard exists for: it must report None, not a fabricated RT60.
        let steady = crate::generators::white_noise(48_000, 5);
        assert_eq!(reverb_time(&steady, SR, DecayFit::T20), None);
        // Nor does silence.
        assert_eq!(reverb_time(&vec![0.0; 4800], SR, DecayFit::T20), None);
    }

    #[test]
    fn a_tail_that_ends_too_early_is_not_measurable() {
        // A 2 s decay truncated to 100 ms shows only ~3 dB of real decay; the
        // rest of the curve is truncation, which is not straight in dB.
        let ir = synthetic_ir(2.0, 0.1, 9);
        assert_eq!(reverb_time(&ir, SR, DecayFit::T20), None);
    }

    #[test]
    fn per_band_covers_every_octave() {
        let bands = reverb_time_per_band(&synthetic_ir(1.0, 3.0, 13), SR, DecayFit::T20);
        assert_eq!(bands.len(), OCTAVE_CENTRES_HZ.len());
        assert_eq!(bands[0].0, 62.5);
        // A broadband decay should be fittable in the middle bands.
        assert!(bands[4].1.is_some(), "1 kHz band should measure");
    }

    #[test]
    fn identical_responses_compare_as_a_perfect_match() {
        let ir = synthetic_ir(1.2, 4.0, 17);
        let cmp = compare_decay(&ir, &ir, SR, DecayFit::T20);
        let worst = cmp.worst_ratio_error.expect("some band compares");
        assert!(
            worst < 1e-9,
            "identical IRs must match exactly, got {worst}"
        );
    }

    #[test]
    fn a_longer_tail_shows_up_as_a_ratio_above_one() {
        let reference = synthetic_ir(1.0, 4.0, 21);
        let candidate = synthetic_ir(2.0, 6.0, 21);
        let cmp = compare_decay(&reference, &candidate, SR, DecayFit::T20);

        let mid = cmp.bands.iter().find(|b| b.centre_hz == 1000.0).unwrap();
        let ratio = mid.ratio.expect("1 kHz should compare");
        assert!(
            (ratio - 2.0).abs() < 0.2,
            "twice the decay → ratio ~2, got {ratio}"
        );
        assert!(cmp.worst_ratio_error.unwrap() > 0.5);
    }

    #[test]
    fn unmeasurable_bands_report_none_rather_than_skewing_the_result() {
        let reference = synthetic_ir(1.0, 4.0, 23);
        let silent = vec![0.0f32; reference.len()];
        let cmp = compare_decay(&reference, &silent, SR, DecayFit::T20);
        assert!(cmp.bands.iter().all(|b| b.ratio.is_none()));
        assert_eq!(cmp.worst_ratio_error, None);
    }
}
