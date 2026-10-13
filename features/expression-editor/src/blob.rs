//! The Melodyne decomposition, as a derived view over any pitch curve.
//!
//! ```text
//! pitch(t) = center + drift(t)·drift_amount + modulation(t)·modulation_amount
//! ```
//!
//! - **center** — the note's pitch center. Dragging a blob moves only
//!   this; snapping quantizes only this.
//! - **drift** — the slow slide of the contour away from center (a
//!   singer going flat across a held note), ≲ 3 Hz.
//! - **modulation** — the vibrato residual, everything faster.
//!
//! Melodyne's "pitch drift" and "pitch modulation" tools are exactly
//! the two amount scalars. `tune_dsp::model::NoteBlob` stores this
//! decomposition as truth for analyzed audio; here it is *derived on
//! demand* from the authored curve, so the same two sliders work on a
//! hand-drawn MPE bend that was never analyzed.
//!
//! Raw points stay the source of truth in both domains. A decomposition
//! that round-trips through `recompose` reproduces the curve it came
//! from, so opening the drift/vibrato controls is never destructive.

use crate::doc::{Curve, Point};

/// Frequencies below this are drift; above, vibrato. Vibrato sits at
/// 4–7 Hz, drift well under 2.
pub const DRIFT_CUTOFF_HZ: f64 = 3.0;

/// A curve split into its three terms, sampled on a uniform grid.
#[derive(Clone, Debug, PartialEq)]
pub struct Decomposition {
    /// Uniform sample times.
    pub times: Vec<f64>,
    /// The pitch center (median of the contour).
    pub center: f64,
    /// Slow deviation from center, per sample.
    pub drift: Vec<f64>,
    /// Vibrato residual, per sample.
    pub modulation: Vec<f64>,
}

impl Decomposition {
    /// Rebuild a curve from the terms, scaling drift and vibrato.
    ///
    /// `1.0 / 1.0` reproduces the original; `0.0 / 0.0` is the robot.
    pub fn recompose(&self, center: f64, drift_amount: f64, modulation_amount: f64) -> Curve {
        Curve::from_points(
            self.times
                .iter()
                .enumerate()
                .map(|(i, &t)| Point {
                    t,
                    value: center
                        + self.drift[i] * drift_amount
                        + self.modulation[i] * modulation_amount,
                    ..Point::default()
                })
                .collect(),
        )
    }

    /// Peak-to-peak vibrato depth in semitones — the readout that tells
    /// you whether a note has vibrato worth editing.
    pub fn modulation_depth(&self) -> f64 {
        let (mut lo, mut hi) = (f64::MAX, f64::MIN);
        for &v in &self.modulation {
            lo = lo.min(v);
            hi = hi.max(v);
        }
        if lo > hi { 0.0 } else { hi - lo }
    }

    /// How far the contour slides overall, in semitones (signed).
    pub fn drift_extent(&self) -> f64 {
        match (self.drift.first(), self.drift.last()) {
            (Some(a), Some(b)) => b - a,
            _ => 0.0,
        }
    }
}

/// Decompose `curve` over `[t0, t1]` at `sample_count` uniform samples.
///
/// `units_per_second` converts document time to seconds so the drift
/// cutoff means the same thing at any tempo or analysis hop.
pub fn decompose(
    curve: &Curve,
    t0: f64,
    t1: f64,
    sample_count: usize,
    units_per_second: f64,
    default: f64,
) -> Decomposition {
    let n = sample_count.max(2);
    let times: Vec<f64> = (0..n)
        .map(|i| t0 + (t1 - t0) * (i as f64 / (n - 1) as f64))
        .collect();
    let contour: Vec<f64> = times.iter().map(|&t| curve.sample(t, default)).collect();

    let center = median(&contour);
    let residual: Vec<f64> = contour.iter().map(|v| v - center).collect();

    // Sample rate of this uniform grid, in Hz.
    let span_s = ((t1 - t0) / units_per_second.max(1e-9)).max(1e-9);
    let grid_rate = (n - 1) as f64 / span_s;

    let drift = zero_phase_lowpass(&residual, DRIFT_CUTOFF_HZ, grid_rate);
    let modulation: Vec<f64> = residual.iter().zip(&drift).map(|(r, d)| r - d).collect();

    Decomposition {
        times,
        center,
        drift,
        modulation,
    }
}

/// Zero-phase one-pole low-pass (forward then backward).
///
/// Two passes, not one: a single pass lags, which would slide the
/// vibrato residual out of alignment with the note and make
/// `recompose` a lossy operation.
fn zero_phase_lowpass(x: &[f64], cutoff_hz: f64, rate_hz: f64) -> Vec<f64> {
    if x.is_empty() {
        return Vec::new();
    }
    if rate_hz <= 0.0 || cutoff_hz <= 0.0 || cutoff_hz * 2.0 >= rate_hz {
        // Cutoff at or above Nyquist: nothing to separate, it is all
        // drift.
        return x.to_vec();
    }
    let coeff = 1.0 - (-core::f64::consts::TAU * cutoff_hz / rate_hz).exp();
    let mut fwd = Vec::with_capacity(x.len());
    let mut state = x[0];
    for &v in x {
        state += (v - state) * coeff;
        fwd.push(state);
    }
    let mut state = *fwd.last().unwrap();
    let mut out = vec![0.0; x.len()];
    for i in (0..x.len()).rev() {
        state += (fwd[i] - state) * coeff;
        out[i] = state;
    }
    out
}

fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut v = values.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
    let mid = v.len() / 2;
    if v.len().is_multiple_of(2) {
        (v[mid - 1] + v[mid]) * 0.5
    } else {
        v[mid]
    }
}

/// Where a curve spends most of its time, as a pitch — the "effective
/// center" a zone scales around.
///
/// Not the note's own row and not the mean: a scoop that starts a
/// fourth below and settles on the target should scale around the
/// *target*, which is where the curve dwells. Computed as the mode over
/// semitone-wide bins, refined by the median of that bin's samples.
pub fn effective_center(curve: &Curve, t0: f64, t1: f64, sample_count: usize, default: f64) -> f64 {
    let n = sample_count.max(2);
    let samples: Vec<f64> = (0..n)
        .map(|i| {
            let t = t0 + (t1 - t0) * (i as f64 / (n - 1) as f64);
            curve.sample(t, default)
        })
        .collect();
    if samples.is_empty() {
        return default;
    }
    let lo = samples.iter().cloned().fold(f64::MAX, f64::min);
    let mut counts: Vec<(i64, Vec<f64>)> = Vec::new();
    for &v in &samples {
        let bin = (v - lo).floor() as i64;
        match counts.iter_mut().find(|(b, _)| *b == bin) {
            Some((_, xs)) => xs.push(v),
            None => counts.push((bin, vec![v])),
        }
    }
    let best = counts
        .iter()
        .max_by_key(|(_, xs)| xs.len())
        .map(|(_, xs)| xs.as_slice())
        .unwrap_or(&samples);
    median(best)
}
