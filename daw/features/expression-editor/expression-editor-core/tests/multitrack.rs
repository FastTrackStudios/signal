//! A workspace where every track has its own mode, shown at once.
//!
//! The point of the feature is comparing *different parts* on one
//! timeline — a vocal against its reference MIDI against the kit — so
//! these are about tracks disagreeing with each other and the editor
//! coping, not about any one surface.

use expression_editor_core::rows::{RowSpace, SliceBands};
use expression_editor_core::tracks::{Track, Workspace};
use expression_editor_core::{Editor, ExpressionDoc, Mode, TimeBase, Viewport};
use expression_editor_core::{Note, NoteId};

fn doc() -> ExpressionDoc {
    ExpressionDoc::new(TimeBase::Frames { frame_rate: 100.0 }, 0.0, 400.0)
}

fn viewport() -> Viewport {
    Viewport {
        w: 1000.0,
        h: 600.0,
    }
}

/// A vocal, its reference MIDI, a guitar and a kit — the arrangement the
/// stacked view exists for.
fn band() -> Editor {
    let mut ed = Editor::new(doc(), viewport());
    ed.set_mode(Mode::PitchedAudio);
    ed.tracks.rename(0, "Lead Vox");
    for (name, mode) in [
        ("Ref MIDI", Mode::Midi),
        ("Guitar", Mode::Guitar),
        ("Kit", Mode::UnpitchedAudio),
    ] {
        ed.tracks.push(Track::in_mode(name, doc(), mode));
    }
    ed
}

#[test]
fn each_track_keeps_its_own_mode() {
    let ed = band();
    let modes: Vec<Mode> = ed.tracks.tracks().iter().map(|t| t.mode).collect();
    assert_eq!(
        modes,
        vec![
            Mode::PitchedAudio,
            Mode::Midi,
            Mode::Guitar,
            Mode::UnpitchedAudio
        ]
    );
}

#[test]
fn switching_track_brings_its_surface_with_it() {
    // Without this the document changes and nothing else does, so a kit
    // gets edited on whatever surface the vocal left behind.
    let mut ed = band();
    assert_eq!(ed.mode, Mode::PitchedAudio);

    assert!(ed.switch_track(3));
    assert_eq!(ed.mode, Mode::UnpitchedAudio);
    assert!(matches!(ed.row_space, RowSpace::Bands(_)));

    assert!(ed.switch_track(2));
    assert_eq!(ed.mode, Mode::Guitar);
    assert!(matches!(ed.row_space, RowSpace::Strings(_)));
}

#[test]
fn changing_mode_changes_the_active_tracks_mode() {
    // `Editor::mode` is a view of the active track's, not a second
    // source of truth — a stack drawn from the workspace must agree with
    // the surface on screen.
    let mut ed = band();
    ed.set_mode(Mode::Vocals);
    assert_eq!(ed.tracks.track(0).unwrap().mode, Mode::Vocals);
    // And the others are untouched.
    assert_eq!(ed.tracks.track(3).unwrap().mode, Mode::UnpitchedAudio);
}

#[test]
fn a_tuned_row_space_survives_leaving_the_track_and_coming_back() {
    // Switching tracks re-applies the mode preset. A preset that
    // overwrote the row space would throw away band splits the user had
    // moved — an edit silently undone by navigating away.
    let mut ed = band();
    assert!(ed.switch_track(3));

    let custom = SliceBands {
        splits: vec![100.0, 500.0, 4000.0],
        names: vec!["Sub".into(), "Body".into(), "Snap".into(), "Air".into()],
    };
    ed.doc.row_space = RowSpace::Bands(custom.clone());

    assert!(ed.switch_track(0));
    assert!(ed.switch_track(3));
    match &ed.doc.row_space {
        RowSpace::Bands(b) => assert_eq!(b, &custom, "the splits came back changed"),
        other => panic!("expected band space, got {other:?}"),
    }
}

#[test]
fn the_stack_covers_the_height_exactly_and_keeps_track_order() {
    let ed = band();
    let rows = ed.tracks.stack(600.0, 1.0, 20.0);
    assert_eq!(rows.len(), 4);

    assert_eq!(rows[0].y, 0.0);
    for pair in rows.windows(2) {
        assert!(
            (pair[0].y + pair[0].height - pair[1].y).abs() < 1e-3,
            "lanes must meet: {pair:?}"
        );
    }
    let last = rows.last().unwrap();
    assert!((last.y + last.height - 600.0).abs() < 1e-3);
    assert_eq!(
        rows.iter().map(|r| r.lane).collect::<Vec<_>>(),
        vec![0, 1, 2, 3]
    );
}

#[test]
fn a_vocal_lane_gets_more_room_than_a_slice_strip() {
    // Splitting evenly gives the kit the same height as the vocal, and
    // two octaves of pitch do not fit where three bands do.
    let ed = band();
    let rows = ed.tracks.stack(600.0, 1.0, 0.0);
    let vox = rows.iter().find(|r| r.lane == 0).unwrap().height;
    let kit = rows.iter().find(|r| r.lane == 3).unwrap().height;
    assert!(vox > kit, "vocal {vox} should be taller than kit {kit}");
}

#[test]
fn the_lane_being_edited_can_be_given_extra_room() {
    let mut ed = band();
    let plain = ed.tracks.stack(600.0, 1.0, 0.0);
    let boosted = ed.tracks.stack(600.0, 3.0, 0.0);
    let of = |rows: &[expression_editor_core::tracks::StackRow], t: usize| {
        rows.iter().find(|r| r.lane == t).unwrap().height
    };
    assert!(of(&boosted, 0) > of(&plain, 0));
    // And the boost follows the active track rather than the first one.
    assert!(ed.switch_track(3));
    let moved = ed.tracks.stack(600.0, 3.0, 0.0);
    assert!(of(&moved, 3) > of(&boosted, 3));
    assert!(of(&moved, 0) < of(&boosted, 0));
}

#[test]
fn every_lane_stays_clickable_however_many_there_are() {
    // A dimension squeezed to nothing cannot be clicked to select, so a user
    // cannot get back out of the state they fell into.
    let mut ed = band();
    for i in 0..20 {
        ed.tracks
            .push(Track::in_mode(format!("T{i}"), doc(), Mode::Midi));
    }
    let rows = ed.tracks.stack(600.0, 1.0, 12.0);
    assert!(rows.iter().all(|r| r.height >= 12.0 - 1e-3));
    let last = rows.last().unwrap();
    assert!(
        last.y + last.height <= 600.0 + 1e-3,
        "the floor must not overflow the viewport"
    );
}

#[test]
fn a_floor_that_does_not_fit_overflows_and_scrolls() {
    // Changed by #198. This used to assert an even split — 44 lanes
    // sharing 600 px at 13 px each — on the reasoning that if nothing
    // fits well, everything should be equally bad. That is right for
    // four tracks and wrong for an orchestra: 13 px lanes are no more
    // usable than lanes you cannot see, so the floor is honoured and the
    // stack overflows instead.
    let mut ed = band();
    for i in 0..40 {
        ed.tracks
            .push(Track::in_mode(format!("T{i}"), doc(), Mode::Midi));
    }
    let rows = ed.tracks.stack(600.0, 1.0, 40.0);
    assert!(
        rows.iter().all(|r| r.height >= 40.0 - 1e-3),
        "floor honoured"
    );
    let total = rows.last().unwrap().y + rows.last().unwrap().height;
    assert!(total > 600.0, "and the stack is taller than the viewport");
}

#[test]
fn a_viewport_too_small_for_one_floored_lane_is_the_last_resort() {
    // The even split survives for exactly this case: nothing can be
    // readable, so be uniformly wrong rather than arbitrarily so.
    let mut ed = band();
    for i in 0..40 {
        ed.tracks
            .push(Track::in_mode(format!("T{i}"), doc(), Mode::Midi));
    }
    let rows = ed.tracks.stack(30.0, 1.0, 40.0);
    let last = rows.last().unwrap();
    assert!((last.y + last.height - 30.0).abs() < 1e-3, "fits exactly");
    let first = rows.first().unwrap().height;
    assert!(rows.iter().all(|r| (r.height - first).abs() < 1e-3));
}

#[test]
fn hidden_tracks_leave_the_stack_but_keep_their_place() {
    let mut ed = band();
    ed.tracks.track_mut(1).unwrap().hidden = true;
    let rows = ed.tracks.stack(600.0, 1.0, 0.0);
    assert_eq!(
        rows.iter().map(|r| r.lane).collect::<Vec<_>>(),
        vec![0, 2, 3],
        "hiding removes a dimension without renumbering the others"
    );
    // Still in the workspace, still editable by index.
    assert_eq!(ed.tracks.len(), 4);
    assert_eq!(ed.tracks.track(1).unwrap().name, "Ref MIDI");
}

#[test]
fn a_y_coordinate_resolves_to_the_lane_under_it() {
    let ed = band();
    let rows = ed.tracks.stack(600.0, 1.0, 0.0);
    for r in &rows {
        let mid = r.y + r.height / 2.0;
        assert_eq!(Workspace::row_at(&rows, mid), Some(r.lane));
    }
    // Just inside the top edge belongs to that dimension, not the one above.
    for r in &rows {
        assert_eq!(Workspace::row_at(&rows, r.y), Some(r.lane));
    }
    assert_eq!(Workspace::row_at(&rows, -1.0), None);
    assert_eq!(Workspace::row_at(&rows, 601.0), None);
}

#[test]
fn a_hand_sized_lane_is_not_resized_by_a_mode_change() {
    let mut t = Track::in_mode("Kit", doc(), Mode::UnpitchedAudio);
    let natural = t.weight;
    t.set_mode(Mode::PitchedAudio);
    assert!(
        (t.weight - natural).abs() > f32::EPSILON,
        "an untouched dimension follows its mode"
    );

    t.weight = 5.0;
    t.set_mode(Mode::UnpitchedAudio);
    assert_eq!(t.weight, 5.0, "a hand-sized dimension keeps its height");
}

// ── Stable identity ──────────────────────────────────────────────────
//
// The rule these defend is narrow and absolute: indices are fine for
// in-memory addressing, but nothing *durable* may hold one. Both of the
// operations below are exactly what breaks a layout stored by position
// or by name.

#[test]
fn a_track_inserted_above_does_not_move_anyone_else_s_identity() {
    let mut ws = Workspace::single("Lead Vox", doc());
    let lead = ws.track(0).unwrap().guid.clone();
    ws.push(Track::new("Kit", doc()));
    let kit = ws.track(1).unwrap().guid.clone();

    // The index of "Kit" is about to change. Its guid must not.
    let mut reordered = Workspace::single("Inserted", doc());
    reordered.push(Track::with_guid(lead.clone(), "Lead Vox", doc()));
    reordered.push(Track::with_guid(kit.clone(), "Kit", doc()));

    assert_eq!(reordered.index_of_guid(&kit), Some(2), "index moved");
    assert_eq!(
        reordered.track_by_guid(&kit).map(|t| t.name.as_str()),
        Some("Kit"),
        "the guid still resolves to the same track"
    );
    assert_ne!(lead, kit, "generated guids are distinct");
}

#[test]
fn renaming_a_track_leaves_its_guid_alone() {
    let mut ws = Workspace::single("Lead Vox", doc());
    let before = ws.track(0).unwrap().guid.clone();
    assert!(ws.rename(0, "Lead Vocal Comp 3"));
    let after = ws.track(0).unwrap().guid.clone();

    assert_eq!(before, after, "a rename is not a change of identity");
    assert_eq!(ws.index_of("Lead Vox"), None, "the old name is gone");
    assert_eq!(ws.index_of_guid(&after), Some(0), "the guid still resolves");
}

#[test]
fn a_host_supplied_guid_is_kept_verbatim() {
    // REAPER hands us its own GUID string; we store it, we do not mint
    // our own, or persisted state would not survive reopening.
    let host = "{A1B2C3D4-0000-0000-0000-000000000001}";
    let ws = Workspace::single("x", doc());
    let mut ws = ws;
    ws.push(Track::with_guid(host, "Lead Vox", doc()));
    assert_eq!(
        ws.track_by_guid(host).map(|t| t.name.as_str()),
        Some("Lead Vox")
    );
}

#[test]
fn generated_guids_do_not_collide() {
    let a = Track::new("a", doc());
    let b = Track::new("b", doc());
    let c = Track::new("c", doc());
    assert_ne!(a.guid, b.guid);
    assert_ne!(b.guid, c.guid);
    assert_ne!(a.guid, c.guid);
}

#[test]
fn the_adapter_can_hand_the_active_track_the_host_s_identity() {
    let mut ed = Editor::new(doc(), Viewport::new(800.0, 600.0));
    let generated = ed.tracks.track(0).unwrap().guid.clone();

    ed.adopt_track_identity("{HOST-GUID}", Some("Lead Vox".into()));

    let t = ed.tracks.track(0).unwrap();
    assert_eq!(t.guid, "{HOST-GUID}");
    assert_eq!(t.name, "Lead Vox");
    assert_ne!(t.guid, generated, "the generated id was replaced");
}

// ── Lanes ────────────────────────────────────────────────────────────
//
// A lane holds N tracks. These fix the properties the rest of the lane
// work leans on: identity is membership, weight belongs to the lane,
// hiding is per-track, and a lane with nothing visible is not drawn.

fn ws_with(names: &[&str]) -> Workspace {
    let mut ws = Workspace::single(names[0], doc());
    for n in &names[1..] {
        ws.push(Track::new(*n, doc()));
    }
    ws
}

#[test]
fn a_new_track_arrives_in_a_lane_of_its_own() {
    let ws = ws_with(&["Lead Vox", "Ref MIDI", "Kit"]);
    assert_eq!(
        ws.layout().len(),
        3,
        "one lane each until something pairs them"
    );
    for i in 0..3 {
        assert_eq!(ws.lane_tracks(i).len(), 1);
    }
}

#[test]
fn a_lane_can_hold_several_tracks() {
    let mut ws = ws_with(&["Lead Vox", "Ref MIDI"]);
    let ref_guid = ws.track(1).unwrap().guid.clone();
    ws.layout_mut().forget(&ref_guid);
    ws.layout_mut().lane_mut(0).unwrap().tracks.push(ref_guid);

    assert_eq!(ws.layout().len(), 1, "two tracks, one lane");
    assert_eq!(ws.lane_tracks(0), vec![0, 1]);
}

#[test]
fn a_lane_takes_the_tallest_members_height() {
    let mut ws = Workspace::single("Kit", doc());
    ws.track_mut(0).unwrap().set_mode(Mode::UnpitchedAudio);
    ws.push(Track::in_mode("Ref MIDI", doc(), Mode::Midi));
    let midi_guid = ws.track(1).unwrap().guid.clone();
    ws.layout_mut().forget(&midi_guid);
    ws.layout_mut().lane_mut(0).unwrap().tracks.push(midi_guid);

    let kit = Mode::UnpitchedAudio.stack_weight();
    let midi = Mode::Midi.stack_weight();
    let natural = ws.natural_weight(0);
    assert_eq!(
        natural,
        kit.max(midi),
        "the tallest member's need is what makes the lane readable"
    );
}

#[test]
fn a_hand_dragged_lane_keeps_its_height_when_a_member_changes_mode() {
    let mut ws = Workspace::single("Kit", doc());
    ws.layout_mut().lane_mut(0).unwrap().weight = 9.0;
    assert!(ws.layout().lane(0).unwrap().is_hand_sized());

    ws.track_mut(0).unwrap().set_mode(Mode::UnpitchedAudio);
    ws.refresh_lane_weights();

    assert_eq!(
        ws.layout().lane(0).unwrap().weight,
        9.0,
        "a hand-sized lane is not silently resized"
    );
}

#[test]
fn an_untouched_lane_follows_its_members_mode() {
    let mut ws = Workspace::single("Kit", doc());
    let before = ws.layout().lane(0).unwrap().weight;
    ws.track_mut(0).unwrap().set_mode(Mode::UnpitchedAudio);
    ws.refresh_lane_weights();
    let after = ws.layout().lane(0).unwrap().weight;
    assert_ne!(before, after);
    assert_eq!(after, Mode::UnpitchedAudio.stack_weight());
}

#[test]
fn a_lane_with_every_track_hidden_is_not_drawn() {
    let mut ws = ws_with(&["Lead Vox", "Kit"]);
    assert_eq!(ws.stack(600.0, 1.0, 0.0).len(), 2);

    ws.track_mut(1).unwrap().hidden = true;
    let rows = ws.stack(600.0, 1.0, 0.0);
    assert_eq!(rows.len(), 1, "the emptied lane leaves the stack");
    assert_eq!(rows[0].lane, 0);

    // Hiding a track inside a lane that still has a visible member does
    // not remove the lane — that is the second gesture, for free.
    let kit_guid = ws.track(1).unwrap().guid.clone();
    ws.layout_mut().forget(&kit_guid);
    ws.layout_mut().lane_mut(0).unwrap().tracks.push(kit_guid);
    assert_eq!(ws.stack(600.0, 1.0, 0.0).len(), 1);
    assert!(ws.lane_is_visible(0), "Lead Vox is still visible in it");
}

#[test]
fn removing_a_track_removes_the_lane_it_emptied() {
    let mut ws = ws_with(&["Lead Vox", "Kit"]);
    assert!(ws.remove(1));
    assert_eq!(ws.layout().len(), 1);
    assert_eq!(ws.lane_tracks(0), vec![0]);
}

#[test]
fn the_boost_goes_to_the_lane_holding_the_active_track() {
    let mut ws = ws_with(&["A", "B", "C"]);
    ws.push(Track::new("D", doc()));
    // Put the active track (0) in the same lane as D, so a per-track
    // boost and a per-lane boost would give different answers.
    let d = ws.track(3).unwrap().guid.clone();
    ws.layout_mut().forget(&d);
    ws.layout_mut().lane_mut(0).unwrap().tracks.push(d);

    let plain = ws.stack(600.0, 1.0, 0.0);
    let boosted = ws.stack(600.0, 3.0, 0.0);
    let h = |rows: &[expression_editor_core::tracks::StackRow], lane: usize| {
        rows.iter().find(|r| r.lane == lane).unwrap().height
    };
    assert!(h(&boosted, 0) > h(&plain, 0), "the active lane grew");
    assert!(h(&boosted, 1) < h(&plain, 1), "the others yielded room");
}

#[test]
fn lane_membership_survives_a_track_being_inserted_above() {
    let ws = ws_with(&["Lead Vox", "Kit"]);
    let kit = ws.track(1).unwrap().guid.clone();
    assert_eq!(ws.layout().lane_of(&kit), Some(1));

    // Whatever happens to indices, the guid still finds its lane.
    assert_eq!(ws.lane_tracks(1), vec![1]);
    assert_eq!(ws.track_by_guid(&kit).map(|t| t.name.as_str()), Some("Kit"));
}

// ── Auto-pairing ─────────────────────────────────────────────────────

use expression_editor_core::tracks::normalize_track_name as norm;

fn named(ws: &mut Workspace, name: &str, folder: Option<&str>) -> String {
    let mut t = Track::new(name, doc());
    t.folder = folder.map(str::to_string);
    let guid = t.guid.clone();
    ws.push(t);
    guid
}

#[test]
fn normalization_strips_case_punctuation_and_one_trailing_role() {
    assert_eq!(norm("Lead Vox"), "leadvox");
    assert_eq!(norm("lead_vox MIDI"), "leadvox");
    assert_eq!(norm("Lead Vox (Ref)"), "leadvox");
    assert_eq!(norm("  LEAD   VOX  "), "leadvox");
    // Only trailing, and only one: a leading role word is part of the
    // name, and stripping two would collapse genuinely distinct tracks.
    assert_ne!(norm("MIDI Lead"), norm("Lead"));
    assert_eq!(norm("Lead Vox Ref MIDI"), "leadvoxref");
}

#[test]
fn a_vocal_and_its_guide_open_in_one_lane() {
    let mut ws = Workspace::single("Lead Vox", doc());
    named(&mut ws, "Lead Vox MIDI", None);
    named(&mut ws, "Kit", None);
    ws.auto_pair();

    assert_eq!(ws.layout().len(), 2, "the pair merged, the kit did not");
    assert_eq!(ws.lane_tracks(0).len(), 2);
    assert_eq!(ws.lane_tracks(1).len(), 1);
}

#[test]
fn matching_does_not_cross_folders() {
    let mut ws = Workspace::single("Anchor", doc());
    named(&mut ws, "Lead Vox", Some("Band"));
    named(&mut ws, "Lead Vox", Some("Choir"));
    ws.auto_pair();

    assert_eq!(
        ws.layout().len(),
        3,
        "same name, different folders, so not a pair"
    );
}

#[test]
fn an_unmatched_track_gets_its_own_lane() {
    let mut ws = Workspace::single("Lead Vox", doc());
    named(&mut ws, "Snare Bottom", None);
    named(&mut ws, "Room Mic", None);
    ws.auto_pair();
    assert_eq!(ws.layout().len(), 3);
}

#[test]
fn a_hand_arranged_layout_is_never_re_inferred() {
    let mut ws = Workspace::single("Lead Vox", doc());
    named(&mut ws, "Lead Vox MIDI", None);
    ws.layout_mut().mark_arranged();
    let before = ws.layout().clone();

    ws.auto_pair();

    assert_eq!(&before, ws.layout(), "the matcher left it alone");
    assert!(ws.layout().is_arranged());
}

#[test]
fn an_inferred_layout_is_not_marked_arranged() {
    let mut ws = Workspace::single("Lead Vox", doc());
    named(&mut ws, "Lead Vox MIDI", None);
    ws.auto_pair();
    assert!(
        !ws.layout().is_arranged(),
        "only a human arranging it makes it worth persisting"
    );
}

#[test]
fn a_track_recorded_later_lands_in_the_lane_it_belongs_to() {
    let mut ws = Workspace::single("Lead Vox", doc());
    named(&mut ws, "Kit", None);
    ws.auto_pair();
    assert_eq!(ws.layout().len(), 2);

    // A new comp on the vocal, arriving after the layout was made.
    let new = named(&mut ws, "Lead Vox Audio", None);
    ws.place_new_track(&new);

    assert_eq!(ws.layout().len(), 2, "no new lane was appended");
    assert_eq!(ws.lane_tracks(0).len(), 2, "it joined the vocal's lane");
}

#[test]
fn a_track_matching_nothing_appends_a_lane() {
    let mut ws = Workspace::single("Lead Vox", doc());
    ws.auto_pair();
    let new = named(&mut ws, "Tambourine", None);
    ws.place_new_track(&new);
    assert_eq!(ws.layout().len(), 2);
}

#[test]
fn a_stale_layout_entry_is_dropped_without_complaint() {
    let mut ws = Workspace::single("Lead Vox", doc());
    named(&mut ws, "Kit", None);
    ws.auto_pair();
    ws.layout_mut().mark_arranged();

    // A guid nobody has: exactly what a deleted track leaves behind.
    ws.layout_mut()
        .lane_mut(0)
        .unwrap()
        .tracks
        .push("{GONE}".into());
    ws.prune_layout();

    assert_eq!(ws.lane_tracks(0).len(), 1);
    assert_eq!(ws.layout().len(), 2, "the rest of the layout survived");
}

#[test]
fn pairing_places_the_lane_by_first_appearance() {
    // Stable order, not hash order — otherwise the stack reshuffles
    // between runs for no reason the user can see.
    let mut ws = Workspace::single("Zebra", doc());
    named(&mut ws, "Apple", None);
    named(&mut ws, "Zebra MIDI", None);
    ws.auto_pair();

    assert_eq!(ws.lane_tracks(0).len(), 2, "Zebra's lane came first");
    assert_eq!(ws.track(ws.lane_tracks(1)[0]).unwrap().name, "Apple");
}

// ── Per-lane vertical cameras ────────────────────────────────────────
//
// Time is shared, vertical is per lane. The load-bearing property is
// that a lane's fit does NOT chase content: re-fitting on every edit
// would rescale a lane under the cursor mid-gesture.

use expression_editor_core::camera::VerticalCamera;

fn doc_at_rows(rows: &[i32]) -> ExpressionDoc {
    let mut d = doc();
    d.notes.clear();
    for (i, &r) in rows.iter().enumerate() {
        d.notes.push(Note::new(NoteId(i as u64 + 1), 0.0, 480.0, r));
    }
    d
}

#[test]
fn each_lane_fits_its_own_range() {
    let mut ed = Editor::new(doc_at_rows(&[36, 38]), Viewport::new(800.0, 600.0));
    ed.add_track("Piccolo", doc_at_rows(&[96, 100]));
    ed.tracks.auto_pair();
    ed.fit_lanes();

    let bass = ed.lane_camera(0).unwrap();
    let picc = ed.lane_camera(1).unwrap();
    assert!(bass.center < 60.0, "the bass lane sits low");
    assert!(picc.center > 80.0, "the piccolo lane sits high");
}

#[test]
fn zooming_one_lane_leaves_the_others_alone() {
    let mut ed = Editor::new(doc_at_rows(&[60]), Viewport::new(800.0, 600.0));
    ed.add_track("Other", doc_at_rows(&[72]));
    ed.tracks.auto_pair();
    ed.fit_lanes();

    let untouched = ed.lane_camera(1).unwrap();
    ed.lane_cameras[0].zoom_about(60.0, 2.0);

    assert_eq!(ed.lane_camera(1).unwrap(), untouched);
    assert_ne!(ed.lane_camera(0).unwrap(), untouched);
}

#[test]
fn the_horizontal_camera_is_shared_and_stays_that_way() {
    // There is exactly one time axis. If this ever stops being true,
    // two instruments doubling a line stop being comparable.
    let mut ed = Editor::new(doc_at_rows(&[60]), Viewport::new(800.0, 600.0));
    ed.add_track("Other", doc_at_rows(&[72]));
    ed.tracks.auto_pair();
    ed.fit_lanes();

    let t0 = ed.camera.t0;
    ed.camera.pan_px(50.0, 0.0);
    assert_ne!(ed.camera.t0, t0, "panning moved the one shared axis");
}

#[test]
fn an_edit_never_re_fits_a_lane() {
    let mut ed = Editor::new(doc_at_rows(&[60]), Viewport::new(800.0, 600.0));
    ed.fit_lanes();
    let before = ed.lane_camera(0).unwrap();

    // Move content far outside the fitted range.
    ed.doc.notes[0].row = 100;

    assert_eq!(
        ed.lane_camera(0).unwrap(),
        before,
        "the lane must not rescale under the cursor"
    );
}

#[test]
fn reset_view_is_what_re_fits() {
    let mut ed = Editor::new(doc_at_rows(&[60]), Viewport::new(800.0, 600.0));
    ed.fit_lanes();
    let before = ed.lane_camera(0).unwrap();

    ed.doc.notes[0].row = 100;
    ed.reset_view();

    assert_ne!(
        ed.lane_camera(0).unwrap(),
        before,
        "Reset View catches up with the content"
    );
}

#[test]
fn a_fitted_camera_maps_its_range_onto_the_lane() {
    let cam = VerticalCamera::fitted(48.0, 72.0, 240.0);
    // Centre of the range lands at the centre of the lane.
    assert!((cam.y(60.0, 0.0, 240.0) - 120.0).abs() < 1e-9);
    // And the mapping inverts.
    assert!((cam.row_at(cam.y(55.0, 0.0, 240.0), 0.0, 240.0) - 55.0).abs() < 1e-9);
}

#[test]
fn zoom_keeps_the_anchor_row_under_the_pointer() {
    let mut cam = VerticalCamera::fitted(48.0, 72.0, 240.0);
    let before = cam.y(55.0, 0.0, 240.0);
    cam.zoom_about(55.0, 2.0);
    assert!((cam.y(55.0, 0.0, 240.0) - before).abs() < 1e-9);
}

// ── Merge and split ──────────────────────────────────────────────────

#[test]
fn merging_takes_the_upper_lanes_position() {
    let mut ws = Workspace::single("A", doc());
    named(&mut ws, "B", None);
    named(&mut ws, "C", None);
    ws.auto_pair();

    // Merge in either order: the answer must be the same.
    assert!(ws.merge_lanes(2, 1));
    assert_eq!(ws.layout().len(), 2);
    assert_eq!(ws.track(ws.lane_tracks(0)[0]).unwrap().name, "A");
    assert_eq!(ws.lane_tracks(1).len(), 2, "B and C, at B's position");
    assert_eq!(ws.track(ws.lane_tracks(1)[0]).unwrap().name, "B");
}

#[test]
fn merging_takes_the_greater_weight() {
    let mut ws = Workspace::single("A", doc());
    named(&mut ws, "B", None);
    ws.auto_pair();
    ws.layout_mut().lane_mut(0).unwrap().weight = 2.0;
    ws.layout_mut().lane_mut(1).unwrap().weight = 7.0;

    ws.merge_lanes(0, 1);
    assert_eq!(ws.layout().lane(0).unwrap().weight, 7.0);
}

#[test]
fn splitting_peels_one_track_directly_below() {
    let mut ws = Workspace::single("Lead Vox", doc());
    named(&mut ws, "Lead Vox MIDI", None);
    named(&mut ws, "Kit", None);
    ws.auto_pair();
    assert_eq!(ws.layout().len(), 2);

    let midi = ws.track(1).unwrap().guid.clone();
    let at = ws.split_track_out(&midi).unwrap();

    assert_eq!(at, 1, "directly below the lane it came from");
    assert_eq!(ws.layout().len(), 3);
    assert_eq!(ws.lane_tracks(0).len(), 1, "the vocal stayed put");
    assert_eq!(ws.lane_tracks(1), vec![1]);
    assert_eq!(
        ws.track(ws.lane_tracks(2)[0]).unwrap().name,
        "Kit",
        "the kit moved down, it did not get split"
    );
}

#[test]
fn splitting_a_lone_track_does_nothing() {
    let mut ws = Workspace::single("A", doc());
    ws.auto_pair();
    let a = ws.track(0).unwrap().guid.clone();
    assert_eq!(ws.split_track_out(&a), None);
    assert_eq!(ws.layout().len(), 1);
}

#[test]
fn merge_then_split_returns_to_the_starting_arrangement() {
    let mut ws = Workspace::single("A", doc());
    named(&mut ws, "B", None);
    ws.auto_pair();
    let before: Vec<Vec<usize>> = (0..ws.layout().len()).map(|i| ws.lane_tracks(i)).collect();

    ws.merge_lanes(0, 1);
    let b = ws.track(1).unwrap().guid.clone();
    ws.split_track_out(&b);

    let after: Vec<Vec<usize>> = (0..ws.layout().len()).map(|i| ws.lane_tracks(i)).collect();
    assert_eq!(before, after);
}

#[test]
fn rearranging_marks_the_layout_worth_persisting() {
    let mut ws = Workspace::single("A", doc());
    named(&mut ws, "B", None);
    ws.auto_pair();
    assert!(!ws.layout().is_arranged());

    ws.merge_lanes(0, 1);
    assert!(ws.layout().is_arranged(), "a merge is a decision");

    // And the matcher must not undo it.
    ws.auto_pair();
    assert_eq!(ws.layout().len(), 1);
}

#[test]
fn a_split_also_marks_the_layout_arranged() {
    let mut ws = Workspace::single("Lead Vox", doc());
    named(&mut ws, "Lead Vox MIDI", None);
    ws.auto_pair();
    assert!(!ws.layout().is_arranged());

    let midi = ws.track(1).unwrap().guid.clone();
    ws.split_track_out(&midi);
    assert!(ws.layout().is_arranged());
}

#[test]
fn merging_a_lane_with_itself_is_refused() {
    let mut ws = Workspace::single("A", doc());
    named(&mut ws, "B", None);
    ws.auto_pair();
    assert!(!ws.merge_lanes(1, 1));
    assert!(!ws.merge_lanes(0, 9));
    assert_eq!(ws.layout().len(), 2);
}

// ── One active track per lane ────────────────────────────────────────
//
// Only the active track takes gestures. Proximity hit-testing is the
// trap this exists to avoid: a vocal superimposed on its reference MIDI
// puts two notes a few pixels apart, and picking the nearer one silently
// edits the reference you were tuning against.

fn paired_workspace() -> Workspace {
    let mut ws = Workspace::single("Lead Vox", doc());
    named(&mut ws, "Lead Vox MIDI", None);
    named(&mut ws, "Kit", None);
    ws.auto_pair();
    ws
}

#[test]
fn cycling_moves_within_the_lane_and_wraps() {
    let mut ed = Editor::new(doc(), Viewport::new(800.0, 600.0));
    ed.add_track("Track 1 MIDI", doc());
    ed.tracks.auto_pair();
    assert_eq!(ed.tracks.lane_tracks(0).len(), 2, "they paired");

    let first = ed.tracks.active();
    assert!(ed.cycle_track_in_lane());
    let second = ed.tracks.active();
    assert_ne!(first, second);

    assert!(ed.cycle_track_in_lane());
    assert_eq!(ed.tracks.active(), first, "and wraps back");
}

#[test]
fn cycling_never_leaves_the_lane() {
    let ws = paired_workspace();
    // Lane 0 is the vocal pair, lane 1 the kit.
    assert_eq!(ws.active_lane(), Some(0));
    let next = ws.next_in_lane(0, ws.active()).unwrap();
    assert!(ws.lane_tracks(0).contains(&next));
    assert!(!ws.lane_tracks(1).contains(&next));
}

#[test]
fn cycling_a_lone_track_is_a_no_op() {
    let mut ed = Editor::new(doc(), Viewport::new(800.0, 600.0));
    ed.add_track("Kit", doc());
    ed.tracks.auto_pair();
    let before = ed.tracks.active();
    assert!(!ed.cycle_track_in_lane(), "nothing else in this lane");
    assert_eq!(ed.tracks.active(), before);
}

#[test]
fn cycling_skips_hidden_members() {
    let mut ws = Workspace::single("Lead Vox", doc());
    named(&mut ws, "Lead Vox MIDI", None);
    named(&mut ws, "Lead Vox Ref", None);
    ws.auto_pair();
    assert_eq!(ws.lane_tracks(0).len(), 3);

    ws.track_mut(1).unwrap().hidden = true;
    let next = ws.next_in_lane(0, 0).unwrap();
    assert_eq!(next, 2, "the hidden member is not reachable by cycling");
}

#[test]
fn cycling_parks_the_document_instead_of_moving_the_index() {
    // The bug this guards: setting `active` directly would leave the
    // live document behind and hand the editor a stale one.
    let mut ed = Editor::new(doc(), Viewport::new(800.0, 600.0));
    ed.add_track("Track 1 MIDI", doc());
    ed.tracks.auto_pair();

    let marker = NoteId(9999);
    ed.doc.notes.push(Note::new(marker, 0.0, 10.0, 64));
    let moved_from = ed.tracks.active();

    assert!(ed.cycle_track_in_lane());
    assert!(
        !ed.doc.notes.iter().any(|n| n.id == marker),
        "the other track's document is now live"
    );

    assert!(ed.cycle_track_in_lane());
    assert_eq!(ed.tracks.active(), moved_from);
    assert!(
        ed.doc.notes.iter().any(|n| n.id == marker),
        "coming back restores the edit that was parked"
    );
}

// ── Fit while it fits, then scroll ───────────────────────────────────

fn many_lanes(n: usize) -> Editor {
    let mut ed = Editor::new(doc(), Viewport::new(800.0, 600.0));
    for i in 1..n {
        ed.add_track(format!("T{i}"), doc());
    }
    ed.tracks.auto_pair();
    ed
}

#[test]
fn the_floor_comes_from_the_lane_count_not_a_pixel_value() {
    let mut ed = many_lanes(3);
    ed.lanes_visible = 5;
    assert_eq!(ed.lane_floor(), 120.0, "600 / 5");
    // Same setting, taller screen, taller lanes.
    ed.viewport = Viewport::new(800.0, 1000.0);
    assert_eq!(ed.lane_floor(), 200.0);
    // More lanes on screen, shorter lanes.
    ed.lanes_visible = 10;
    assert_eq!(ed.lane_floor(), 100.0);
}

#[test]
fn with_fewer_lanes_than_the_setting_nothing_scrolls() {
    let ed = many_lanes(3);
    let total = ed.stack_height(1.0);
    assert!(
        total <= ed.viewport.h as f32 + 0.01,
        "three lanes fit in a five-lane viewport"
    );
}

#[test]
fn the_sixth_lane_starts_a_scroll_rather_than_shrinking_the_first_five() {
    let ed = many_lanes(6);
    let floor = ed.lane_floor();
    let rows = ed.tracks.stack(ed.viewport.h as f32, 1.0, floor);

    assert_eq!(rows.len(), 6);
    for r in &rows {
        assert!(
            r.height >= floor - 0.01,
            "every lane keeps the floor: {} < {floor}",
            r.height
        );
    }
    assert!(
        ed.stack_height(1.0) > ed.viewport.h as f32,
        "so the stack overflows and scrolls"
    );
}

#[test]
fn twenty_lanes_stay_readable_instead_of_becoming_equally_bad() {
    let ed = many_lanes(20);
    let floor = ed.lane_floor();
    let rows = ed.tracks.stack(ed.viewport.h as f32, 1.0, floor);
    // The old behaviour divided 600px twenty ways: 30px each.
    for r in &rows {
        assert!(r.height >= floor - 0.01);
    }
}

#[test]
fn a_viewport_too_small_for_one_lane_still_degrades_evenly() {
    let mut ed = many_lanes(4);
    ed.viewport = Viewport::new(800.0, 40.0);
    let rows = ed.tracks.stack(40.0, 1.0, ed.lane_floor());
    let total: f32 = rows.iter().map(|r| r.height).sum();
    assert!((total - 40.0).abs() < 0.01, "the last resort still fits");
}

#[test]
fn switching_to_an_off_screen_lane_scrolls_it_just_into_view() {
    let mut ed = many_lanes(10);
    assert_eq!(ed.stack_scroll, 0.0);

    let last = ed.tracks.len() - 1;
    ed.switch_track(last);

    let rows = ed
        .tracks
        .stack(ed.viewport.h as f32, Editor::ACTIVE_BOOST, ed.lane_floor());
    let lane = ed.tracks.active_lane().unwrap();
    let row = rows.iter().find(|r| r.lane == lane).unwrap();

    assert!(ed.stack_scroll > 0.0, "it scrolled");
    let bottom = (row.y + row.height) as f64;
    assert!(
        (ed.stack_scroll + ed.viewport.h - bottom).abs() < 0.01,
        "just into view, not centred"
    );
}

#[test]
fn switching_to_a_visible_lane_does_not_scroll_at_all() {
    let mut ed = many_lanes(10);
    ed.switch_track(1);
    assert_eq!(ed.stack_scroll, 0.0, "lane 1 was already on screen");
}

#[test]
fn scrolling_never_runs_past_the_ends() {
    let mut ed = many_lanes(10);
    ed.switch_track(ed.tracks.len() - 1);
    let max = (ed.stack_height(Editor::ACTIVE_BOOST) as f64 - ed.viewport.h).max(0.0);
    assert!(ed.stack_scroll <= max + 0.01);
    assert!(ed.stack_scroll >= 0.0);
}

// r[verify drums.lanes.roles]
// r[verify drums.lanes.heights]
#[test]
fn a_kit_folds_into_role_lanes_kick_at_the_bottom() {
    use expression_editor_core::kit::{LaneRole, kit_role};
    let mut ed = Editor::new(doc(), viewport());
    ed.set_mode(Mode::UnpitchedAudio);
    ed.tracks.rename(0, "In");
    let kit = [
        ("In", vec!["SUM", "Kick", "Drums"]),
        ("Out", vec!["SUM", "Kick", "Drums"]),
        ("Top", vec!["SUM", "Snare", "Drums"]),
        ("Bottom", vec!["SUM", "Snare", "Drums"]),
        ("T1 - Unused", vec!["Toms", "Drums"]),
        ("T2", vec!["Toms", "Drums"]),
        ("Hi-Hat", vec!["Drums"]),
        ("Room", vec!["Drums"]),
    ];
    let mut members = Vec::new();
    for (i, (name, folders)) in kit.iter().enumerate() {
        let guid = if i == 0 {
            ed.tracks.track(0).unwrap().guid.clone()
        } else {
            let t = Track::in_mode(*name, doc(), Mode::UnpitchedAudio);
            let g = t.guid.clone();
            ed.tracks.push(t);
            g
        };
        members.push((guid, kit_role(name, folders)));
    }
    ed.tracks.fold_roles(&members);

    let lanes = ed.tracks.layout().lanes();
    let roles: Vec<_> = lanes.iter().map(|l| l.role).collect();
    assert_eq!(
        roles,
        vec![
            Some(LaneRole::Other),
            Some(LaneRole::Toms),
            Some(LaneRole::Snare),
            Some(LaneRole::Kick)
        ],
        "top to bottom: Other, Toms, Snare, Kick"
    );
    assert_eq!(ed.tracks.role_members(LaneRole::Kick).len(), 2);
    assert_eq!(ed.tracks.role_members(LaneRole::Snare).len(), 2);
    assert_eq!(ed.tracks.role_members(LaneRole::Toms).len(), 2);
    assert_eq!(ed.tracks.role_members(LaneRole::Other).len(), 2);
    assert!(
        lanes
            .iter()
            .find(|l| l.role == Some(LaneRole::Toms))
            .unwrap()
            .split
    );
    assert!(
        !lanes
            .iter()
            .find(|l| l.role == Some(LaneRole::Kick))
            .unwrap()
            .split
    );

    // Equal heights for the four role lanes.
    let rows = ed.tracks.stack(400.0, 1.0, 10.0);
    assert_eq!(rows.len(), 4);
    let h0 = rows[0].height;
    assert!(rows.iter().all(|r| (r.height - h0).abs() < 1e-3));
    // The fold is inference, not arrangement.
    assert!(!ed.tracks.layout().is_arranged());
}
