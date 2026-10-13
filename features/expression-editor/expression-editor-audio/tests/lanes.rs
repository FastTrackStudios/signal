//! Four lanes, one summed volume envelope.
//!
//! The composition the design rests on: each concern is edited on its
//! own, and what the host applies is their sum — so turning one off, or
//! changing one, is heard without anything else moving.

use expression_editor_audio::dynamics::GainPoint;
use expression_editor_audio::lanes::{db_to_take_volume, take_volume_to_db, thin};
use expression_editor_audio::{DynamicsLane, Lanes};

fn flat(frames: usize, db: f64) -> Vec<GainPoint> {
    (0..frames).map(|frame| GainPoint { frame, db }).collect()
}

fn lanes(frames: usize) -> Lanes {
    let mut l = Lanes::default();
    // `from_dynamics` is the usual route; this reaches the same state
    // with known values so the arithmetic is checkable.
    l.set(DynamicsLane::Gate, flat(frames, -3.0));
    l.set(DynamicsLane::Sibilance, flat(frames, -5.0));
    l
}

#[test]
fn the_sum_is_every_active_lane_added_in_db() {
    let mut l = Lanes::from_dynamics(&Default::default(), 10);
    l.set(DynamicsLane::Gate, flat(10, -3.0));
    l.set(DynamicsLane::Sibilance, flat(10, -5.0));

    let sum = l.sum().expect("something is on");
    assert_eq!(sum.len(), 10);
    assert!(
        (sum[5].db + 8.0).abs() < 1e-9,
        "-3 and -5 make -8, got {}",
        sum[5].db
    );
}

#[test]
fn switching_one_lane_off_leaves_the_others_exactly_as_they_were() {
    let mut l = Lanes::from_dynamics(&Default::default(), 10);
    l.set(DynamicsLane::Gate, flat(10, -3.0));
    l.set(DynamicsLane::Sibilance, flat(10, -5.0));

    l.clear(DynamicsLane::Sibilance);
    assert!(!l.is_active(DynamicsLane::Sibilance));
    assert!(l.is_active(DynamicsLane::Gate));
    assert!((l.sum().unwrap()[5].db + 3.0).abs() < 1e-9, "only the gate");

    // And it can come back without the gate having moved.
    l.set(DynamicsLane::Sibilance, flat(10, -5.0));
    assert!((l.sum().unwrap()[5].db + 8.0).abs() < 1e-9);
}

#[test]
fn with_nothing_on_there_is_no_envelope_rather_than_a_flat_one() {
    // A flat envelope at unity and no envelope sound identical. They
    // differ in whose job it is to delete the dead automation.
    let mut l = Lanes::from_dynamics(&Default::default(), 10);
    assert!(l.is_empty());
    assert!(l.sum().is_none());

    l.set(DynamicsLane::Gate, flat(10, 0.0));
    assert!(
        l.sum().is_some(),
        "a dimension at 0 dB is still switched on"
    );
}

#[test]
fn each_lane_is_readable_on_its_own() {
    let l = lanes(8);
    assert_eq!(l.get(DynamicsLane::Gate)[0].db, -3.0);
    assert_eq!(l.get(DynamicsLane::Sibilance)[0].db, -5.0);
    assert!(l.get(DynamicsLane::Breath).is_empty());
    assert!(l.get(DynamicsLane::Compressor).is_empty());
}

// ── the take volume envelope is linear, not dB ───────────────────────

#[test]
fn db_converts_to_the_linear_multiplier_a_volume_envelope_wants() {
    assert!((db_to_take_volume(0.0) - 1.0).abs() < 1e-12, "unity");
    assert!(
        (db_to_take_volume(-6.0) - 0.5011).abs() < 1e-3,
        "about half"
    );
    assert!(
        (db_to_take_volume(6.0) - 1.9953).abs() < 1e-3,
        "about double"
    );
    // Round trip.
    for db in [-24.0, -6.0, -0.5, 0.0, 3.0] {
        assert!((take_volume_to_db(db_to_take_volume(db)) - db).abs() < 1e-9);
    }
}

// ── thinning ─────────────────────────────────────────────────────────

#[test]
fn a_straight_line_thins_to_its_two_ends() {
    // The reason thinning exists: one point per analysis frame is
    // hundreds per second, and a user who wants to nudge the gate would
    // have to select a thousand points to do it.
    let ramp: Vec<GainPoint> = (0..200)
        .map(|frame| GainPoint {
            frame,
            db: -12.0 * frame as f64 / 199.0,
        })
        .collect();
    let thinned = thin(&ramp, 0.25);
    assert_eq!(thinned.len(), 2, "a ramp is two points");
    assert_eq!(thinned[0].frame, 0);
    assert_eq!(thinned[1].frame, 199);
}

#[test]
fn thinning_keeps_the_corners_that_carry_the_shape() {
    // Flat, then a dip, then flat. The corners are the content.
    let mut points = flat(60, 0.0);
    for p in points.iter_mut().take(40).skip(20) {
        p.db = -9.0;
    }
    let thinned = thin(&points, 0.25);

    assert!(thinned.len() >= 4, "got {}", thinned.len());
    assert!(thinned.len() < 12, "but not all sixty: {}", thinned.len());
    // The dip survives.
    let deepest = thinned.iter().map(|p| p.db).fold(0.0_f64, f64::min);
    assert!((deepest + 9.0).abs() < 1e-9);
    // Both ends survive, or the envelope would not start and stop where
    // the take does.
    assert_eq!(thinned.first().unwrap().frame, 0);
    assert_eq!(thinned.last().unwrap().frame, 59);
}

#[test]
fn thinning_never_deviates_further_than_it_was_told_to() {
    let wobble: Vec<GainPoint> = (0..300)
        .map(|frame| GainPoint {
            frame,
            db: -6.0 + 3.0 * (frame as f64 / 11.0).sin(),
        })
        .collect();
    let thinned = thin(&wobble, 0.25);
    assert!(thinned.len() < wobble.len());

    // Reconstruct by joining the kept points and compare.
    for original in &wobble {
        let after = thinned
            .iter()
            .position(|p| p.frame >= original.frame)
            .unwrap_or(thinned.len() - 1);
        let hi = thinned[after];
        let lo = if after == 0 { hi } else { thinned[after - 1] };
        let span = (hi.frame - lo.frame) as f64;
        let approx = if span <= 0.0 {
            hi.db
        } else {
            let t = (original.frame - lo.frame) as f64 / span;
            lo.db + (hi.db - lo.db) * t
        };
        assert!(
            (approx - original.db).abs() < 0.26,
            "frame {} off by {}",
            original.frame,
            (approx - original.db).abs()
        );
    }
}

#[test]
fn a_short_curve_is_left_alone() {
    let two = flat(2, -3.0);
    assert_eq!(thin(&two, 0.25).len(), 2);
    assert!(thin(&[], 0.25).is_empty());
}
