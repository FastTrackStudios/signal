//! The gain-compensation verification harness — spec
//! `docs/spec/fx/gain-comp.md` (`fx.gain-comp.verify-harness`).
//!
//! THE definition of "compensated": render the pink-noise reference at
//! −18 dBFS RMS through a [`Stage`], compare output RMS to input RMS, and
//! assert the deviation stays inside the spec bound (±1 dB over the full
//! reachable range of a character control, ±0.5 dB over its middle 80 %).
//! Every production plugin has a test that sweeps each character control
//! through this and fails outside the bound — a plugin that passes is
//! compliant, one that does not is not, regardless of intent.
//!
//! std-only (test infrastructure, not audio-thread code).

use crate::Stage;
use alloc::vec::Vec;

/// The reference level, dBFS RMS (K-20 / line level).
pub const REFERENCE_DBFS: f64 = -18.0;

/// Spec bound over a control's full reachable range, dB.
pub const FULL_RANGE_BOUND_DB: f64 = 1.0;

/// Spec bound over the middle 80 % of a control's travel, dB.
pub const TYPICAL_RANGE_BOUND_DB: f64 = 0.5;

/// Deterministic white noise: xorshift64*, fixed seed — the harness must be
/// reproducible run to run (`fx.gain-comp.deterministic` applies to the test
/// as much as the plugin).
struct Rng(u64);

impl Rng {
    fn next_f64(&mut self) -> f64 {
        // xorshift64* — plenty for noise.
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        let u = x.wrapping_mul(0x2545_F491_4F6C_DD1D);
        // Map the top 53 bits to (−1, 1).
        ((u >> 11) as f64 / (1u64 << 52) as f64) - 1.0
    }
}

/// Pink noise via the Paul Kellet economy filter over deterministic white
/// noise, band-limited to the audible band (2nd-order high-pass at 20 Hz —
/// sub-audible pink power would otherwise charge a stage's DC blocker with
/// a level "loss" no ear can hear), scaled to [`REFERENCE_DBFS`] RMS.
/// Stereo: two decorrelated channels (different seeds). Assumes ~48 kHz.
pub fn pink_reference(len: usize) -> (Vec<f64>, Vec<f64>) {
    fn channel(len: usize, seed: u64) -> Vec<f64> {
        let mut rng = Rng(seed);
        let (mut b0, mut b1, mut b2) = (0.0f64, 0.0, 0.0);
        // Two cascaded one-pole high-passes at 20 Hz (48 kHz).
        let r = 1.0 - core::f64::consts::TAU * 20.0 / 48_000.0;
        let (mut h1x, mut h1y, mut h2x, mut h2y) = (0.0f64, 0.0, 0.0, 0.0);
        let mut v: Vec<f64> = (0..len)
            .map(|_| {
                let white = rng.next_f64();
                b0 = 0.99765 * b0 + white * 0.099_046;
                b1 = 0.96300 * b1 + white * 0.296_396_6;
                b2 = 0.57000 * b2 + white * 1.052_651_3;
                let pink = b0 + b1 + b2 + white * 0.1848;
                let y1 = pink - h1x + r * h1y;
                h1x = pink;
                h1y = y1;
                let y2 = y1 - h2x + r * h2y;
                h2x = y1;
                h2y = y2;
                y2
            })
            .collect();
        // Scale to the reference RMS.
        let rms = rms(&v);
        let target = db_to_lin(REFERENCE_DBFS);
        let g = if rms > 0.0 { target / rms } else { 1.0 };
        v.iter_mut().for_each(|x| *x *= g);
        v
    }
    (
        channel(len, 0x9E37_79B9_7F4A_7C15),
        channel(len, 0xD1B5_4A32_D192_ED03),
    )
}

/// Plain RMS of a buffer.
pub fn rms(buf: &[f64]) -> f64 {
    if buf.is_empty() {
        return 0.0;
    }
    let sum: f64 = buf.iter().map(|x| x * x).sum();
    crate::sqrt(sum / buf.len() as f64)
}

/// RMS in dBFS.
pub fn rms_db(buf: &[f64]) -> f64 {
    lin_to_db(rms(buf))
}

pub fn db_to_lin(db: f64) -> f64 {
    libm::pow(10.0, db / 20.0)
}

pub fn lin_to_db(lin: f64) -> f64 {
    20.0 * libm::log10(lin.max(1e-12))
}

/// Render the reference through `stage` and return the level deviation in
/// dB: `RMS(out) − RMS(in)`, measured after a warm-up region so envelopes
/// and compensation smoothing have settled. 0.0 = perfectly compensated.
///
/// `sample_rate` only sizes the render: 2 s of signal, first 25 % skipped.
// r[impl fx.gain-comp.verify-harness]
pub fn level_deviation_db<S: Stage>(stage: &mut S, sample_rate: f64) -> f64 {
    let len = (sample_rate * 2.0) as usize;
    let (mut l, mut r) = pink_reference(len);
    let (dry_l, dry_r) = (l.clone(), r.clone());
    // Blockwise, like a host would.
    let block = 256;
    let mut i = 0;
    while i < len {
        let end = (i + block).min(len);
        let (lb, rb) = (&mut l[i..end], &mut r[i..end]);
        stage.process(lb, rb);
        i = end;
    }
    let skip = len / 4;
    let wet = rms_db(&l[skip..]).max(rms_db(&r[skip..]));
    let dry = rms_db(&dry_l[skip..]).max(rms_db(&dry_r[skip..]));
    wet - dry
}

/// Sweep one character control through `points` positions (0..=1 of its
/// travel), building a fresh stage per point via `make`, and return
/// `(worst_full_range, worst_typical_range)` deviations in dB. The typical
/// range is the middle 80 % of travel (`fx.gain-comp.reference`).
///
/// Assert against [`FULL_RANGE_BOUND_DB`] / [`TYPICAL_RANGE_BOUND_DB`].
// r[impl fx.gain-comp.reference]
pub fn sweep_deviation_db<S: Stage>(
    mut make: impl FnMut(f64) -> S,
    points: usize,
    sample_rate: f64,
) -> (f64, f64) {
    let mut worst_full = 0.0f64;
    let mut worst_typical = 0.0f64;
    for p in 0..points.max(2) {
        let t = p as f64 / (points.max(2) - 1) as f64;
        let mut stage = make(t);
        let dev = level_deviation_db(&mut stage, sample_rate).abs();
        worst_full = worst_full.max(dev);
        if (0.1..=0.9).contains(&t) {
            worst_typical = worst_typical.max(dev);
        }
    }
    (worst_full, worst_typical)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Stage;

    struct Gain(f64);
    impl Stage for Gain {
        fn process(&mut self, l: &mut [f64], r: &mut [f64]) {
            l.iter_mut().for_each(|x| *x *= self.0);
            r.iter_mut().for_each(|x| *x *= self.0);
        }
    }

    // r[verify fx.gain-comp.verify-harness]
    #[test]
    fn the_reference_is_minus_18_dbfs_and_deterministic() {
        let (l, r) = pink_reference(96_000);
        assert!((rms_db(&l) - REFERENCE_DBFS).abs() < 0.01, "{}", rms_db(&l));
        assert!((rms_db(&r) - REFERENCE_DBFS).abs() < 0.01);
        let (l2, _) = pink_reference(96_000);
        assert_eq!(l[1234], l2[1234], "reference must be reproducible");
        // Channels are decorrelated, not copies.
        assert_ne!(l[1234], r[1234]);
    }

    // r[verify fx.gain-comp.verify-harness]
    #[test]
    fn deviation_measures_gain_exactly() {
        assert!(level_deviation_db(&mut Gain(1.0), 48_000.0).abs() < 1e-9);
        let dev = level_deviation_db(&mut Gain(0.5), 48_000.0);
        assert!((dev - (-6.0206)).abs() < 0.01, "{dev}");
    }

    // r[verify fx.gain-comp.reference]
    #[test]
    fn a_sweep_reports_the_worst_case_and_the_typical_band() {
        // A stage whose gain error grows toward the extremes: ±0.8 dB at the
        // ends, ±0.3 dB inside — passes the spec bounds exactly as intended.
        let (full, typical) =
            sweep_deviation_db(|t| Gain(db_to_lin(0.8 * (2.0 * t - 1.0))), 11, 48_000.0);
        assert!(full <= FULL_RANGE_BOUND_DB, "{full}");
        assert!(typical <= TYPICAL_RANGE_BOUND_DB + 0.15, "{typical}");
        assert!(full > typical);
    }
}
