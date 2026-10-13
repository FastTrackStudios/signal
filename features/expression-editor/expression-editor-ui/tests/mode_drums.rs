//! Drums, in depth: kit lanes, sticking, and the split that shows both
//! hands.
//!
//! The across-modes suite only asserts that a sweep reaches the drum
//! surface. What makes Drums a different product from the piano roll is
//! narrower and worth its own file: a row is a *piece of the kit* rather
//! than a pitch, a piece can be played by either hand, and the roll only
//! grows the second row when something asks it to.
//!
//! The sticking model, since the assertions below lean on it: most
//! pieces are one row, because for most parts it does not matter which
//! hand hit the snare and a roll that always asked would be twice as
//! tall for nothing. A piece splits into two rows only when a part needs
//! it — a flam, or notation that specifies sticking — and moving a hit
//! to a hand opens the piece, because a note that moved to a row you
//! cannot see has vanished as far as the user is concerned.

use std::cell::RefCell;

use dioxus::prelude::*;
use dioxus_test::keyboard_types::Modifiers;
use dioxus_test::{by_testid, render};
use expression_editor_core::doc::{ExpressionDoc, Note, NoteId, TimeBase};
use expression_editor_core::rows::DrumMap;
use expression_editor_core::{Editor, Mode, RowSpace, Tool, Viewport};
use expression_editor_ui::ExpressionEditor;

const PPQ: f64 = 960.0;

thread_local! {
    static STAGED: RefCell<Option<Editor>> = const { RefCell::new(None) };
}

fn stage(ed: Editor) {
    STAGED.with(|s| *s.borrow_mut() = Some(ed));
}

/// The FTS kit, and the row of the first piece that has two hands.
fn two_handed_row(map: &DrumMap) -> usize {
    (0..map.lanes.len())
        .find(|&r| map.other_hand_row(r).is_some())
        .expect("the FTS kit has two-handed pieces")
}

/// A drum editor with one hit on a two-handed piece.
fn drum_editor() -> (Editor, NoteId, usize) {
    let map = DrumMap::fts();
    let row = two_handed_row(&map);
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 4.0);
    doc.push(Note::new(NoteId(1), 0.0, PPQ * 0.25, row as i32));
    doc.row_space = RowSpace::Drums(map);

    let mut ed = Editor::new(doc, Viewport::new(900.0, 480.0));
    ed.set_mode(Mode::Drums);
    ed.tool = Tool::Select;
    ed.reset_view();
    (ed, NoteId(1), row)
}

#[component]
fn Surface() -> Element {
    let editor = use_signal(|| STAGED.with(|s| s.borrow_mut().take()).expect("staged"));
    let ed = editor.read();
    let readout = format!(
        "notes={} rows={} split={}",
        ed.doc.notes.len(),
        match &ed.row_space {
            RowSpace::Drums(m) => m.lanes.len(),
            _ => 0,
        },
        ed.split_pieces.len(),
    );
    drop(ed);
    rsx! {
        div { "data-testid": "readout", "{readout}" }
        ExpressionEditor { editor }
    }
}

fn field(html: &str, key: &str) -> usize {
    html.split_whitespace()
        .find_map(|kv| kv.strip_prefix(&format!("{key}="))?.parse().ok())
        .unwrap_or_else(|| panic!("no `{key}` in readout: {html}"))
}

// ── the row space ────────────────────────────────────────────────────

#[test]
fn a_row_is_a_piece_of_the_kit_not_a_pitch() {
    // The founding difference. A drum roll's row 0 is the kick, not
    // MIDI note 0, and `pitch_of_row` is the only thing that knows it.
    let map = DrumMap::fts();
    assert!(!map.lanes.is_empty());
    for row in 0..map.lanes.len() {
        let pitch = map.pitch_of_row(row).expect("every lane has a pitch");
        assert_ne!(
            pitch, row as i32,
            "row {row} and its pitch coincide, which would hide a bug \
             that treated one as the other"
        );
    }
}

#[test]
fn the_roll_is_as_tall_as_the_kit_not_the_midi_range() {
    let (ed, _, _) = drum_editor();
    let (lo, hi) = ed.doc.row_space.bounds();
    let rows = hi - lo + 1;
    assert!(
        rows < 128,
        "a drum roll {rows} rows tall is a pitch roll wearing a kit"
    );
    assert!(rows > 1, "a kit with one row is not a kit");
}

// ── sticking ─────────────────────────────────────────────────────────

#[test]
fn moving_a_hit_to_the_other_hand_moves_it_to_that_hand_s_row() {
    let (mut ed, id, row) = drum_editor();
    let RowSpace::Drums(map) = ed.row_space.clone() else {
        panic!("drums");
    };
    let hand = map.hand_of(row).expect("a two-handed piece has a hand");

    ed.selection.set_single(id);
    let moved = ed.set_hand_of_selection(hand.other());
    assert_eq!(moved, 1, "the hit did not move hands");
    assert_eq!(
        ed.hand_of_note(id),
        Some(hand.other()),
        "it reports the hand it did not move to"
    );
}

#[test]
fn moving_a_hit_to_a_hand_opens_the_piece() {
    // Otherwise the note lands on a row that is not drawn, and to the
    // user it has simply disappeared.
    let (mut ed, id, _) = drum_editor();
    assert!(ed.split_pieces.is_empty(), "starts collapsed");

    let RowSpace::Drums(map) = ed.row_space.clone() else {
        panic!("drums");
    };
    let row = ed.doc.note(id).unwrap().row as usize;
    let hand = map.hand_of(row).unwrap();
    ed.selection.set_single(id);
    ed.set_hand_of_selection(hand.other());

    assert!(
        !ed.split_pieces.is_empty(),
        "the hit moved to a hand whose row is still collapsed"
    );
}

#[test]
fn a_one_handed_piece_is_skipped_rather_than_refusing_the_gesture() {
    // A selection spanning both kinds should move what it can. Refusing
    // the lot because one hit has no other hand is the behaviour this
    // guards against.
    let map = DrumMap::fts();
    let two = two_handed_row(&map);
    let one = (0..map.lanes.len())
        .find(|&r| map.other_hand_row(r).is_none())
        .expect("the kit has one-handed pieces too");

    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 4.0);
    doc.push(Note::new(NoteId(1), 0.0, PPQ * 0.25, two as i32));
    doc.push(Note::new(NoteId(2), PPQ, PPQ * 1.25, one as i32));
    doc.row_space = RowSpace::Drums(map.clone());
    let mut ed = Editor::new(doc, Viewport::new(900.0, 480.0));
    ed.set_mode(Mode::Drums);
    ed.selection.notes = vec![NoteId(1), NoteId(2)];

    let hand = map.hand_of(two).unwrap().other();
    let moved = ed.set_hand_of_selection(hand);
    assert_eq!(moved, 1, "expected the two-handed hit alone to move");
    assert_eq!(
        ed.doc.note(NoteId(2)).unwrap().row,
        one as i32,
        "the one-handed hit was moved anyway"
    );
}

#[test]
fn sticking_survives_undo_as_one_step() {
    // It is one gesture to the user, so it has to be one to the history.
    let (mut ed, id, row) = drum_editor();
    let RowSpace::Drums(map) = ed.row_space.clone() else {
        panic!("drums");
    };
    ed.selection.set_single(id);
    ed.set_hand_of_selection(map.hand_of(row).unwrap().other());
    assert!(ed.undo(), "nothing on the history stack");
    assert_eq!(
        ed.doc.note(id).unwrap().row,
        row as i32,
        "undo left the hit on the other hand's row"
    );
}

// ── on the real surface ──────────────────────────────────────────────

#[tokio::test]
async fn the_kit_renders_every_lane_it_has() -> dioxus_test::Result<()> {
    let (ed, _, _) = drum_editor();
    let lanes = match &ed.row_space {
        RowSpace::Drums(m) => m.lanes.len(),
        _ => 0,
    };
    stage(ed);
    let tester = render(Surface).with_window_size(1000, 620).build();
    let html = tester
        .query(by_testid("readout"))
        .immediately()?
        .inner_html();
    assert_eq!(
        field(&html, "rows"),
        lanes,
        "the mounted surface lost lanes: {html}"
    );
    Ok(())
}

#[tokio::test]
async fn a_sweep_paints_hits_across_the_kit() -> dioxus_test::Result<()> {
    // Drums is the one mode where a plain drag paints rather than
    // selects — "named kit lanes, triangle heads, paint-on-drag" is the
    // mode's own description of itself.
    let (ed, _, _) = drum_editor();
    let before = ed.doc.notes.len();
    stage(ed);
    let tester = render(Surface).with_window_size(1000, 620).build();
    let el = tester.query(by_testid("roll")).immediately()?;
    let (ox, oy) = el.document_origin();
    let (w, h) = el.size();
    let x0 = ox + expression_editor_ui::canvas::GUTTER_W + 6.0;
    let y0 = oy + expression_editor_ui::canvas::RULER_H + 6.0;
    let (x1, y1) = (ox + w as f64 - 6.0, oy + h as f64 - 6.0);

    // `drain` mid-gesture, `pump` once at the end. Pumping waits for
    // work, so a pointer move that renders nothing blocks for the full
    // one-second timeout — twenty of them turned this sweep into twenty
    // seconds of waiting. See `DocumentTester::drain`.
    tester.pointer_down_mods(x0, y0, Modifiers::empty());
    tester.drain();
    for i in 1..=20 {
        let t = i as f64 / 20.0;
        tester.pointer_move_mods(
            x0 + (x1 - x0) * t,
            y0 + (y1 - y0) * t,
            true,
            Modifiers::empty(),
        );
        tester.drain();
    }
    tester.pointer_up_mods(x1, y1, Modifiers::empty());
    let _ = tester.pump().await;

    let html = tester
        .query(by_testid("readout"))
        .immediately()?
        .inner_html();
    assert!(
        field(&html, "notes") > before,
        "a diagonal sweep across the kit painted nothing (was {before}): {html}"
    );
    Ok(())
}
