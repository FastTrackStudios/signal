//! A piece with no working trigger, from its close mic alone.
//!
//! With nothing to calibrate against, two things stand in for the
//! trigger:
//!
//! - **The mic's own level split.** Its attacks fall in two clusters —
//!   its own strokes, and everything else's bleed — and the threshold is
//!   the valley between them ([`otsu_db`]), found from the take itself.
//! - **Every other close mic's bleed, predicted.** How loud a snare is on
//!   the tom mic, relative to the snare mic, is a fixed property of where
//!   the mics stand. It is learned at the other piece's *strokes* —
//!   trigger-verified hits where it has a working trigger, else its own
//!   confident moments ([`bleed_gain`]), and a moment is this piece's
//!   only if it is louder than that prediction by a margin.
//!
//! The strokes matter. Learned from another mic's raw loud moments, the
//! gain from one floor tom to the other came out above unity — both
//! mics were hearing the kick at the same level — and every real stroke
//! on the second was refused as the first one's bleed.
//!
//! Measured on Unbreakable against the triggers it stands in for: about
//! 0.97 F1 on kick and rack toms, but as low as 0.36 on a floor tom,
//! which the kick's low end reaches at full strength. So its hits carry
//! a confidence that says so, and they are the ones to look at.

use crate::attack::{attack_peak, attack_start};
use crate::fuse::Hit;
use crate::gate::{Gate, Scan, gate};

/// How much louder than the predicted bleed a moment must be, linear.
pub const MARGIN: f64 = 2.0;
/// The loose gate that proposes moments: the scan's lowest threshold,
/// the usual crest.
const PROPOSE: Gate = Gate {
    threshold_db: -66.0,
    crest_db: 5.0,
    retrig_secs: 0.03,
};
/// The highest confidence a hit found this way gets.
pub const MOST: f64 = 0.7;
/// The lowest.
pub const LEAST: f64 = 0.2;

/// How loud the attack around `at` is: the loudest sample from 1 ms
/// before to 8 ms after, linear.
#[must_use]
pub fn attack_level(samples: &[f32], rate: f64, at: usize) -> f64 {
    let from = at.saturating_sub(crate::samples(0.001, rate));
    f64::from(attack_peak(samples, rate, from).max(attack_peak(
        samples,
        rate,
        at.saturating_add(crate::samples(0.002, rate)),
    )))
}

/// The valley between two clusters of levels (dB): Otsu's split on a
/// half-decibel histogram. `None` for fewer than two levels.
#[must_use]
pub fn otsu_db(levels: &[f64]) -> Option<f64> {
    let lo = levels.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = levels.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if levels.len() < 2 || !lo.is_finite() || !hi.is_finite() {
        return None;
    }
    let bins = num_traits::ToPrimitive::to_usize(&((hi - lo) / 0.5).ceil())?.saturating_add(1);
    let mut histogram = vec![0.0_f64; bins];
    for v in levels {
        let b = num_traits::ToPrimitive::to_usize(&((v - lo) / 0.5)).unwrap_or(0);
        if let Some(c) = histogram.get_mut(b.min(bins.saturating_sub(1))) {
            *c += 1.0;
        }
    }
    let index = |i: usize| crate::seconds(i, 1.0);
    let total: f64 = histogram.iter().sum();
    let sum_all: f64 = histogram
        .iter()
        .enumerate()
        .map(|(i, c)| index(i) * c)
        .sum();
    let (mut below, mut below_sum) = (0.0, 0.0);
    let mut best = (f64::NEG_INFINITY, 0_usize);
    for (i, c) in histogram.iter().enumerate() {
        below += c;
        if below <= 0.0 {
            continue;
        }
        let above = total - below;
        if above <= 0.0 {
            break;
        }
        below_sum += index(i) * c;
        let spread = below * above * (below_sum / below - (sum_all - below_sum) / above).powi(2);
        if spread > best.0 {
            best = (spread, i);
        }
    }
    Some((index(best.1) + 1.0).mul_add(0.5, lo))
}

/// A mic's confident moments: the loose gate's, kept above the valley
/// of their levels. With the valley, in dBFS.
#[must_use]
pub fn confident(samples: &[f32], scan: &Scan, crest_db: f64) -> (Vec<usize>, f64) {
    let rate = scan.rate();
    let propose = Gate {
        crest_db,
        ..PROPOSE
    };
    let moments = scan
        .gate_at(&propose)
        .unwrap_or_else(|| gate(samples, rate, &propose));
    let levels: Vec<f64> = moments
        .iter()
        .map(|&t| crate::db(attack_level(samples, rate, t)))
        .collect();
    let Some(valley) = otsu_db(&levels) else {
        return (moments, f64::NEG_INFINITY);
    };
    let kept = moments
        .into_iter()
        .zip(levels)
        .filter(|(_, l)| *l >= valley)
        .map(|(t, _)| t)
        .collect();
    (kept, valley)
}

/// How loud `other`'s strokes arrive on `own`, as a gain: the median,
/// over `other`'s strokes, of how loud each was on `own` against on
/// `other`. `None` with too few strokes to say.
///
/// A stroke counts unless `own` was the louder of the two there — then
/// it is `own`'s drum the moment belongs to (a kick's bleed among the
/// snare mic's loud moments, when the snare has no trigger to say which
/// are its own). Keeping only the strokes where `own` was *quiet*, as
/// this once did, selects the ones that bled least and the gain comes out
/// low: on Unbreakable a snare stroke that reached every tom mic at
/// −32 dBFS was predicted at −41 on the floor tom's and passed as a hit.
#[must_use]
pub fn bleed_gain(own: &[f32], other: &[f32], rate: f64, strokes: &[usize]) -> Option<f64> {
    let mut ratios: Vec<f64> = strokes
        .iter()
        .filter_map(|&t| {
            let mine = attack_level(own, rate, t);
            let theirs = attack_level(other, rate, t);
            (theirs > 0.0 && mine < theirs).then(|| mine / theirs)
        })
        .collect();
    if ratios.len() < 5 {
        return None;
    }
    ratios.sort_by(f64::total_cmp);
    ratios.get(ratios.len() / 2).copied()
}

/// Another close mic, and when its own drum was struck.
#[derive(Clone, Copy, Debug)]
pub struct Other<'a> {
    pub samples: &'a [f32],
    /// Its strokes, in samples: its fused hits where it has a working
    /// trigger, else its [`confident`] moments.
    pub strokes: &'a [usize],
}

/// The piece's hits from its close mic `own` alone, refusing the bleed
/// `others` (the other close mics, same clock and rate) would put there.
#[must_use]
pub fn without_trigger(own: &[f32], scan: &Scan, others: &[Other<'_>], crest_db: f64) -> Vec<Hit> {
    let rate = scan.rate();
    let (moments, _) = confident(own, scan, crest_db);
    let gains: Vec<(usize, f64)> = others
        .iter()
        .enumerate()
        .filter_map(|(n, other)| {
            bleed_gain(own, other.samples, rate, other.strokes).map(|g| (n, g))
        })
        .collect();
    let hits = moments
        .into_iter()
        .filter_map(|t| {
            let mine = attack_level(own, rate, t);
            let predicted = gains
                .iter()
                .filter_map(|(n, g)| others.get(*n).map(|o| g * attack_level(o.samples, rate, t)))
                .fold(0.0_f64, f64::max);
            let over = if predicted > 0.0 {
                mine / predicted
            } else {
                f64::INFINITY
            };
            (over >= MARGIN).then(|| {
                // 6 dB over the prediction is the least kept; 18 dB over
                // is as sure as a mic alone gets.
                let sure = ((crate::db(over) - 6.0) / 12.0).clamp(0.0, 1.0);
                let at = attack_start(own, rate, t);
                Hit {
                    at,
                    peak: attack_peak(own, rate, at),
                    confidence: sure.mul_add(MOST - LEAST, LEAST),
                    from_trigger: false,
                    from_mic: true,
                    ghost: false,
                    flam_with: None,
                }
            })
        })
        .collect();
    crate::fuse::settle(hits, rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_valley_is_between_the_clusters() {
        let mut levels = vec![-50.0, -49.0, -51.0, -48.5, -50.5];
        levels.extend([-12.0, -10.0, -11.5, -9.0]);
        let v = otsu_db(&levels).unwrap();
        assert!(v > -48.5 && v < -12.0, "{v}");
    }
}
