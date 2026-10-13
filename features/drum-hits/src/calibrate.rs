//! A mic's gate fitted to its trigger.
//!
//! The gate's shipped settings (−60 dBFS, 5 dB) on a close mic report
//! the bleed: Unbreakable's tom mics give some 1,500 hits for 13 to 75
//! strokes. The settings that fit a mic are a property of the mic — its
//! gain, the bleed reaching it — and hold from take to take, so they are
//! found once, against the one thing that knows when the drum was hit:
//! its trigger.
//!
//! The search is the [`Scan`]'s grid of thresholds and crests, scored
//! by how well the gate's hits match the trigger's ([`agree`]). The
//! signal is walked once; each setting is a walk over its stretches.

use crate::gate::{CRESTS, Gate, Scan, THRESHOLDS};

/// How two lists of hits match.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Agreement {
    /// Hits in both.
    pub matched: usize,
    /// Hits found (the list being judged).
    pub found: usize,
    /// Hits in the reference.
    pub truth: usize,
}

/// `part` over `whole`, 0 when there is no whole.
fn ratio(part: usize, whole: usize) -> f64 {
    let (p, w) = (super::seconds(part, 1.0), super::seconds(whole, 1.0));
    if w > 0.0 { p / w } else { 0.0 }
}

impl Agreement {
    /// How many of the hits found are real.
    #[must_use]
    pub fn precision(&self) -> f64 {
        ratio(self.matched, self.found)
    }

    /// How many real hits were found.
    #[must_use]
    pub fn recall(&self) -> f64 {
        ratio(self.matched, self.truth)
    }

    /// The two together.
    #[must_use]
    pub fn f1(&self) -> f64 {
        let (p, r) = (self.precision(), self.recall());
        if p + r > 0.0 {
            2.0 * p * r / (p + r)
        } else {
            0.0
        }
    }
}

/// Match `found` against `truth` (both sorted, in samples): a pair is
/// within `tolerance` samples, each hit used once.
#[must_use]
pub fn agree(found: &[usize], truth: &[usize], tolerance: usize) -> Agreement {
    let (mut i, mut j, mut matched) = (0_usize, 0_usize, 0_usize);
    while let (Some(&f), Some(&t)) = (found.get(i), truth.get(j)) {
        if f.abs_diff(t) <= tolerance {
            matched = matched.saturating_add(1);
            i = i.saturating_add(1);
            j = j.saturating_add(1);
        } else if f < t {
            i = i.saturating_add(1);
        } else {
            j = j.saturating_add(1);
        }
    }
    Agreement {
        matched,
        found: found.len(),
        truth: truth.len(),
    }
}

/// A gate fitted to a trigger, and how well it fits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Calibration {
    pub gate: Gate,
    pub agreement: Agreement,
}

/// How far apart a mic's hit and its trigger's may be and still be one
/// stroke, seconds. The gate fires a little after the attack starts and
/// the trigger part way up its spike; 10 ms covers both.
pub const TOLERANCE: f64 = 0.010;

/// The gate on the scanned signal that agrees best with `truth`.
///
/// `truth` is the trigger's hits, in samples; `retrig_secs` is kept as
/// given. Ties go to the lower threshold — the one that keeps more ghost
/// notes. `None` with no truth to fit to.
#[must_use]
pub fn calibrate(scan: &Scan, truth: &[usize], retrig_secs: f64) -> Option<Calibration> {
    if truth.is_empty() || scan.is_empty() {
        return None;
    }
    let tolerance = super::samples(TOLERANCE, scan.rate());
    let mut best: Option<Calibration> = None;
    for (t, threshold_db) in THRESHOLDS.iter().enumerate() {
        for (c, crest_db) in CRESTS.iter().enumerate() {
            let gate = Gate {
                threshold_db: *threshold_db,
                crest_db: *crest_db,
                retrig_secs,
            };
            let agreement = agree(&scan.gate(c, t, retrig_secs), truth, tolerance);
            if best.is_none_or(|b| agreement.f1() > b.agreement.f1()) {
                best = Some(Calibration { gate, agreement });
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agreement_counts_each_hit_once() {
        let a = agree(&[100, 200, 205, 400], &[101, 203, 300], 5);
        assert_eq!(a.matched, 2);
        assert!((a.precision() - 0.5).abs() < 1e-9);
        assert!((a.recall() - 2.0 / 3.0).abs() < 1e-9);
    }
}
