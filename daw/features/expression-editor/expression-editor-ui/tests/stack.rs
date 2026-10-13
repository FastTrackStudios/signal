//! The stacked multitrack view.
//!
//! The claim under test is the one the feature exists to support: two
//! things that happened at the same moment are drawn at the same x,
//! whatever mode their tracks are in and whatever units their documents
//! use.

use expression_editor_core::rows::{RowSpace, SliceBands, StringTuning};
use expression_editor_core::tracks::Track;
use expression_editor_core::{Editor, ExpressionDoc, Mode, Note, NoteId, TimeBase, Viewport};
use expression_editor_ui::stack;

fn viewport() -> Viewport {
    Viewport {
        w: 1200.0,
        h: 600.0,
    }
}

/// A document in frames at `rate`, with a note at each given *second*.
fn frames_doc(rate: f64, seconds: &[f64], row: i32) -> ExpressionDoc {
    let mut doc = ExpressionDoc::new(TimeBase::Frames { frame_rate: rate }, 0.0, rate * 4.0);
    for (i, s) in seconds.iter().enumerate() {
        doc.push(Note::new(
            NoteId(i as u64 + 1),
            s * rate,
            (s + 0.25) * rate,
            row,
        ));
    }
    doc
}

/// A MIDI document in ticks, with a note at each given second at 120 bpm.
fn ppq_doc(seconds: &[f64], row: i32) -> ExpressionDoc {
    let ppq = 960.0;
    // 120 bpm → 2 beats per second.
    let per_second = ppq * 2.0;
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq }, 0.0, per_second * 4.0);
    for (i, s) in seconds.iter().enumerate() {
        doc.push(Note::new(
            NoteId(i as u64 + 1),
            s * per_second,
            (s + 0.25) * per_second,
            row,
        ));
    }
    doc
}

/// Vocal (pitch hop), reference MIDI (ticks), guitar, kit (onset hop) —
/// all with an event at exactly 1.0 s.
fn band() -> Editor {
    let hits = [0.5, 1.0, 1.5];
    let mut ed = Editor::new(frames_doc(172.265625, &hits, 62), viewport());
    ed.set_mode(Mode::PitchedAudio);
    ed.tracks.rename(0, "Lead Vox");

    ed.tracks
        .push(Track::in_mode("Ref MIDI", ppq_doc(&hits, 62), Mode::Midi));

    // Rows are sounding pitches; the fixture's notes have to sit on the
    // neck or they are off the row space entirely.
    let tuning = StringTuning::guitar_standard();
    let mut guitar = frames_doc(100.0, &hits, 2);
    for (i, n) in guitar.notes.iter_mut().enumerate() {
        n.string = Some((i % tuning.strings()) as u8);
        n.row = tuning.open(i % tuning.strings()) + 5;
    }
    guitar.row_space = RowSpace::Strings(tuning);
    ed.tracks
        .push(Track::in_mode("Guitar", guitar, Mode::Guitar));

    // The kit is analysed at a different hop from the vocal — the case
    // that makes the shared timeline non-trivial.
    let mut kit = frames_doc(86.1328125, &hits, 1);
    kit.row_space = RowSpace::Bands(SliceBands::default());
    ed.tracks
        .push(Track::in_mode("Kit", kit, Mode::UnpitchedAudio));

    ed
}

#[test]
fn a_lane_per_track_in_workspace_order() {
    let ed = band();
    let lanes = stack::lanes(&ed, 1.0, 12.0);
    assert_eq!(
        lanes.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(),
        vec!["Lead Vox", "Ref MIDI", "Guitar", "Kit"]
    );
    assert_eq!(
        lanes.iter().map(|l| l.mode).collect::<Vec<_>>(),
        vec![
            Mode::PitchedAudio,
            Mode::Midi,
            Mode::Guitar,
            Mode::UnpitchedAudio
        ]
    );
    assert!(lanes[0].active);
    assert!(lanes[1..].iter().all(|l| !l.active));
}

#[test]
fn events_at_the_same_moment_share_an_x_across_every_time_base() {
    // The whole point. A vocal in pitch-hop frames, a MIDI reference in
    // ticks, a guitar at 100 fps and a kit at a third rate all have an
    // event at 1.0 s, and all four must be drawn at the same x. Without
    // the conversion through seconds they land at four different places
    // — and it looks entirely plausible.
    let ed = band();
    let lanes = stack::lanes(&ed, 1.0, 12.0);

    let xs: Vec<f64> = lanes
        .iter()
        .map(|l| {
            // The middle note of the three is the one at 1.0 s.
            l.notes[1].x
        })
        .collect();
    let first = xs[0];
    for (lane, x) in lanes.iter().zip(&xs) {
        assert!(
            (x - first).abs() < 1.0,
            "'{}' put 1.0s at x={x}, the vocal put it at {first}",
            lane.name
        );
    }
}

#[test]
fn lanes_tile_the_viewport_without_overlapping() {
    let ed = band();
    let lanes = stack::lanes(&ed, 1.0, 12.0);
    for pair in lanes.windows(2) {
        assert!(
            pair[0].y + pair[0].h <= pair[1].y + 1e-3,
            "'{}' overlaps '{}'",
            pair[0].name,
            pair[1].name
        );
    }
    let last = lanes.last().unwrap();
    assert!(last.y + last.h <= 600.0 + 1e-3);
}

#[test]
fn every_note_is_drawn_inside_its_own_lane() {
    // A note escaping its lane draws over the neighbouring track, which
    // reads as that track having a note it does not have.
    let ed = band();
    for lane in stack::lanes(&ed, 1.0, 12.0) {
        for n in &lane.notes {
            assert!(
                n.y >= lane.y - 0.5 && n.y + n.h <= lane.y + lane.h + 0.5,
                "'{}' drew a note at y={}..{} outside its lane {}..{}",
                lane.name,
                n.y,
                n.y + n.h,
                lane.y,
                lane.y + lane.h
            );
        }
    }
}

#[test]
fn a_kit_fills_its_lane_rather_than_a_twentieth_of_it() {
    // Three bands over the same rows-per-pixel as 128 pitch rows would
    // leave the kit a sliver. Each lane fits its own row space.
    let ed = band();
    let lanes = stack::lanes(&ed, 1.0, 12.0);
    let kit = lanes.iter().find(|l| l.name == "Kit").unwrap();

    let top = kit.notes.iter().map(|n| n.y).fold(f64::INFINITY, f64::min);
    let bottom = kit.notes.iter().map(|n| n.y + n.h).fold(0.0f64, f64::max);
    // The kit's hits are all in one band, so they occupy about a third
    // of the lane — the point is that the band is a third, not a
    // fortieth.
    let used = bottom - top;
    assert!(
        used > kit.h / 6.0,
        "a band occupies {used}px of a {}px lane",
        kit.h
    );
}

#[test]
fn a_slice_too_short_to_see_is_still_given_width() {
    // At a whole-song zoom a hit is a fraction of a pixel. A zero-width
    // rect is a track that looks empty.
    let mut ed = band();
    ed.camera.units_per_px = 500.0;
    let lanes = stack::lanes(&ed, 1.0, 12.0);
    let kit = lanes.iter().find(|l| l.name == "Kit").unwrap();
    assert!(!kit.notes.is_empty());
    assert!(kit.notes.iter().all(|n| n.w >= 1.0));
}

#[test]
fn named_row_spaces_get_dividers_and_pitch_does_not() {
    let ed = band();
    let lanes = stack::lanes(&ed, 1.0, 12.0);
    let of = |name: &str| {
        let l = lanes.iter().find(|l| l.name == name).unwrap();
        (l.dividers.len(), l.labels.len())
    };
    // Three bands — each row is a named thing worth a line. The guitar
    // is a pitch roll now, so it gets the same treatment as pitch: no
    // line per semitone.
    assert_eq!(of("Kit"), (3, 3));
    assert_eq!(of("Guitar"), (0, 0));
    // 128 semitones inside a 150 px lane is a grey block, and pitch has
    // a keyboard of its own to read.
    assert_eq!(of("Lead Vox"), (0, 0));
    assert_eq!(of("Ref MIDI"), (0, 0));
}

#[test]
fn the_active_lane_is_drawn_at_full_strength_and_the_others_dimmed() {
    let ed = band();
    let lanes = stack::lanes(&ed, 1.0, 12.0);
    let active = lanes.iter().find(|l| l.active).unwrap();
    assert!(active.notes.iter().all(|n| !n.fill.ends_with("80")));
    for parked in lanes.iter().filter(|l| !l.active) {
        assert!(
            parked.notes.iter().all(|n| n.fill.ends_with("80")),
            "'{}' should be dimmed",
            parked.name
        );
    }
}

#[test]
fn the_active_lane_shows_live_edits_not_the_parked_copy() {
    // The active track's slot holds a stale document by design. A stack
    // that drew from it would show the state before the last edit, which
    // is the subtlest possible way for this view to lie.
    let mut ed = band();
    let before = stack::lanes(&ed, 1.0, 12.0)[0].notes.len();
    ed.doc.push(Note::new(NoteId(99), 300.0, 340.0, 64));
    let after = stack::lanes(&ed, 1.0, 12.0)[0].notes.len();
    assert_eq!(after, before + 1);
}

#[test]
fn switching_track_moves_which_lane_is_live() {
    let mut ed = band();
    assert!(ed.switch_track(3));
    let lanes = stack::lanes(&ed, 1.0, 12.0);
    assert!(lanes[3].active);
    assert!(!lanes[0].active);
    // And the vocal is drawn from its parked copy, which is now the
    // authoritative one for it.
    assert_eq!(lanes[0].notes.len(), 3);
}

#[test]
fn hidden_tracks_do_not_get_a_lane() {
    let mut ed = band();
    ed.tracks.track_mut(2).unwrap().hidden = true;
    let lanes = stack::lanes(&ed, 1.0, 12.0);
    assert_eq!(
        lanes.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(),
        vec!["Lead Vox", "Ref MIDI", "Kit"]
    );
}

#[test]
fn an_empty_track_gets_a_sane_lane_rather_than_all_128_rows() {
    // Fitting to no content has no answer; falling back to the full
    // pitch range would draw anything later loaded as a smear at the
    // bottom of the lane.
    let mut ed = band();
    ed.tracks.push(Track::in_mode(
        "Empty",
        ExpressionDoc::new(TimeBase::Frames { frame_rate: 100.0 }, 0.0, 400.0),
        Mode::Midi,
    ));
    let lanes = stack::lanes(&ed, 1.0, 12.0);
    let empty = lanes.iter().find(|l| l.name == "Empty").unwrap();
    assert!(empty.notes.is_empty());
    assert!(empty.h > 0.0);
}

#[test]
fn slices_and_drum_hits_draw_as_triangles_and_sung_notes_do_not() {
    let ed = band();
    let lanes = stack::lanes(&ed, 1.0, 12.0);
    let kit = lanes.iter().find(|l| l.name == "Kit").unwrap();
    assert!(kit.notes.iter().all(|n| n.triangle));
    let vox = lanes.iter().find(|l| l.name == "Lead Vox").unwrap();
    assert!(vox.notes.iter().all(|n| !n.triangle));
}

// ── Guitar colour by string, or by pitch class ───────────────────────

#[test]
fn guitar_notes_colour_by_string_by_default_and_by_pitch_when_off() {
    use expression_editor_core::rows::{RowSpace, StringTuning};

    let tuning = StringTuning::guitar_standard();
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: 960.0 }, 0.0, 3840.0);
    doc.row_space = RowSpace::Strings(tuning.clone());
    // Two notes on *different* strings. By string they differ; by pitch
    // class they are both open strings a fourth apart, so they differ
    // there too — what matters is that the two modes disagree.
    // The same sounding pitch, fingered two ways: the fifth fret of
    // the low E and the open A. By string they differ; by pitch class
    // they cannot, which is the sharpest form of the distinction.
    let mut a = Note::new(NoteId(1), 0.0, 480.0, tuning.open(0) + 5);
    a.string = Some(0);
    let mut b = Note::new(NoteId(2), 960.0, 1440.0, tuning.open(1));
    b.string = Some(1);
    doc.push(a);
    doc.push(b);

    let mut ed = Editor::new(doc, Viewport::new(900.0, 500.0));
    ed.set_mode(expression_editor_core::Mode::Guitar);
    ed.reset_view();

    assert!(
        ed.color_by_string,
        "on by default — it is what shows the fingering"
    );

    let by_string = expression_editor_ui::stack::lanes(&ed, 1.0, 20.0);
    ed.color_by_string = false;
    let by_pitch = expression_editor_ui::stack::lanes(&ed, 1.0, 20.0);

    let fills = |ls: &[expression_editor_ui::stack::LaneView]| -> Vec<String> {
        ls.iter()
            .flat_map(|l| l.notes.iter().map(|n| n.fill.clone()))
            .collect()
    };
    assert_ne!(
        fills(&by_string),
        fills(&by_pitch),
        "the toggle changed nothing"
    );
}

#[test]
fn the_toggle_leaves_non_guitar_rows_alone() {
    // Drums and bands carry their own row colours; the option is about
    // string rolls only.
    use expression_editor_core::rows::{DrumMap, RowSpace};
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: 960.0 }, 0.0, 3840.0);
    doc.row_space = RowSpace::Drums(DrumMap::fts());
    doc.push(Note::new(NoteId(1), 0.0, 120.0, 1));

    let mut ed = Editor::new(doc, Viewport::new(900.0, 500.0));
    ed.set_mode(expression_editor_core::Mode::Drums);
    ed.reset_view();

    let on = expression_editor_ui::stack::lanes(&ed, 1.0, 20.0);
    ed.color_by_string = false;
    let off = expression_editor_ui::stack::lanes(&ed, 1.0, 20.0);

    let fill = |ls: &[expression_editor_ui::stack::LaneView]| {
        ls.iter()
            .flat_map(|l| l.notes.iter().map(|n| n.fill.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(fill(&on), fill(&off));
}
