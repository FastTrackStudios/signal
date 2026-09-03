//! The comparison report: run all three metrics over a reference/candidate
//! pair and say whether the candidate matches.
//!
//! The three metrics answer different questions and are deliberately not
//! collapsed into one score:
//!
//! - [`crate::null`] — *is it the same processing?* Strict. The right bar for
//!   FTS-EQ against Pro-Q 4, which share a design pipeline and should null
//!   deeply. Meaningless for reverb.
//! - [`crate::decay`] — *does it ring like the same space?* The reverb bar.
//! - [`crate::loudness`] — *does it sit at the same level and balance?* The
//!   bar that decides whether a translated preset drops into a mix correctly.
//!
//! A profile picks which of the three must pass, because "matching" means
//! something different for an EQ than for a reverb.

use crate::decay::{compare_decay, DecayComparison, DecayFit};
use crate::loudness::{compare_loudness, LoudnessComparison};
use crate::null::{null_test, NullTest};

/// What a given comparison is allowed to differ by.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Thresholds {
    /// Minimum null depth in dB. `None` disables the null criterion — correct
    /// for reverb, where two different algorithms never null.
    pub min_null_depth_db: Option<f64>,
    /// Maximum per-band RT60 ratio error, where 0.1 means "within 10%".
    /// `None` disables the decay criterion.
    pub max_decay_ratio_error: Option<f64>,
    /// Maximum loudness and per-band difference in dB. `None` disables it.
    pub max_loudness_difference_db: Option<f64>,
    /// Which decay fit to use.
    pub decay_fit: DecayFit,
}

impl Thresholds {
    /// For a processor meant to be numerically identical to its reference —
    /// FTS-EQ against Pro-Q 4. Demands a deep null; decay is irrelevant.
    ///
    /// 60 dB is well below anything audible while still leaving room for the
    /// f32/f64 rounding differences between two implementations.
    #[must_use]
    pub fn exact_match() -> Self {
        Self {
            min_null_depth_db: Some(60.0),
            max_decay_ratio_error: None,
            max_loudness_difference_db: Some(0.1),
            decay_fit: DecayFit::T20,
        }
    }

    /// For a reverb matched to a reference reverb. No null requirement —
    /// judged on decay character and spectral balance.
    #[must_use]
    pub fn reverb_match() -> Self {
        Self {
            min_null_depth_db: None,
            max_decay_ratio_error: Some(0.15),
            max_loudness_difference_db: Some(1.5),
            decay_fit: DecayFit::T20,
        }
    }
}

/// Which criteria a comparison was judged on, and how each fared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Criterion {
    Null,
    Decay,
    Loudness,
}

/// One criterion's outcome.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CriterionResult {
    pub criterion: Criterion,
    pub passed: bool,
    /// The measured value that was thresholded (null depth dB / ratio error /
    /// worst band difference dB).
    pub measured: Option<f64>,
    pub threshold: f64,
}

/// A full comparison of a candidate render against a reference render.
#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    pub null: NullTest,
    pub decay: DecayComparison,
    pub loudness: LoudnessComparison,
    /// One entry per *enabled* criterion.
    pub results: Vec<CriterionResult>,
}

impl Comparison {
    /// Whether every enabled criterion passed.
    ///
    /// A comparison with no enabled criteria is **not** a pass — that would
    /// mean reporting success without measuring anything.
    #[must_use]
    pub fn passed(&self) -> bool {
        !self.results.is_empty() && self.results.iter().all(|r| r.passed)
    }

    /// The criteria that failed.
    pub fn failures(&self) -> impl Iterator<Item = &CriterionResult> {
        self.results.iter().filter(|r| !r.passed)
    }
}

/// Compare a candidate render against a reference render.
///
/// Both buffers must already be sample-aligned — use
/// [`crate::null::align_by_latency`] with the plugin's reported latency
/// first, or the null test measures the offset rather than the processing.
///
/// `reference_ir` / `candidate_ir` are the impulse responses used for the
/// decay criterion. Pass the same buffers as the main renders when the
/// stimulus *was* an impulse.
#[must_use]
pub fn compare(
    reference: &[f32],
    candidate: &[f32],
    reference_ir: &[f32],
    candidate_ir: &[f32],
    sample_rate: f64,
    thresholds: Thresholds,
) -> Comparison {
    let null = null_test(reference, candidate);
    let decay = compare_decay(reference_ir, candidate_ir, sample_rate, thresholds.decay_fit);
    let loudness = compare_loudness(reference, candidate, sample_rate);

    let mut results = Vec::new();

    if let Some(t) = thresholds.min_null_depth_db {
        results.push(CriterionResult {
            criterion: Criterion::Null,
            passed: null.passes(t),
            measured: null.null_depth_db.is_finite().then_some(null.null_depth_db),
            threshold: t,
        });
    }

    if let Some(t) = thresholds.max_decay_ratio_error {
        // An unmeasurable decay is a failure, not a pass: nothing was verified.
        let measured = decay.worst_ratio_error;
        results.push(CriterionResult {
            criterion: Criterion::Decay,
            passed: measured.is_some_and(|e| e <= t),
            measured,
            threshold: t,
        });
    }

    if let Some(t) = thresholds.max_loudness_difference_db {
        results.push(CriterionResult {
            criterion: Criterion::Loudness,
            passed: loudness.passes(t),
            measured: loudness.worst_band_difference_db,
            threshold: t,
        });
    }

    Comparison {
        null,
        decay,
        loudness,
        results,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generators::white_noise;

    const SR: f64 = 48_000.0;

    fn decaying(rt60: f64, len_s: f64, seed: u64) -> Vec<f32> {
        let n = (len_s * SR) as usize;
        let noise = white_noise(n, seed);
        (0..n)
            .map(|i| {
                let t = i as f64 / SR;
                noise[i] * 10.0f64.powf(-3.0 * t / rt60) as f32
            })
            .collect()
    }

    #[test]
    fn an_identical_candidate_passes_every_criterion() {
        let ir = decaying(1.0, 4.0, 51);
        let c = compare(&ir, &ir, &ir, &ir, SR, Thresholds::exact_match());
        assert!(c.passed());
        assert_eq!(c.failures().count(), 0);
    }

    #[test]
    fn exact_match_ignores_decay_and_reverb_match_ignores_null() {
        let ir = decaying(1.0, 4.0, 53);
        let exact = compare(&ir, &ir, &ir, &ir, SR, Thresholds::exact_match());
        assert!(!exact.results.iter().any(|r| r.criterion == Criterion::Decay));

        let reverb = compare(&ir, &ir, &ir, &ir, SR, Thresholds::reverb_match());
        assert!(!reverb.results.iter().any(|r| r.criterion == Criterion::Null));
        assert!(reverb.results.iter().any(|r| r.criterion == Criterion::Decay));
    }

    #[test]
    fn two_different_reverbs_fail_a_null_but_can_pass_a_reverb_match() {
        // Same decay time, different noise — i.e. a different algorithm
        // producing the same space.
        let reference = decaying(1.0, 4.0, 61);
        let candidate = decaying(1.0, 4.0, 62);

        let strict = compare(
            &reference,
            &candidate,
            &reference,
            &candidate,
            SR,
            Thresholds::exact_match(),
        );
        assert!(!strict.passed(), "uncorrelated renders must fail a null");

        // Widened loudness tolerance for this case specifically. The two
        // sides are independent noise realizations, so their per-octave-band
        // levels differ by a little pure chance — around 1.5 dB in the
        // narrowest bands. That variance is an artifact of the synthetic
        // stimulus, not a balance difference, and this test is about decay.
        let lenient = compare(
            &reference,
            &candidate,
            &reference,
            &candidate,
            SR,
            Thresholds {
                max_loudness_difference_db: Some(2.5),
                ..Thresholds::reverb_match()
            },
        );
        assert!(
            lenient.passed(),
            "same decay + balance should pass a reverb match: {:?}",
            lenient.failures().collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_wrong_decay_time_fails_the_reverb_match() {
        let reference = decaying(1.0, 4.0, 71);
        let candidate = decaying(2.0, 6.0, 72);
        let c = compare(
            &reference,
            &candidate,
            &reference,
            &candidate,
            SR,
            Thresholds::reverb_match(),
        );
        assert!(!c.passed());
        assert!(c.failures().any(|r| r.criterion == Criterion::Decay));
    }

    #[test]
    fn an_unmeasurable_decay_fails_rather_than_passing_vacuously() {
        let reference = decaying(1.0, 4.0, 81);
        let silent = vec![0.0f32; reference.len()];
        let c = compare(
            &reference,
            &silent,
            &reference,
            &silent,
            SR,
            Thresholds::reverb_match(),
        );
        assert!(!c.passed(), "silence must not pass by being unmeasurable");
        let decay = c
            .results
            .iter()
            .find(|r| r.criterion == Criterion::Decay)
            .unwrap();
        assert_eq!(decay.measured, None);
        assert!(!decay.passed);
    }

    #[test]
    fn a_comparison_with_no_enabled_criteria_is_not_a_pass() {
        let ir = decaying(1.0, 2.0, 91);
        let none = Thresholds {
            min_null_depth_db: None,
            max_decay_ratio_error: None,
            max_loudness_difference_db: None,
            decay_fit: DecayFit::T20,
        };
        let c = compare(&ir, &ir, &ir, &ir, SR, none);
        assert!(c.results.is_empty());
        assert!(!c.passed(), "measuring nothing is not success");
    }

    #[test]
    fn failures_name_the_criterion_that_broke() {
        let reference = white_noise(48_000, 101);
        // +6 dB: nulls badly and is far too loud.
        let hot: Vec<f32> = reference.iter().map(|&s| s * 2.0).collect();
        let c = compare(
            &reference,
            &hot,
            &reference,
            &hot,
            SR,
            Thresholds::exact_match(),
        );
        let failed: Vec<_> = c.failures().map(|r| r.criterion).collect();
        assert!(failed.contains(&Criterion::Null));
        assert!(failed.contains(&Criterion::Loudness));
    }
}
