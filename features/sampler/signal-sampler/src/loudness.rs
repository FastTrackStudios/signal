//! Integrated loudness metering — ITU-R BS.1770-4 / EBU R128.
//!
//! This is the perceptual "how loud does it *sound*" measure (LUFS), the same
//! standard NAM's trainer uses to compute a model's `loudness` metadata. We use
//! it to guarantee every amp model in a library lands on the same average
//! output loudness: run a fixed DI clip through each model (see
//! [`crate::nam_calibrate`]), measure the integrated loudness here, and derive a
//! makeup gain toward a common target. Because the measure is K-weighted (not
//! raw RMS), a scooped clean and a saturated high-gain model that "read" the
//! same here actually *sound* the same — so switching amps never jumps the
//! level.
//!
//! The pipeline is the textbook BS.1770 one:
//!   1. K-weighting: a high-shelf "head" filter + a ~38 Hz high-pass (RLB).
//!   2. 400 ms mean-square blocks with 75 % overlap (100 ms hop).
//!   3. Two-stage gating: absolute −70 LUFS, then a relative −10 LU gate.
//!   4. Integrated loudness = −0.691 + 10·log10(mean gated block power).
//!
//! Mono only — guitar amp models are mono in / mono out, so the channel weight
//! is 1.0 and there is no L/R summation to worry about here.

/// A transposed-direct-form-II biquad. One `f64` section of the K-weighting
/// filter; coefficients are normalized so `a0 == 1`.
#[derive(Clone, Copy, Debug)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    fn new(b0: f64, b1: f64, b2: f64, a1: f64, a2: f64) -> Self {
        Self {
            b0,
            b1,
            b2,
            a1,
            a2,
            z1: 0.0,
            z2: 0.0,
        }
    }

    #[inline]
    fn process(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// Stage 1 of K-weighting: the "pre-filter" / head high-shelf (~ +4 dB above
/// ~1.68 kHz). Coefficients derived from the BS.1770 analog prototype via the
/// bilinear transform so they are correct at any sample rate (libebur128's
/// parametrisation), not just the 48 kHz constants tabulated in the spec.
fn prefilter(fs: f64) -> Biquad {
    let f0 = 1681.974450955533;
    let g = 3.999843853973347; // dB
    let q = 0.7071752369554196;

    let k = (core::f64::consts::PI * f0 / fs).tan();
    let vh = 10f64.powf(g / 20.0);
    let vb = vh.powf(0.4996667741545416);
    let a0 = 1.0 + k / q + k * k;
    Biquad::new(
        (vh + vb * k / q + k * k) / a0,
        2.0 * (k * k - vh) / a0,
        (vh - vb * k / q + k * k) / a0,
        2.0 * (k * k - 1.0) / a0,
        (1.0 - k / q + k * k) / a0,
    )
}

/// Stage 2 of K-weighting: the RLB high-pass (~38 Hz), which de-emphasises
/// rumble/DC. Same bilinear parametrisation as [`prefilter`].
fn rlb_highpass(fs: f64) -> Biquad {
    let f0 = 38.13547087602444;
    let q = 0.5003270373238773;

    let k = (core::f64::consts::PI * f0 / fs).tan();
    let a0 = 1.0 + k / q + k * k;
    Biquad::new(
        1.0,
        -2.0,
        1.0,
        2.0 * (k * k - 1.0) / a0,
        (1.0 - k / q + k * k) / a0,
    )
}

/// The `−0.691` dB offset in the BS.1770 loudness equation (accounts for the
/// K-weighting's reference gain).
const ABS_OFFSET: f64 = -0.691;
/// Absolute gate: blocks quieter than this never count toward the integral.
const ABSOLUTE_GATE_LUFS: f64 = -70.0;
/// Relative gate width below the ungated mean loudness.
const RELATIVE_GATE_LU: f64 = -10.0;
/// Sentinel returned for silence / signal shorter than one 400 ms block.
pub const SILENCE_LUFS: f64 = f64::NEG_INFINITY;

/// Measure the **integrated loudness** (LUFS) of a mono `f64` signal at
/// `sample_rate` Hz, per ITU-R BS.1770-4 with EBU R128 two-stage gating.
///
/// Returns [`SILENCE_LUFS`] (−∞) when the signal is silent or shorter than one
/// 400 ms measurement block. This is an offline measurement — it allocates and
/// is not for the audio hot path.
pub fn integrated_lufs(samples: &[f64], sample_rate: f64) -> f64 {
    if sample_rate <= 0.0 {
        return SILENCE_LUFS;
    }
    // K-weight the whole signal.
    let mut pre = prefilter(sample_rate);
    let mut hp = rlb_highpass(sample_rate);
    let weighted: Vec<f64> = samples
        .iter()
        .map(|&x| hp.process(pre.process(x)))
        .collect();

    // 400 ms blocks, 100 ms hop (75 % overlap).
    let block_len = (0.4 * sample_rate).round() as usize;
    let hop = (0.1 * sample_rate).round() as usize;
    if block_len == 0 || hop == 0 || weighted.len() < block_len {
        return SILENCE_LUFS;
    }

    // Per-block mean square ("power" z_j) and its block loudness l_j.
    let mut powers: Vec<f64> = Vec::new();
    let mut start = 0;
    while start + block_len <= weighted.len() {
        let block = &weighted[start..start + block_len];
        let mean_sq = block.iter().map(|&y| y * y).sum::<f64>() / block_len as f64;
        powers.push(mean_sq);
        start += hop;
    }

    // Stage 1 — absolute gate at −70 LUFS.
    let block_loudness = |z: f64| -> f64 {
        if z > 0.0 {
            ABS_OFFSET + 10.0 * z.log10()
        } else {
            SILENCE_LUFS
        }
    };
    let gated_abs: Vec<f64> = powers
        .iter()
        .copied()
        .filter(|&z| block_loudness(z) >= ABSOLUTE_GATE_LUFS)
        .collect();
    if gated_abs.is_empty() {
        return SILENCE_LUFS;
    }

    // Stage 2 — relative gate at (mean loudness − 10 LU).
    let mean_abs = gated_abs.iter().sum::<f64>() / gated_abs.len() as f64;
    let relative_threshold = block_loudness(mean_abs) + RELATIVE_GATE_LU;
    let gated_rel: Vec<f64> = gated_abs
        .into_iter()
        .filter(|&z| block_loudness(z) >= relative_threshold)
        .collect();
    if gated_rel.is_empty() {
        return SILENCE_LUFS;
    }

    let mean_rel = gated_rel.iter().sum::<f64>() / gated_rel.len() as f64;
    block_loudness(mean_rel)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f64, amp: f64, secs: f64, sr: f64) -> Vec<f64> {
        let n = (secs * sr) as usize;
        (0..n)
            .map(|i| amp * (2.0 * core::f64::consts::PI * freq * i as f64 / sr).sin())
            .collect()
    }

    /// Doubling the amplitude must raise integrated loudness by exactly ~6 dB
    /// (loudness is scale-invariant in shape, so gating picks the same blocks).
    #[test]
    fn doubling_amplitude_adds_six_db() {
        let sr = 48_000.0;
        let quiet = integrated_lufs(&sine(1000.0, 0.1, 3.0, sr), sr);
        let loud = integrated_lufs(&sine(1000.0, 0.2, 3.0, sr), sr);
        assert!(quiet.is_finite() && loud.is_finite());
        assert!(
            (loud - quiet - 6.020_599).abs() < 0.01,
            "expected +6.02 LU, got {}",
            loud - quiet
        );
    }

    /// A 1 kHz sine near −20 dBFS RMS should read in a sane LUFS range (the
    /// K-weighting adds ~+2 dB of head gain at 1 kHz, minus the 0.691 offset).
    #[test]
    fn thousand_hz_reference_is_in_range() {
        let sr = 48_000.0;
        // −20 dBFS RMS → peak amplitude 10^(−20/20)·√2.
        let amp = 10f64.powf(-20.0 / 20.0) * 2f64.sqrt();
        let lufs = integrated_lufs(&sine(1000.0, amp, 3.0, sr), sr);
        assert!(
            (-21.0..=-17.0).contains(&lufs),
            "1kHz −20dBFS sine read {lufs} LUFS, outside plausible band"
        );
    }

    /// Silence and too-short signals return the sentinel, not NaN/0.
    #[test]
    fn silence_and_short_signals_are_sentinel() {
        let sr = 48_000.0;
        assert_eq!(integrated_lufs(&[], sr), SILENCE_LUFS);
        assert_eq!(integrated_lufs(&vec![0.0; 48_000], sr), SILENCE_LUFS);
        // 100 ms < one 400 ms block.
        assert_eq!(
            integrated_lufs(&sine(1000.0, 0.5, 0.1, sr), sr),
            SILENCE_LUFS
        );
    }

    /// Works at a non-48k rate too (coefficients are SR-parametrised): the same
    /// tone at 44.1 kHz reads within a hair of the 48 kHz measurement.
    #[test]
    fn sample_rate_independent() {
        let a = integrated_lufs(&sine(1000.0, 0.25, 3.0, 48_000.0), 48_000.0);
        let b = integrated_lufs(&sine(1000.0, 0.25, 3.0, 44_100.0), 44_100.0);
        assert!((a - b).abs() < 0.15, "48k={a} vs 44.1k={b}");
    }
}
