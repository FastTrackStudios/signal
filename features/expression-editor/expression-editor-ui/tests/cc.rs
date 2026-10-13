//! Controller-dimension gestures, driven through the real pointer path.
//!
//! `interaction::pointer_down` resolves everything through the mouse
//! map, so these assert the *whole* route — context detection, binding,
//! drag, and the edit it writes — rather than calling the edits
//! directly, which the core suite already covers.

use expression_editor_core::cc;
use expression_editor_core::doc::{ExpressionDoc, TimeBase};
use expression_editor_core::mouse::{Context, Gesture};
use expression_editor_core::tools::Mods;
use expression_editor_core::{Editor, Viewport};
use expression_editor_ui::interaction::{self, Drag};

const PPQ: f64 = 960.0;
const W: f64 = 900.0;
const H: f64 = 480.0;

fn editor() -> Editor {
    let doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 8.0);
    let mut ed = Editor::new(doc, Viewport::new(W, H));
    ed.edit_cc(1);
    ed
}

const NONE: Mods = Mods {
    ctrl: false,
    shift: false,
    alt: false,
};
const ALT: Mods = Mods {
    ctrl: false,
    shift: false,
    alt: true,
};
const CTRL: Mods = Mods {
    ctrl: true,
    shift: false,
    alt: false,
};
const SHIFT: Mods = Mods {
    ctrl: false,
    shift: true,
    alt: false,
};

/// Sweep a drag from one point to another and release.
fn sweep(ed: &mut Editor, from: (f64, f64), to: (f64, f64), mods: Mods, button: u16) -> Drag {
    let mut drag = interaction::pointer_down(ed, from.0, from.1, mods, button);
    interaction::pointer_move(ed, &mut drag, to.0, to.1, mods);
    interaction::pointer_up(ed, drag, to.0, to.1, mods)
}

fn value_at(ed: &Editor, t: f64) -> f64 {
    let dimension = ed.doc.cc.get(1).expect("dimension");
    dimension.curve.sample(t, dimension.default_value())
}

#[test]
fn cc_edit_mode_claims_the_roll_from_the_notes_behind_it() {
    let mut ed = editor();
    // An empty CC1 rests at 0, drawn at the bottom of the roll, so the
    // middle of the canvas is open dimension rather than curve.
    assert_eq!(
        interaction::context_at(&ed, 400.0, H * 0.5),
        Context::CcLane
    );

    ed.exit_cc_edit();
    assert_eq!(
        interaction::context_at(&ed, 400.0, H * 0.5),
        Context::PianoRoll,
        "leaving CC edit mode hands the roll back"
    );
}

#[test]
fn the_pointer_on_the_drawn_curve_is_a_cc_event() {
    let mut ed = editor();
    // CC1 defaults to 0 — the very bottom of the roll.
    let y = cc::cc_y(0.0, H);
    assert_eq!(interaction::context_at(&ed, 400.0, y), Context::CcEvent);
    assert_eq!(
        interaction::context_at(&ed, 400.0, y - 60.0),
        Context::CcLane
    );

    // And it tracks the curve rather than a fixed height: draw a value
    // mid-roll and the event follows it up.
    sweep(&mut ed, (100.0, H * 0.5), (500.0, H * 0.5), NONE, 0);
    assert_eq!(
        interaction::context_at(&ed, 300.0, H * 0.5),
        Context::CcEvent
    );
}

#[test]
fn an_unmodified_drag_draws_freehand() {
    let mut ed = editor();
    let before = value_at(&ed, ed.camera.t_at(300.0));
    sweep(&mut ed, (200.0, H * 0.25), (400.0, H * 0.25), NONE, 0);
    let after = value_at(&ed, ed.camera.t_at(300.0));

    assert!(
        after > before,
        "the stroke wrote values: {before} -> {after}"
    );
    // y = H/4 is three quarters of the way up the dimension.
    assert!(
        (after - 0.75).abs() < 0.05,
        "the value follows the pointer height, got {after}"
    );
}

#[test]
fn alt_drag_draws_a_ramp_that_survives_release() {
    let mut ed = editor();
    let drag = sweep(&mut ed, (200.0, H), (600.0, 0.0), ALT, 0);

    assert!(
        matches!(drag.live_cc_line(), Some((1, _, _))),
        "the ramp stays live so the shape buttons can restyle it"
    );

    // Linear shape: the midpoint of the ramp sits halfway up.
    let mid = ed.camera.t_at(400.0);
    let v = value_at(&ed, mid);
    assert!(
        (v - 0.5).abs() < 0.08,
        "a linear ramp is half-way at half-way, got {v}"
    );
    assert!(value_at(&ed, ed.camera.t_at(590.0)) > 0.9);
}

#[test]
fn a_ramps_endpoints_snap_to_the_grid_and_shift_reverses_it() {
    let mut ed = editor();
    ed.grid.enabled = true;

    // Start deliberately off the grid; the written span should begin on
    // a division rather than where the pointer happened to be.
    let raw0 = ed.camera.t_at(213.0);
    sweep(&mut ed, (213.0, H), (600.0, 0.0), ALT, 0);
    let snapped = ed.snap_time(raw0);
    assert!(
        (snapped - raw0).abs() > f64::EPSILON,
        "the test's start point has to be off-grid to be meaningful"
    );
    let first = ed.doc.cc.get(1).unwrap().curve.points()[0].t;
    assert!(
        (first - snapped).abs() < 1.0,
        "endpoint snapped to {snapped}, curve starts at {first}"
    );

    // Shift is the reverse, everywhere in this surface.
    let mut ed = editor();
    ed.grid.enabled = true;
    let mods = Mods { alt: true, ..SHIFT };
    let raw0 = ed.camera.t_at(213.0);
    sweep(&mut ed, (213.0, H), (600.0, 0.0), mods, 0);
    let first = ed.doc.cc.get(1).unwrap().curve.points()[0].t;
    assert!(
        (first - raw0).abs() < 1.0,
        "shift kept the raw position {raw0}, got {first}"
    );
}

#[test]
fn a_right_drag_erases_back_to_the_lanes_default() {
    let mut ed = editor();
    sweep(&mut ed, (100.0, H * 0.25), (700.0, H * 0.25), NONE, 0);
    assert!(value_at(&ed, ed.camera.t_at(400.0)) > 0.5);

    sweep(&mut ed, (300.0, H * 0.5), (500.0, H * 0.5), NONE, 2);

    assert!(
        value_at(&ed, ed.camera.t_at(400.0)) < 0.05,
        "the swept range is back at CC1's default of 0"
    );
    assert!(
        value_at(&ed, ed.camera.t_at(150.0)) > 0.5,
        "and only the swept range — the rest of the stroke stands"
    );
}

#[test]
fn ctrl_drag_rides_the_fader_without_compounding() {
    let mut ed = editor();
    // A ramp to scale: something with actual shape, so a depth change
    // is distinguishable from a level change.
    sweep(&mut ed, (100.0, H), (700.0, 0.0), ALT, 0);
    let before_lo = value_at(&ed, ed.camera.t_at(200.0));
    let before_hi = value_at(&ed, ed.camera.t_at(600.0));
    let spread_before = before_hi - before_lo;

    // Drag down: shallower. Crucially, walk the pointer there in steps
    // — a live scale that re-scaled the already-scaled curve would go
    // exponential and this is what catches it.
    let mut drag = interaction::pointer_down(&mut ed, 150.0, H * 0.5, CTRL, 0);
    for step in 1..=8 {
        let y = H * 0.5 + (H * 0.25) * (step as f64 / 8.0);
        interaction::pointer_move(&mut ed, &mut drag, 650.0, y, CTRL);
    }
    interaction::pointer_up(&mut ed, drag, 650.0, H * 0.75, CTRL);

    let spread_after = value_at(&ed, ed.camera.t_at(600.0)) - value_at(&ed, ed.camera.t_at(200.0));
    assert!(
        spread_after < spread_before,
        "dragging down flattens the ramp: {spread_before} -> {spread_after}"
    );
    assert!(
        spread_after > spread_before * 0.25,
        "but only once — eight moves compounding would have crushed it, \
         {spread_before} -> {spread_after}"
    );
}

#[test]
fn scaling_a_narrowed_sweep_puts_back_what_the_wider_one_touched() {
    let mut ed = editor();
    sweep(&mut ed, (100.0, H), (700.0, 0.0), ALT, 0);
    let original = value_at(&ed, ed.camera.t_at(600.0));

    // Sweep out wide, then pull the range back in. The far end was
    // scaled on the way out and must be restored on the way back.
    let mut drag = interaction::pointer_down(&mut ed, 150.0, H * 0.5, CTRL, 0);
    interaction::pointer_move(&mut ed, &mut drag, 650.0, H * 0.8, CTRL);
    interaction::pointer_move(&mut ed, &mut drag, 300.0, H * 0.8, CTRL);
    interaction::pointer_up(&mut ed, drag, 300.0, H * 0.8, CTRL);

    let after = value_at(&ed, ed.camera.t_at(600.0));
    assert!(
        (after - original).abs() < 0.02,
        "outside the final range the ramp is untouched, {original} -> {after}"
    );
}

#[test]
fn every_cc_gesture_is_one_undo_step() {
    let mut ed = editor();
    sweep(&mut ed, (200.0, H * 0.25), (400.0, H * 0.25), NONE, 0);
    assert!(ed.can_undo());
    ed.undo();
    assert!(
        value_at(&ed, ed.camera.t_at(300.0)) < 0.05,
        "one undo takes the whole stroke, not its last sample"
    );
}

#[test]
fn cc_gestures_stay_out_of_the_way_when_no_lane_is_being_edited() {
    let mut ed = editor();
    ed.exit_cc_edit();
    let before = ed.doc.cc.get(1).map(|l| l.curve.points().len());

    // The same drags that draw in CC edit mode must not touch the dimension
    // once it is off; they belong to the notes again.
    sweep(&mut ed, (200.0, H * 0.25), (400.0, H * 0.25), ALT, 0);
    sweep(&mut ed, (200.0, H * 0.25), (400.0, H * 0.25), CTRL, 0);

    assert_eq!(before, ed.doc.cc.get(1).map(|l| l.curve.points().len()));
}

#[test]
fn the_map_owns_the_bindings_so_a_preset_can_disagree() {
    let map = expression_editor_core::mouse::MouseMap::reaper_like();
    use expression_editor_core::mouse::Action as A;

    assert_eq!(
        map.resolve(Context::CcLane, Gesture::Drag, NONE),
        A::EditCcEvents
    );
    assert_eq!(
        map.resolve(Context::CcLane, Gesture::Drag, ALT),
        A::DrawCcLine
    );
    assert_eq!(
        map.resolve(Context::CcLane, Gesture::Drag, CTRL),
        A::ScaleCcEvents
    );
    assert_eq!(
        map.resolve(Context::CcLane, Gesture::RightClick, NONE),
        A::EraseCcEvents
    );

    // Shift stays unbound in the dimension: it is the snap-reverse key, and
    // giving it a tool would cost the only consistent "other behaviour"
    // modifier the surface has.
    assert_eq!(
        map.resolve(Context::CcLane, Gesture::Drag, SHIFT),
        A::EditCcEvents,
        "shift falls back to the unmodified binding"
    );
}

// ── mode-dependent keys ──────────────────────────────────────────────

use expression_editor_core::Mode;
use expression_editor_core::doc::Note;

fn two_track_editor(mode: Mode) -> Editor {
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 8.0);
    for i in 0..3 {
        let mut n = Note::new(
            expression_editor_core::NoteId(i + 1),
            PPQ * i as f64,
            PPQ * (i as f64 + 0.9),
            60 + i as i32 * 2,
        );
        n.channel = Some(2);
        doc.push(n);
    }
    let mut ed = Editor::new(doc, Viewport::new(W, H));
    ed.set_mode(mode);
    let other = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 8.0);
    let b = ed.add_track("Harmony", other);
    ed.tracks.track_mut(b).unwrap().reference = true;
    ed
}

#[test]
fn bare_r_brings_references_forward_wherever_mpe_is_not_using_it() {
    // Audio and vocal notes have no member channel to assign, so the
    // key is free and takes Vovious's meaning.
    for mode in [
        Mode::PitchedAudio,
        Mode::Vocals,
        Mode::Midi,
        Mode::Drums,
        Mode::Guitar,
    ] {
        let mut ed = two_track_editor(mode);
        let drag = Drag::None;
        assert!(
            interaction::key_down(&mut ed, &drag, "r", NONE),
            "{mode:?} should handle bare R"
        );
        assert!(ed.refs_to_front, "{mode:?} should bring references forward");
    }
}

#[test]
fn in_mpe_bare_r_still_reassigns_channels() {
    let mut ed = two_track_editor(Mode::Mpe);
    ed.selection.notes = ed.doc.notes.iter().map(|n| n.id).collect();
    let before: Vec<Option<u8>> = ed.doc.notes.iter().map(|n| n.channel).collect();

    assert!(interaction::key_down(&mut ed, &Drag::None, "r", NONE));
    assert!(
        !ed.refs_to_front,
        "MPE keeps the older binding — the notes need distinct channels"
    );
    let after: Vec<Option<u8>> = ed.doc.notes.iter().map(|n| n.channel).collect();
    assert_ne!(before, after, "channels were actually reassigned");
}

#[test]
fn shift_r_reaches_references_from_every_mode_including_mpe() {
    let mut ed = two_track_editor(Mode::Mpe);
    assert!(interaction::key_down(&mut ed, &Drag::None, "r", SHIFT));
    assert!(
        ed.refs_to_front,
        "the gesture stays reachable where bare R is spoken for"
    );
}
