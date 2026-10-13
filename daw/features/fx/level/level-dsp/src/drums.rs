//! Drum leveling: one gain a stroke.
//!
//! A voice is ridden phrase by phrase; a drum is struck. Ridden block by
//! block like a voice, a kick pumps — every ring pulled up as it decays,
//! every attack pushed down as it lands. Leveling a drum is what an
//! engineer does by hand with clip gain, or a compressor with a long
//! release does in real time: each stroke's level read once, at its
//! attack, and the gain that brings it toward the others set just before
//! it and held through its ring until the next stroke lands.
//!
//! A ghost note, bleed from another drum, a flam's grace — none is a
//! stroke of its own: it rides under the gain of the stroke before it,
//! so a ghost stays a ghost.
//!
//! The curve is points in the caller's timeline (an item's, its source's):
//! a step a little before each stroke whose gain differs from the one
//! before it, ramped over a millisecond so it does not click.

use crate::envelope::EnvPoint;

/// One hit of a drum, as the leveler reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    /// Where, seconds.
    pub t_s: f64,
    /// How loud its attack is, dB (a peak, or any consistent measure).
    pub level_db: f64,
    /// A stroke, leveled — or a ghost, bleed or grace, left under the
    /// stroke before it.
    pub stroke: bool,
}

/// How hard a drum is leveled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrumRide {
    /// How much of the way to the target each stroke is brought, 0..1: 1
    /// makes every stroke as loud as the target, 0.5 halves the spread.
    pub amount: f64,
    /// The most a stroke is raised, dB.
    pub max_gain_db: f64,
    /// The most a stroke is lowered, dB.
    pub max_cut_db: f64,
    /// How long before its stroke a gain is in place, seconds: before the
    /// attack, after the ring before it.
    pub lead_s: f64,
    /// How long a change takes, seconds.
    pub ramp_s: f64,
    /// Changes smaller than this are not made, dB.
    pub least_db: f64,
}

impl DrumRide {
    /// A kick: the most even of the kit — a steady floor under the song,
    /// its strokes brought most of the way together.
    pub const KICK: Self = Self {
        amount: 0.75,
        max_gain_db: 6.0,
        max_cut_db: 6.0,
        lead_s: 0.004,
        ramp_s: 0.001,
        least_db: 0.1,
    };

    /// A snare: even backbeats, with the accents still above them.
    pub const SNARE: Self = Self {
        amount: 0.6,
        max_gain_db: 6.0,
        max_cut_db: 6.0,
        lead_s: 0.004,
        ramp_s: 0.001,
        least_db: 0.1,
    };

    /// A tom: fills keep their shape — half the spread taken out.
    pub const TOM: Self = Self {
        amount: 0.5,
        max_gain_db: 4.0,
        max_cut_db: 6.0,
        lead_s: 0.004,
        ramp_s: 0.001,
        least_db: 0.1,
    };
}

/// The level the strokes are brought toward: their median, dB — what the
/// drum mostly sounds like. `None` with no strokes.
#[must_use]
pub fn target(hits: &[Hit]) -> Option<f64> {
    let mut levels: Vec<f64> = hits.iter().filter(|h| h.stroke).map(|h| h.level_db).collect();
    levels.sort_by(f64::total_cmp);
    levels.get(levels.len() / 2).copied()
}

/// The gain each stroke takes toward `target`, dB.
#[must_use]
pub fn gain(level_db: f64, target_db: f64, ride: &DrumRide) -> f64 {
    ((target_db - level_db) * ride.amount.clamp(0.0, 1.0))
        .clamp(-ride.max_cut_db.abs(), ride.max_gain_db.abs())
}

/// How many strokes around one its level is held up to: about two bars
/// of a busy kick. A drum is evened against the playing around it, so a
/// quiet verse stays quieter than the chorus — only stroke to stroke is
/// it made even.
pub const AROUND: usize = 16;

/// The level the strokes around `t` sit at: the median of the [`AROUND`]
/// strokes of `hits` (by time) nearest it. `None` with no strokes.
#[must_use]
pub fn target_at(hits: &[Hit], t: f64) -> Option<f64> {
    let strokes: Vec<&Hit> = hits.iter().filter(|h| h.stroke).collect();
    let at = strokes.partition_point(|h| h.t_s < t);
    let (mut lo, mut hi) = (at, at);
    while hi.saturating_sub(lo) < AROUND {
        let left = lo.checked_sub(1).and_then(|i| strokes.get(i)).map(|h| t - h.t_s);
        let right = strokes.get(hi).map(|h| h.t_s - t);
        match (left, right) {
            (Some(l), Some(r)) if l <= r => lo = lo.saturating_sub(1),
            (Some(_), None) => lo = lo.saturating_sub(1),
            (_, Some(_)) => hi = hi.saturating_add(1),
            (None, None) => break,
        }
    }
    let mut levels: Vec<f64> = strokes
        .get(lo..hi)
        .unwrap_or_default()
        .iter()
        .map(|h| h.level_db)
        .collect();
    levels.sort_by(f64::total_cmp);
    levels.get(levels.len() / 2).copied()
}

/// The leveling curve for `hits` (by time), each stroke toward the level
/// `target` says for its time (see [`target_at`]): unity before the first
/// stroke, then each stroke's gain from a lead before it until the next
/// stroke's.
#[must_use]
pub fn ride(hits: &[Hit], target: impl Fn(f64) -> f64, ride: &DrumRide) -> Vec<EnvPoint> {
    let mut points: Vec<EnvPoint> = Vec::new();
    let mut held = 0.0_f64;
    for hit in hits.iter().filter(|h| h.stroke) {
        let g = gain(hit.level_db, target(hit.t_s), ride);
        if (g - held).abs() < ride.least_db {
            continue;
        }
        let at = (hit.t_s - ride.lead_s).max(0.0);
        let from = (at - ride.ramp_s).max(points.last().map_or(0.0, |p| p.t_s));
        if points.is_empty() || from > points.last().map_or(0.0, |p| p.t_s) {
            points.push(EnvPoint::new(from, held));
        }
        points.push(EnvPoint::new(at.max(from), g));
        held = g;
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(t_s: f64, level_db: f64) -> Hit {
        Hit {
            t_s,
            level_db,
            stroke: true,
        }
    }

    /// Strokes come closer together; a soft one is raised, a hard one
    /// lowered, each by a share of how far it is from the rest.
    #[test]
    fn strokes_are_brought_together() {
        let hits = [hit(1.0, -20.0), hit(2.0, -12.0), hit(3.0, -16.0)];
        let to = target(&hits).expect("strokes");
        assert!((to + 16.0).abs() < 1e-9, "the median");
        let curve = ride(&hits, |_| to, &DrumRide::KICK);
        let at = |t: f64| {
            curve
                .iter()
                .rev()
                .find(|p| p.t_s <= t + 1e-9)
                .map_or(0.0, |p| p.db)
        };
        assert!((at(1.0) - 3.0).abs() < 1e-9, "the soft one up: {curve:?}");
        assert!((at(2.0) + 3.0).abs() < 1e-9, "the hard one down");
        assert!(at(3.0).abs() < 1e-9, "the one at the target, as it is");
        // The gain is in place before each attack, and only just.
        let step = curve.iter().find(|p| (p.db - 3.0).abs() < 1e-9).expect("a step");
        assert!((step.t_s - 0.996).abs() < 1e-9);
    }

    /// A ghost is not leveled up to a stroke: it rides under the gain of
    /// the stroke before it.
    #[test]
    fn a_ghost_stays_a_ghost() {
        let hits = [
            hit(1.0, -16.0),
            Hit {
                t_s: 1.2,
                level_db: -40.0,
                stroke: false,
            },
            hit(2.0, -16.0),
        ];
        let curve = ride(&hits, |_| -16.0, &DrumRide::SNARE);
        assert!(curve.is_empty(), "nothing to change: {curve:?}");
    }

    /// A quiet passage is evened against itself, not raised to a loud
    /// one: the target is the playing around each stroke.
    #[test]
    fn a_quiet_passage_stays_quiet() {
        let mut hits: Vec<Hit> = (0..32).map(|i| hit(f64::from(i), -30.0)).collect();
        hits.extend((32..64).map(|i| hit(f64::from(i), -12.0)));
        assert!((target_at(&hits, 5.0).expect("strokes") + 30.0).abs() < 1e-9);
        assert!((target_at(&hits, 60.0).expect("strokes") + 12.0).abs() < 1e-9);
        let curve = ride(&hits, |t| target_at(&hits, t).unwrap_or(-12.0), &DrumRide::KICK);
        assert!(curve.is_empty(), "each passage even already: {curve:?}");
    }

    /// No stroke is moved further than the ride allows.
    #[test]
    fn the_ride_is_bounded() {
        let ride = DrumRide::TOM;
        assert!((gain(-40.0, -10.0, &ride) - ride.max_gain_db).abs() < 1e-9);
        assert!((gain(0.0, -30.0, &ride) + ride.max_cut_db).abs() < 1e-9);
    }
}
