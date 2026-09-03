//! Null testing: subtract two renders and measure what is left.
//!
//! This is the strict metric. Where our processor is meant to *be* the
//! reference — FTS-EQ against Pro-Q 4, which share the same ZPK design
//! pipeline — the residual should fall into the noise floor, and any real
//! deviation shows up immediately. It is deliberately unforgiving, and
//! near-meaningless for reverb, where two different algorithms never null.

/// Level helpers, in dBFS. `-inf` for digital silence.
#[must_use]
pub fn rms_db(x: &[f32]) -> f64 {
    let r = rms(x);
    if r <= 0.0 {
        f64::NEG_INFINITY
    } else {
        20.0 * r.log10()
    }
}

/// Root-mean-square level, linear.
#[must_use]
pub fn rms(x: &[f32]) -> f64 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>() / x.len() as f64).sqrt()
}

/// Peak absolute level, linear.
#[must_use]
pub fn peak(x: &[f32]) -> f64 {
    x.iter().fold(0.0f64, |m, &s| m.max((s as f64).abs()))
}

/// Peak level in dBFS.
#[must_use]
pub fn peak_db(x: &[f32]) -> f64 {
    let p = peak(x);
    if p <= 0.0 {
        f64::NEG_INFINITY
    } else {
        20.0 * p.log10()
    }
}

/// The result of nulling a candidate against a reference.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NullTest {
    /// Reference level, dBFS RMS.
    pub reference_db: f64,
    /// Residual (reference − candidate) level, dBFS RMS.
    pub residual_db: f64,
    /// Residual peak, dBFS.
    pub residual_peak_db: f64,
    /// How far the residual sits below the reference, in dB. Larger is better;
    /// `inf` means a perfect null.
    pub null_depth_db: f64,
    /// Samples compared.
    pub samples: usize,
}

impl NullTest {
    /// Whether the null is at least `required_db` deep.
    ///
    /// A perfect null (`inf`) passes any threshold; a comparison over no
    /// samples never passes.
    #[must_use]
    pub fn passes(&self, required_db: f64) -> bool {
        self.samples > 0 && self.null_depth_db >= required_db
    }
}

/// Sample-wise difference. Truncates to the shorter input.
#[must_use]
pub fn residual(reference: &[f32], candidate: &[f32]) -> Vec<f32> {
    reference
        .iter()
        .zip(candidate)
        .map(|(&r, &c)| r - c)
        .collect()
}

/// Null a candidate render against a reference render.
///
/// Both buffers must be sample-aligned — compensate any plugin latency
/// *before* calling this, or the residual measures the misalignment rather
/// than the processing difference. [`align_by_latency`] does that.
#[must_use]
pub fn null_test(reference: &[f32], candidate: &[f32]) -> NullTest {
    let res = residual(reference, candidate);
    let n = res.len();
    let reference_db = rms_db(&reference[..n.min(reference.len())]);
    let residual_db = rms_db(&res);

    let null_depth_db = match (reference_db, residual_db) {
        (_, d) if d.is_infinite() && d.is_sign_negative() => f64::INFINITY, // exact null
        (r, _) if r.is_infinite() => f64::NEG_INFINITY, // silent reference, noisy candidate
        (r, d) => r - d,
    };

    NullTest {
        reference_db,
        residual_db,
        residual_peak_db: peak_db(&res),
        null_depth_db,
        samples: n,
    }
}

/// Drop `latency` leading samples from the candidate so it lines up with the
/// reference, and trim both to a common length.
///
/// Reported plugin latency is the reliable source here; cross-correlation is
/// not, because a reverb's own pre-delay would be mistaken for latency.
#[must_use]
pub fn align_by_latency<'a>(
    reference: &'a [f32],
    candidate: &'a [f32],
    latency: usize,
) -> (&'a [f32], &'a [f32]) {
    let candidate = candidate.get(latency..).unwrap_or(&[]);
    let n = reference.len().min(candidate.len());
    (&reference[..n], &candidate[..n])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generators::{sine, white_noise};

    const SR: f64 = 48_000.0;

    #[test]
    fn identical_signals_null_perfectly() {
        let x = sine(1000.0, SR, 4800);
        let t = null_test(&x, &x);
        assert!(t.null_depth_db.is_infinite() && t.null_depth_db.is_sign_positive());
        assert!(t.passes(120.0));
    }

    #[test]
    fn a_scaled_copy_nulls_to_a_predictable_depth() {
        let x = sine(1000.0, SR, 48_000);
        // 1% amplitude error → residual 40 dB down.
        let y: Vec<f32> = x.iter().map(|&s| s * 1.01).collect();
        let t = null_test(&x, &y);
        assert!((t.null_depth_db - 40.0).abs() < 0.5, "got {}", t.null_depth_db);
        assert!(t.passes(35.0));
        assert!(!t.passes(45.0));
    }

    #[test]
    fn uncorrelated_signals_barely_null() {
        let a = white_noise(48_000, 1);
        let b = white_noise(48_000, 2);
        let t = null_test(&a, &b);
        // Two independent noise sources sum in power: residual is ~3 dB up.
        assert!(t.null_depth_db < 0.0, "got {}", t.null_depth_db);
        assert!(!t.passes(6.0));
    }

    #[test]
    fn a_silent_reference_with_a_noisy_candidate_fails_rather_than_passing() {
        // Guard against "silence nulls against everything" scoring as perfect.
        let t = null_test(&vec![0.0; 1024], &white_noise(1024, 3));
        assert!(t.null_depth_db.is_infinite() && t.null_depth_db.is_sign_negative());
        assert!(!t.passes(0.0));
    }

    #[test]
    fn both_silent_is_an_exact_null() {
        let t = null_test(&vec![0.0; 512], &vec![0.0; 512]);
        assert!(t.null_depth_db.is_infinite() && t.null_depth_db.is_sign_positive());
        assert!(t.passes(200.0));
    }

    #[test]
    fn an_empty_comparison_never_passes() {
        let t = null_test(&[], &[]);
        assert_eq!(t.samples, 0);
        assert!(!t.passes(0.0));
    }

    #[test]
    fn latency_alignment_restores_a_null() {
        let x = sine(500.0, SR, 9600);
        // Candidate delayed by 128 samples — unaligned it nulls terribly.
        let mut delayed = vec![0.0f32; 128];
        delayed.extend_from_slice(&x);

        let naive = null_test(&x, &delayed);
        assert!(naive.null_depth_db < 6.0, "misaligned should not null");

        let (r, c) = align_by_latency(&x, &delayed, 128);
        let aligned = null_test(r, c);
        assert!(
            aligned.null_depth_db.is_infinite() || aligned.null_depth_db > 100.0,
            "aligned should null, got {}",
            aligned.null_depth_db
        );
    }

    #[test]
    fn alignment_past_the_end_yields_an_empty_comparison() {
        let x = sine(500.0, SR, 100);
        let (r, c) = align_by_latency(&x, &x, 1000);
        assert!(r.is_empty() && c.is_empty());
    }

    #[test]
    fn level_helpers_agree_with_known_values() {
        // Full-scale sine: peak 0 dBFS, RMS -3.01 dBFS.
        let x = sine(1000.0, SR, 48_000);
        assert!(peak_db(&x).abs() < 0.01);
        assert!((rms_db(&x) + 3.0103).abs() < 0.01);
        assert!(rms_db(&[]).is_infinite());
    }
}
