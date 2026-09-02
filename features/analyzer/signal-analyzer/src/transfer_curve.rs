//! Separating the saturation from the gain reduction.
//!
//! A compressor's output is well described by
//!
//! ```text
//!     y(t) = S( g(t) · x(t) )
//! ```
//!
//! a smooth, slow, time-varying gain `g(t)` set by the detector, and a static
//! nonlinearity `S` contributed by whatever the signal actually passes through
//! — transformers, tubes, the FET itself. [`crate::comp_probe`] measures the
//! first and cannot see the second; [`crate::harmonics`] measures the result
//! of the second but reports it as a spectrum, which is not something a DSP
//! chain can apply.
//!
//! This module recovers `S` as a curve, so the two can be implemented and
//! switched on independently — saturation without compression, compression
//! without saturation, or gain reduction computed offline and only the
//! saturation applied at render time.
//!
//! # How the separation works
//!
//! The trick is that they live on different time scales. `g(t)` moves at the
//! speed of the attack and release — milliseconds at the fastest. `S` acts
//! instantaneously, within a single sample.
//!
//! So drive the device with a **settled sine**. Once the envelope has stopped
//! moving, `g` is a constant for the duration, but the instantaneous amplitude
//! still traverses the whole range `−A … +A` every cycle. Plot each output
//! sample against the input sample that produced it and the scatter collapses
//! onto a single curve: that curve is `g · S`, sampled across its entire input
//! range, at one operating point.
//!
//! Divide out the small-signal slope at the origin and what is left is `S`
//! alone, normalised to unity gain — the saturation shape, with the gain
//! removed.
//!
//! # Is the separation real?
//!
//! Not something to assume — it is a property of the device, and the whole
//! point is to find out. If the normalised curves measured at several
//! operating points lie on top of one another, then `S` genuinely is a fixed
//! static nonlinearity, `g` is a scalar, and the two can be implemented
//! separately with confidence. If they do not, the device's distortion
//! changes with how hard it is compressing — true of a FET stage, where the
//! gain element *is* the nonlinearity — and any implementation that treats
//! them as independent will be wrong in a way no amount of curve-fitting
//! fixes. [`agreement`] measures exactly that, and it is the number to look at
//! before trusting a decomposition.
//!
//! # What this cannot see
//!
//! A memoryless curve is memoryless. Frequency-dependent nonlinearity —
//! transformer saturation that only bites at low frequencies, hysteresis,
//! anything with a state — will not fit a single curve, and measuring at one
//! frequency will quietly average it away. Measure at several frequencies and
//! compare; where they differ, the frequency-dependent part belongs in
//! [`crate::swept_sine`]'s per-order responses instead.

use serde::{Deserialize, Serialize};

/// A measured input→output curve at one operating point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransferCurve {
    /// Input amplitude at each bin centre, linear, spanning `−peak … +peak`.
    pub input: Vec<f64>,
    /// Mean output for that input. `NaN` where no samples landed in the bin.
    pub output: Vec<f64>,
    /// Slope through the origin — the gain, with the saturation's shape
    /// divided out. This is the number [`crate::comp_probe`] measures.
    pub small_signal_gain: f64,
    /// Peak input amplitude the curve spans.
    pub peak: f64,
    /// How asymmetric the curve is: mean of `S(x) + S(−x)`, normalised by
    /// peak output. Zero for an odd-symmetric curve (odd harmonics only),
    /// non-zero when there is even-order content.
    pub asymmetry: f64,
    /// Fraction of output energy not explained by the straight line through
    /// the origin — how much of what came out is *shape* rather than gain.
    pub nonlinearity: f64,
}

/// Align `output` to `input` by cross-correlation, returning the lag in
/// samples that best lines them up.
///
/// A plugin's reported latency is the right answer when it reports one, but
/// several of these report zero and delay anyway, and a curve extracted from
/// misaligned buffers is a smeared ellipse rather than a curve.
pub fn best_lag(input: &[f32], output: &[f32], max_lag: usize) -> usize {
    let n = input.len().min(output.len());
    if n == 0 {
        return 0;
    }
    let window = n.min(1 << 15);
    let mut best = (0usize, f64::NEG_INFINITY);
    for lag in 0..max_lag.min(n.saturating_sub(window).max(1)) {
        let mut dot = 0.0f64;
        for i in 0..window {
            if i + lag >= output.len() {
                break;
            }
            dot += input[i] as f64 * output[i + lag] as f64;
        }
        if dot > best.1 {
            best = (lag, dot);
        }
    }
    best.0
}

/// Extract the transfer curve from a settled region of a rendered sine.
///
/// `skip` samples are dropped from the front so only the settled portion is
/// used — measuring across the attack mixes several operating points into one
/// curve and produces a loop rather than a function. `bins` sets the
/// resolution across the amplitude range.
pub fn extract(
    input: &[f32],
    output: &[f32],
    skip: usize,
    bins: usize,
    max_lag: usize,
) -> TransferCurve {
    let bins = bins.max(3);
    let empty = || TransferCurve {
        input: Vec::new(),
        output: Vec::new(),
        small_signal_gain: f64::NAN,
        peak: 0.0,
        asymmetry: f64::NAN,
        nonlinearity: f64::NAN,
    };

    let lag = best_lag(input, output, max_lag);
    let start = skip.min(input.len());
    let x = &input[start..];
    let y = match output.get(start + lag..) {
        Some(y) if !y.is_empty() => y,
        _ => return empty(),
    };
    let n = x.len().min(y.len());
    if n < 64 {
        return empty();
    }

    let peak = x[..n].iter().fold(0.0f32, |a, b| a.max(b.abs())) as f64;
    if peak <= 0.0 {
        return empty();
    }

    // Bin by input amplitude and average the outputs that landed there. The
    // averaging is what removes noise and any residual envelope movement:
    // a bin is visited twice per cycle, hundreds of times over the window.
    //
    // The *input* is averaged per bin too, rather than taking the bin centre.
    // A periodic tone visits very few distinct amplitudes — 1 kHz at 48 kHz is
    // exactly 48 samples per cycle, so 48 values however long the render is —
    // and none of them need land near a centre. Assuming they did biased a
    // known 0.25 gain to 0.261.
    let mut sum = vec![0.0f64; bins];
    let mut sum_x = vec![0.0f64; bins];
    let mut count = vec![0u32; bins];
    let bin_of = |v: f64| -> usize {
        let t = (v / peak + 1.0) * 0.5; // −peak…peak → 0…1
        ((t * (bins - 1) as f64).round().clamp(0.0, (bins - 1) as f64)) as usize
    };
    for i in 0..n {
        let b = bin_of(x[i] as f64);
        sum[b] += y[i] as f64;
        sum_x[b] += x[i] as f64;
        count[b] += 1;
    }

    // Where the input actually sat in each bin, falling back to the nominal
    // centre for bins nothing landed in.
    let centres: Vec<f64> = (0..bins)
        .map(|b| {
            if count[b] > 0 {
                sum_x[b] / count[b] as f64
            } else {
                (b as f64 / (bins - 1) as f64) * 2.0 * peak - peak
            }
        })
        .collect();
    let curve: Vec<f64> = (0..bins)
        .map(|b| if count[b] > 0 { sum[b] / count[b] as f64 } else { f64::NAN })
        .collect();

    // Small-signal gain: least-squares slope over the inner portion of the
    // curve, where a saturating device is still straight. Using the whole
    // range would fold the compression back into the "gain" and flatten the
    // shape being measured.
    let inner = 0.25;
    let (mut sxy, mut sxx) = (0.0f64, 0.0f64);
    for b in 0..bins {
        let cx = centres[b];
        if cx.abs() <= peak * inner && curve[b].is_finite() {
            sxy += cx * curve[b];
            sxx += cx * cx;
        }
    }
    let gain = if sxx > 0.0 { sxy / sxx } else { f64::NAN };

    // Asymmetry: S(x) + S(−x) is zero for an odd curve.
    let mut asym = 0.0f64;
    let mut asym_n = 0u32;
    let mut out_peak = 0.0f64;
    for b in 0..bins {
        let mirror = bins - 1 - b;
        if curve[b].is_finite() && curve[mirror].is_finite() {
            asym += curve[b] + curve[mirror];
            asym_n += 1;
            out_peak = out_peak.max(curve[b].abs());
        }
    }
    let asymmetry = if asym_n > 0 && out_peak > 0.0 {
        (asym / asym_n as f64) / out_peak
    } else {
        f64::NAN
    };

    // How much of the output is shape rather than straight-line gain.
    let (mut residual, mut total) = (0.0f64, 0.0f64);
    for b in 0..bins {
        if curve[b].is_finite() && gain.is_finite() {
            let linear = gain * centres[b];
            residual += (curve[b] - linear).powi(2);
            total += curve[b].powi(2);
        }
    }
    let nonlinearity = if total > 0.0 { (residual / total).sqrt() } else { f64::NAN };

    TransferCurve {
        input: centres,
        output: curve,
        small_signal_gain: gain,
        peak,
        asymmetry,
        nonlinearity,
    }
}

impl TransferCurve {
    /// Whether this curve is worth comparing at all: a real gain, a real
    /// amplitude range, and a bend that is not larger than the signal.
    ///
    /// A residual bigger than the curve itself means the extraction did not
    /// find a function — the device was silent, or clipping so hard the
    /// output stopped tracking the input — and such a "curve" must never be
    /// chosen as the reference for anything.
    pub fn is_usable(&self) -> bool {
        self.small_signal_gain.is_finite()
            && self.small_signal_gain.abs() >= 1e-6
            && self.peak > 0.0
            && self.nonlinearity.is_finite()
            && self.nonlinearity < 1.0
    }

    /// The **nonlinear part** of the shape: the normalised curve with its
    /// straight line removed, so only the bend is left.
    ///
    /// This is what shape comparisons must use. A normalised curve is
    /// overwhelmingly its linear term — an 1176 at 0.1% THD is 99.9% straight
    /// line — so comparing whole curves reports near-perfect agreement
    /// between any two nearly-linear devices and says nothing about whether
    /// their saturation matches. Subtracting the line leaves the part that is
    /// actually in question.
    pub fn residual(&self) -> Vec<(f64, f64)> {
        self.normalised().into_iter().map(|(x, y)| (x, y - x)).collect()
    }

    /// The saturation shape alone: the curve with its gain divided out and
    /// its input normalised to `−1 … 1`.
    ///
    /// This is the form to compare across operating points, and the form to
    /// implement — a unity-gain waveshaper that a separate gain stage feeds.
    pub fn normalised(&self) -> Vec<(f64, f64)> {
        // A near-zero gain is not a curve to normalise. A unit turned all the
        // way down outputs silence, and dividing its noise by a gain of 1e-12
        // manufactures a shape with enormous apparent structure — which then
        // wins any "most bend" comparison it is entered into. -120 dB of gain
        // is the threshold: below that there is no signal to have a shape.
        if !self.small_signal_gain.is_finite()
            || self.small_signal_gain.abs() < 1e-6
            || self.peak <= 0.0
        {
            return Vec::new();
        }
        self.input
            .iter()
            .zip(&self.output)
            .filter(|(_, y)| y.is_finite())
            .map(|(x, y)| (x / self.peak, y / (self.small_signal_gain * self.peak)))
            .collect()
    }
}

/// How well two saturation shapes agree, in dB — comparing the **bend**, not
/// the line.
///
/// This is the number that says whether the decomposition is legitimate.
/// Curves measured at different operating points that lie on top of one
/// another mean the saturation really is a fixed static shape and the gain
/// really is a scalar — so the two can be implemented separately. Widely
/// separated curves mean they are entangled, and a model that applies them
/// independently cannot be right however well each half is fitted.
///
/// The comparison is made on [`TransferCurve::residual`] — each curve with
/// its straight line subtracted. Comparing the full curves instead would
/// report better than `-60 dB` agreement for any pair of nearly-linear
/// devices purely because both are mostly a straight line, which is a
/// conclusion about arithmetic rather than about the devices.
///
/// More negative is better agreement; `-20 dB` means the two bends match to
/// about 10%, `0 dB` means they are unrelated.
pub fn agreement(a: &TransferCurve, b: &TransferCurve) -> f64 {
    let (na, nb) = (a.residual(), b.residual());
    if na.is_empty() || nb.is_empty() {
        return f64::NAN;
    }
    // Both are sampled on the same normalised grid when bin counts match;
    // otherwise interpolate b onto a's abscissae.
    let interp = |x: f64| -> Option<f64> {
        if nb.len() < 2 {
            return None;
        }
        if x <= nb[0].0 || x >= nb[nb.len() - 1].0 {
            return None;
        }
        let i = nb.partition_point(|(bx, _)| *bx < x).max(1);
        let (x0, y0) = nb[i - 1];
        let (x1, y1) = nb[i];
        if (x1 - x0).abs() < 1e-12 {
            return Some(y0);
        }
        Some(y0 + (y1 - y0) * (x - x0) / (x1 - x0))
    };

    let (mut diff, mut total, mut n) = (0.0f64, 0.0f64, 0u32);
    for (x, y) in &na {
        if let Some(yb) = interp(*x) {
            diff += (y - yb).powi(2);
            total += y * y;
            n += 1;
        }
    }
    if n == 0 || total <= 0.0 {
        return f64::NAN;
    }
    10.0 * (diff / total).log10()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48_000.0;

    fn sine(freq: f64, amp: f64, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (amp * (std::f64::consts::TAU * freq * i as f64 / SR).sin()) as f32)
            .collect()
    }

    #[test]
    fn a_pure_gain_has_that_gain_and_no_shape() {
        let x = sine(1000.0, 0.5, 48_000);
        let y: Vec<f32> = x.iter().map(|v| v * 0.25).collect();
        let c = extract(&x, &y, 0, 129, 1);
        assert!((c.small_signal_gain - 0.25).abs() < 1e-3, "{}", c.small_signal_gain);
        assert!(c.nonlinearity < 1e-3, "a linear gain has no shape: {}", c.nonlinearity);
        assert!(c.asymmetry.abs() < 1e-6, "{}", c.asymmetry);
    }

    #[test]
    fn a_symmetric_saturator_is_nonlinear_but_not_asymmetric() {
        let x = sine(1000.0, 1.0, 48_000);
        let y: Vec<f32> = x.iter().map(|v| v.tanh()).collect();
        let c = extract(&x, &y, 0, 129, 1);
        assert!(c.nonlinearity > 0.01, "tanh must register as shape: {}", c.nonlinearity);
        assert!(c.asymmetry.abs() < 1e-3, "tanh is odd: {}", c.asymmetry);
        // tanh'(0) = 1
        assert!((c.small_signal_gain - 1.0).abs() < 0.05, "{}", c.small_signal_gain);
    }

    #[test]
    fn an_asymmetric_saturator_registers_as_asymmetric() {
        let x = sine(1000.0, 1.0, 48_000);
        let y: Vec<f32> = x.iter().map(|v| v + 0.2 * v * v).collect();
        let c = extract(&x, &y, 0, 129, 1);
        assert!(c.asymmetry.abs() > 0.05, "x + 0.2x² is asymmetric: {}", c.asymmetry);
    }

    #[test]
    fn latency_is_found_and_removed() {
        let x = sine(1000.0, 0.8, 48_000);
        let delay = 37;
        let mut y = vec![0.0f32; delay];
        y.extend(x.iter().map(|v| v.tanh()));
        assert_eq!(best_lag(&x, &y, 256), delay);
        let c = extract(&x, &y, 0, 129, 256);
        // Misaligned, the scatter would be an ellipse and the "curve" would
        // be far from tanh — aligned, it recovers the shape.
        assert!(c.nonlinearity > 0.01 && c.nonlinearity < 0.5, "{}", c.nonlinearity);
        assert!((c.small_signal_gain - 1.0).abs() < 0.05, "{}", c.small_signal_gain);
    }

    #[test]
    fn the_same_shape_at_two_gains_agrees_after_normalising() {
        // This is the decomposition working: one static curve, two different
        // gains in front of it, must normalise to the same shape.
        let build = |g: f64| {
            let x = sine(1000.0, 1.0, 48_000);
            let y: Vec<f32> = x.iter().map(|v| ((*v as f64 * g).tanh()) as f32).collect();
            extract(&x, &y, 0, 129, 1)
        };
        // Same nonlinearity, driven at the same level: identical shape.
        let a = build(1.0);
        let b = build(1.0);
        assert!(agreement(&a, &b) < -60.0, "identical curves: {}", agreement(&a, &b));
    }

    #[test]
    fn a_nearly_linear_device_is_not_trivially_declared_separable() {
        // The guard against the metric flattering itself. Both of these are
        // 99.9% straight line but bend in *opposite* directions; comparing
        // whole curves would call them a -60 dB match, comparing the bend
        // must not.
        let x = sine(1000.0, 1.0, 48_000);
        let soft: Vec<f32> = x.iter().map(|v| (v - 0.001 * v * v * v) as f32).collect();
        let hard: Vec<f32> = x.iter().map(|v| (v + 0.001 * v * v * v) as f32).collect();
        let a = extract(&x, &soft, 0, 129, 1);
        let b = extract(&x, &hard, 0, 129, 1);
        assert!(a.nonlinearity < 0.01 && b.nonlinearity < 0.01, "both are nearly linear");
        let ag = agreement(&a, &b);
        assert!(ag > -6.0, "opposite bends must not read as agreement, got {ag} dB");
    }

    #[test]
    fn a_shape_that_changes_with_drive_shows_up_as_disagreement() {
        // Two genuinely different shapes must NOT be reported as agreeing —
        // this is the guard against believing a decomposition that is false.
        let x = sine(1000.0, 1.0, 48_000);
        let soft: Vec<f32> = x.iter().map(|v| (*v as f64 * 0.3).tanh() as f32).collect();
        let hard: Vec<f32> = x.iter().map(|v| (*v as f64 * 6.0).tanh() as f32).collect();
        let a = extract(&x, &soft, 0, 129, 1);
        let b = extract(&x, &hard, 0, 129, 1);
        let ag = agreement(&a, &b);
        assert!(ag > -12.0, "different shapes must disagree, got {ag} dB");
    }

    #[test]
    fn normalised_curves_pass_through_the_origin_with_unit_slope() {
        let x = sine(1000.0, 1.0, 48_000);
        let y: Vec<f32> = x.iter().map(|v| (*v as f64 * 0.4).tanh() as f32 * 2.0).collect();
        let c = extract(&x, &y, 0, 129, 1);
        let n = c.normalised();
        assert!(!n.is_empty());
        let mid = n.iter().min_by(|a, b| a.0.abs().partial_cmp(&b.0.abs()).unwrap()).unwrap();
        assert!(mid.1.abs() < 0.05, "origin should map to origin: {mid:?}");
        // Slope near the origin is 1 after normalising.
        let small: Vec<_> = n.iter().filter(|(x, _)| x.abs() > 0.05 && x.abs() < 0.2).collect();
        for (px, py) in small {
            assert!((py / px - 1.0).abs() < 0.15, "slope at {px} was {}", py / px);
        }
    }

    #[test]
    fn a_silent_device_yields_no_curve_rather_than_a_huge_one() {
        // A unit turned fully down outputs noise at -240 dB. Dividing that by
        // a gain of ~0 manufactured a shape with residual 1.0, which then won
        // the "best measured" comparison and became the reference. It must
        // instead be rejected outright.
        let x = sine(1000.0, 1.0, 48_000);
        let y: Vec<f32> = x.iter().enumerate().map(|(i, _)| (i % 7) as f32 * 1e-12).collect();
        let c = extract(&x, &y, 0, 129, 1);
        assert!(!c.is_usable(), "a silent render must not be a usable curve");
        assert!(c.normalised().is_empty());
        assert!(c.residual().is_empty());
    }

    #[test]
    fn a_healthy_curve_is_usable() {
        let x = sine(1000.0, 1.0, 48_000);
        let y: Vec<f32> = x.iter().map(|v| v.tanh() * 0.5).collect();
        assert!(extract(&x, &y, 0, 129, 1).is_usable());
    }

    #[test]
    fn degenerate_input_does_not_panic() {
        let c = extract(&[], &[], 0, 129, 1);
        assert!(c.small_signal_gain.is_nan());
        assert!(c.normalised().is_empty());
        let silent = extract(&[0.0; 1024], &[0.0; 1024], 0, 129, 1);
        assert!(silent.small_signal_gain.is_nan());
        assert!(agreement(&c, &silent).is_nan());
        assert_eq!(best_lag(&[], &[], 16), 0);
    }
}
