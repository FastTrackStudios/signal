//! Regression suite for the portable engine — the headless half.
//!
//! Everything here is reachable without a GPU, a DAW, or a browser,
//! which is the point of keeping the engine dependency-free.

use expression_editor_core::blob;
use expression_editor_core::camera::{self, Bounds, Camera, Content, VerticalCamera, Viewport};
use expression_editor_core::doc::{
    Curve, Dimension, ExpressionDoc, Note, NoteId, Point, Target, TimeBase,
};
use expression_editor_core::edit::{Edit, History};
use expression_editor_core::modulation::{CurveTarget, Row, Stack, Wave};
use expression_editor_core::shape::{self, Shape};
use expression_editor_core::tools::{self, Grid, Mods, Selection, Tool};
use expression_editor_core::tuning::{self, Tuning};
use expression_editor_core::{Editor, content_of};

const PPQ: f64 = 960.0;

fn doc_with_note() -> ExpressionDoc {
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 8.0);
    let mut n = Note::new(NoteId(1), 0.0, PPQ * 2.0, 60);
    n.channel = Some(2);
    doc.push(n);
    doc
}

// ── curves ───────────────────────────────────────────────────────────

#[test]
fn curve_replaces_rather_than_stacks_at_a_repeated_time() {
    let mut c = Curve::new();
    c.set(100.0, 1.0);
    c.set(100.0, 2.0);
    assert_eq!(c.len(), 1, "a revisited tick must replace, not duplicate");
    assert_eq!(c.points()[0].value, 2.0);
}

#[test]
fn curve_stays_sorted_when_drawn_right_to_left() {
    let mut c = Curve::new();
    for t in [500.0, 100.0, 300.0, 200.0] {
        c.set(t, t / 1000.0);
    }
    let ts: Vec<f64> = c.points().iter().map(|p| p.t).collect();
    assert_eq!(ts, vec![100.0, 200.0, 300.0, 500.0]);
}

#[test]
fn curve_holds_its_endpoints_outside_the_authored_range() {
    let mut c = Curve::new();
    c.set(100.0, 3.0);
    c.set(200.0, 5.0);
    // Not the default — an authored curve must not snap to center at
    // the note edges.
    assert_eq!(c.sample(0.0, 0.0), 3.0);
    assert_eq!(c.sample(999.0, 0.0), 5.0);
    assert_eq!(c.sample(150.0, 0.0), 4.0, "linear between points");
}

#[test]
fn splice_preserves_points_outside_the_interval() {
    let mut c = Curve::from_points(vec![
        Point {
            t: 0.0,
            value: 0.0,
            ..Point::default()
        },
        Point {
            t: 100.0,
            value: 1.0,
            ..Point::default()
        },
        Point {
            t: 200.0,
            value: 2.0,
            ..Point::default()
        },
        Point {
            t: 300.0,
            value: 3.0,
            ..Point::default()
        },
    ]);
    c.splice(
        100.0,
        200.0,
        &[Point {
            t: 150.0,
            value: 9.0,
            ..Point::default()
        }],
    );
    let pts: Vec<(f64, f64)> = c.points().iter().map(|p| (p.t, p.value)).collect();
    assert_eq!(pts, vec![(0.0, 0.0), (150.0, 9.0), (300.0, 3.0)]);
}

#[test]
fn reshape_preserves_endpoints_exactly() {
    let mut c = Curve::new();
    c.set(0.0, 0.0);
    c.set(400.0, 12.0);
    for shape in Shape::ALL {
        let mut c2 = c.clone();
        c2.reshape(0.0, 400.0, shape, 32, 0.0);
        assert!((c2.sample(0.0, 0.0) - 0.0).abs() < 1e-9, "{shape:?} start");
        assert!((c2.sample(400.0, 0.0) - 12.0).abs() < 1e-6, "{shape:?} end");
    }
}

#[test]
fn scale_about_below_zero_inverts_the_gesture() {
    let mut c = Curve::from_points(vec![
        Point {
            t: 0.0,
            value: 0.0,
            ..Point::default()
        },
        Point {
            t: 100.0,
            value: 2.0,
            ..Point::default()
        },
    ]);
    c.scale_about(0.0, 100.0, 1.0, -1.0);
    assert_eq!(c.sample(0.0, 0.0), 2.0);
    assert_eq!(c.sample(100.0, 0.0), 0.0);
}

#[test]
fn remap_time_stretches_owned_expression_onto_new_bounds() {
    let mut c = Curve::from_points(vec![
        Point {
            t: 0.0,
            value: 0.0,
            ..Point::default()
        },
        Point {
            t: 100.0,
            value: 1.0,
            ..Point::default()
        },
    ]);
    c.remap_time(0.0, 100.0, 0.0, 400.0);
    assert_eq!(c.points().last().unwrap().t, 400.0);
}

// ── shapes ───────────────────────────────────────────────────────────

#[test]
fn every_shape_is_a_unit_map() {
    for shape in Shape::ALL {
        assert!(shape.amount(0.0).abs() < 1e-9, "{shape:?} at 0");
        assert!((shape.amount(1.0) - 1.0).abs() < 1e-9, "{shape:?} at 1");
        // Monotone: no shape may double back.
        let mut prev = -1.0;
        for i in 0..=50 {
            let v = shape.amount(i as f64 / 50.0);
            assert!(v >= prev - 1e-9, "{shape:?} not monotone at {i}");
            prev = v;
        }
    }
}

#[test]
fn simplify_trims_collinear_runs_but_keeps_the_gesture() {
    let straight: Vec<(f64, f64)> = (0..50).map(|i| (i as f64, i as f64 * 2.0)).collect();
    assert_eq!(shape::simplify(&straight, 0.01).len(), 2);

    let peaked: Vec<(f64, f64)> = (0..=20)
        .map(|i| (i as f64, if i == 10 { 50.0 } else { 0.0 }))
        .collect();
    let kept = shape::simplify(&peaked, 0.5);
    assert!(
        kept.iter().any(|&(_, y)| y == 50.0),
        "the peak must survive simplification"
    );
}

// ── zones ────────────────────────────────────────────────────────────

#[test]
fn a_note_without_splits_still_has_exactly_one_zone() {
    let n = Note::new(NoteId(1), 0.0, 100.0, 60);
    assert_eq!(n.zone_count(), 1);
    assert_eq!(n.zones(), vec![(0.0, 100.0)]);
}

#[test]
fn splits_stay_sorted_and_interior() {
    let mut n = Note::new(NoteId(1), 0.0, 100.0, 60);
    assert!(n.add_split(60.0));
    assert!(n.add_split(30.0));
    assert!(
        !n.add_split(0.0),
        "a split at the note start is not interior"
    );
    assert!(!n.add_split(100.0), "nor at the end");
    assert!(!n.add_split(30.0), "nor a duplicate");
    assert_eq!(n.splits, vec![30.0, 60.0]);
    assert_eq!(n.zones(), vec![(0.0, 30.0), (30.0, 60.0), (60.0, 100.0)]);
}

#[test]
fn inserting_a_split_before_the_active_zone_keeps_the_same_zone_active() {
    let mut n = Note::new(NoteId(1), 0.0, 100.0, 60);
    n.add_split(50.0);
    n.target = Target::Zone(1); // the 50..100 half
    n.add_split(25.0);
    assert_eq!(n.target, Target::Zone(2));
    assert_eq!(n.target_span(), (50.0, 100.0), "still the same region");
}

#[test]
fn target_toggles_between_one_zone_and_the_whole_note() {
    assert_eq!(tools::toggle_target(Target::WholeNote, 1), Target::Zone(1));
    assert_eq!(tools::toggle_target(Target::Zone(1), 1), Target::WholeNote);
    assert_eq!(tools::toggle_target(Target::Zone(1), 2), Target::Zone(2));
}

// ── edits ────────────────────────────────────────────────────────────

#[test]
fn drawing_extends_to_the_note_edges_so_no_gap_is_left() {
    let mut doc = doc_with_note();
    let edit = Edit::DrawDimension {
        note: NoteId(1),
        dimension: Dimension::Pitch,
        t0: 500.0,
        t1: 1000.0,
        points: vec![
            Point {
                t: 500.0,
                value: 1.0,
                ..Point::default()
            },
            Point {
                t: 1000.0,
                value: 2.0,
                ..Point::default()
            },
        ],
    };
    assert!(edit.apply(&mut doc));
    let n = doc.note(NoteId(1)).unwrap();
    assert_eq!(n.pitch.sample(n.start, 0.0), 1.0, "held back to note start");
    assert_eq!(n.pitch.sample(n.end, 0.0), 2.0, "held out to note end");
}

#[test]
fn erasing_one_lane_leaves_the_others_untouched() {
    let mut doc = doc_with_note();
    for dimension in Dimension::ALL {
        Edit::DrawDimension {
            note: NoteId(1),
            dimension,
            t0: 0.0,
            t1: 1000.0,
            points: vec![
                Point {
                    t: 0.0,
                    value: 0.5,
                    ..Point::default()
                },
                Point {
                    t: 1000.0,
                    value: 0.5,
                    ..Point::default()
                },
            ],
        }
        .apply(&mut doc);
    }
    let before = doc.note(NoteId(1)).unwrap().pitch.clone();
    Edit::EraseDimension {
        note: NoteId(1),
        dimension: Dimension::Pressure,
        // The whole note: drawing held the curve out to the note edges,
        // so erasing only the drawn interval would leave those.
        t0: 0.0,
        t1: PPQ * 2.0,
    }
    .apply(&mut doc);
    let n = doc.note(NoteId(1)).unwrap();
    assert_eq!(
        n.pitch, before,
        "pitch must survive a pressure edit byte-for-byte"
    );
    assert!(n.pressure.is_empty());
}

#[test]
fn resize_stretches_expression_and_zones_together() {
    let mut doc = doc_with_note();
    {
        let n = doc.note_mut(NoteId(1)).unwrap();
        n.add_split(PPQ);
        n.pitch.set(0.0, 0.0);
        n.pitch.set(PPQ * 2.0, 4.0);
    }
    assert!(
        Edit::Resize {
            note: NoteId(1),
            start: 0.0,
            end: PPQ * 4.0,
        }
        .apply(&mut doc)
    );
    let n = doc.note(NoteId(1)).unwrap();
    assert_eq!(n.splits, vec![PPQ * 2.0], "the split scales with the note");
    assert_eq!(n.pitch.points().last().unwrap().t, PPQ * 4.0);
}

#[test]
fn splitting_a_note_gives_both_halves_the_boundary_value() {
    let mut doc = doc_with_note();
    {
        let n = doc.note_mut(NoteId(1)).unwrap();
        n.pitch.set(0.0, 0.0);
        n.pitch.set(PPQ * 2.0, 4.0);
    }
    assert!(
        Edit::SplitNote {
            note: NoteId(1),
            t: PPQ,
        }
        .apply(&mut doc)
    );
    assert_eq!(doc.notes.len(), 2);
    let left = doc.note(NoteId(1)).unwrap();
    assert_eq!(left.end, PPQ);
    let right = doc.notes.iter().find(|n| n.id != NoteId(1)).unwrap();
    assert_eq!(right.start, PPQ);
    // Both sides hold the value at the split — no jump across the cut.
    assert!((left.pitch.sample(PPQ, 0.0) - right.pitch.sample(PPQ, 0.0)).abs() < 1e-9);
}

#[test]
fn transpose_moves_the_row_and_leaves_the_curve_shape_alone() {
    let mut doc = doc_with_note();
    doc.note_mut(NoteId(1)).unwrap().pitch.set(0.0, 0.25);
    Edit::Transpose {
        notes: vec![NoteId(1)],
        semitones: 3,
    }
    .apply(&mut doc);
    let n = doc.note(NoteId(1)).unwrap();
    assert_eq!(n.row, 63);
    assert_eq!(n.pitch.sample(0.0, 0.0), 0.25, "the gesture moves rigidly");
    assert!((n.sounding_midi(0.0) - 63.25).abs() < 1e-9);
}

#[test]
fn move_time_carries_owned_expression() {
    let mut doc = doc_with_note();
    doc.note_mut(NoteId(1)).unwrap().pitch.set(100.0, 1.0);
    Edit::MoveTime {
        notes: vec![NoteId(1)],
        delta: PPQ,
    }
    .apply(&mut doc);
    let n = doc.note(NoteId(1)).unwrap();
    assert_eq!(n.start, PPQ);
    assert_eq!(n.pitch.points()[0].t, 100.0 + PPQ);
}

// ── MPE safety ───────────────────────────────────────────────────────

#[test]
fn overlapping_notes_sharing_a_channel_are_flagged_ambiguous() {
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 8.0);
    let mut a = Note::new(NoteId(1), 0.0, PPQ * 2.0, 60);
    a.channel = Some(2);
    let mut b = Note::new(NoteId(2), PPQ, PPQ * 3.0, 64);
    b.channel = Some(2);
    doc.push(a);
    doc.push(b);
    doc.mark_ambiguity();
    assert!(doc.notes.iter().all(|n| n.ambiguous));
}

#[test]
fn notes_on_different_channels_are_never_ambiguous() {
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 8.0);
    let mut a = Note::new(NoteId(1), 0.0, PPQ * 2.0, 60);
    a.channel = Some(2);
    let mut b = Note::new(NoteId(2), PPQ, PPQ * 3.0, 64);
    b.channel = Some(3);
    doc.push(a);
    doc.push(b);
    doc.mark_ambiguity();
    assert!(doc.notes.iter().all(|n| !n.ambiguous));
}

#[test]
fn channel_assignment_separates_overlapping_and_consecutive_notes() {
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 16.0);
    // Three overlapping, then one starting exactly where another ends.
    doc.push(Note::new(NoteId(1), 0.0, PPQ * 4.0, 60));
    doc.push(Note::new(NoteId(2), PPQ, PPQ * 5.0, 64));
    doc.push(Note::new(NoteId(3), PPQ * 2.0, PPQ * 6.0, 67));
    doc.push(Note::new(NoteId(4), PPQ * 4.0, PPQ * 8.0, 72));
    let ids: Vec<NoteId> = doc.notes.iter().map(|n| n.id).collect();
    assert!(
        Edit::AssignChannels {
            notes: ids,
            seed: 42
        }
        .apply(&mut doc)
    );

    for n in &doc.notes {
        let ch = n.channel.expect("every note gets a member channel");
        assert!((2..=16).contains(&ch), "channel 1 stays the MPE master");
    }
    // Touching counts as conflicting: note 4 starts where note 1 ends.
    let ch1 = doc.note(NoteId(1)).unwrap().channel;
    let ch4 = doc.note(NoteId(4)).unwrap().channel;
    assert_ne!(ch1, ch4, "a touching handoff must not reuse the channel");
    assert!(doc.notes.iter().all(|n| !n.ambiguous));
}

#[test]
fn channel_assignment_is_deterministic_per_seed() {
    let build = |seed: u64| {
        let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 16.0);
        for i in 0..6 {
            doc.push(Note::new(
                NoteId(i + 1),
                PPQ * i as f64 * 0.5,
                PPQ * (i as f64 * 0.5 + 2.0),
                60 + i as i32,
            ));
        }
        let ids: Vec<NoteId> = doc.notes.iter().map(|n| n.id).collect();
        Edit::AssignChannels { notes: ids, seed }.apply(&mut doc);
        doc.notes.iter().map(|n| n.channel).collect::<Vec<_>>()
    };
    assert_eq!(build(7), build(7));
}

// ── tuning ───────────────────────────────────────────────────────────

#[test]
fn equal_temperament_has_no_offsets() {
    assert!(tuning::EQUAL.is_equal());
    for t in tuning::PRESETS.iter().skip(1) {
        assert!(!t.is_equal(), "{} should not be equal", t.name);
    }
}

#[test]
fn bend_round_trips_through_the_14_bit_word() {
    for st in [-24.0, -1.0, 0.0, 0.5, 12.0, 47.0] {
        let raw = tuning::semitones_to_bend14(st, 48.0);
        let back = tuning::bend14_to_semitones(raw, 48.0);
        assert!((back - st).abs() < 0.01, "{st} round-tripped to {back}");
    }
    assert_eq!(tuning::semitones_to_bend14(0.0, 48.0), 8192, "center");
}

#[test]
fn snapping_offers_microtonal_and_ordinary_centers_together() {
    let t = Tuning {
        temperament: tuning::RAST,
        key_pc: 0,
        snap_12tet: true,
    };
    // E in the key of C is Rast's half-flat third: 50 cents low.
    assert!((t.center(64) - 63.5).abs() < 1e-6);
    let targets = t.targets_near(63.6, 1.0);
    assert!(targets.iter().any(|x| (x.pitch - 63.5).abs() < 1e-6));
    assert!(
        targets.iter().any(|x| (x.pitch - 64.0).abs() < 1e-6),
        "with snap_12tet on, the plain semitone is offered too"
    );
    assert!((t.snap(63.6).pitch - 63.5).abs() < 1e-6);
}

#[test]
fn snapping_without_12tet_lands_only_on_the_temperament() {
    let t = Tuning {
        temperament: tuning::RAST,
        key_pc: 0,
        snap_12tet: false,
    };
    let targets = t.targets_near(63.9, 1.0);
    assert!(
        !targets.iter().any(|x| (x.pitch - 64.0).abs() < 1e-6),
        "the plain semitone must not be offered"
    );
}

#[test]
fn note_names_follow_the_midi_60_is_c4_convention() {
    assert_eq!(tuning::note_name(60), "C4");
    assert_eq!(tuning::note_name(69), "A4");
    assert_eq!(tuning::note_name(0), "C-1");
}

// ── decomposition ────────────────────────────────────────────────────

#[test]
fn decomposition_round_trips_the_curve_it_came_from() {
    // A slide plus vibrato, sampled as if analyzed.
    let mut c = Curve::new();
    for i in 0..200 {
        let t = i as f64 * 10.0;
        let secs = t / 960.0;
        let slide = -0.4 * secs;
        let vib = 0.3 * (core::f64::consts::TAU * 5.5 * secs).sin();
        c.set(t, slide + vib);
    }
    let d = blob::decompose(&c, 0.0, 1990.0, 200, 960.0, 0.0);
    let back = d.recompose(d.center, 1.0, 1.0);
    for i in 0..200 {
        let t = i as f64 * 10.0;
        let a = c.sample(t, 0.0);
        let b = back.sample(t, 0.0);
        assert!((a - b).abs() < 1e-6, "at t={t}: {a} vs {b}");
    }
}

#[test]
fn flattening_both_amounts_gives_a_dead_flat_line() {
    let mut c = Curve::new();
    for i in 0..100 {
        let t = i as f64 * 10.0;
        c.set(t, (i as f64 * 0.3).sin() * 0.5);
    }
    let d = blob::decompose(&c, 0.0, 990.0, 100, 960.0, 0.0);
    let robot = d.recompose(d.center, 0.0, 0.0);
    let values: Vec<f64> = robot.points().iter().map(|p| p.value).collect();
    assert!(values.iter().all(|v| (v - d.center).abs() < 1e-12));
}

#[test]
fn drift_and_vibrato_separate_at_the_three_hertz_line() {
    // 0.5 Hz slide + 6 Hz vibrato over two seconds at 960 units/sec.
    let mut c = Curve::new();
    let n = 400;
    for i in 0..n {
        let secs = i as f64 / 200.0;
        let t = secs * 960.0;
        let drift = 0.8 * (core::f64::consts::TAU * 0.5 * secs).sin();
        let vib = 0.25 * (core::f64::consts::TAU * 6.0 * secs).sin();
        c.set(t, drift + vib);
    }
    let d = blob::decompose(&c, 0.0, 1920.0, n, 960.0, 0.0);
    // The vibrato lands in `modulation`, the slide in `drift`.
    assert!(
        d.modulation_depth() > 0.3,
        "vibrato depth {} should be near 0.5 peak-to-peak",
        d.modulation_depth()
    );
    let drift_pp = d.drift.iter().cloned().fold(f64::MIN, f64::max)
        - d.drift.iter().cloned().fold(f64::MAX, f64::min);
    assert!(
        drift_pp > 1.0,
        "the 0.5 Hz slide belongs to drift, got {drift_pp}"
    );
}

#[test]
fn effective_center_finds_where_the_curve_dwells_not_its_mean() {
    // A scoop: starts a fourth low, settles on target for most of the
    // note. The mean would sit well below the target.
    let mut c = Curve::new();
    for i in 0..100 {
        let t = i as f64 * 10.0;
        c.set(t, if i < 15 { -5.0 + i as f64 * 0.33 } else { 0.0 });
    }
    let center = blob::effective_center(&c, 0.0, 990.0, 100, 0.0);
    assert!(
        center.abs() < 0.5,
        "should settle on the target, got {center}"
    );
}

#[test]
fn reblending_pitch_flattens_vibrato_without_moving_the_center() {
    let mut doc = doc_with_note();
    {
        let n = doc.note_mut(NoteId(1)).unwrap();
        for i in 0..100 {
            let t = i as f64 * (PPQ * 2.0 / 100.0);
            n.pitch.set(t, 0.4 * (i as f64 * 0.4).sin());
        }
    }
    assert!(
        Edit::ReblendPitch {
            note: NoteId(1),
            t0: 0.0,
            t1: PPQ * 2.0,
            drift_amount: 0.0,
            modulation_amount: 0.0,
        }
        .apply(&mut doc)
    );
    let n = doc.note(NoteId(1)).unwrap();
    let (lo, hi) = n.pitch.value_bounds().unwrap();
    assert!(
        hi - lo < 1e-6,
        "robot mode should be flat, spread {}",
        hi - lo
    );
}

// ── history ──────────────────────────────────────────────────────────

#[test]
fn undo_restores_zones_notes_and_expression_as_one_step() {
    let mut doc = doc_with_note();
    let mut history = History::new(10);
    let before = doc.clone();

    history.apply(
        &mut doc,
        &Edit::AddZoneSplit {
            note: NoteId(1),
            t: PPQ,
        },
    );
    assert_eq!(doc.note(NoteId(1)).unwrap().splits.len(), 1);

    assert!(history.undo(&mut doc));
    assert_eq!(doc, before);
    assert!(history.redo(&mut doc));
    assert_eq!(doc.note(NoteId(1)).unwrap().splits.len(), 1);
}

#[test]
fn a_failed_edit_records_no_undo_step() {
    let mut doc = doc_with_note();
    let mut history = History::new(10);
    assert!(!history.apply(
        &mut doc,
        &Edit::Transpose {
            notes: vec![NoteId(999)],
            semitones: 1
        }
    ));
    assert!(!history.can_undo());
}

#[test]
fn history_is_bounded() {
    let mut doc = doc_with_note();
    let mut history = History::new(3);
    for i in 0..10 {
        history.apply(
            &mut doc,
            &Edit::AddZoneSplit {
                note: NoteId(1),
                t: 10.0 + i as f64 * 10.0,
            },
        );
    }
    let mut count = 0;
    while history.undo(&mut doc) {
        count += 1;
    }
    assert_eq!(count, 3);
}

// ── camera ───────────────────────────────────────────────────────────

fn test_content() -> Content {
    Content {
        t_start: 0.0,
        t_end: 4000.0,
        pitch_lo: 58.0,
        pitch_hi: 70.0,
    }
}

#[test]
fn zoom_pins_the_anchor_under_the_pointer() {
    let vp = Viewport::new(800.0, 480.0);
    let mut cam = camera::reset_view(test_content(), vp, 0.03, 0.35, Default::default());
    let anchor_t = cam.t_at(200.0);
    cam.zoom_time_about(anchor_t, 2.0);
    assert!(
        (cam.x(anchor_t) - 200.0).abs() < 1e-6,
        "the anchored time must stay at the same pixel"
    );

    let anchor_p = cam.pitch_at(120.0, vp);
    cam.zoom_pitch_about(anchor_p, 2.0, vp);
    assert!((cam.y(anchor_p, vp) - 120.0).abs() < 1e-6);
}

#[test]
fn reset_view_frames_the_content_with_headroom() {
    let vp = Viewport::new(800.0, 480.0);
    let c = test_content();
    let cam = camera::reset_view(c, vp, 0.03, 0.35, Default::default());
    let (lo, hi) = cam.pitch_span(vp);
    assert!(
        lo < c.pitch_lo && hi > c.pitch_hi,
        "content must fit inside"
    );
    let (t0, t1) = cam.time_span(vp);
    assert!(t0 < c.t_start && t1 > c.t_end);
    assert!(
        (cam.vertical.center - 64.0).abs() < 1e-6,
        "centered on the content midpoint"
    );
}

#[test]
fn blending_with_no_influences_is_the_identity() {
    let vp = Viewport::new(800.0, 480.0);
    let cam = camera::reset_view(test_content(), vp, 0.03, 0.35, Default::default());
    assert_eq!(camera::blend(cam, &[]), cam);
}

#[test]
fn a_full_weight_influence_fully_replaces_the_base() {
    let vp = Viewport::new(800.0, 480.0);
    let base = camera::reset_view(test_content(), vp, 0.03, 0.35, Default::default());
    let target = Camera {
        fold: Default::default(),
        t0: 1234.0,
        units_per_px: 2.0,
        vertical: VerticalCamera {
            center: 70.0,
            px_per_row: 20.0,
        },
    };
    let out = camera::blend(
        base,
        &[camera::Influence {
            camera: target,
            weight: 1.0,
        }],
    );
    assert!((out.t0 - target.t0).abs() < 1e-6);
    assert!((out.vertical.px_per_row - target.vertical.px_per_row).abs() < 1e-6);
}

#[test]
fn scales_blend_geometrically_so_zoom_stays_even() {
    let base = Camera {
        fold: Default::default(),
        t0: 0.0,
        units_per_px: 1.0,
        vertical: VerticalCamera {
            center: 60.0,
            px_per_row: 10.0,
        },
    };
    let target = Camera {
        fold: Default::default(),
        units_per_px: 100.0,
        ..base
    };
    let out = camera::blend(
        base,
        &[camera::Influence {
            camera: target,
            weight: 0.5,
        }],
    );
    // Geometric midpoint of 1 and 100 is 10, not the arithmetic 50.5 —
    // an arithmetic blend would bias every magnet toward zoomed-out.
    assert!(
        (out.units_per_px - 10.0).abs() < 1e-6,
        "got {}",
        out.units_per_px
    );
}

#[test]
fn the_edge_magnet_is_inert_in_the_middle_of_the_item() {
    let vp = Viewport::new(800.0, 480.0);
    let c = test_content();
    let cam = camera::reset_view(c, vp, 0.03, 0.35, Default::default());
    assert!(
        camera::edge_magnet(cam, 2000.0, c, vp, 0.35, 0.2).is_none(),
        "no pull at the item center"
    );
    let near_edge = camera::edge_magnet(cam, 3950.0, c, vp, 0.35, 0.2);
    assert!(
        near_edge.is_some_and(|i| i.weight > 0.9),
        "full pull at the edge"
    );
}

#[test]
fn the_reset_tail_stays_out_until_the_final_stretch() {
    let vp = Viewport::new(800.0, 480.0);
    let reset = camera::reset_view(test_content(), vp, 0.03, 0.35, Default::default());
    // Deep zoom-in: nowhere near reset.
    let deep = Camera {
        fold: Default::default(),
        units_per_px: reset.units_per_px / 50.0,
        vertical: VerticalCamera {
            px_per_row: reset.vertical.px_per_row * 50.0,
            ..reset.vertical
        },
        ..reset
    };
    assert!(
        camera::reset_tail(deep, reset, 0.8).is_none(),
        "the reset magnet must not fight an ordinary zoom-out"
    );
    // Essentially there.
    let close = Camera {
        fold: Default::default(),
        units_per_px: reset.units_per_px * 0.98,
        ..reset
    };
    assert!(camera::reset_tail(close, reset, 0.8).is_some());
}

#[test]
fn zooming_out_can_see_past_either_end_of_the_item() {
    // Time is open. An item sits in a timeline, and placing it against
    // what surrounds it means being able to pull back off it — this used
    // to stop dead at the cushioned item, so you could never see the
    // space either side.
    let vp = Viewport::new(800.0, 480.0);
    let bounds = Bounds {
        t_min: 0.0,
        t_max: 4000.0,
        ..Bounds::default()
    };
    let mut cam = Camera {
        fold: Default::default(),
        t0: -99999.0,
        units_per_px: 1000.0,
        vertical: VerticalCamera {
            center: 60.0,
            px_per_row: 8.0,
        },
    };
    cam.constrain(bounds, vp);
    let (t0, t1) = cam.time_span(vp);
    assert!(
        t1 - t0 > 4000.0,
        "should be able to zoom out past the item, got {}",
        t1 - t0
    );

    // Centre what is now a wider-than-the-item view on the item, so the
    // question is whether both ends can be off-screen at once rather
    // than whether this particular camera happened to be against a
    // clamp.
    cam.t0 = (bounds.t_min + bounds.t_max) / 2.0 - (t1 - t0) / 2.0;
    cam.constrain(bounds, vp);
    let (t0, t1) = cam.time_span(vp);
    assert!(t0 < bounds.t_min, "cannot see before the item starts: {t0}");
    assert!(t1 > bounds.t_max, "cannot see after it ends: {t1}");
}

#[test]
fn zooming_out_still_stops_somewhere() {
    // Open is not unbounded: past `max_zoom_out` the item is a smear and
    // there is nothing to edit.
    let vp = Viewport::new(800.0, 480.0);
    let bounds = Bounds {
        t_min: 0.0,
        t_max: 4000.0,
        ..Bounds::default()
    };
    let mut cam = Camera {
        fold: Default::default(),
        t0: 0.0,
        units_per_px: 1e9,
        vertical: VerticalCamera {
            center: 60.0,
            px_per_row: 8.0,
        },
    };
    cam.constrain(bounds, vp);
    let (t0, t1) = cam.time_span(vp);
    assert!(
        t1 - t0 <= 4000.0 * bounds.max_zoom_out + 1e-6,
        "unbounded zoom-out: {}",
        t1 - t0
    );
}

#[test]
fn the_roll_never_shows_empty_space_above_or_below_the_keys() {
    // Pitch is closed, unlike time: the rows *are* the instrument, and
    // there is nothing past the top or bottom key to look at.
    let vp = Viewport::new(800.0, 480.0);
    let bounds = Bounds {
        t_min: 0.0,
        t_max: 4000.0,
        ..Bounds::default()
    };
    let mut cam = Camera {
        fold: Default::default(),
        t0: 0.0,
        units_per_px: 1.0,
        // Far too small to fill the lane, and centred off the top.
        vertical: VerticalCamera {
            center: 200.0,
            px_per_row: 0.001,
        },
    };
    cam.constrain(bounds, vp);

    let rows_visible = vp.h / cam.vertical.px_per_row;
    let total = bounds.row_max - bounds.row_min + 1.0;
    assert!(
        rows_visible <= total + 1e-6,
        "showing {rows_visible} rows of {total}"
    );
    // And the visible window sits over real rows at both edges.
    let half = rows_visible / 2.0;
    assert!(
        cam.vertical.center - half >= bounds.row_min - 1e-6,
        "empty space below: centre {} half {half}",
        cam.vertical.center
    );
    assert!(
        cam.vertical.center + half <= bounds.row_max + 1.0 + 1e-6,
        "empty space above: centre {} half {half}",
        cam.vertical.center
    );
}

#[test]
fn a_short_row_space_is_fitted_to_its_own_lanes() {
    // A drum map is twenty lanes, not 128. Fitting the roll to the pitch
    // range would leave a screen of empty rows under the kit.
    let vp = Viewport::new(800.0, 480.0);
    let bounds = Bounds {
        t_min: 0.0,
        t_max: 4000.0,
        row_min: 0.0,
        row_max: 19.0,
        ..Bounds::default()
    };
    let mut cam = Camera {
        fold: Default::default(),
        t0: 0.0,
        units_per_px: 1.0,
        vertical: VerticalCamera {
            center: 10.0,
            px_per_row: 0.5,
        },
    };
    cam.constrain(bounds, vp);
    assert!(
        vp.h / cam.vertical.px_per_row <= 20.0 + 1e-6,
        "showing more than the twenty lanes there are"
    );
}

// ── editor integration ───────────────────────────────────────────────

fn test_editor() -> Editor {
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 8.0);
    for i in 0..4 {
        let mut n = Note::new(
            NoteId(i + 1),
            PPQ * i as f64,
            PPQ * (i as f64 + 0.9),
            60 + i as i32 * 2,
        );
        n.channel = Some(2 + i as u8);
        doc.push(n);
    }
    Editor::new(doc, Viewport::new(900.0, 500.0))
}

#[test]
fn a_new_editor_frames_its_content() {
    let ed = test_editor();
    let c = content_of(&ed.doc);
    let (lo, hi) = ed.camera.pitch_span(ed.viewport);
    assert!(lo <= c.pitch_lo && hi >= c.pitch_hi);
    // Select, and it matters more than it used to: a tool now claims the
    // plain drag, so the default tool decides what dragging does in an
    // editor nobody has configured. Select is the one that claims
    // nothing, so an unconfigured editor behaves exactly as its mouse
    // map says.
    assert_eq!(ed.tool, Tool::Select, "Select is the default tool");
    assert!(
        ed.tool.claims().is_empty(),
        "the default tool must not reassign gestures"
    );
    assert_eq!(ed.dimension, Dimension::Pitch);
}

#[test]
fn v_returns_to_the_same_camera_from_anywhere() {
    let mut ed = test_editor();
    let reset = ed.camera;
    ed.zoom_in_at(300.0, 200.0, 4.0);
    assert_ne!(ed.camera, reset);
    ed.reset_view();
    assert_eq!(ed.camera, reset, "V is exact, not approximate");
}

#[test]
fn zooming_in_never_drifts_toward_reset_view() {
    let mut ed = test_editor();
    let start = ed.camera.units_per_px;
    for _ in 0..5 {
        ed.zoom_in_at(450.0, 250.0, 1.3);
    }
    assert!(
        ed.camera.units_per_px < start,
        "zoom-in must monotonically increase magnification"
    );
}

#[test]
fn repeated_zoom_out_converges_on_reset_view() {
    let mut ed = test_editor();
    ed.zoom_in_at(450.0, 250.0, 20.0);
    for _ in 0..80 {
        ed.zoom_out_at(450.0, 250.0, 1.2);
    }
    let reset = ed.reset_camera();
    assert!(
        (ed.camera.units_per_px / reset.units_per_px - 1.0).abs() < 0.3,
        "zoom-out should land near Reset View: {} vs {}",
        ed.camera.units_per_px,
        reset.units_per_px
    );
}

#[test]
fn hit_testing_prefers_a_note_body_over_empty_canvas() {
    let ed = test_editor();
    let n = &ed.doc.notes[0];
    let x = ed.camera.x((n.start + n.end) * 0.5);
    let y = ed.camera.y(n.row as f64, ed.viewport);
    match ed.hit_test(x, y) {
        expression_editor_core::Hit::Note { id, zone } => {
            assert_eq!(id, n.id);
            assert_eq!(zone, 0);
        }
        other => panic!("expected the note body, got {other:?}"),
    }
}

#[test]
fn hit_testing_prefers_an_edge_handle_over_the_body() {
    let ed = test_editor();
    let n = &ed.doc.notes[0];
    let x = ed.camera.x(n.end);
    let y = ed.camera.y(n.row as f64, ed.viewport);
    assert!(matches!(
        ed.hit_test(x, y),
        expression_editor_core::Hit::NoteEdge {
            start_edge: false,
            ..
        }
    ));
}

#[test]
fn the_whole_row_height_belongs_to_the_note() {
    let ed = test_editor();
    let n = &ed.doc.notes[0];
    let x = ed.camera.x((n.start + n.end) * 0.5);
    // Just inside the top of the row band.
    let y = ed.camera.y(n.row as f64 + 0.45, ed.viewport);
    assert!(matches!(
        ed.hit_test(x, y),
        expression_editor_core::Hit::Note { .. }
    ));
}

#[test]
fn marquee_selects_the_notes_it_covers() {
    let ed = test_editor();
    let mut sel = Selection::default();
    let x0 = ed.camera.x(0.0);
    // Stops just short of the third note's onset — marquee selection is
    // intersection-based, so touching its start would include it.
    let x1 = ed.camera.x(PPQ * 1.95);
    let y0 = ed.camera.y(70.0, ed.viewport);
    let y1 = ed.camera.y(58.0, ed.viewport);
    sel.marquee(&ed.doc, &ed.camera, ed.viewport, (x0, y0), (x1, y1), false);
    assert_eq!(sel.notes.len(), 2, "the first two notes only");
}

#[test]
fn a_drag_belongs_to_the_selection_not_the_note_under_the_pointer() {
    let mut sel = Selection::default();
    sel.add(NoteId(1));
    sel.add(NoteId(2));
    assert_eq!(
        tools::gesture_targets(&sel, Some(NoteId(3))),
        vec![NoteId(1), NoteId(2)]
    );
    assert_eq!(
        tools::gesture_targets(&Selection::default(), Some(NoteId(3))),
        vec![NoteId(3)]
    );
}

#[test]
fn gestures_clamp_directionally_when_they_start_outside_the_span() {
    // Starting left of the note extends only to the left boundary.
    assert_eq!(
        tools::clamp_gesture((100.0, 200.0), 50.0, 50.0, 150.0),
        (100.0, 150.0)
    );
    // Starting right extends only to the right boundary.
    assert_eq!(
        tools::clamp_gesture((100.0, 200.0), 250.0, 150.0, 250.0),
        (150.0, 200.0)
    );
    // Starting inside clamps both ends.
    assert_eq!(
        tools::clamp_gesture((100.0, 200.0), 150.0, 50.0, 250.0),
        (100.0, 200.0)
    );
}

#[test]
fn the_local_grid_reads_out_as_musical_fractions() {
    let mut g = Grid::default();
    assert_eq!(g.label(), "1/16");
    g.triplet = true;
    assert_eq!(g.label(), "1/16T");
    g.triplet = false;
    g.coarser();
    assert_eq!(g.label(), "1/8");
    g.finer();
    g.finer();
    assert_eq!(g.label(), "1/32");
}

#[test]
fn grid_snapping_uses_the_documents_own_origin() {
    let g = Grid::default();
    // 1/16 at 960 ppq = 240 units.
    assert_eq!(g.step(PPQ), 240.0);
    assert_eq!(g.snap(250.0, 0.0, PPQ), 240.0);
    assert_eq!(g.snap(250.0, 100.0, PPQ), 340.0, "origin shifts the phase");
}

#[test]
fn pressure_and_timbre_map_into_a_fixed_two_semitone_box() {
    let ed = test_editor();
    let row = 60;
    for v in [0.0, 0.25, 0.5, 1.0] {
        let y = tools::lane_box_y(&ed.camera, ed.viewport, row, v);
        let back = tools::lane_box_value(&ed.camera, ed.viewport, row, y);
        assert!((back - v).abs() < 1e-9, "{v} round-tripped to {back}");
    }
}

#[test]
fn the_active_lane_draws_last_and_overlays_never_duplicate_it() {
    let mut ed = test_editor();
    ed.dimension = Dimension::Pressure;
    ed.overlays = vec![Dimension::Pitch, Dimension::Pressure];
    assert_eq!(ed.draw_order(), vec![Dimension::Pitch, Dimension::Pressure]);
}

// ── modulation ───────────────────────────────────────────────────────

#[test]
fn an_oscillator_alone_stays_within_its_amplitude() {
    let stack = Stack {
        rows: vec![Row::Oscillator {
            wave: Wave::Sine,
            amplitude: 0.5,
            rate: 4.0,
        }],
    };
    for v in stack.render(200) {
        assert!(v.abs() <= 0.5 + 1e-9);
    }
}

#[test]
fn a_following_curve_row_envelopes_the_oscillator() {
    let stack = Stack::growing_vibrato();
    let rendered = stack.render(200);
    let early = rendered[..20].iter().fold(0.0f64, |a, b| a.max(b.abs()));
    let late = rendered[180..].iter().fold(0.0f64, |a, b| a.max(b.abs()));
    assert!(late > early * 2.0, "vibrato should grow: {early} → {late}");

    let receding = Stack::receding_vibrato().render(200);
    let early = receding[..20].iter().fold(0.0f64, |a, b| a.max(b.abs()));
    let late = receding[180..].iter().fold(0.0f64, |a, b| a.max(b.abs()));
    assert!(early > late * 2.0, "and recede: {early} → {late}");
}

#[test]
fn a_rate_curve_accelerates_phase_instead_of_stepping_it() {
    let stack = Stack {
        rows: vec![
            Row::Oscillator {
                wave: Wave::Sine,
                amplitude: 1.0,
                rate: 8.0,
            },
            Row::Curve {
                shape: Shape::Linear,
                depth: 1.0,
                up: true,
                target: CurveTarget::Rate,
            },
        ],
    };
    // Count zero crossings in the first and last quarter: an
    // accelerating vibrato has more cycles late than early.
    let r = stack.render(2000);
    let crossings = |s: &[f64]| {
        s.windows(2)
            .filter(|w| w[0].signum() != w[1].signum())
            .count()
    };
    let early = crossings(&r[..500]);
    let late = crossings(&r[1500..]);
    assert!(late > early, "rate should ramp up: {early} → {late}");
    // And the output stays continuous — no jump between cycles.
    let max_step = r
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0.0f64, f64::max);
    assert!(
        max_step < 0.2,
        "phase must integrate smoothly, max step {max_step}"
    );
}

#[test]
fn modulation_tapers_to_nothing_at_the_boundaries() {
    let mut values = vec![0.0f64; 200];
    let stack = Stack {
        rows: vec![Row::Oscillator {
            wave: Wave::Square,
            amplitude: 1.0,
            rate: 6.0,
        }],
    };
    expression_editor_core::modulation::apply(&mut values, &stack, 0.1);
    assert!(values[0].abs() < 1e-9, "no step at the start");
    assert!(values[199].abs() < 1e-9, "no step at the end");
    assert!(
        values.iter().any(|v| v.abs() > 0.5),
        "but full depth in the middle"
    );
}

// ── modulation as an edit ────────────────────────────────────────────

#[test]
fn applying_modulation_tapers_at_the_span_edges() {
    let mut doc = doc_with_note();
    {
        let n = doc.note_mut(NoteId(1)).unwrap();
        n.pitch.set(0.0, 0.0);
        n.pitch.set(PPQ * 2.0, 0.0);
    }
    let stack = Stack {
        rows: vec![Row::Oscillator {
            wave: Wave::Sine,
            amplitude: 0.5,
            rate: 8.0,
        }],
    };
    assert!(
        Edit::ApplyModulation {
            note: NoteId(1),
            dimension: Dimension::Pitch,
            t0: 0.0,
            t1: PPQ * 2.0,
            stack,
            taper: 0.1,
            samples: 128,
        }
        .apply(&mut doc)
    );

    let n = doc.note(NoteId(1)).unwrap();
    // No step at the boundaries — a modulation dropped in cold at a
    // nonzero phase would click.
    assert!(n.pitch.sample(0.0, 0.0).abs() < 1e-6);
    assert!(n.pitch.sample(PPQ * 2.0, 0.0).abs() < 1e-6);
    let (lo, hi) = n.pitch.value_bounds().unwrap();
    assert!(hi - lo > 0.5, "but full depth inside, got {}", hi - lo);
}

#[test]
fn modulation_preserves_data_outside_its_target_range() {
    let mut doc = doc_with_note();
    {
        let n = doc.note_mut(NoteId(1)).unwrap();
        n.pitch.set(0.0, 3.0);
        n.pitch.set(PPQ * 2.0, 3.0);
    }
    // Target only the second half.
    Edit::ApplyModulation {
        note: NoteId(1),
        dimension: Dimension::Pitch,
        t0: PPQ,
        t1: PPQ * 2.0,
        stack: Stack::growing_vibrato(),
        taper: 0.1,
        samples: 64,
    }
    .apply(&mut doc);
    let n = doc.note(NoteId(1)).unwrap();
    assert_eq!(
        n.pitch.sample(0.0, 0.0),
        3.0,
        "the untouched half is intact"
    );
    assert!(
        (n.pitch.sample(PPQ, 0.0) - 3.0).abs() < 1e-6,
        "and the seam"
    );
}

#[test]
fn restore_puts_a_captured_curve_back_exactly() {
    let mut doc = doc_with_note();
    {
        let n = doc.note_mut(NoteId(1)).unwrap();
        for i in 0..40 {
            n.pitch.set(i as f64 * 48.0, (i as f64 * 0.3).sin());
        }
    }
    let captured = doc.note(NoteId(1)).unwrap().pitch.clone();

    Edit::ApplyModulation {
        note: NoteId(1),
        dimension: Dimension::Pitch,
        t0: 0.0,
        t1: PPQ * 2.0,
        stack: Stack::growing_vibrato(),
        taper: 0.1,
        samples: 64,
    }
    .apply(&mut doc);
    assert_ne!(doc.note(NoteId(1)).unwrap().pitch, captured);

    Edit::RestoreDimension {
        note: NoteId(1),
        dimension: Dimension::Pitch,
        t0: 0.0,
        t1: PPQ * 2.0,
        points: captured.points().to_vec(),
    }
    .apply(&mut doc);
    assert_eq!(
        doc.note(NoteId(1)).unwrap().pitch,
        captured,
        "cancel must restore byte-for-byte"
    );
}

// ── full MIDI editing ────────────────────────────────────────────────

use expression_editor_core::mouse::{Action, Context, Gesture, ModKey, MouseMap};
use expression_editor_core::rows::{Articulation, DrumMap, NoteShape, RowSpace, StringTuning};

fn doc_with_notes(n: usize) -> ExpressionDoc {
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 16.0);
    for i in 0..n {
        let mut note = Note::new(
            NoteId(i as u64 + 1),
            PPQ * i as f64,
            PPQ * (i as f64 + 0.5),
            60 + i as i32,
        );
        note.channel = Some(2 + i as u8);
        doc.push(note);
    }
    doc
}

fn ids(doc: &ExpressionDoc) -> Vec<NoteId> {
    doc.notes.iter().map(|n| n.id).collect()
}

#[test]
fn velocity_nudges_accumulate_and_clamp() {
    let mut doc = doc_with_notes(1);
    Edit::SetVelocity {
        notes: vec![NoteId(1)],
        velocity: 0.5,
    }
    .apply(&mut doc);
    for _ in 0..10 {
        Edit::NudgeVelocity {
            notes: vec![NoteId(1)],
            delta: 0.1,
        }
        .apply(&mut doc);
    }
    assert_eq!(doc.note(NoteId(1)).unwrap().velocity, 1.0, "clamps at full");
}

#[test]
fn channel_nudge_wraps_inside_the_member_range() {
    let mut doc = doc_with_notes(1);
    doc.note_mut(NoteId(1)).unwrap().channel = Some(16);
    Edit::NudgeChannel {
        notes: vec![NoteId(1)],
        delta: 1,
    }
    .apply(&mut doc);
    // Channel 1 is the MPE master and must never be assigned.
    assert_eq!(doc.note(NoteId(1)).unwrap().channel, Some(2));

    Edit::NudgeChannel {
        notes: vec![NoteId(1)],
        delta: -1,
    }
    .apply(&mut doc);
    assert_eq!(doc.note(NoteId(1)).unwrap().channel, Some(16));
}

#[test]
fn muting_is_not_deleting() {
    let mut doc = doc_with_notes(2);
    Edit::ToggleMuted {
        notes: vec![NoteId(1)],
    }
    .apply(&mut doc);
    assert!(doc.note(NoteId(1)).unwrap().muted);
    assert_eq!(doc.notes.len(), 2, "the note is still in the document");
    Edit::ToggleMuted {
        notes: vec![NoteId(1)],
    }
    .apply(&mut doc);
    assert!(!doc.note(NoteId(1)).unwrap().muted);
}

#[test]
fn doubling_and_halving_length_round_trip() {
    let mut doc = doc_with_notes(1);
    let len = doc.note(NoteId(1)).unwrap().len();
    Edit::ScaleLength {
        notes: vec![NoteId(1)],
        factor: 2.0,
    }
    .apply(&mut doc);
    assert!((doc.note(NoteId(1)).unwrap().len() - len * 2.0).abs() < 1e-6);
    Edit::ScaleLength {
        notes: vec![NoteId(1)],
        factor: 0.5,
    }
    .apply(&mut doc);
    assert!((doc.note(NoteId(1)).unwrap().len() - len).abs() < 1e-6);
}

#[test]
fn stretching_positions_arpeggiates_about_the_pivot() {
    let mut doc = doc_with_notes(3);
    let all = ids(&doc);
    Edit::StretchPositions {
        notes: all,
        pivot: 0.0,
        factor: 2.0,
    }
    .apply(&mut doc);
    assert_eq!(
        doc.note(NoteId(1)).unwrap().start,
        0.0,
        "the pivot is fixed"
    );
    assert!((doc.note(NoteId(2)).unwrap().start - PPQ * 2.0).abs() < 1e-6);
    assert!((doc.note(NoteId(3)).unwrap().start - PPQ * 4.0).abs() < 1e-6);
}

#[test]
fn copying_notes_leaves_the_originals_and_carries_expression() {
    let mut doc = doc_with_notes(1);
    doc.note_mut(NoteId(1)).unwrap().pitch.set(0.0, 0.75);
    Edit::CopyNotes {
        notes: vec![NoteId(1)],
        time_delta: PPQ * 4.0,
        row_delta: 12,
    }
    .apply(&mut doc);
    assert_eq!(doc.notes.len(), 2);
    let copy = doc.notes.iter().find(|n| n.id != NoteId(1)).unwrap();
    assert_eq!(copy.row, 72);
    assert_eq!(copy.start, PPQ * 4.0);
    assert_eq!(
        copy.pitch.sample(PPQ * 4.0, 0.0),
        0.75,
        "expression came too"
    );
    assert_eq!(
        doc.note(NoteId(1)).unwrap().start,
        0.0,
        "original untouched"
    );
}

#[test]
fn partial_quantize_pulls_toward_the_grid_without_nailing_to_it() {
    let mut doc = doc_with_notes(1);
    Edit::MoveTime {
        notes: vec![NoteId(1)],
        delta: 100.0,
    }
    .apply(&mut doc);

    let mut half = doc.clone();
    Edit::Quantize {
        notes: vec![NoteId(1)],
        step: PPQ,
        origin: 0.0,
        strength: 0.5,
    }
    .apply(&mut half);
    assert!(
        (half.note(NoteId(1)).unwrap().start - 50.0).abs() < 1e-6,
        "half strength moves half the distance"
    );

    Edit::Quantize {
        notes: vec![NoteId(1)],
        step: PPQ,
        origin: 0.0,
        strength: 1.0,
    }
    .apply(&mut doc);
    assert_eq!(doc.note(NoteId(1)).unwrap().start, 0.0);
}

#[test]
fn legato_extends_each_note_to_the_next_on_its_row() {
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 8.0);
    doc.push(Note::new(NoteId(1), 0.0, PPQ * 0.25, 60));
    doc.push(Note::new(NoteId(2), PPQ, PPQ * 1.25, 60));
    // A different row must not close the first note's gap.
    doc.push(Note::new(NoteId(3), PPQ * 0.5, PPQ * 0.75, 64));

    Edit::Legato {
        notes: vec![NoteId(1)],
        gap: 0.0,
    }
    .apply(&mut doc);
    assert_eq!(doc.note(NoteId(1)).unwrap().end, PPQ);
}

#[test]
fn a_note_carries_its_lyric() {
    let mut doc = doc_with_notes(1);
    Edit::SetText {
        note: NoteId(1),
        text: Some("Ha".into()),
    }
    .apply(&mut doc);
    assert_eq!(doc.note(NoteId(1)).unwrap().text.as_deref(), Some("Ha"));
    // And a lyric is the note's label, outranking the note name.
    assert_eq!(
        RowSpace::Pitch.note_label(doc.note(NoteId(1)).unwrap()),
        Some("Ha".to_string())
    );
}

// ── guitar / bass ────────────────────────────────────────────────────

#[test]
fn a_string_roll_sounds_open_pitch_plus_fret() {
    let t = StringTuning::guitar_standard();
    assert_eq!(t.pitch(0, 0), 40, "low E");
    assert_eq!(t.pitch(5, 0), 64, "high E");
    assert_eq!(t.pitch(0, 12), 52, "twelfth fret is an octave");
}

#[test]
fn a_capo_raises_every_open_string() {
    let mut t = StringTuning::guitar_standard();
    t.capo = 2;
    assert_eq!(t.pitch(0, 0), 42);
    assert_eq!(t.pitch(5, 3), 69);
}

#[test]
fn fingering_prefers_the_position_nearest_the_hand() {
    let t = StringTuning::guitar_standard();
    // A4 = 69 is reachable on several strings.
    let low = t.best_position(69, 0).unwrap();
    let high = t.best_position(69, 14).unwrap();
    assert_ne!(
        low, high,
        "the preferred fret must actually steer the choice"
    );
    assert!(low.1 < high.1, "a hand at the nut plays the lower fret");
}

#[test]
fn moving_a_note_to_another_string_keeps_its_sounding_pitch() {
    let tuning = StringTuning::guitar_standard();
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 8.0);
    doc.row_space = RowSpace::Strings(tuning.clone());
    // 5th fret of the high E string = A4 (69). The row is the pitch.
    let mut n = Note::new(NoteId(1), 0.0, PPQ, tuning.open(5) + 5);
    n.string = Some(5);
    doc.push(n);
    let before = doc.row_space.pitch_of(doc.note(NoteId(1)).unwrap());

    assert!(
        Edit::SetString {
            note: NoteId(1),
            string: 4,
        }
        .apply(&mut doc)
    );

    let n = doc.note(NoteId(1)).unwrap();
    assert_eq!(
        doc.row_space.pitch_of(n),
        before,
        "changing string re-fingers, it does not transpose"
    );
    assert_eq!(n.string, Some(4), "it is on the B string now");
    assert_eq!(
        expression_editor_core::rows::fret_of(n, &tuning),
        Some(10),
        "A4 on the B string is the tenth fret"
    );
}

#[test]
fn an_unreachable_string_refuses_the_move() {
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 8.0);
    doc.row_space = RowSpace::Strings(StringTuning::guitar_standard());
    // E2 = 40, open on the low E string.
    let mut n2 = Note::new(NoteId(2), 0.0, PPQ, 40);
    n2.string = Some(0);
    doc.push(n2);
    assert!(
        !Edit::SetString {
            note: NoteId(2),
            string: 5,
        }
        .apply(&mut doc),
        "E2 is not reachable on the high E string"
    );
}

#[test]
fn a_string_roll_labels_notes_with_their_fret() {
    let space = RowSpace::Strings(StringTuning::guitar_standard());
    let tuning = StringTuning::guitar_standard();
    // Seventh fret of the D string.
    let mut n = Note::new(NoteId(1), 0.0, 100.0, tuning.open(2) + 7);
    n.string = Some(2);
    assert_eq!(space.note_label(&n), Some("7".to_string()));
    // Rows are pitches now, so they are named as pitches — a row does
    // not belong to a string, because several strings reach it.
    assert_eq!(space.row_label(tuning.open(2)), "D3");
}

#[test]
fn legato_articulations_are_marked_as_such() {
    for a in Articulation::ALL {
        let legato = matches!(
            a,
            Articulation::HammerOn | Articulation::PullOff | Articulation::LegatoSlide
        );
        assert_eq!(a.is_legato(), legato, "{a:?}");
    }
    // Natural harmonics only speak at certain frets.
    assert!(
        Articulation::NaturalHarmonic
            .valid_frets()
            .unwrap()
            .contains(&12)
    );
    assert!(Articulation::PalmMute.valid_frets().is_none());
}

#[test]
fn setting_an_articulation_sets_the_legato_flag_with_it() {
    let mut doc = doc_with_notes(1);
    Edit::SetArticulation {
        notes: vec![NoteId(1)],
        articulation: Some(Articulation::HammerOn),
    }
    .apply(&mut doc);
    assert!(doc.note(NoteId(1)).unwrap().legato);

    Edit::SetArticulation {
        notes: vec![NoteId(1)],
        articulation: Some(Articulation::PalmMute),
    }
    .apply(&mut doc);
    assert!(!doc.note(NoteId(1)).unwrap().legato);
}

// ── drums ────────────────────────────────────────────────────────────

#[test]
fn drum_rows_map_to_their_pitches_both_ways() {
    let map = DrumMap::general_midi();
    let space = RowSpace::Drums(map.clone());
    let kick_row = map.row_of_pitch(36).unwrap();
    let mut n = Note::new(NoteId(1), 0.0, 100.0, kick_row as i32);
    assert_eq!(space.pitch_of(&n), 36);
    assert_eq!(space.row_label(kick_row as i32), "Kick");
    assert_eq!(
        space.row_of_pitch(38),
        map.row_of_pitch(38).map(|r| r as i32)
    );
    n.row = 0;
    // A drum hit has no meaningful length, so it gets a fixed head
    // whose flat edge marks the attack.
    assert_eq!(space.note_shape(), NoteShape::Triangle);
    assert_eq!(RowSpace::Pitch.note_shape(), NoteShape::Bar);
}

#[test]
fn drum_rows_have_no_accidental_shading() {
    let space = RowSpace::Drums(DrumMap::general_midi());
    assert!(!space.is_accidental(1), "black keys mean nothing on a kit");
    assert!(RowSpace::Pitch.is_accidental(61));
}

// ── mouse map ────────────────────────────────────────────────────────

#[test]
fn modifiers_resolve_to_their_bound_action() {
    let m = MouseMap::reaper_like();
    let none = Mods::default();
    let ctrl = Mods {
        ctrl: true,
        ..Default::default()
    };
    let alt = Mods {
        alt: true,
        ..Default::default()
    };
    assert_eq!(
        m.resolve(Context::PianoRoll, Gesture::Drag, none),
        Action::MarqueeSelect
    );
    assert_eq!(
        m.resolve(Context::Note, Gesture::Drag, ctrl),
        Action::EditNoteVelocity
    );
    assert_eq!(
        m.resolve(Context::Note, Gesture::Drag, alt),
        Action::CopyNote
    );
    assert_eq!(
        m.resolve(Context::NoteEdge, Gesture::Drag, none),
        Action::MoveNoteEdge
    );
}

#[test]
fn an_unbound_modifier_falls_back_to_the_plain_binding() {
    let m = MouseMap::reaper_like();
    let weird = Mods {
        shift: true,
        ctrl: true,
        alt: true,
    };
    // Never dead: a user holding a stray modifier gets the obvious
    // thing rather than nothing.
    assert_eq!(
        m.resolve(Context::Ruler, Gesture::Click, weird),
        Action::MovePlayhead
    );
}

#[test]
fn presets_differ_where_the_products_differ() {
    let none = Mods::default();
    let reaper = MouseMap::reaper_like();
    let drums = MouseMap::drums();
    let riffer = MouseMap::riffer();

    // A drum part is built by sweeping hits, not by marquee.
    assert_eq!(
        reaper.resolve(Context::PianoRoll, Gesture::Drag, none),
        Action::MarqueeSelect
    );
    assert_eq!(
        drums.resolve(Context::PianoRoll, Gesture::Drag, none),
        Action::PaintNotes
    );
    // Riffer inserts on plain click and deletes on double-click.
    assert_eq!(
        riffer.resolve(Context::PianoRoll, Gesture::Click, none),
        Action::InsertNote
    );
    assert_eq!(
        riffer.resolve(Context::Note, Gesture::DoubleClick, none),
        Action::EraseNote
    );
    // And every preset can still be listed for a cheat sheet.
    for name in MouseMap::PRESETS {
        assert!(!MouseMap::preset(name).bindings().is_empty());
    }
}

#[test]
fn rebinding_replaces_rather_than_stacking() {
    let mut m = MouseMap::reaper_like();
    let before = m.bindings().len();
    m.set(
        Context::Note,
        Gesture::Drag,
        ModKey::NONE,
        Action::EditNoteVelocity,
    );
    assert_eq!(m.bindings().len(), before, "no duplicate binding");
    assert_eq!(
        m.resolve(Context::Note, Gesture::Drag, Mods::default()),
        Action::EditNoteVelocity
    );
}

#[test]
fn edits_are_distinguished_from_navigation_for_undo_grouping() {
    assert!(Action::MoveNote.is_edit());
    assert!(Action::EditNoteVelocity.is_edit());
    assert!(!Action::MarqueeSelect.is_edit());
    assert!(!Action::Pan.is_edit());
    assert!(!Action::Audition.is_edit());
    assert!(Action::MoveNoteNoSnap.ignores_snap());
    assert!(!Action::MoveNote.ignores_snap());
}

// ── contextual zoom (MeMagic) ────────────────────────────────────────

use expression_editor_core::zoom::{
    self, HorizontalMode, SmartZoom, Span, VerticalMode, ZoomModes,
};

fn spans_at(starts: &[f64], len: f64, row: i32) -> Vec<Span> {
    starts
        .iter()
        .map(|&s| Span {
            start: s,
            end: s + len,
            row,
        })
        .collect()
}

#[test]
fn smart_zoom_follows_local_density_not_the_item_average() {
    // Sixteenths on the left, whole notes on the right.
    let mut spans = spans_at(&[0.0, 240.0, 480.0, 720.0, 960.0, 1200.0], 240.0, 60);
    spans.extend(spans_at(&[10000.0, 14000.0, 18000.0], 3840.0, 60));
    let cfg = SmartZoom::default();

    let dense = zoom::weighted_note_length(&spans, 600.0, cfg).unwrap();
    let sparse = zoom::weighted_note_length(&spans, 14000.0, cfg).unwrap();
    assert!(
        sparse > dense * 3.0,
        "the sparse passage must zoom out much further: {dense} vs {sparse}"
    );
}

#[test]
fn zero_smoothing_ignores_distance_entirely() {
    let mut spans = spans_at(&[0.0, 240.0, 480.0], 240.0, 60);
    spans.extend(spans_at(&[10000.0, 14000.0], 3840.0, 60));
    let cfg = SmartZoom {
        smoothing: 0.0,
        ..Default::default()
    };
    let a = zoom::weighted_note_length(&spans, 200.0, cfg).unwrap();
    let b = zoom::weighted_note_length(&spans, 14000.0, cfg).unwrap();
    assert!(
        (a - b).abs() < 1e-6,
        "with no smoothing every position sees the same average"
    );
}

#[test]
fn smart_zoom_scales_with_the_requested_note_count() {
    let spans = spans_at(&[0.0, 240.0, 480.0, 720.0], 240.0, 60);
    let ten = zoom::weighted_note_length(
        &spans,
        360.0,
        SmartZoom {
            notes_visible: 10.0,
            ..Default::default()
        },
    )
    .unwrap();
    let twenty = zoom::weighted_note_length(
        &spans,
        360.0,
        SmartZoom {
            notes_visible: 20.0,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(twenty > ten, "more notes means a wider span");
}

#[test]
fn an_empty_stretch_still_reveals_the_nearest_note() {
    let spans = spans_at(&[0.0], 240.0, 60);
    let cfg = SmartZoom::default();
    // Far away from the only note.
    let span = zoom::weighted_note_length(&spans, 100_000.0, cfg).unwrap();
    assert!(
        span >= 100_000.0 * 2.0,
        "the view must widen enough to show something, got {span}"
    );
}

#[test]
fn no_notes_gives_no_smart_span() {
    assert!(zoom::weighted_note_length(&[], 0.0, SmartZoom::default()).is_none());
}

#[test]
fn cursor_alignment_places_the_anchor_in_the_view() {
    assert_eq!(zoom::align(100.0, 40.0, 0.0), (100.0, 140.0), "left edge");
    assert_eq!(zoom::align(100.0, 40.0, 0.5), (80.0, 120.0), "centred");
    assert_eq!(zoom::align(100.0, 40.0, 1.0), (60.0, 100.0), "right edge");
}

#[test]
fn restricting_to_the_item_slides_the_view_rather_than_squashing_it() {
    let vp = Viewport::new(800.0, 480.0);
    let content = Content {
        t_start: 0.0,
        t_end: 10000.0,
        pitch_lo: 55.0,
        pitch_hi: 67.0,
    };
    let cam = camera::reset_view(content, vp, 0.03, 0.35, Default::default());
    let spans = spans_at(&[0.0, 240.0, 480.0], 240.0, 60);

    let free = zoom::apply_horizontal(
        cam,
        HorizontalMode::SmartNotes,
        &spans,
        0.0,
        content,
        vp,
        3840.0,
        SmartZoom::default(),
    );
    let clamped = zoom::apply_horizontal(
        cam,
        HorizontalMode::SmartNotesInItem,
        &spans,
        0.0,
        content,
        vp,
        3840.0,
        SmartZoom::default(),
    );
    assert!(
        free.t0 < content.t_start,
        "unclamped runs off the item start"
    );
    assert_eq!(clamped.t0, content.t_start, "clamped starts at the item");
    assert!(
        (free.units_per_px - clamped.units_per_px).abs() < 1e-9,
        "clamping must slide the view, not change the zoom level"
    );
}

#[test]
fn vertical_fit_respects_the_row_floor_and_ceiling() {
    let vp = Viewport::new(800.0, 480.0);
    let cam = Camera {
        fold: Default::default(),
        t0: 0.0,
        units_per_px: 10.0,
        vertical: VerticalCamera {
            center: 60.0,
            px_per_row: 10.0,
        },
    };
    // One note: without a floor this would fill the screen with a
    // single row.
    let one = spans_at(&[0.0], 240.0, 60);
    let cfg = SmartZoom::default();
    let out = zoom::apply_vertical(
        cam,
        VerticalMode::AllNotes,
        &one,
        60.0,
        vp,
        (0.0, 8000.0),
        cfg,
    );
    assert!(out.vertical.px_per_row <= cfg.max_px_per_row);
    assert!(
        vp.h / out.vertical.px_per_row >= cfg.min_rows - 1e-6,
        "at least min_rows must stay visible"
    );
}

#[test]
fn notes_in_view_ignores_notes_outside_the_horizontal_span() {
    let vp = Viewport::new(800.0, 480.0);
    let cam = Camera {
        fold: Default::default(),
        t0: 0.0,
        units_per_px: 1.0,
        vertical: VerticalCamera {
            center: 60.0,
            px_per_row: 10.0,
        },
    };
    let mut spans = spans_at(&[0.0], 240.0, 60);
    spans.extend(spans_at(&[50_000.0], 240.0, 100)); // far away, high
    let out = zoom::apply_vertical(
        cam,
        VerticalMode::CenterOfNotesInView,
        &spans,
        60.0,
        vp,
        (0.0, 800.0),
        SmartZoom::default(),
    );
    assert!(
        (out.vertical.center - 60.0).abs() < 1e-6,
        "the offscreen note must not drag the view up"
    );
}

#[test]
fn contextual_modes_zoom_differently_per_pointer_region() {
    // Needs a part longer than the smart span wants, or both modes
    // clamp to the item and there is nothing to tell apart.
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 64.0);
    for i in 0..64 {
        doc.push(Note::new(
            NoteId(i + 1),
            PPQ * i as f64 * 0.5,
            PPQ * (i as f64 * 0.5 + 0.4),
            60 + (i % 5) as i32,
        ));
    }
    let mut ed = Editor::new(doc, Viewport::new(900.0, 500.0));
    let anchor = PPQ * 8.0;

    let mut keys = ed.clone();
    keys.smart_zoom(ZoomModes::KEYS, anchor, 62.0);
    let mut notes = ed.clone();
    notes.smart_zoom(ZoomModes::NOTE_AREA, anchor, 62.0);

    // Over the keys the whole item is framed; over the notes the view
    // hugs the local passage.
    assert!(
        notes.camera.units_per_px < keys.camera.units_per_px,
        "note-area zoom should be tighter than item zoom"
    );

    // The ruler leaves pitch alone entirely.
    let before = ed.camera.vertical.px_per_row;
    ed.smart_zoom(ZoomModes::RULER, anchor, 62.0);
    assert_eq!(ed.camera.vertical.px_per_row, before);
}

#[test]
fn smart_zoom_keeps_the_anchor_in_view() {
    let mut ed = test_editor();
    let anchor = PPQ * 2.5;
    ed.smart_zoom(ZoomModes::NOTE_AREA, anchor, 62.0);
    let (t0, t1) = ed.camera.time_span(ed.viewport);
    assert!(
        anchor >= t0 && anchor <= t1,
        "anchor {anchor} fell outside {t0}..{t1}"
    );
}

// ── razor edits ──────────────────────────────────────────────────────

use expression_editor_core::razor::{self, RazorArea, RazorSet};

/// One held note per row, spanning the whole bar — so every test can
/// razor a rectangle out of the middle of real material.
fn held_doc() -> ExpressionDoc {
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 16.0);
    for (i, row) in [60, 64, 67].iter().enumerate() {
        let mut n = Note::new(NoteId(i as u64 + 1), 0.0, PPQ * 8.0, *row);
        n.pitch.set(0.0, 0.0);
        n.pitch.set(PPQ * 8.0, 2.0);
        doc.push(n);
    }
    doc
}

#[test]
fn a_razor_slices_notes_at_its_edges_rather_than_selecting_whole_ones() {
    let mut doc = held_doc();
    let area = RazorArea::new(PPQ * 2.0, PPQ * 4.0, 60, 60);
    let inside = razor::carve(&mut doc, area);

    assert_eq!(inside.len(), 1, "exactly the middle piece");
    let piece = doc.note(inside[0]).unwrap();
    assert!((piece.start - PPQ * 2.0).abs() < 1e-6);
    assert!((piece.end - PPQ * 4.0).abs() < 1e-6);
    // The held note became three: before, inside, after.
    let row60: Vec<_> = doc.notes.iter().filter(|n| n.row == 60).collect();
    assert_eq!(row60.len(), 3);
    // Rows the razor did not cover are untouched.
    assert_eq!(doc.notes.iter().filter(|n| n.row == 64).count(), 1);
}

#[test]
fn slicing_preserves_the_expression_at_the_cut() {
    let mut doc = held_doc();
    let area = RazorArea::new(PPQ * 2.0, PPQ * 4.0, 60, 60);
    let inside = razor::carve(&mut doc, area);
    let piece = doc.note(inside[0]).unwrap();
    // The pitch curve ramps 0→2 across 8 beats, so at beat 2 it is 0.5.
    assert!(
        (piece.pitch.sample(PPQ * 2.0, 0.0) - 0.5).abs() < 1e-6,
        "the cut must inherit the curve value, not reset to centre"
    );
}

#[test]
fn deleting_an_area_leaves_a_hole_and_keeps_the_rest() {
    let mut doc = held_doc();
    let area = RazorArea::new(PPQ * 2.0, PPQ * 4.0, 60, 60);
    assert!(razor::delete_contents(&mut doc, area));

    let row60: Vec<_> = doc.notes.iter().filter(|n| n.row == 60).collect();
    assert_eq!(row60.len(), 2, "before and after survive");
    assert!(
        !row60
            .iter()
            .any(|n| n.start < PPQ * 4.0 && n.end > PPQ * 2.0),
        "nothing is left inside the hole"
    );
}

#[test]
fn moving_an_area_carries_its_contents_and_clears_the_destination() {
    let mut doc = held_doc();
    let area = RazorArea::new(PPQ * 2.0, PPQ * 3.0, 60, 60);
    assert!(razor::move_contents(
        &mut doc,
        area,
        PPQ * 4.0,
        0,
        false,
        false
    ));

    // The moved piece landed at 6..7.
    assert!(
        doc.notes
            .iter()
            .any(|n| n.row == 60 && (n.start - PPQ * 6.0).abs() < 1e-6),
        "the piece moved"
    );
    // And it replaced what was there rather than overlapping it.
    let overlapping = doc
        .notes
        .iter()
        .filter(|n| n.row == 60 && n.start < PPQ * 7.0 && n.end > PPQ * 6.0)
        .count();
    assert_eq!(overlapping, 1, "destination was cleared, not stacked on");
}

#[test]
fn copying_an_area_leaves_the_original_in_place() {
    let mut doc = held_doc();
    let area = RazorArea::new(PPQ * 2.0, PPQ * 3.0, 60, 60);
    assert!(razor::move_contents(
        &mut doc,
        area,
        PPQ * 4.0,
        0,
        true,
        false
    ));

    assert!(
        doc.notes
            .iter()
            .any(|n| n.row == 60 && (n.start - PPQ * 2.0).abs() < 1e-6),
        "original still there"
    );
    assert!(
        doc.notes
            .iter()
            .any(|n| n.row == 60 && (n.start - PPQ * 6.0).abs() < 1e-6),
        "and a copy landed"
    );
}

#[test]
fn a_nudge_does_not_delete_its_own_source() {
    let mut doc = held_doc();
    let area = RazorArea::new(PPQ * 2.0, PPQ * 4.0, 60, 60);
    // Destination overlaps the source — the classic way to lose data.
    assert!(razor::move_contents(
        &mut doc,
        area,
        PPQ * 0.5,
        0,
        false,
        false
    ));
    assert!(
        doc.notes
            .iter()
            .any(|n| n.row == 60 && (n.start - PPQ * 2.5).abs() < 1e-6),
        "the nudged piece survives"
    );
}

#[test]
fn moving_an_area_vertically_transposes_its_contents() {
    let mut doc = held_doc();
    let area = RazorArea::new(PPQ * 2.0, PPQ * 3.0, 60, 60);
    razor::move_contents(&mut doc, area, 0.0, 5, false, false);
    assert!(doc.notes.iter().any(|n| n.row == 65));
}

#[test]
fn stretching_an_area_scales_positions_and_lengths_together() {
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 16.0);
    for i in 0..4 {
        doc.push(Note::new(
            NoteId(i + 1),
            PPQ * i as f64 * 0.25,
            PPQ * (i as f64 * 0.25 + 0.25),
            60,
        ));
    }
    let area = RazorArea::new(0.0, PPQ, 60, 60);
    assert!(razor::stretch_contents(&mut doc, area, 0.0, PPQ * 2.0));

    let mut starts: Vec<f64> = doc.notes.iter().map(|n| n.start).collect();
    starts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    // Sixteenths became eighths.
    assert!((starts[1] - PPQ * 0.5).abs() < 1e-6, "got {starts:?}");
    let n = doc.notes.iter().find(|n| n.start == 0.0).unwrap();
    assert!((n.len() - PPQ * 0.5).abs() < 1e-6, "lengths scaled too");
}

#[test]
fn reversing_an_area_mirrors_it_about_its_own_centre() {
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 16.0);
    doc.push(Note::new(NoteId(1), 0.0, PPQ * 0.25, 60));
    doc.push(Note::new(NoteId(2), PPQ * 0.75, PPQ, 60));
    let area = RazorArea::new(0.0, PPQ, 60, 60);
    assert!(razor::reverse_contents(&mut doc, area));

    let mut starts: Vec<f64> = doc.notes.iter().map(|n| n.start).collect();
    starts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    // The note at 0..0.25 mirrors to 0.75..1, and vice versa.
    assert!((starts[0] - 0.0).abs() < 1e-6, "got {starts:?}");
    assert!((starts[1] - PPQ * 0.75).abs() < 1e-6, "got {starts:?}");
}

#[test]
fn razor_velocity_applies_only_inside_the_rectangle() {
    let mut doc = held_doc();
    let area = RazorArea::new(PPQ * 2.0, PPQ * 4.0, 60, 60);
    assert!(razor::set_velocity(&mut doc, area, 0.25));

    let inside = doc
        .notes
        .iter()
        .find(|n| n.row == 60 && n.start >= PPQ * 2.0 && n.end <= PPQ * 4.0)
        .unwrap();
    assert_eq!(inside.velocity, 0.25);
    // Another row is untouched.
    let other = doc.notes.iter().find(|n| n.row == 64).unwrap();
    assert_ne!(other.velocity, 0.25);
}

#[test]
fn clearing_a_lane_across_an_area_keeps_the_notes() {
    let mut doc = held_doc();
    let before = doc.notes.len();
    let area = RazorArea::new(PPQ * 2.0, PPQ * 4.0, 60, 60);
    assert!(razor::clear_lane(&mut doc, area, Dimension::Pitch));
    assert_eq!(doc.notes.len(), before, "notes survive a dimension clear");
}

#[test]
fn overlapping_areas_on_the_same_rows_merge() {
    let mut set = RazorSet::default();
    set.add(RazorArea::new(0.0, 100.0, 60, 60));
    set.add(RazorArea::new(50.0, 200.0, 60, 60));
    assert_eq!(set.areas.len(), 1, "merged");
    assert_eq!(set.areas[0].t0, 0.0);
    assert_eq!(set.areas[0].t1, 200.0);

    // Different rows stay separate.
    set.add(RazorArea::new(0.0, 100.0, 64, 64));
    assert_eq!(set.areas.len(), 2);
}

#[test]
fn an_empty_area_is_never_added() {
    let mut set = RazorSet::default();
    set.add(RazorArea::new(100.0, 100.0, 60, 60));
    assert!(set.is_empty(), "a zero-width drag is not a razor");
}

#[test]
fn hit_testing_a_razor_set_finds_the_topmost_area() {
    let mut set = RazorSet::default();
    set.add(RazorArea::new(0.0, 100.0, 60, 62));
    assert!(set.at(50.0, 61).is_some());
    assert!(set.at(50.0, 70).is_none(), "outside the row range");
    assert!(set.at(150.0, 61).is_none(), "outside the time range");
    assert!(set.remove_at(50.0, 61));
    assert!(set.is_empty());
}

#[test]
fn razor_bindings_do_not_steal_the_plain_marquee_drag() {
    let m = MouseMap::reaper_like();
    // Creating a razor is a deliberate modifier gesture.
    assert_eq!(
        m.resolve(Context::PianoRoll, Gesture::Drag, Mods::default()),
        Action::MarqueeSelect
    );
    assert_eq!(
        m.resolve(
            Context::PianoRoll,
            Gesture::Drag,
            Mods {
                shift: true,
                alt: true,
                ..Default::default()
            }
        ),
        Action::RazorCreate
    );
    // And once an area exists, dragging inside it moves its contents.
    assert_eq!(
        m.resolve(Context::RazorArea, Gesture::Drag, Mods::default()),
        Action::RazorMoveContents
    );
    assert_eq!(
        m.resolve(
            Context::RazorArea,
            Gesture::Drag,
            Mods {
                alt: true,
                ..Default::default()
            }
        ),
        Action::RazorCopyContents
    );
}

#[test]
fn no_preset_binds_the_same_gesture_twice() {
    // A duplicate binding is silently unreachable: `resolve` finds the
    // first and the second never fires. The literal tables are hand
    // written, so this invariant needs guarding.
    for name in MouseMap::PRESETS {
        let m = MouseMap::preset(name);
        let mut seen: Vec<(Context, Gesture, ModKey)> = Vec::new();
        for b in m.bindings() {
            let key = (b.context, b.gesture, b.mods);
            assert!(
                !seen.contains(&key),
                "{name}: duplicate binding for {:?} {:?} {}",
                b.context,
                b.gesture,
                b.mods.notation()
            );
            seen.push(key);
        }
    }
}

// ── chords (keyflow-backed) ──────────────────────────────────────────

use expression_editor_core::chord;

#[test]
fn triads_and_sevenths_are_recognised() {
    let name = |pitches: &[i32]| chord::identify(pitches).map(|c| chord::name(&c));
    assert_eq!(name(&[60, 64, 67]).as_deref(), Some("C"));
    assert_eq!(name(&[60, 63, 67]).as_deref(), Some("Cm"));
    // A seventh must not be read as a plain triad.
    assert_eq!(name(&[60, 64, 67, 71]).as_deref(), Some("Cmaj7"));
    assert_eq!(name(&[60, 64, 67, 70]).as_deref(), Some("C7"));
    assert_eq!(name(&[60, 63, 67, 70]).as_deref(), Some("Cm7"));
}

#[test]
fn an_inversion_reads_as_a_slash_chord_not_a_different_chord() {
    // C major with E in the bass.
    let c = chord::identify(&[64, 67, 72]).unwrap();
    assert_eq!(
        chord::name(&c),
        "C/E",
        "first inversion of C is a slash chord, not Em"
    );
}

#[test]
fn a_single_note_is_not_a_chord() {
    assert!(chord::identify(&[60]).is_none());
    assert!(chord::identify(&[]).is_none());
}

#[test]
fn the_chord_box_reads_the_selection() {
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 8.0);
    for (i, row) in [60, 64, 67].iter().enumerate() {
        doc.push(Note::new(NoteId(i as u64 + 1), 0.0, PPQ * 2.0, *row));
    }
    let mut ed = Editor::new(doc, Viewport::new(900.0, 500.0));

    // Nothing selected and no playhead: nothing to say.
    assert!(ed.current_chord().is_none());

    ed.selection.notes = vec![NoteId(1), NoteId(2), NoteId(3)];
    assert_eq!(ed.chord_pitches(), vec![60, 64, 67]);
    assert_eq!(
        ed.current_chord().map(|c| chord::name(&c)).as_deref(),
        Some("C")
    );
}

#[test]
fn with_nothing_selected_the_box_follows_the_playhead() {
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 8.0);
    for (i, row) in [60, 64, 67].iter().enumerate() {
        doc.push(Note::new(NoteId(i as u64 + 1), 0.0, PPQ * 2.0, *row));
    }
    // A note that starts later must not be counted early.
    doc.push(Note::new(NoteId(9), PPQ * 4.0, PPQ * 6.0, 70));
    let mut ed = Editor::new(doc, Viewport::new(900.0, 500.0));
    ed.playhead = Some(PPQ);
    assert_eq!(ed.chord_pitches(), vec![60, 64, 67]);

    // Muted notes are not sounding, so they are not in the chord.
    ed.doc.note_mut(NoteId(2)).unwrap().muted = true;
    assert_eq!(ed.chord_pitches(), vec![60, 67]);
}

#[test]
fn a_string_roll_reports_sounding_pitches_not_string_numbers() {
    let tuning = StringTuning::guitar_standard();
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 8.0);
    doc.row_space = RowSpace::Strings(tuning.clone());
    // An open C major shape on the top three strings: G B E.
    for (i, (string, fret)) in [(3usize, 0i32), (4, 1), (5, 0)].iter().enumerate() {
        let mut n = Note::new(NoteId(i as u64 + 1), 0.0, PPQ, tuning.open(*string) + fret);
        n.string = Some(*string as u8);
        doc.push(n);
    }
    let mut ed = Editor::new(doc, Viewport::new(900.0, 500.0));
    ed.row_space = RowSpace::Strings(tuning);
    ed.selection.notes = vec![NoteId(1), NoteId(2), NoteId(3)];
    // G=55, C=60, E=64 — the chord box must see pitches, not rows 3/4/5.
    assert_eq!(ed.chord_pitches(), vec![55, 60, 64]);
    assert!(ed.current_chord().is_some());
}

// ── controller lanes ─────────────────────────────────────────────────

use expression_editor_core::cc::{self, CcSet};

#[test]
fn volume_like_controllers_rest_at_full_not_silence() {
    let set = CcSet::orchestral();
    // A CC11 lane that defaulted to zero would mute the part the moment
    // it was pinned.
    assert_eq!(set.get(11).unwrap().default_value(), 1.0);
    assert_eq!(set.get(11).unwrap().value(0.0), 127);
    // Modulation rests at zero — no vibrato until asked for.
    assert_eq!(set.get(1).unwrap().default_value(), 0.0);
    assert_eq!(set.get(1).unwrap().value(0.0), 0);
}

#[test]
fn the_orchestral_default_pins_modulation_and_expression() {
    let set = CcSet::orchestral();
    assert_eq!(set.pinned_count(), 2);
    let pinned: Vec<u8> = set.pinned().map(|l| l.number).collect();
    assert_eq!(pinned, vec![1, 11]);
    // And they are told apart by colour.
    assert_ne!(set.color_of(1), set.color_of(11));
}

#[test]
fn ensuring_a_lane_is_idempotent_and_assigns_a_fresh_colour() {
    let mut set = CcSet::default();
    let a = set.ensure(1);
    let b = set.ensure(1);
    assert_eq!(a, b, "the same controller must not be added twice");
    set.ensure(11);
    set.ensure(2);
    assert_eq!(set.lanes.len(), 3);
    assert_eq!(set.get(2).unwrap().name, "Breath");
}

#[test]
fn pinning_toggles_and_removal_works() {
    let mut set = CcSet::orchestral();
    assert!(!set.toggle_pin(1), "was pinned, now unpinned");
    assert_eq!(set.pinned_count(), 1);
    assert!(set.remove(1));
    assert!(!set.remove(1), "removing twice is a no-op");
}

#[test]
fn cc_edits_are_document_level_not_per_note() {
    let mut doc = doc_with_notes(1);
    assert!(
        Edit::DrawCc {
            number: 11,
            t0: 0.0,
            t1: PPQ * 4.0,
            points: vec![
                Point {
                    t: 0.0,
                    value: 0.2,
                    ..Point::default()
                },
                Point {
                    t: PPQ * 4.0,
                    value: 1.0,
                    ..Point::default()
                },
            ],
        }
        .apply(&mut doc)
    );

    let dimension = doc.cc.get(11).unwrap();
    // The swell exists independently of where notes start and end.
    assert_eq!(dimension.value(0.0), 25);
    assert_eq!(dimension.value(PPQ * 4.0), 127);
    assert_eq!(
        dimension.value(PPQ * 2.0),
        76,
        "linear between authored points"
    );
}

#[test]
fn scaling_a_controller_cannot_push_it_past_the_wire_range() {
    let mut doc = doc_with_notes(1);
    Edit::DrawCc {
        number: 1,
        t0: 0.0,
        t1: PPQ * 4.0,
        points: vec![
            Point {
                t: 0.0,
                value: 0.9,
                ..Point::default()
            },
            Point {
                t: PPQ * 4.0,
                value: 0.1,
                ..Point::default()
            },
        ],
    }
    .apply(&mut doc);

    Edit::ScaleCc {
        number: 1,
        t0: 0.0,
        t1: PPQ * 4.0,
        pivot: 0.5,
        factor: 8.0,
    }
    .apply(&mut doc);

    let dimension = doc.cc.get(1).unwrap();
    for p in dimension.curve.points() {
        assert!(
            (0.0..=1.0).contains(&p.value),
            "a controller must not clip on export, got {}",
            p.value
        );
    }
    assert_eq!(dimension.value(0.0), 127);
    assert_eq!(dimension.value(PPQ * 4.0), 0);
}

#[test]
fn erasing_a_controller_leaves_the_notes_alone() {
    let mut doc = doc_with_notes(2);
    Edit::DrawCc {
        number: 11,
        t0: 0.0,
        t1: PPQ * 4.0,
        points: vec![Point {
            t: PPQ,
            value: 0.5,
            ..Point::default()
        }],
    }
    .apply(&mut doc);
    let before = doc.notes.len();
    assert!(
        Edit::EraseCc {
            number: 11,
            t0: 0.0,
            t1: PPQ * 4.0,
        }
        .apply(&mut doc)
    );
    assert_eq!(doc.notes.len(), before);
    // Erased back to its resting value, not to zero.
    assert_eq!(doc.cc.get(11).unwrap().value(PPQ), 127);
}

#[test]
fn clearing_part_of_a_curve_splices_the_default_in_at_both_edges() {
    // The case that only deleting interior points cannot handle: the
    // shape is defined entirely by points *outside* the cleared range,
    // so there is nothing in it to delete and the curve would sail
    // straight through untouched.
    let mut c = Curve::new();
    c.set(0.0, 0.0);
    c.set(PPQ * 4.0, 1.0);

    assert!(c.clear_range(PPQ, PPQ * 3.0, 0.0));
    assert!(
        c.sample(PPQ * 2.0, 0.0).abs() < 1e-9,
        "the middle reads as cleared, got {}",
        c.sample(PPQ * 2.0, 0.0)
    );
    assert!(
        (c.sample(PPQ * 4.0, 0.0) - 1.0).abs() < 1e-9,
        "and the ramp outside it is untouched"
    );
}

#[test]
fn clearing_a_whole_curve_empties_it_rather_than_authoring_defaults() {
    // The exception: with nothing outside to bleed in, leaving two
    // default points behind would call an untouched lane "authored".
    let mut c = Curve::new();
    c.set(PPQ, 0.5);
    c.set(PPQ * 2.0, 0.8);

    assert!(c.clear_range(0.0, PPQ * 4.0, 0.0));
    assert!(c.is_empty());
    assert!(
        !c.clear_range(0.0, PPQ * 4.0, 0.0),
        "and clearing an already-empty curve changes nothing"
    );
}

#[test]
fn erasing_part_of_a_controller_reads_as_cleared() {
    let mut doc = doc_with_notes(2);
    // A swell whose only points are outside the range about to be
    // erased — the shape EraseCc used to miss entirely.
    Edit::DrawCc {
        number: 1,
        t0: 0.0,
        t1: PPQ * 4.0,
        points: vec![
            Point {
                t: 0.0,
                value: 0.0,
                ..Point::default()
            },
            Point {
                t: PPQ * 4.0,
                value: 1.0,
                ..Point::default()
            },
        ],
    }
    .apply(&mut doc);
    assert!(doc.cc.get(1).unwrap().value(PPQ * 2.0) > 60);

    assert!(
        Edit::EraseCc {
            number: 1,
            t0: PPQ,
            t1: PPQ * 3.0,
        }
        .apply(&mut doc)
    );

    assert_eq!(
        doc.cc.get(1).unwrap().value(PPQ * 2.0),
        0,
        "CC1 rests at 0, and the erased range has to read that way"
    );
    assert_eq!(
        doc.cc.get(1).unwrap().value(PPQ * 4.0),
        127,
        "outside the erased range the swell stands"
    );
}

#[test]
fn entering_cc_edit_mode_pins_the_lane_it_edits() {
    let mut ed = test_editor();
    assert!(!ed.cc_editing());
    // Editing a dimension you cannot see is a trap, so entering must pin it.
    ed.edit_cc(11);
    assert!(ed.cc_editing());
    assert!(ed.doc.cc.get(11).unwrap().pinned);
    ed.exit_cc_edit();
    assert!(!ed.cc_editing());
    assert!(ed.doc.cc.get(11).unwrap().pinned, "unpinning is separate");
}

#[test]
fn cc_maps_onto_the_full_roll_height_both_ways() {
    for v in [0.0, 0.25, 0.5, 1.0] {
        let y = cc::cc_y(v, 400.0);
        assert!((cc::cc_value(y, 400.0) - v).abs() < 1e-9, "{v} round-trip");
    }
    assert_eq!(cc::cc_y(1.0, 400.0), 0.0, "full value is at the top");
    assert_eq!(cc::cc_y(0.0, 400.0), 400.0);
}

#[test]
fn row_colour_follows_what_the_row_means() {
    // Pitch space keeps pitch-class colour; the others colour by row,
    // because that is the thing being tracked.
    assert!(RowSpace::Pitch.row_color(60).is_none());

    // A string roll's rows are pitches, and a pitch is reachable on
    // several strings — so the row has no colour and the *note* carries
    // it, which is what shows a phrase crossing the neck.
    let strings = RowSpace::Strings(StringTuning::guitar_standard());
    assert!(strings.row_color(64).is_none());
    assert_ne!(
        expression_editor_core::rows::string_color(0),
        expression_editor_core::rows::string_color(5),
        "strings must be told apart by colour"
    );

    let kit = RowSpace::Drums(DrumMap::general_midi());
    let map = DrumMap::general_midi();
    let row_of = |name: &str| map.lanes.iter().position(|l| l.name == name).unwrap() as i32;
    // Kit sections, not individual lanes: both snares read as snare.
    assert_eq!(
        kit.row_color(row_of("Snare")),
        kit.row_color(row_of("Snare 2"))
    );
    assert_ne!(
        kit.row_color(row_of("Kick")),
        kit.row_color(row_of("Snare"))
    );
    assert_ne!(
        kit.row_color(row_of("HH Closed")),
        kit.row_color(row_of("Kick"))
    );
}

// ── multi tool ───────────────────────────────────────────────────────

use expression_editor_core::multitool::{self, Bend, Capture, Drag as MtDrag, Pt, Steepness, Zone};

fn ramp(n: usize) -> Capture {
    Capture::new(
        (0..n)
            .map(|i| {
                let f = i as f64 / (n - 1) as f64;
                Pt {
                    t: f * 1000.0,
                    value: f,
                }
            })
            .collect(),
    )
    .unwrap()
}

fn plain() -> MtDrag {
    MtDrag {
        amount: 0.0,
        symmetric: false,
    }
}

#[test]
fn a_zero_drag_changes_nothing() {
    let cap = ramp(9);
    for z in Zone::ALL {
        let out = multitool::apply(z, &cap, plain(), Bend::Sine, Steepness::default());
        for (a, b) in cap.points.iter().zip(&out) {
            assert!(
                (a.t - b.t).abs() < 1e-9 && (a.value - b.value).abs() < 1e-9,
                "{z:?} moved something at rest"
            );
        }
    }
}

#[test]
fn scaling_holds_the_opposite_edge_still() {
    let cap = ramp(9);
    let drag = MtDrag {
        amount: -0.5,
        symmetric: false,
    };
    // Grabbing the top and pulling down must shrink toward the bottom,
    // not slide the whole selection.
    let out = multitool::apply(Zone::ScaleTop, &cap, drag, Bend::Sine, Steepness::default());
    assert!(
        (out[0].value - cap.points[0].value).abs() < 1e-6,
        "the pivot edge must not move"
    );
    assert!(out[8].value < cap.points[8].value, "the far edge shrinks");
}

#[test]
fn tilting_hinges_at_the_far_end() {
    let cap = ramp(9);
    let drag = MtDrag {
        amount: 0.5,
        symmetric: false,
    };
    let out = multitool::apply(Zone::TiltLeft, &cap, drag, Bend::Power, Steepness(1.0));
    // A tilt with a fixed far end is a ramp; if both ends move it is
    // just a move.
    assert!(out[0].value > cap.points[0].value, "the grabbed end lifts");
    assert!(
        (out[8].value - cap.points[8].value).abs() < 1e-6,
        "the hinge stays put"
    );
}

#[test]
fn compressing_pulls_toward_the_named_edge() {
    let cap = ramp(9);
    let drag = MtDrag {
        amount: 1.0,
        symmetric: false,
    };
    let top = multitool::apply(Zone::CompressTop, &cap, drag, Bend::Sine, Steepness(2.0));
    let bottom = multitool::apply(Zone::CompressBottom, &cap, drag, Bend::Sine, Steepness(2.0));
    // Full compression at the strong end of the envelope reaches the
    // edge it is named for.
    assert!(top[8].value >= cap.v_hi - 1e-6);
    assert!(bottom[8].value <= cap.v_lo + 1e-6);
}

#[test]
fn stretching_anchors_the_opposite_edge() {
    let cap = ramp(5);
    let drag = MtDrag {
        amount: 1.0,
        symmetric: false,
    };
    let right = multitool::apply(
        Zone::StretchRight,
        &cap,
        drag,
        Bend::Sine,
        Steepness::default(),
    );
    assert!((right[0].t - cap.t0).abs() < 1e-6, "left edge anchored");
    assert!(right[4].t > cap.t1, "right edge extends");
}

#[test]
fn warping_redistributes_without_losing_or_reordering_points() {
    let cap = ramp(11);
    let drag = MtDrag {
        amount: 0.9,
        symmetric: false,
    };
    let out = multitool::apply(Zone::Warp, &cap, drag, Bend::Power, Steepness(2.0));
    assert_eq!(out.len(), cap.points.len());
    // Endpoints pinned, order preserved, values untouched.
    assert!((out[0].t - cap.t0).abs() < 1e-6);
    assert!((out[10].t - cap.t1).abs() < 1e-6);
    for w in out.windows(2) {
        assert!(w[0].t <= w[1].t + 1e-9, "warp must not reorder");
    }
    for (a, b) in cap.points.iter().zip(&out) {
        assert!((a.value - b.value).abs() < 1e-9, "warp moves time only");
    }
}

#[test]
fn steepness_has_a_detent_at_neutral() {
    // Sweeping across zero pauses there, so linear is easy to return
    // to. Without this the control has no findable home.
    let s = Steepness(0.1).nudge(-0.15);
    assert!(s.is_neutral(), "crossing zero should stop at it, got {s:?}");
    // And it does not stick: a decisive move continues past.
    let s = s.nudge(-0.9);
    assert!(!s.is_neutral());
    assert!(s.0 < 0.0);
    // Clamped at the extremes.
    assert_eq!(Steepness(0.0).nudge(99.0).0, Steepness::MAX);
}

#[test]
fn a_neutral_curve_is_the_identity() {
    let s = Steepness::default();
    for bend in [Bend::Sine, Bend::Power] {
        for i in 0..=10 {
            let x = i as f64 / 10.0;
            assert!((s.curve(x, bend) - x).abs() < 1e-9);
        }
    }
}

#[test]
fn every_curve_stays_within_the_unit_box() {
    for bend in [Bend::Sine, Bend::Power] {
        for k in [-4.0, -1.5, 0.0, 1.5, 4.0] {
            let s = Steepness(k);
            assert!(s.curve(0.0, bend).abs() < 1e-6, "{bend:?} {k} at 0");
            assert!((s.curve(1.0, bend) - 1.0).abs() < 1e-6, "{bend:?} {k} at 1");
            for i in 0..=20 {
                let v = s.curve(i as f64 / 20.0, bend);
                assert!(
                    (-1e-9..=1.0 + 1e-9).contains(&v),
                    "{bend:?} {k} escaped: {v}"
                );
            }
        }
    }
}

#[test]
fn wheel_alternatives_do_what_their_labels_say() {
    let cap = ramp(5);
    // Flip absolute mirrors about the range midpoint.
    let flipped = multitool::apply_wheel(Zone::CompressTop, &cap);
    assert!((flipped[0].value - 1.0).abs() < 1e-6);
    assert!(flipped[4].value.abs() < 1e-6);

    // Reverse mirrors positions.
    let reversed = multitool::apply_wheel(Zone::StretchLeft, &cap);
    assert_eq!(reversed.len(), 5);
    assert!((reversed[0].t - cap.t0).abs() < 1e-6);

    // Even out equalises spacing.
    let mut uneven = cap.clone();
    uneven.points[1].t = 10.0;
    let even = multitool::apply_wheel(Zone::Warp, &uneven);
    let gaps: Vec<f64> = even.windows(2).map(|w| w[1].t - w[0].t).collect();
    for g in &gaps {
        assert!((g - gaps[0]).abs() < 1e-6, "spacing must be uniform");
    }
}

#[test]
fn zones_are_hit_testable_and_corners_beat_the_field() {
    // A corner target must not be shadowed by the large zone behind it.
    assert_eq!(multitool::zone_at(0.02, 0.02), Some(Zone::TiltLeft));
    assert_eq!(multitool::zone_at(0.98, 0.98), Some(Zone::Redo));
    assert_eq!(multitool::zone_at(0.5, 0.5), Some(Zone::Warp));
    assert_eq!(multitool::zone_at(0.5, 0.02), Some(Zone::CompressTop));
    // Every zone's own centre resolves to itself.
    for z in Zone::ALL {
        let (x, y, w, h) = multitool::layout(z);
        let hit = multitool::zone_at(x + w * 0.5, y + h * 0.5);
        assert!(hit.is_some(), "{z:?} centre hit nothing");
    }
}

#[test]
fn positional_zones_are_the_ones_safe_across_lanes() {
    // A value transform needs one dimension's range to be meaningful; a
    // positional one does not.
    assert!(Zone::Warp.is_positional());
    assert!(Zone::Move.is_positional());
    assert!(!Zone::CompressTop.is_positional());
    assert!(!Zone::TiltLeft.is_positional());
}

// ── modes ────────────────────────────────────────────────────────────

use expression_editor_core::Mode;

#[test]
fn each_mode_brings_its_own_preset() {
    let mut ed = test_editor();

    ed.set_mode(Mode::Drums);
    assert!(matches!(ed.row_space, RowSpace::Drums(_)));
    assert_eq!(ed.doc.row_space, ed.row_space, "doc and view must agree");
    assert_eq!(ed.mouse.name, "Drums");

    ed.set_mode(Mode::Guitar);
    assert!(matches!(ed.row_space, RowSpace::Strings(_)));
    assert_eq!(ed.mouse.name, "Riffer (Ample)");

    ed.set_mode(Mode::Midi);
    assert!(matches!(ed.row_space, RowSpace::Pitch));
    // Plain MIDI cannot carry per-note pressure, so the active dimension
    // must not be left pointing at it.
    assert_eq!(ed.dimension, Dimension::Pitch);
    assert!(ed.overlays.is_empty());
}

#[test]
fn modes_declare_which_controls_apply() {
    assert!(Mode::Mpe.has_expression_lanes());
    assert!(!Mode::Midi.has_expression_lanes());
    assert!(Mode::Mpe.has_mpe_channels());
    assert!(!Mode::Drums.has_mpe_channels());
    assert!(Mode::Guitar.has_techniques());
    assert!(Mode::Vocals.has_lyrics());
    // The blend controls need a contour to decompose; a plain MIDI
    // note's is flat, so they would do nothing.
    assert!(Mode::PitchedAudio.has_pitch_shape());
    assert!(!Mode::Midi.has_pitch_shape());
    // Tuning targets mean nothing on a drum kit.
    assert!(!Mode::Drums.has_tuning());
    assert!(Mode::Vocals.has_tuning());
}

/// The switcher groups by family, so every mode must land in exactly one
/// and the two lists together must be `Mode::ALL`.
///
/// The invariant is easy to break by hand: adding a mode compiles fine
/// while `ModeFamily::modes` still returns the old list, and the only
/// symptom is a button that never appears.
#[test]
fn every_mode_belongs_to_exactly_one_family() {
    use expression_editor_core::ModeFamily;

    let mut listed: Vec<Mode> = Vec::new();
    for family in ModeFamily::ALL {
        for m in family.modes() {
            assert_eq!(m.family(), family, "{m:?} listed under the wrong family");
            assert!(!listed.contains(m), "{m:?} listed twice");
            listed.push(*m);
        }
    }
    assert_eq!(listed, Mode::ALL.to_vec());

    // The split is where the notes came from, and `is_analysed_audio`
    // must agree with it.
    assert_eq!(ModeFamily::Audio.modes().len(), 2);
    assert!(Mode::PitchedAudio.is_analysed_audio());
    assert!(Mode::UnpitchedAudio.is_analysed_audio());
    assert!(!Mode::Guitar.is_analysed_audio());
    // Only the unpitched one gives up pitch editing.
    assert!(Mode::PitchedAudio.has_pitch());
    assert!(!Mode::UnpitchedAudio.has_pitch());
}

#[test]
fn switching_modes_is_reversible() {
    let mut ed = test_editor();
    ed.set_mode(Mode::Guitar);
    ed.set_mode(Mode::Mpe);
    assert!(matches!(ed.row_space, RowSpace::Pitch));
    assert_eq!(ed.overlays, vec![Dimension::Pitch]);
    // Back to the default map, which is FTS. The claim is reversibility,
    // not which map: leaving Guitar must undo Guitar's preset rather
    // than leaving the riffer bindings behind.
    assert_eq!(ed.mouse.name, "FTS");
}

// ── clipboard and the context menu ───────────────────────────────────

use expression_editor_core::menu::{self, Command};

/// An editor with four notes, each carrying real expression, so a copy
/// that drops curves is distinguishable from one that keeps them.
fn menu_editor() -> Editor {
    let mut doc = doc_with_notes(4);
    for n in doc.notes.iter_mut() {
        n.pitch.set(n.start, -0.5);
        n.pitch.set(n.end, 0.25);
    }
    Editor::new(doc, Viewport::new(900.0, 500.0))
}

#[test]
fn a_copied_note_brings_its_curves_and_its_spacing() {
    let mut ed = menu_editor();
    let picked = vec![NoteId(2), NoteId(3)];
    assert!(ed.clipboard.copy_from(&ed.doc, &picked));

    // Normalized to the earliest note, so the phrase can land anywhere.
    let placed = ed.clipboard.placed(PPQ * 10.0, 60);
    assert_eq!(placed.len(), 2);
    assert!((placed[0].start - PPQ * 10.0).abs() < 1e-9);
    // The gap between the two notes is preserved, not flattened.
    let gap = placed[1].start - placed[0].start;
    assert!((gap - PPQ).abs() < 1e-9, "spacing survived: {gap}");
    // And the curve travelled with the note it belongs to.
    assert!(
        !placed[0].pitch.is_empty(),
        "a copied note is the whole note, not a rectangle"
    );
    assert!(
        (placed[0].pitch.points()[0].t - placed[0].start).abs() < 1e-9,
        "the curve moved with the note rather than staying behind"
    );
}

#[test]
fn pasting_mints_fresh_ids_so_a_double_paste_is_two_phrases() {
    let mut ed = menu_editor();
    ed.clipboard.copy_from(&ed.doc, &[NoteId(1)]);
    let before = ed.doc.notes.len();

    ed.apply(&Edit::PasteNotes(ed.clipboard.placed(PPQ * 8.0, 60)));
    ed.apply(&Edit::PasteNotes(ed.clipboard.placed(PPQ * 12.0, 60)));

    assert_eq!(ed.doc.notes.len(), before + 2);
    let all: Vec<NoteId> = ids(&ed.doc);
    let mut uniq = all.clone();
    uniq.sort_by_key(|i| i.0);
    uniq.dedup();
    assert_eq!(all.len(), uniq.len(), "every pasted note got its own id");
}

#[test]
fn cut_copies_before_it_deletes() {
    let mut ed = menu_editor();
    ed.selection.notes = vec![NoteId(1), NoteId(2)];
    let before = ed.doc.notes.len();

    assert!(ed.run_command(&Command::Cut, None));
    assert_eq!(ed.doc.notes.len(), before - 2);
    assert_eq!(ed.clipboard.len(), 2, "and what was cut is pasteable");

    assert!(ed.run_command(&Command::Paste, None));
    assert_eq!(ed.doc.notes.len(), before);
}

#[test]
fn a_command_on_an_unselected_note_acts_on_that_note() {
    let mut ed = menu_editor();
    ed.selection.notes = vec![NoteId(1)];

    // Right-clicking note 3 while note 1 is selected must not delete
    // note 1 — the classic "menu ate the wrong thing" bug.
    assert_eq!(ed.command_targets(Some(NoteId(3))), vec![NoteId(3)]);
    assert!(ed.run_command(&Command::Delete, Some(NoteId(3))));
    assert!(ed.doc.note(NoteId(1)).is_some());
    assert!(ed.doc.note(NoteId(3)).is_none());

    // But right-clicking *inside* the selection acts on all of it.
    ed.selection.notes = vec![NoteId(1), NoteId(2)];
    assert_eq!(ed.command_targets(Some(NoteId(1))).len(), 2);
}

#[test]
fn a_failed_copy_leaves_the_clipboard_alone() {
    let mut ed = menu_editor();
    ed.clipboard.copy_from(&ed.doc, &[NoteId(1)]);
    assert_eq!(ed.clipboard.len(), 1);

    // Copying nothing must not destroy what was already held.
    assert!(!ed.clipboard.copy_from(&ed.doc, &[]));
    assert_eq!(ed.clipboard.len(), 1);
}

#[test]
fn the_menu_greys_items_out_rather_than_hiding_them() {
    let mut ed = menu_editor();
    ed.selection.clear();
    let empty = menu::note_menu(&ed, None, 0.0);

    let paste = empty.iter().find(|i| i.command == Command::Paste).unwrap();
    assert!(!paste.enabled, "nothing on the clipboard yet");
    let copy = empty.iter().find(|i| i.command == Command::Copy).unwrap();
    assert!(!copy.enabled, "nothing selected");

    ed.selection.notes = vec![NoteId(1)];
    ed.run_command(&Command::Copy, None);
    let ready = menu::note_menu(&ed, None, 0.0);
    assert_eq!(
        ready.len(),
        empty.len(),
        "a menu whose shape moves with the selection is one you cannot learn"
    );
    assert!(
        ready
            .iter()
            .find(|i| i.command == Command::Paste)
            .unwrap()
            .enabled
    );
}

#[test]
fn the_menu_offers_what_the_mode_can_actually_carry() {
    let mut ed = menu_editor();

    ed.set_mode(Mode::Midi);
    let midi = menu::note_menu(&ed, Some(NoteId(1)), 0.0);
    assert!(
        !midi
            .iter()
            .any(|i| matches!(i.command, Command::EditLyric(_)))
    );

    ed.set_mode(Mode::Vocals);
    let vocals = menu::note_menu(&ed, Some(NoteId(1)), 0.0);
    assert!(
        vocals
            .iter()
            .any(|i| matches!(i.command, Command::EditLyric(_)))
    );

    ed.set_mode(Mode::Guitar);
    let guitar = menu::note_menu(&ed, Some(NoteId(1)), 0.0);
    assert!(
        guitar
            .iter()
            .any(|i| matches!(i.command, Command::CycleString(_)))
    );
    assert!(
        guitar
            .iter()
            .any(|i| matches!(i.command, Command::ToggleLegato(_)))
    );

    ed.set_mode(Mode::PitchedAudio);
    let audio = menu::note_menu(&ed, Some(NoteId(1)), 0.0);
    assert!(
        audio
            .iter()
            .any(|i| matches!(i.command, Command::SplitNote(_, _)))
    );

    // Mode-specific items need a concrete note; clicking empty canvas
    // offers only what works without one.
    let nowhere = menu::note_menu(&ed, None, 0.0);
    assert!(
        !nowhere
            .iter()
            .any(|i| matches!(i.command, Command::SplitNote(_, _)))
    );
}

#[test]
fn commands_needing_a_panel_report_that_they_did_not_run() {
    let mut ed = menu_editor();
    ed.set_mode(Mode::Vocals);
    // `EditLyric` now *opens the field* rather than inventing a
    // syllable, so it runs — and still writes nothing, which is the
    // property this test was really protecting.
    assert!(ed.run_command(&Command::EditLyric(NoteId(1)), Some(NoteId(1))));
    assert_eq!(ed.editing_lyric, Some(NoteId(1)));
    assert_eq!(ed.doc.note(NoteId(1)).and_then(|n| n.text.clone()), None);
    // Properties still has no panel to open.
    assert!(!ed.run_command(&Command::Properties, Some(NoteId(1))));
}

#[test]
fn merging_extends_the_survivor_instead_of_rebuilding_it() {
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 8.0);
    let mut a = Note::new(NoteId(1), 0.0, PPQ, 60);
    a.pitch.set(0.0, -0.4);
    doc.push(a);
    doc.push(Note::new(NoteId(2), PPQ, PPQ * 2.0, 60));
    let mut ed = Editor::new(doc, Viewport::new(900.0, 500.0));

    assert!(ed.run_command(&Command::MergeNotes(NoteId(1)), Some(NoteId(1))));
    assert!(ed.doc.note(NoteId(2)).is_none());
    let n = ed.doc.note(NoteId(1)).unwrap();
    assert!((n.end - PPQ * 2.0).abs() < 1e-9, "the survivor covers both");
    assert!(
        !n.pitch.is_empty(),
        "and keeps its own expression rather than being re-derived"
    );
}

#[test]
fn clearing_expression_keeps_the_notes() {
    let mut ed = menu_editor();
    ed.selection.notes = vec![NoteId(1), NoteId(2)];
    let before = ed.doc.notes.len();

    assert!(ed.run_command(&Command::ClearExpression, None));
    assert_eq!(ed.doc.notes.len(), before);
    assert!(ed.doc.note(NoteId(1)).unwrap().pitch.is_empty());
    assert!(
        !ed.doc.note(NoteId(3)).unwrap().pitch.is_empty(),
        "and only the selection"
    );
}

#[test]
fn a_measure_command_reads_the_bar_under_the_pointer() {
    let mut ed = menu_editor();
    // doc_with_notes puts one note per quarter at rows 60.., so bar 1
    // (4 beats at PPQ each) holds all four.
    let in_bar_1 = ed.notes_in_measure(PPQ * 0.5);
    assert_eq!(in_bar_1.len(), 4);

    ed.playhead = Some(PPQ * 0.5);
    assert!(ed.run_command(&Command::CopyMeasure, None));
    assert_eq!(ed.clipboard.len(), 4);
}

// ── multitrack ───────────────────────────────────────────────────────

use expression_editor_core::tracks::{RefColor, Track};

#[test]
fn each_track_keeps_its_own_undo_history() {
    let mut ed = menu_editor();
    let other = doc_with_notes(2);
    let b = ed.add_track("Harmony", other);

    // Edit track A, then switch away and back.
    ed.apply(&Edit::DeleteNotes(vec![NoteId(1)]));
    assert!(ed.can_undo());
    let a_notes = ed.doc.notes.len();

    assert!(ed.switch_track(b));
    assert!(
        !ed.can_undo(),
        "a fresh track starts with a clean history — an undo here would \
         otherwise reach into the track you just left"
    );
    assert_eq!(ed.doc.notes.len(), 2);

    // Edit B, then go back to A: A's history is exactly where it was.
    ed.apply(&Edit::DeleteNotes(vec![NoteId(1)]));
    assert!(ed.switch_track(0));
    assert_eq!(ed.doc.notes.len(), a_notes, "A came back as it was left");
    assert!(ed.can_undo(), "and with its own history intact");
    ed.undo();
    assert_eq!(ed.doc.notes.len(), a_notes + 1);
}

#[test]
fn switching_parks_edits_rather_than_discarding_them() {
    let mut ed = menu_editor();
    let b = ed.add_track("Harmony", doc_with_notes(2));

    ed.apply(&Edit::DeleteNotes(vec![NoteId(1), NoteId(2)]));
    let left_with = ed.doc.notes.len();
    ed.switch_track(b);
    ed.switch_track(0);
    assert_eq!(ed.doc.notes.len(), left_with);
}

#[test]
fn the_view_does_not_move_when_the_track_does() {
    let mut ed = menu_editor();
    let b = ed.add_track("Harmony", doc_with_notes(2));
    ed.zoom_in_at(400.0, 200.0, 1.6);
    let camera = ed.camera;

    ed.switch_track(b);
    assert_eq!(
        ed.camera, camera,
        "switching changes what you edit, not where you are looking"
    );
}

#[test]
fn a_switch_drops_state_that_names_the_old_document() {
    let mut ed = menu_editor();
    let b = ed.add_track("Harmony", doc_with_notes(2));
    ed.selection.notes = vec![NoteId(1), NoteId(2)];
    ed.edit_cc(1);

    ed.switch_track(b);
    assert!(
        ed.selection.is_empty(),
        "note ids are per-document; a carried selection points at strangers"
    );
    assert!(!ed.cc_editing());
}

#[test]
fn the_active_slots_parked_copy_is_never_handed_out() {
    let mut ed = menu_editor();
    let b = ed.add_track("Harmony", doc_with_notes(2));

    assert!(ed.tracks.doc_of(ed.active_track()).is_none());
    assert!(ed.tracks.doc_of(b).is_some());

    // After a switch the refusal follows the active slot.
    ed.switch_track(b);
    assert!(ed.tracks.doc_of(b).is_none());
    assert!(ed.tracks.doc_of(0).is_some());
}

#[test]
fn a_rejected_switch_leaves_the_editor_whole() {
    let mut ed = menu_editor();
    let before = ed.doc.clone();
    ed.apply(&Edit::DeleteNotes(vec![NoteId(1)]));

    assert!(!ed.switch_track(ed.active_track()), "already there");
    assert!(!ed.switch_track(99), "out of range");

    // The guard runs before anything is moved out, so the document and
    // its history both survive a refused switch.
    assert_ne!(ed.doc, before);
    assert!(ed.can_undo());
    ed.undo();
    assert_eq!(ed.doc, before);
}

#[test]
fn only_marked_tracks_are_references_and_never_the_active_one() {
    let mut ed = menu_editor();
    let b = ed.add_track("Harmony", doc_with_notes(2));
    let c = ed.add_track("Bass", doc_with_notes(1));

    assert_eq!(ed.tracks.references().count(), 0);
    ed.tracks.track_mut(b).unwrap().reference = true;
    ed.tracks.track_mut(c).unwrap().reference = true;
    assert_eq!(ed.tracks.references().count(), 2);

    // Switching onto a reference track drops it from the overlay: it is
    // the thing being edited now, not a backdrop.
    ed.switch_track(b);
    let refs: Vec<usize> = ed.tracks.references().map(|(i, _)| i).collect();
    assert_eq!(refs, vec![c]);
}

#[test]
fn removing_a_track_keeps_the_active_index_pointing_at_the_same_track() {
    let mut ed = menu_editor();
    let b = ed.add_track("Harmony", doc_with_notes(2));
    let _c = ed.add_track("Bass", doc_with_notes(1));
    ed.switch_track(b);
    assert_eq!(ed.active_track(), b);

    // Removing a track *before* the active one shifts it down by one.
    assert!(ed.tracks.remove(0));
    assert_eq!(ed.active_track(), b - 1);
    assert_eq!(ed.tracks.len(), 2);

    // And the active track can never be closed from in here.
    assert!(!ed.tracks.remove(ed.active_track()));
}

#[test]
fn a_reference_track_carries_its_own_colour_choice() {
    let mut ed = menu_editor();
    let b = ed.add_track("Harmony", doc_with_notes(2));
    let t = ed.tracks.track_mut(b).unwrap();
    t.reference = true;
    t.ref_color = RefColor::Shadow;
    t.color = Some("#ff8800".into());

    let (_, track) = ed.tracks.references().next().unwrap();
    assert_eq!(track.ref_color, RefColor::Shadow);
    assert_eq!(track.color.as_deref(), Some("#ff8800"));
}

#[test]
fn tracks_are_named_and_findable_in_switcher_order() {
    let mut ed = menu_editor();
    ed.add_track("Harmony", doc_with_notes(2));
    ed.add_track("Bass", doc_with_notes(1));

    assert_eq!(ed.tracks.names(), vec!["Track 1", "Harmony", "Bass"]);
    assert_eq!(ed.tracks.index_of("Bass"), Some(2));
    assert!(ed.tracks.rename(0, "Lead"));
    assert_eq!(ed.tracks.names()[0], "Lead");
    assert_eq!(ed.tracks.index_of("Track 1"), None);
}

#[test]
fn a_pushed_track_starts_with_an_empty_history_of_its_own() {
    let mut ed = menu_editor();
    let doc = doc_with_notes(2);
    let b = ed.tracks.push(Track::new("Harmony", doc));
    ed.switch_track(b);
    assert!(!ed.can_undo());
    assert!(!ed.can_redo());
}

// ── note handles and the temporary note ──────────────────────────────

use expression_editor_core::handles::{self, Handle, HandleDrag, Scope};

/// One note with a scoop in and a vibrato — enough shape that a handle
/// which flattens the contour is distinguishable from one that moves it.
fn handle_editor() -> Editor {
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 8.0);
    let mut n = Note::new(NoteId(1), 0.0, PPQ * 4.0, 60);
    // The note is 4 quarters = 2 s at 120 bpm. Ten cycles across it is
    // 5 Hz, which is real vibrato; anything under the 3 Hz cutoff would
    // be classified as drift and the vibrato handle would not see it.
    for k in 0..256 {
        let f = k as f64 / 255.0;
        let t = n.start + (n.end - n.start) * f;
        let scoop = -1.5 * (1.0 - (f / 0.15_f64).min(1.0)).powi(3);
        let vib = 0.25 * (f * core::f64::consts::TAU * 10.0).sin();
        n.pitch.set(t, scoop + vib);
    }
    doc.push(n);
    let mut ed = Editor::new(doc, Viewport::new(900.0, 500.0));
    ed.snap_pitch = false;
    ed
}

fn sounding(ed: &Editor, t: f64) -> f64 {
    let n = ed.doc.note(NoteId(1)).unwrap();
    n.row as f64 + n.pitch.sample(t, 0.0)
}

fn center_of(ed: &Editor) -> f64 {
    let n = ed.doc.note(NoteId(1)).unwrap();
    n.row as f64
        + expression_editor_core::blob::decompose(
            &n.pitch,
            n.start,
            n.end,
            128,
            ed.doc.time_base.units_per_second(ed.bpm),
            0.0,
        )
        .center
}

fn drag_to(ed: &mut Editor, handle: Handle, scope: Scope, dy: f64) -> HandleDrag {
    let note = ed.doc.note(NoteId(1)).unwrap().clone();
    let mut d = HandleDrag::begin(handle, &note, scope, 250.0);
    ed.begin_gesture();
    ed.drag_handle(&mut d, 250.0 - dy, ed.snap_pitch);
    d
}

#[test]
fn the_pitch_handle_moves_the_contour_without_flattening_it() {
    let mut ed = handle_editor();
    let before_scoop = sounding(&ed, 0.0) - center_of(&ed);
    let before_center = center_of(&ed);

    // Drag up a fifth of the viewport: 24 semitones of range.
    let mut d = drag_to(&mut ed, Handle::Pitch, Scope::Note, 100.0);
    ed.end_handle_drag(&d);
    d.applied = 0.0;

    let after_center = center_of(&ed);
    assert!(
        after_center > before_center + 3.0,
        "the note moved: {before_center} -> {after_center}"
    );
    let after_scoop = sounding(&ed, 0.0) - after_center;
    assert!(
        (after_scoop - before_scoop).abs() < 0.05,
        "the scoop travelled with it rather than being flattened: \
         {before_scoop} -> {after_scoop}"
    );
}

#[test]
fn a_pitch_drag_leaves_the_row_as_the_rounded_centre() {
    let mut ed = handle_editor();
    let d = drag_to(&mut ed, Handle::Pitch, Scope::Note, 100.0);
    ed.end_handle_drag(&d);

    let n = ed.doc.note(NoteId(1)).unwrap();
    let center_offset = expression_editor_core::blob::decompose(
        &n.pitch,
        n.start,
        n.end,
        128,
        ed.doc.time_base.units_per_second(ed.bpm),
        0.0,
    )
    .center;
    assert!(
        center_offset.abs() <= 0.5 + 1e-9,
        "the whole semitones went into the row; the curve keeps only the \
         remainder, got {center_offset}"
    );
}

#[test]
fn coarse_pitch_snaps_and_fine_pitch_does_not() {
    let mut ed = handle_editor();
    ed.snap_pitch = true;
    let d = drag_to(&mut ed, Handle::Pitch, Scope::Note, 37.0);
    ed.end_handle_drag(&d);
    let snapped = center_of(&ed);
    assert!(
        (snapped - snapped.round()).abs() < 0.01,
        "coarse pitch lands on a tuning degree, got {snapped}"
    );

    // The fine handle has to be able to sit between them.
    let mut ed = handle_editor();
    let d = drag_to(&mut ed, Handle::FinePitch, Scope::Note, 37.0);
    ed.end_handle_drag(&d);
    let fine = center_of(&ed);
    assert!(
        (fine - fine.round()).abs() > 0.01,
        "fine pitch is cents-resolution, got {fine}"
    );
}

#[test]
fn a_handle_drag_never_compounds_across_moves() {
    let mut ed = handle_editor();
    let note = ed.doc.note(NoteId(1)).unwrap().clone();
    let mut d = HandleDrag::begin(Handle::FinePitch, &note, Scope::Note, 250.0);
    ed.begin_gesture();

    // Walk the pointer down in steps and back to where it started. A
    // drag that applied deltas instead of rebuilding from the snapshot
    // would have drifted badly by now.
    let before = center_of(&ed);
    for step in 1..=10 {
        ed.drag_handle(&mut d, 250.0 - step as f64 * 5.0, false);
    }
    for step in (0..10).rev() {
        ed.drag_handle(&mut d, 250.0 - step as f64 * 5.0, false);
    }
    ed.drag_handle(&mut d, 250.0, false);

    let after = center_of(&ed);
    assert!(
        (after - before).abs() < 1e-6,
        "back where it started: {before} -> {after}"
    );
}

#[test]
fn the_slope_handles_hinge_on_the_far_end() {
    let mut ed = handle_editor();
    let n = ed.doc.note(NoteId(1)).unwrap();
    let (start, end) = (n.start, n.end);
    let before_end = sounding(&ed, end);
    let before_start = sounding(&ed, start);

    drag_to(&mut ed, Handle::LeftSlope, Scope::Note, 60.0);

    assert!(
        (sounding(&ed, end) - before_end).abs() < 0.01,
        "the far end is the hinge and must not move"
    );
    assert!(
        sounding(&ed, start) > before_start + 0.5,
        "and the near end tilted up: {before_start} -> {}",
        sounding(&ed, start)
    );
}

#[test]
fn the_right_slope_hinges_on_the_start() {
    let mut ed = handle_editor();
    let n = ed.doc.note(NoteId(1)).unwrap();
    let (start, end) = (n.start, n.end);
    let before_start = sounding(&ed, start);
    let before_end_r = sounding(&ed, end);

    drag_to(&mut ed, Handle::RightSlope, Scope::Note, 60.0);

    assert!((sounding(&ed, start) - before_start).abs() < 0.01);
    assert!(sounding(&ed, end) > before_end_r + 0.5);
}

#[test]
fn the_vibrato_handle_changes_depth_and_leaves_the_centre() {
    let mut ed = handle_editor();
    let n = ed.doc.note(NoteId(1)).unwrap();
    let (start, end) = (n.start, n.end);
    let before_center = center_of(&ed);
    let before_depth = expression_editor_core::blob::decompose(
        &ed.doc.note(NoteId(1)).unwrap().pitch,
        start,
        end,
        128,
        ed.doc.time_base.units_per_second(ed.bpm),
        0.0,
    )
    .modulation_depth();

    // Drag down: flatten the vibrato toward robotic.
    drag_to(&mut ed, Handle::Vibrato, Scope::Note, -120.0);

    let after_depth = expression_editor_core::blob::decompose(
        &ed.doc.note(NoteId(1)).unwrap().pitch,
        start,
        end,
        128,
        ed.doc.time_base.units_per_second(ed.bpm),
        0.0,
    )
    .modulation_depth();
    assert!(
        after_depth < before_depth * 0.8,
        "vibrato shallowed: {before_depth} -> {after_depth}"
    );
    assert!(
        (center_of(&ed) - before_center).abs() < 0.05,
        "and the note did not move"
    );
}

#[test]
fn the_trim_handles_set_a_level_rather_than_a_contour() {
    let mut ed = handle_editor();
    drag_to(&mut ed, Handle::Amplitude, Scope::Note, 60.0);

    let n = ed.doc.note(NoteId(1)).unwrap();
    let a = n
        .pressure
        .sample(n.start + 1.0, Dimension::Pressure.default_value());
    let b = n
        .pressure
        .sample(n.end - 1.0, Dimension::Pressure.default_value());
    assert!((a - b).abs() < 1e-6, "flat across the note: {a} vs {b}");
    assert!(a > Dimension::Pressure.default_value());
}

// ── the temporary note ───────────────────────────────────────────────

#[test]
fn a_temporary_note_scopes_the_handles_to_a_range() {
    let mut ed = handle_editor();
    let n = ed.doc.note(NoteId(1)).unwrap();
    let (start, end) = (n.start, n.end);
    let quarter = start + (end - start) * 0.25;
    let half = start + (end - start) * 0.5;
    let before_late = sounding(&ed, end - 1.0);

    assert!(ed.set_temp_note(NoteId(1), quarter, half));
    assert_eq!(
        ed.scope_for(NoteId(1)),
        Scope::Range {
            t0: quarter,
            t1: half
        }
    );

    let scope = ed.scope_for(NoteId(1));
    drag_to(&mut ed, Handle::FinePitch, scope, 100.0);

    assert!(
        sounding(&ed, (quarter + half) * 0.5) > before_late,
        "inside the range moved"
    );
    assert!(
        (sounding(&ed, end - 1.0) - before_late).abs() < 0.05,
        "and outside it did not"
    );
}

#[test]
fn drawing_a_new_range_discards_the_previous_one() {
    let mut ed = handle_editor();
    assert!(ed.set_temp_note(NoteId(1), 0.0, PPQ));
    assert!(ed.set_temp_note(NoteId(1), PPQ * 2.0, PPQ * 3.0));
    assert_eq!(
        ed.temp_note,
        Some((NoteId(1), PPQ * 2.0, PPQ * 3.0)),
        "there is only ever one temporary note"
    );
    ed.clear_temp_note();
    assert_eq!(ed.scope_for(NoteId(1)), Scope::Note);
}

#[test]
fn a_temporary_note_is_clipped_to_its_note_and_a_stray_click_is_refused() {
    let mut ed = handle_editor();
    // Past both edges: clipped, not rejected.
    assert!(ed.set_temp_note(NoteId(1), -PPQ, PPQ * 99.0));
    assert_eq!(ed.temp_note, Some((NoteId(1), 0.0, PPQ * 4.0)));

    // Too narrow to see: refused, so no invisible scope is left armed.
    ed.clear_temp_note();
    assert!(!ed.set_temp_note(NoteId(1), PPQ, PPQ + 0.001));
    assert_eq!(ed.temp_note, None);
}

#[test]
fn a_temporary_note_belongs_to_one_note_only() {
    let mut ed = handle_editor();
    ed.doc.push(Note::new(NoteId(2), PPQ * 5.0, PPQ * 6.0, 62));
    ed.set_temp_note(NoteId(1), 0.0, PPQ);

    assert!(matches!(ed.scope_for(NoteId(1)), Scope::Range { .. }));
    assert_eq!(
        ed.scope_for(NoteId(2)),
        Scope::Note,
        "another note is unaffected by a range open on this one"
    );
}

// ── layout ───────────────────────────────────────────────────────────

#[test]
fn a_wide_note_carries_all_seven_handles() {
    let rects = handles::layout(100.0, 200.0, 200.0, 20.0);
    assert_eq!(rects.len(), 7);
    for h in Handle::ALL {
        assert!(rects.iter().any(|r| r.handle == h), "{h:?} is laid out");
    }
    // The strips sit above and below rather than on the body, so the
    // body stays one large target.
    let body = rects.iter().find(|r| r.handle == Handle::Pitch).unwrap();
    let fine = rects
        .iter()
        .find(|r| r.handle == Handle::FinePitch)
        .unwrap();
    assert!(fine.y + fine.h <= body.y + 1e-9);
}

#[test]
fn a_narrow_note_drops_handles_rather_than_shrinking_them() {
    // Six three-pixel targets on a 32nd note is worse than none.
    let mid = handles::layout(0.0, 0.0, 30.0, 20.0);
    assert_eq!(mid.len(), 3);
    assert!(mid.iter().any(|r| r.handle == Handle::FinePitch));
    assert!(mid.iter().any(|r| r.handle == Handle::Amplitude));
    assert!(!mid.iter().any(|r| r.handle == Handle::Vibrato));

    let tiny = handles::layout(0.0, 0.0, 10.0, 20.0);
    assert_eq!(tiny.len(), 1, "the body handle always survives");
    assert_eq!(tiny[0].handle, Handle::Pitch);
}

#[test]
fn a_strip_handle_outranks_the_body_it_overlaps() {
    let rects = handles::layout(100.0, 200.0, 200.0, 20.0);
    // In the band above the note: fine pitch, not the body behind it.
    assert_eq!(
        handles::hit(&rects, 200.0, 200.0 - handles::STRIP_H * 0.5),
        Some(Handle::FinePitch)
    );
    // In the body: the body.
    assert_eq!(handles::hit(&rects, 200.0, 210.0), Some(Handle::Pitch));
    // The corners are where the manual puts them.
    assert_eq!(
        handles::hit(&rects, 110.0, 200.0 - 2.0),
        Some(Handle::LeftSlope)
    );
    assert_eq!(
        handles::hit(&rects, 290.0, 220.0 + 2.0),
        Some(Handle::Vibrato)
    );
    assert_eq!(handles::hit(&rects, 500.0, 500.0), None);
}

// ── pitch drawing ────────────────────────────────────────────────────

use expression_editor_core::draft::{self, PitchDraft};

fn draft_editor() -> Editor {
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 8.0);
    let mut n = Note::new(NoteId(1), 0.0, PPQ * 4.0, 60);
    for k in 0..32 {
        let f = k as f64 / 31.0;
        n.pitch.set(n.start + (n.end - n.start) * f, -0.3);
    }
    doc.push(n);
    Editor::new(doc, Viewport::new(900.0, 500.0))
}

#[test]
fn a_drawn_line_eases_rather_than_ramping() {
    // The shape is the point: a voice accelerates out of one pitch and
    // decelerates into the next. A linear ramp arrives at full speed
    // and stops dead, which is what a synthesiser glide sounds like.
    assert!((draft::sine_ease(0.0)).abs() < 1e-12);
    assert!((draft::sine_ease(1.0) - 1.0).abs() < 1e-12);
    assert!((draft::sine_ease(0.5) - 0.5).abs() < 1e-12);
    // Flat at the ends, steep in the middle — the opposite of linear.
    let near_start = draft::sine_ease(0.1);
    let near_mid = draft::sine_ease(0.55) - draft::sine_ease(0.45);
    assert!(near_start < 0.1, "eases in, got {near_start}");
    assert!(near_mid > 0.1, "steepest at the middle, got {near_mid}");
}

#[test]
fn the_drawing_runs_through_its_anchors() {
    let ed = draft_editor();
    let mut d = PitchDraft::open(&ed.doc, NoteId(1)).unwrap();
    d.add(0.0, -1.0);
    d.add(PPQ * 2.0, 1.0);
    d.add(PPQ * 4.0, 0.0);

    let curve = draft::as_curve(&d.rendered(0.0, PPQ * 4.0));
    for (t, want) in [(0.0, -1.0), (PPQ * 2.0, 1.0), (PPQ * 4.0, 0.0)] {
        let got = curve.sample(t, 0.0);
        assert!(
            (got - want).abs() < 0.02,
            "at {t}: wanted {want}, got {got}"
        );
    }
}

#[test]
fn an_anchor_in_an_unvoiced_region_is_allowed() {
    // Forbidding it would break dragging *through* a consonant, and the
    // anchor still shapes the voiced line either side of it.
    let mut ed = draft_editor();
    ed.doc.unvoiced = vec![(PPQ, PPQ * 2.0)];
    let mut d = PitchDraft::open(&ed.doc, NoteId(1)).unwrap();
    d.add(0.0, 0.0);
    d.add(PPQ * 1.5, 2.0);
    d.add(PPQ * 4.0, 0.0);
    assert_eq!(d.anchors().len(), 3);
    assert!(!d.rendered(0.0, PPQ * 4.0).is_empty());
}

#[test]
fn the_draft_has_its_own_undo_and_the_document_sees_none_of_it() {
    let mut ed = draft_editor();
    let before = ed.doc.note(NoteId(1)).unwrap().pitch.clone();
    let mut d = PitchDraft::open(&ed.doc, NoteId(1)).unwrap();

    d.add(0.0, -1.0);
    d.add(PPQ * 2.0, 1.0);
    d.add(PPQ * 4.0, 0.5);
    assert!(d.can_undo());

    // Previews are applied live, so the document changes — but never
    // records, so the history is untouched.
    ed.preview_draft(&mut d);
    assert!(!ed.can_undo(), "a preview is not an undo step");

    d.undo();
    d.undo();
    assert_eq!(d.anchors().len(), 1);
    assert!(d.can_redo());
    d.redo();
    assert_eq!(d.anchors().len(), 2);

    // Dismiss puts back exactly what was captured.
    ed.dismiss_draft(&d);
    assert_eq!(ed.doc.note(NoteId(1)).unwrap().pitch, before);
}

#[test]
fn applying_a_whole_drawing_is_one_step_of_history() {
    let mut ed = draft_editor();
    let before = ed.doc.note(NoteId(1)).unwrap().pitch.clone();
    let mut d = PitchDraft::open(&ed.doc, NoteId(1)).unwrap();

    // A long session: many anchors, moves and undos.
    for k in 0..8 {
        d.add(PPQ * 0.5 * k as f64, (k as f64 * 0.3).sin());
    }
    d.begin_move();
    d.move_to(3, PPQ * 1.6, 1.2);
    d.undo();
    d.add(PPQ * 3.7, -0.8);
    ed.preview_draft(&mut d);

    // Commit through the editor, which rewinds the live preview first
    // so history snapshots the state before drawing began.
    assert!(ed.apply_draft(&d));
    let drawn = ed.doc.note(NoteId(1)).unwrap().pitch.clone();
    assert_ne!(drawn, before);

    // One undo takes the entire drawing, not its last anchor.
    assert!(ed.undo());
    assert_eq!(ed.doc.note(NoteId(1)).unwrap().pitch, before);
    assert!(!ed.can_undo());
}

#[test]
fn the_original_stays_available_for_the_whole_session() {
    let ed = draft_editor();
    let original: Vec<_> = ed.doc.note(NoteId(1)).unwrap().pitch.points().to_vec();
    let mut d = PitchDraft::open(&ed.doc, NoteId(1)).unwrap();
    d.add(0.0, 3.0);
    d.add(PPQ * 4.0, -3.0);
    d.undo();
    d.add(PPQ * 2.0, 1.0);

    assert_eq!(
        d.original(),
        original.as_slice(),
        "the thin line underneath is what was there before drawing began, \
         and no amount of drawing changes it"
    );
}

#[test]
fn a_drawing_that_covers_half_a_note_leaves_the_rest_alone() {
    let mut ed = draft_editor();
    let mut d = PitchDraft::open(&ed.doc, NoteId(1)).unwrap();
    d.add(0.0, 2.0);
    d.add(PPQ * 1.5, 2.0);
    ed.apply_draft(&d);

    let n = ed.doc.note(NoteId(1)).unwrap();
    assert!((n.pitch.sample(PPQ * 0.7, 0.0) - 2.0).abs() < 0.05, "drawn");
    assert!(
        (n.pitch.sample(PPQ * 3.0, 0.0) - -0.3).abs() < 0.05,
        "and the half that was sung is still as it was sung"
    );
}

#[test]
fn dragging_an_anchor_past_its_neighbour_reorders_rather_than_inverting() {
    let ed = draft_editor();
    let mut d = PitchDraft::open(&ed.doc, NoteId(1)).unwrap();
    d.add(0.0, 0.0);
    d.add(PPQ, 1.0);
    d.add(PPQ * 2.0, 2.0);

    d.begin_move();
    d.move_to(0, PPQ * 3.0, -1.0);

    let times: Vec<f64> = d.anchors().iter().map(|a| a.t).collect();
    let mut sorted = times.clone();
    sorted.sort_by(f64::total_cmp);
    assert_eq!(times, sorted, "anchors stay in time order");
}

#[test]
fn a_drag_is_one_draft_step_however_far_the_pointer_travels() {
    let ed = draft_editor();
    let mut d = PitchDraft::open(&ed.doc, NoteId(1)).unwrap();
    d.add(0.0, 0.0);
    d.add(PPQ * 2.0, 0.0);
    let depth = d.anchors().to_vec();

    d.begin_move();
    for k in 1..=20 {
        d.move_to(1, PPQ * 2.0, k as f64 * 0.1);
    }
    // One undo returns to before the drag, not to its previous frame.
    assert!(d.undo());
    assert_eq!(d.anchors(), depth.as_slice());
}

#[test]
fn an_empty_draft_writes_nothing() {
    let ed = draft_editor();
    let mut d = PitchDraft::open(&ed.doc, NoteId(1)).unwrap();
    assert!(d.is_empty());
    assert!(d.apply_edit().is_none());
    assert!(d.cancel_edit().is_none());
    assert!(d.preview_edits().is_empty());
}

#[test]
fn an_anchor_is_grabbed_by_proximity_in_time() {
    let ed = draft_editor();
    let mut d = PitchDraft::open(&ed.doc, NoteId(1)).unwrap();
    d.add(PPQ, 0.0);
    d.add(PPQ * 3.0, 0.0);

    assert_eq!(d.anchor_at(PPQ * 1.05, PPQ * 0.2), Some(0));
    assert_eq!(d.anchor_at(PPQ * 2.9, PPQ * 0.2), Some(1));
    assert_eq!(d.anchor_at(PPQ * 2.0, PPQ * 0.2), None);
    // Between two in range, the nearer one wins.
    assert_eq!(d.anchor_at(PPQ * 1.4, PPQ * 5.0), Some(0));
}

// ── timing separators ────────────────────────────────────────────────

use expression_editor_core::timing::{self, StretchLaw};

/// Three abutting notes, one bar each.
fn timing_editor() -> Editor {
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 12.0);
    for i in 0..3u64 {
        let s = PPQ * 4.0 * i as f64;
        doc.push(Note::new(NoteId(i + 1), s, s + PPQ * 4.0, 60 + i as i32));
    }
    Editor::new(doc, Viewport::new(900.0, 500.0))
}

fn span(ed: &Editor, id: u64) -> (f64, f64) {
    let n = ed.doc.note(NoteId(id)).unwrap();
    (n.start, n.end)
}

#[test]
fn separators_sit_only_where_notes_actually_meet() {
    let ed = timing_editor();
    let seps = timing::separators(&ed.doc, 1.0);
    assert_eq!(seps.len(), 2);
    assert!((seps[0].t - PPQ * 4.0).abs() < 1e-9);
    assert_eq!(seps[0].left, Some(NoteId(1)));
    assert_eq!(seps[0].right, Some(NoteId(2)));

    // A gap of silence is not a draggable boundary: there is nothing on
    // one side of it whose length would change.
    let mut ed = timing_editor();
    ed.doc.note_mut(NoteId(2)).unwrap().start = PPQ * 6.0;
    assert_eq!(timing::separators(&ed.doc, 1.0).len(), 1);
}

#[test]
fn grabbing_above_the_tick_stretches_left_and_slides_the_rest() {
    let mut ed = timing_editor();
    let sep = timing::separators(&ed.doc, 1.0)[0];
    assert_eq!(
        StretchLaw::at(10.0, 100.0),
        StretchLaw::LeftStretchRightMoves
    );

    for e in timing::plan(&ed.doc, sep, PPQ * 5.0, StretchLaw::LeftStretchRightMoves) {
        ed.apply(&e);
    }

    assert_eq!(span(&ed, 1), (0.0, PPQ * 5.0), "the left note grew");
    assert_eq!(
        span(&ed, 2),
        (PPQ * 5.0, PPQ * 9.0),
        "the right note kept its length and slid"
    );
    assert_eq!(
        span(&ed, 3),
        (PPQ * 9.0, PPQ * 13.0),
        "and so did everything after it"
    );
}

#[test]
fn grabbing_below_the_tick_stretches_both_sides() {
    let mut ed = timing_editor();
    let sep = timing::separators(&ed.doc, 1.0)[0];
    assert_eq!(StretchLaw::at(90.0, 100.0), StretchLaw::BothStretch);

    for e in timing::plan(&ed.doc, sep, PPQ * 5.0, StretchLaw::BothStretch) {
        ed.apply(&e);
    }

    assert_eq!(span(&ed, 1), (0.0, PPQ * 5.0));
    assert_eq!(
        span(&ed, 2),
        (PPQ * 5.0, PPQ * 8.0),
        "the right note absorbed the change instead of moving"
    );
    assert_eq!(
        span(&ed, 3),
        (PPQ * 8.0, PPQ * 12.0),
        "so nothing after the pair moved at all"
    );
}

#[test]
fn a_stretch_past_the_limits_refuses_rather_than_degrading() {
    let ed = timing_editor();
    let sep = timing::separators(&ed.doc, 1.0)[0];

    // Four times is the ceiling: the left note is 4 beats, so a
    // boundary at 16 beats is exactly 4x and allowed...
    assert!(!timing::plan(&ed.doc, sep, PPQ * 16.0, StretchLaw::LeftStretchRightMoves).is_empty());
    // ...and anything beyond it produces nothing at all.
    assert!(timing::plan(&ed.doc, sep, PPQ * 17.0, StretchLaw::LeftStretchRightMoves).is_empty());

    // An eighth is the floor.
    assert!(!timing::plan(&ed.doc, sep, PPQ * 0.5, StretchLaw::LeftStretchRightMoves).is_empty());
    assert!(timing::plan(&ed.doc, sep, PPQ * 0.4, StretchLaw::LeftStretchRightMoves).is_empty());
}

#[test]
fn both_stretch_refuses_when_only_the_far_side_would_break() {
    let mut ed = timing_editor();
    // Make the right note short, so a modest drag over-compresses it
    // even though the left side is well within range.
    ed.doc.note_mut(NoteId(2)).unwrap().end = PPQ * 4.0 + PPQ * 0.5;
    ed.doc.note_mut(NoteId(3)).unwrap().start = PPQ * 4.5;
    let sep = timing::separators(&ed.doc, 1.0)[0];

    let edits = timing::plan(&ed.doc, sep, PPQ * 4.49, StretchLaw::BothStretch);
    assert!(
        edits.is_empty(),
        "the whole gesture refuses; a half-applied stretch would leave \
         the left side moved and the right side not"
    );
}

#[test]
fn a_boundary_reports_how_far_off_the_beat_it_is() {
    let step = PPQ;
    assert!(timing::beat_deviation(PPQ * 2.0, 0.0, step).abs() < 1e-9);
    assert!((timing::beat_deviation(PPQ * 2.25, 0.0, step) - PPQ * 0.25).abs() < 1e-9);
    // Past halfway it reads as early against the *next* beat, not late
    // against the last one.
    assert!(timing::beat_deviation(PPQ * 2.75, 0.0, step) < 0.0);
}

#[test]
fn double_clicking_a_boundary_puts_it_on_the_beat() {
    assert!((timing::snap_to_beat(PPQ * 2.3, 0.0, PPQ) - PPQ * 2.0).abs() < 1e-9);
    assert!((timing::snap_to_beat(PPQ * 2.7, 0.0, PPQ) - PPQ * 3.0).abs() < 1e-9);
    // A disabled grid leaves it where it is rather than snapping to zero.
    assert!((timing::snap_to_beat(PPQ * 2.3, 0.0, 0.0) - PPQ * 2.3).abs() < 1e-9);
}

// ── MIDI reference ───────────────────────────────────────────────────

use expression_editor_core::reference::{self, MidiReference, RefNote, SnapSource};

fn reference() -> MidiReference {
    MidiReference::new(
        "Vocal.mid",
        vec!["Track 1".into(), "Track 2".into()],
        vec![
            RefNote {
                start: 0.0,
                end: PPQ * 2.0,
                row: 60,
            },
            RefNote {
                start: PPQ * 2.0,
                end: PPQ * 4.0,
                row: 64,
            },
            RefNote {
                start: PPQ * 4.0,
                end: PPQ * 6.0,
                row: 67,
            },
        ],
    )
}

#[test]
fn the_reference_answers_what_is_sounding_now() {
    let r = reference();
    assert_eq!(r.at(PPQ, 60.0).map(|n| n.row), Some(60));
    assert_eq!(r.at(PPQ * 3.0, 60.0).map(|n| n.row), Some(64));
    assert_eq!(
        r.at(PPQ * 20.0, 60.0),
        None,
        "nothing sounding is not the same as the nearest note"
    );
}

#[test]
fn transposing_is_non_destructive() {
    let mut r = reference();
    r.transpose = 3;
    assert_eq!(r.at(PPQ, 60.0).map(|n| n.row), Some(63));
    r.transpose = 0;
    assert_eq!(
        r.at(PPQ, 60.0).map(|n| n.row),
        Some(60),
        "nudged into key and back out with no accumulated error"
    );
}

#[test]
fn a_chord_in_the_reference_tunes_each_voice_to_its_own_part() {
    let r = MidiReference::new(
        "chord.mid",
        vec!["Track 1".into()],
        vec![
            RefNote {
                start: 0.0,
                end: PPQ,
                row: 60,
            },
            RefNote {
                start: 0.0,
                end: PPQ,
                row: 64,
            },
            RefNote {
                start: 0.0,
                end: PPQ,
                row: 67,
            },
        ],
    );
    // A singer near the fifth must not be dragged to the root just
    // because the root is listed first.
    assert_eq!(r.at(PPQ * 0.5, 66.6).map(|n| n.row), Some(67));
    assert_eq!(r.at(PPQ * 0.5, 60.2).map(|n| n.row), Some(60));
}

#[test]
fn the_scale_and_the_reference_present_the_same_way() {
    let tuning = Tuning::default();
    let r = reference();

    // Both answer "what should this be?", and the caller cannot tell
    // which kind it holds.
    let sources = [SnapSource::Tuning(&tuning), SnapSource::Reference(&r)];
    for s in sources {
        assert!(s.is_available());
        assert!(s.target(PPQ, 60.3).is_some());
    }

    // Only the reference is time-dependent, which is the one real
    // difference between them.
    assert_eq!(
        SnapSource::Tuning(&tuning).target(PPQ * 99.0, 60.3),
        SnapSource::Tuning(&tuning).target(PPQ, 60.3)
    );
    assert!(SnapSource::Reference(&r).target(PPQ * 99.0, 60.3).is_none());
}

#[test]
fn an_empty_reference_is_not_available_as_a_target() {
    let empty = MidiReference::default();
    assert!(!SnapSource::Reference(&empty).is_available());
    assert!(SnapSource::Reference(&empty).target(0.0, 60.0).is_none());
}

#[test]
fn correction_blends_rather_than_pinning() {
    let r = reference();
    let src = SnapSource::Reference(&r);
    // Sung a semitone under the reference's E.
    let notes = [(NoteId(1), PPQ * 3.0, 63.0)];

    let half = reference::plan_corrections(src, notes.iter().copied(), 0.5);
    assert_eq!(half.len(), 1);
    assert!((half[0].delta - 0.5).abs() < 1e-9, "halfway there");

    let full = reference::plan_corrections(src, notes.iter().copied(), 1.0);
    assert!((full[0].delta - 1.0).abs() < 1e-9);

    // Zero is a no-op, not a correction of zero.
    assert!(reference::plan_corrections(src, notes.iter().copied(), 0.0).is_empty());
}

#[test]
fn a_note_with_nothing_to_tune_to_is_left_alone() {
    let r = reference();
    let src = SnapSource::Reference(&r);
    // Well past the end of the reference part.
    let notes = [(NoteId(1), PPQ * 50.0, 63.0)];
    assert!(
        reference::plan_corrections(src, notes.iter().copied(), 1.0).is_empty(),
        "inventing a target would drag the note to a bar it does not \
         belong to"
    );
}

#[test]
fn selecting_another_track_swaps_the_notes() {
    let mut r = reference();
    assert!(r.set_track(
        1,
        vec![RefNote {
            start: 0.0,
            end: PPQ,
            row: 72
        }]
    ));
    assert_eq!(r.active, 1);
    assert_eq!(r.at(PPQ * 0.5, 60.0).map(|n| n.row), Some(72));
    assert!(!r.set_track(9, Vec::new()), "no such track in the file");
}

// ── Curve shapes ─────────────────────────────────────────────────────
//
// Shapes exist for envelopes (#186). The bar they have to clear is that
// adding them changes nothing for the MIDI and CC curves that were here
// first — hence the default-is-linear tests as well as the shape ones.

use expression_editor_core::CurveShape;

fn two(a: Point, b: Point) -> Curve {
    Curve::from_points(vec![a, b])
}

#[test]
fn a_point_is_linear_unless_told_otherwise() {
    let p = Point::new(0.0, 1.0);
    assert_eq!(p.shape, CurveShape::Linear);
    assert_eq!(p.tension, 0.0);
    assert_eq!(Point::default().shape, CurveShape::Linear);
}

#[test]
fn a_linear_segment_interpolates_exactly_as_before() {
    let c = two(Point::new(0.0, 0.0), Point::new(10.0, 10.0));
    for t in [0.0, 2.5, 5.0, 7.5, 10.0] {
        assert!(
            (c.sample(t, 0.0) - t).abs() < 1e-9,
            "linear is unchanged at {t}"
        );
    }
}

#[test]
fn a_square_segment_holds_then_jumps() {
    let c = two(
        Point::shaped(0.0, 0.0, CurveShape::Square),
        Point::new(10.0, 1.0),
    );
    assert_eq!(c.sample(0.0, 0.0), 0.0);
    assert_eq!(c.sample(5.0, 0.0), 0.0, "holds across the segment");
    assert_eq!(c.sample(9.999, 0.0), 0.0, "still holding just before");
    assert_eq!(c.sample(10.0, 0.0), 1.0, "and jumps at the point");
}

#[test]
fn every_shape_starts_and_ends_where_the_points_say() {
    // Whatever happens in between, a segment must be pinned at both
    // ends — otherwise a shape change silently moves authored values.
    for shape in [
        CurveShape::Linear,
        CurveShape::Square,
        CurveShape::SlowStartEnd,
        CurveShape::FastStart,
        CurveShape::FastEnd,
        CurveShape::Bezier,
    ] {
        let c = two(Point::shaped(0.0, 3.0, shape), Point::new(8.0, 9.0));
        assert!((c.sample(0.0, 0.0) - 3.0).abs() < 1e-9, "{shape:?} start");
        assert!((c.sample(8.0, 0.0) - 9.0).abs() < 1e-9, "{shape:?} end");
    }
}

#[test]
fn the_eases_bend_the_way_their_names_say() {
    let mid = |shape| two(Point::shaped(0.0, 0.0, shape), Point::new(10.0, 1.0)).sample(5.0, 0.0);
    let linear = mid(CurveShape::Linear);
    assert!((linear - 0.5).abs() < 1e-9);
    // S-curve is symmetric, so it also passes through the middle.
    assert!((mid(CurveShape::SlowStartEnd) - 0.5).abs() < 1e-9);
    assert!(
        mid(CurveShape::FastStart) > linear,
        "fast start is ahead by halfway"
    );
    assert!(
        mid(CurveShape::FastEnd) < linear,
        "fast end is behind by halfway"
    );
}

#[test]
fn bezier_tension_zero_is_linear_and_the_sign_picks_a_direction() {
    let at =
        |tension| two(Point::bezier(0.0, 0.0, tension), Point::new(10.0, 1.0)).sample(5.0, 0.0);
    assert!((at(0.0) - 0.5).abs() < 1e-9, "no tension, no bend");
    assert!(at(1.0) > 0.5, "positive tension runs ahead");
    assert!(at(-1.0) < 0.5, "negative tension holds back");
}

#[test]
fn tension_is_clamped_to_what_the_daw_can_express() {
    assert_eq!(Point::bezier(0.0, 0.0, 5.0).tension, 1.0);
    assert_eq!(Point::bezier(0.0, 0.0, -5.0).tension, -1.0);
}

#[test]
fn the_left_point_owns_the_segment() {
    // A point describes how the curve *leaves* it, which is how the DAW
    // stores it. Setting the shape on the right-hand point must not
    // change the segment before it.
    let c = two(
        Point::new(0.0, 0.0),
        Point::shaped(10.0, 1.0, CurveShape::Square),
    );
    assert!(
        (c.sample(5.0, 0.0) - 0.5).abs() < 1e-9,
        "the trailing point's shape is not read"
    );
}

// ── the three operations that had no way in ──────────────────────────

#[test]
fn a_lyric_can_be_typed_and_cleared() {
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 4.0);
    doc.push(Note::new(NoteId(1), 0.0, PPQ, 60));
    let mut ed = Editor::new(doc, Viewport::new(800.0, 400.0));

    assert!(ed.set_lyric(NoteId(1), "ho"));
    assert_eq!(ed.doc.note(NoteId(1)).unwrap().text.as_deref(), Some("ho"));

    // Blank clears rather than storing an empty syllable — deleting is
    // the same gesture as typing.
    assert!(ed.set_lyric(NoteId(1), "   "));
    assert_eq!(ed.doc.note(NoteId(1)).unwrap().text, None);
}

#[test]
fn opening_the_lyric_field_does_not_write_anything() {
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 4.0);
    doc.push(Note::new(NoteId(1), 0.0, PPQ, 60));
    let mut ed = Editor::new(doc, Viewport::new(800.0, 400.0));

    assert!(ed.run_command(
        &expression_editor_core::menu::Command::EditLyric(NoteId(1)),
        None
    ));
    assert_eq!(ed.editing_lyric, Some(NoteId(1)));
    assert_eq!(ed.doc.note(NoteId(1)).unwrap().text, None);

    ed.cancel_lyric();
    assert_eq!(ed.editing_lyric, None);
}

#[test]
fn setting_a_fret_transposes_and_keeps_the_string() {
    let tuning = StringTuning::guitar_standard();
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 4.0);
    doc.row_space = RowSpace::Strings(tuning.clone());
    let mut n = Note::new(NoteId(1), 0.0, PPQ, tuning.open(2) + 5);
    n.string = Some(2);
    doc.push(n);

    let mut ed = Editor::new(doc, Viewport::new(800.0, 400.0));
    ed.selection.set_single(NoteId(1));
    assert!(ed.set_fret_of_selection(7));

    let n = ed.doc.note(NoteId(1)).unwrap();
    assert_eq!(n.string, Some(2), "the hand stayed on the same string");
    assert_eq!(n.row, tuning.open(2) + 7, "and the note sounds higher");
}

#[test]
fn moving_a_band_split_resorts_slices_without_reanalysing() {
    use expression_editor_core::rows::SliceBands;

    let bands = SliceBands::default();
    let mut doc = ExpressionDoc::new(TimeBase::Frames { frame_rate: 100.0 }, 0.0, 100.0);
    doc.row_space = RowSpace::Bands(bands.clone());
    // One slice just under the first split, so a small move of that
    // split is enough to reband it.
    let split = bands.splits[0];
    let mut n = Note::new(NoteId(1), 0.0, 4.0, bands.band_of(split * 0.9) as i32);
    n.centroid_hz = Some(split * 0.9);
    let before = n.row;
    doc.push(n);

    let mut ed = Editor::new(doc, Viewport::new(800.0, 400.0));
    assert!(ed.move_band_split(0, split * 0.5));

    let n = ed.doc.note(NoteId(1)).unwrap();
    assert_ne!(n.row, before, "the slice moved to the band above");
    assert_eq!(
        n.centroid_hz,
        Some(split * 0.9),
        "rebanding is a re-sort: the measured centroid is untouched"
    );
}

#[test]
fn a_slice_with_no_centroid_stays_put_when_bands_move() {
    use expression_editor_core::rows::SliceBands;

    let bands = SliceBands::default();
    let mut doc = ExpressionDoc::new(TimeBase::Frames { frame_rate: 100.0 }, 0.0, 100.0);
    doc.row_space = RowSpace::Bands(bands.clone());
    // A hand-drawn slice has no measured centroid; it must not collapse
    // onto band zero the moment somebody drags a split.
    doc.push(Note::new(NoteId(1), 0.0, 4.0, 2));

    let mut ed = Editor::new(doc, Viewport::new(800.0, 400.0));
    assert!(ed.move_band_split(0, bands.splits[0] * 0.5));
    assert_eq!(ed.doc.note(NoteId(1)).unwrap().row, 2);
}

#[test]
fn undo_brings_the_row_space_view_back_with_the_document() {
    // `SetBands` changes the row space itself. Undo restored the
    // document and left the editor's view on the new splits, so the
    // roll drew one thing while every later edit resorted against
    // another.
    use expression_editor_core::rows::SliceBands;

    let bands = SliceBands::default();
    let mut doc = ExpressionDoc::new(TimeBase::Frames { frame_rate: 100.0 }, 0.0, 100.0);
    doc.row_space = RowSpace::Bands(bands.clone());
    let mut n = Note::new(NoteId(1), 0.0, 4.0, 0);
    n.centroid_hz = Some(bands.splits[0] * 0.9);
    doc.push(n);

    let mut ed = Editor::new(doc, Viewport::new(800.0, 400.0));
    ed.row_space = RowSpace::Bands(bands.clone());
    assert!(ed.move_band_split(0, bands.splits[0] * 0.5));
    assert!(ed.undo());

    assert_eq!(
        ed.row_space, ed.doc.row_space,
        "the view kept the undone splits"
    );
}

// ── #250 / #251 / #252: what code review found on the guitar model ───

#[test]
fn setting_a_fret_on_stringless_notes_reports_no_change() {
    // A plain MIDI note on a guitar track is legitimate — `rows::fret_of`
    // documents exactly that case. `SetFret` cannot move it, so it must
    // not report an edit and push an undo step that undoes nothing.
    let tuning = StringTuning::guitar_standard();
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 4.0);
    doc.row_space = RowSpace::Strings(tuning.clone());
    doc.push(Note::new(NoteId(1), 0.0, PPQ, 60));

    let mut ed = Editor::new(doc, Viewport::new(800.0, 400.0));
    ed.selection.set_single(NoteId(1));
    assert!(!ed.set_fret_of_selection(7), "nothing had a string to fret");
    assert_eq!(ed.doc.note(NoteId(1)).unwrap().row, 60, "the pitch stood");
    assert!(!ed.undo(), "and no empty undo step was recorded");
}

#[test]
fn setting_a_fret_moves_what_it_can_in_a_mixed_selection() {
    let tuning = StringTuning::guitar_standard();
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 4.0);
    doc.row_space = RowSpace::Strings(tuning.clone());
    let mut fretted = Note::new(NoteId(1), 0.0, PPQ, tuning.open(2) + 5);
    fretted.string = Some(2);
    doc.push(fretted);
    doc.push(Note::new(NoteId(2), 0.0, PPQ, 60));

    let mut ed = Editor::new(doc, Viewport::new(800.0, 400.0));
    ed.selection.notes = vec![NoteId(1), NoteId(2)];
    assert!(ed.set_fret_of_selection(7), "one of the two moved");
    assert_eq!(ed.doc.note(NoteId(1)).unwrap().row, tuning.open(2) + 7);
    assert_eq!(
        ed.doc.note(NoteId(2)).unwrap().row,
        60,
        "the stringless note was left alone"
    );
}

/// Cycle a note that starts on `string`, and report where it lands.
fn cycle_from(tuning: &StringTuning, string: Option<u8>, pitch: i32) -> (bool, Option<u8>) {
    use expression_editor_core::menu::Command;

    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 4.0);
    doc.row_space = RowSpace::Strings(tuning.clone());
    let mut n = Note::new(NoteId(1), 0.0, PPQ, pitch);
    n.string = string;
    doc.push(n);

    let mut ed = Editor::new(doc, Viewport::new(800.0, 400.0));
    let ok = ed.run_command(&Command::CycleString(NoteId(1)), None);
    (ok, ed.doc.note(NoteId(1)).unwrap().string)
}

#[test]
fn cycling_string_wraps_past_the_top() {
    let tuning = StringTuning::guitar_standard();
    // High E, 5th fret (A=69) — reachable on the B and G strings too, so
    // cycling off the top string must come back round rather than
    // clamping into a no-op that still claims to have worked.
    let pitch = tuning.pitch(5, 5);
    let (ok, landed) = cycle_from(&tuning, Some(5), pitch);
    assert!(ok, "the cycle wrapped instead of clamping");
    let landed = landed.expect("still on a string");
    assert_ne!(landed, 5, "it left the top string");
    let fret = pitch - tuning.open(landed as usize);
    assert!(
        (0..=tuning.frets as i32).contains(&fret),
        "and landed somewhere the pitch is actually playable (fret {fret})"
    );
}

#[test]
fn cycling_string_skips_a_string_the_pitch_cannot_reach() {
    let tuning = StringTuning::guitar_standard();
    // A string, 2nd fret (B=47). The D string above it would be fret -3,
    // so the old `string + 1` dead-ended here instead of moving on.
    let pitch = tuning.pitch(1, 2);
    let (ok, landed) = cycle_from(&tuning, Some(1), pitch);
    assert!(
        ok,
        "an unreachable string is skipped, not the end of the road"
    );
    let landed = landed.expect("still on a string");
    assert_ne!(landed, 1);
    let fret = pitch - tuning.open(landed as usize);
    assert!((0..=tuning.frets as i32).contains(&fret), "fret {fret}");
}

#[test]
fn a_note_playable_on_one_string_reports_no_change() {
    let tuning = StringTuning::guitar_standard();
    // The low E open (E=40) is below every other string's open pitch, so
    // there is nowhere else to put it.
    let (ok, landed) = cycle_from(&tuning, Some(0), tuning.open(0));
    assert!(!ok, "nothing to cycle to");
    assert_eq!(landed, Some(0), "and it stayed where it was");
}

#[test]
fn cycling_a_stringless_note_finds_it_a_string() {
    let tuning = StringTuning::guitar_standard();
    // The new model allows a note with no string at all; cycling should
    // assign the first one that can play it rather than falling over.
    let pitch = tuning.pitch(2, 5);
    let (ok, landed) = cycle_from(&tuning, None, pitch);
    assert!(ok);
    let landed = landed.expect("picked up a string");
    let fret = pitch - tuning.open(landed as usize);
    assert!((0..=tuning.frets as i32).contains(&fret), "fret {fret}");
}

#[test]
fn repeatedly_nudging_one_band_split_keeps_moving_that_split() {
    use expression_editor_core::rows::SliceBands;

    // The inspector's -/+ buttons key on the render-time index. When a
    // write re-sorted the list, a split dragged past its neighbour swapped
    // positions and the next click moved the *neighbour* instead.
    let bands = SliceBands::default();
    assert!(bands.splits.len() >= 2, "needs two splits to cross");
    let mut doc = ExpressionDoc::new(TimeBase::Frames { frame_rate: 100.0 }, 0.0, 100.0);
    doc.row_space = RowSpace::Bands(bands.clone());
    let mut ed = Editor::new(doc, Viewport::new(800.0, 400.0));

    // Walk split 1 down hard enough that it would have crossed split 0.
    let mut hz = bands.splits[1];
    for _ in 0..8 {
        hz *= 0.5;
        assert!(ed.move_band_split(1, hz));
    }

    let RowSpace::Bands(after) = &ed.doc.row_space else {
        panic!("still banded");
    };
    assert!(
        after.splits[1] >= after.splits[0],
        "split 1 stayed above split 0 ({:?})",
        after.splits
    );
    assert_eq!(
        after.splits[0], bands.splits[0],
        "and split 0 never moved — the clicks all addressed split 1"
    );
}

#[test]
fn a_band_split_cannot_be_pushed_past_its_neighbour() {
    use expression_editor_core::rows::SliceBands;

    let bands = SliceBands::default();
    let mut doc = ExpressionDoc::new(TimeBase::Frames { frame_rate: 100.0 }, 0.0, 100.0);
    doc.row_space = RowSpace::Bands(bands.clone());
    let mut ed = Editor::new(doc, Viewport::new(800.0, 400.0));

    // Shove split 0 way above split 1, and the other way round.
    assert!(ed.move_band_split(0, bands.splits[1] * 4.0));
    assert!(ed.move_band_split(1, 1.0));

    let RowSpace::Bands(after) = &ed.doc.row_space else {
        panic!("still banded");
    };
    assert!(
        after.splits.windows(2).all(|w| w[0] <= w[1]),
        "splits stayed ascending: {:?}",
        after.splits
    );
}

// ── adaptive grid ────────────────────────────────────────────────────

/// The grid follows the zoom, through the editor rather than the maths.
///
/// `libs/adaptive-grid` has its own tests for the arithmetic. What this
/// pins is the wiring: that a camera move actually reaches the grid, in
/// every path that moves a camera.
#[test]
fn an_adaptive_grid_coarsens_as_the_view_zooms_out() {
    let mut ed = grid_editor();
    ed.grid.adaptive.density = adaptive_grid::Density::Medium;

    // Fit the whole take, then keep zooming out.
    ed.reset_view();
    let zoomed_in = ed.grid.effective();
    for _ in 0..8 {
        ed.zoom_time_at(ed.viewport.w * 0.5, 0.5);
    }
    let zoomed_out = ed.grid.effective();
    assert!(
        zoomed_out > zoomed_in,
        "zooming out left the grid at 1/{:.0}, from 1/{:.0}",
        1.0 / zoomed_out,
        1.0 / zoomed_in
    );
}

/// And a fixed grid stays exactly where it was put.
#[test]
fn a_fixed_grid_ignores_the_zoom() {
    use adaptive_grid::Density;

    let mut ed = grid_editor();
    // Said out loud, not inherited. This passed by accident while
    // `Fixed` was the default; the claim is about a grid the user has
    // pinned, and it should hold whatever the default becomes.
    ed.set_grid_density(Density::Fixed);
    let before = ed.grid.effective();
    for _ in 0..8 {
        ed.zoom_time_at(ed.viewport.w * 0.5, 0.5);
    }
    assert_eq!(ed.grid.effective(), before, "a fixed grid moved");
}

/// A triplet grid stays a triplet grid across a zoom.
///
/// The reason `triplet` is its own flag: the division is only ever
/// scaled by powers of two, so the third can never be rounded away.
#[test]
fn an_adaptive_grid_keeps_its_triplets() {
    let mut ed = grid_editor();
    ed.grid.adaptive.density = adaptive_grid::Density::Medium;
    ed.grid.triplet = true;
    ed.grid.division = 1.0 / 16.0;
    for _ in 0..6 {
        ed.zoom_time_at(ed.viewport.w * 0.5, 0.5);
    }
    assert!(ed.grid.triplet, "the triplet flag was lost");
    let ratio = (ed.grid.effective() / (1.0 / 16.0)).log2();
    assert!(
        (ratio - ratio.round()).abs() < 1e-9,
        "the division left the powers of two: 1/{:.3}",
        1.0 / ed.grid.effective()
    );
}

fn grid_editor() -> Editor {
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: 960.0 }, 0.0, 960.0 * 64.0);
    for i in 0..8u64 {
        doc.push(Note::new(
            NoteId(i + 1),
            960.0 * i as f64,
            960.0 * i as f64 + 480.0,
            60 + i as i32,
        ));
    }
    let mut ed = Editor::new(doc, Viewport::new(1000.0, 500.0));
    ed.reset_view();
    ed
}

// ── the chord gun ───────────────────────────────────────────────────

/// The degrees of C major are the chords everybody knows.
///
/// The theory is keyflow's, not ours — this asserts we are *asking it
/// correctly*, which is the part that can be wrong here: the octave, the
/// scale step the degree sits on, and turning intervals into pitches.
#[test]
fn the_degrees_of_c_major_are_the_expected_triads() {
    use expression_editor_core::harmony::ChordGun;

    let gun = ChordGun::default(); // C Ionian, triads, octave 4
    // C4 is MIDI 60 with the (octave + 1) * 12 convention.
    assert_eq!(gun.pitches(1), vec![60, 64, 67], "I should be C E G");
    assert_eq!(gun.pitches(2), vec![62, 65, 69], "ii should be D F A");
    assert_eq!(gun.pitches(5), vec![67, 71, 74], "V should be G B D");
    assert_eq!(gun.pitches(6), vec![69, 72, 76], "vi should be A C E");
}

/// A degree outside the scale is nothing, not a panic or a wrap.
#[test]
fn a_degree_off_the_end_fires_nothing() {
    use expression_editor_core::harmony::ChordGun;

    let gun = ChordGun::default();
    assert!(gun.pitches(0).is_empty());
    assert!(gun.pitches(8).is_empty());
}

/// Depth stacks more of the scale on top, keeping what was there.
#[test]
fn depth_adds_tones_above_the_triad() {
    use expression_editor_core::harmony::{ChordGun, HarmonizationDepth};

    let mut gun = ChordGun::default();
    let triad = gun.pitches(1);
    gun.depth = HarmonizationDepth::Sevenths;
    let seventh = gun.pitches(1);

    assert_eq!(seventh.len(), triad.len() + 1, "a 7th is a triad plus one");
    assert_eq!(&seventh[..3], &triad[..], "the triad moved under the 7th");
    assert_eq!(seventh[3], 71, "Cmaj7's seventh is B");
}

/// Inverting lifts the bottom note an octave, and wraps at the top.
#[test]
fn inversions_rotate_the_chord_and_come_back() {
    use expression_editor_core::harmony::ChordGun;

    let mut gun = ChordGun::default();
    let root = gun.pitches(1);
    gun.cycle_inversion(true);
    assert_eq!(gun.pitches(1), vec![64, 67, 72], "first inversion is E G C");
    gun.cycle_inversion(true);
    assert_eq!(gun.pitches(1), vec![67, 72, 76], "second is G C E");
    gun.cycle_inversion(true);
    assert_eq!(gun.pitches(1), root, "three inversions of a triad is none");
}

/// The mode changes what a degree means; the family does not change under
/// you.
#[test]
fn cycling_the_mode_stays_inside_its_family() {
    use expression_editor_core::harmony::ChordGun;
    use keyflow_proto::key::scale::ScaleType;

    let mut gun = ChordGun::default();
    let ionian_iii = gun.pitches(3);
    for _ in 0..7 {
        gun.cycle_mode(true);
        assert_eq!(
            gun.mode.scale_type(),
            ScaleType::Diatonic,
            "cycling walked out of the diatonic family",
        );
    }
    assert_eq!(
        gun.pitches(3),
        ionian_iii,
        "seven steps through seven modes did not come home",
    );
}

/// Firing a chord inserts it at the playhead, selects it, and is one undo.
#[test]
fn firing_a_chord_inserts_it_at_the_playhead() {
    let mut ed = test_editor();
    let before = ed.doc.notes.len();
    ed.playhead = Some(PPQ * 2.0);
    ed.grid.enabled = true;

    assert!(ed.insert_chord(1), "the chord did not fire");
    // The selection *is* the fired chord — filtering the document by
    // start time would also catch whatever the fixture already had at
    // that beat, which is what this test first did.
    assert_eq!(
        ed.selection.notes.len(),
        3,
        "a triad should be three notes, and they should be selected so \
         the gesture composes with the next one",
    );
    let added: Vec<_> = ed
        .selection
        .notes
        .iter()
        .filter_map(|id| ed.doc.note(*id))
        .collect();
    assert_eq!(
        added.len(),
        3,
        "a selected note is missing from the document"
    );
    assert!(
        added.iter().all(|n| (n.start - PPQ * 2.0).abs() < 1.0),
        "the chord did not land on the playhead",
    );
    assert!(
        added.iter().all(|n| n.end > n.start),
        "a fired chord has no length",
    );

    assert!(ed.undo(), "firing opened no undo step");
    assert_eq!(
        ed.doc.notes.len(),
        before,
        "one undo did not take the whole chord",
    );
}

// ── the action declarations ─────────────────────────────────────────

/// Every declared action is well-formed, and the ones this crate owns
/// all do something.
///
/// The declarations replaced three hand-kept lists — ids, labels, and a
/// dispatch match — none of which were tied together. This is the tie:
/// an action nobody wired up is a test failure rather than a key that
/// silently does nothing.
///
/// Two groups are declared here and run by `expression-editor-ui`:
/// velocity needs `expression-editor-tools`, which depends on this
/// crate, and MeMagic is anchored on a pointer this crate has no notion
/// of. They are skipped *here* and covered *there*, so between the two
/// nothing declared is unreachable.
#[test]
fn every_declared_action_is_wired_up() {
    use expression_editor_core::actions;

    let all = actions::all();
    assert!(all.len() >= 60, "only {} actions declared", all.len());

    let mut seen = std::collections::BTreeSet::new();
    for meta in &all {
        assert!(seen.insert(meta.id), "two actions share the id {}", meta.id);
        assert!(
            !meta.description.is_empty(),
            "{} has no description",
            meta.id
        );
        assert!(
            !meta.shortcut.is_empty(),
            "{} advertises no way to reach it",
            meta.id,
        );
        assert_eq!(
            meta.category, "Expression Editor",
            "{} would land in the wrong menu",
            meta.id,
        );

        let ours =
            !meta.id.starts_with("FTS_EDITOR_VELOCITY") && meta.id != actions::view::MEMAGIC.id;
        if !ours {
            continue;
        }

        // Enough material for every kind: notes, a selection, a razor
        // over them, and a second track to step to.
        let mut ed = grid_editor();
        ed.tracks.push(expression_editor_core::tracks::Track::new(
            "second",
            ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 4.0),
        ));
        ed.selection.notes = ed.doc.notes.iter().map(|n| n.id).collect();
        ed.razor.add(expression_editor_core::RazorArea::new(
            0.0,
            PPQ * 4.0,
            0,
            127,
        ));
        assert!(
            actions::run(&mut ed, meta.id),
            "`{}` ({}) is declared and does nothing",
            meta.id,
            meta.shortcut,
        );
    }
}

/// An id nobody declared is not silently swallowed.
#[test]
fn an_unknown_action_falls_through() {
    let mut ed = grid_editor();
    assert!(!expression_editor_core::actions::run(
        &mut ed,
        "FTS_EDITOR_NOPE"
    ));
}
