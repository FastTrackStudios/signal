//! Guitar, in depth: the string roll, and the fret that is never stored.
//!
//! The model here was reversed once and corrected, and these tests exist
//! mostly to keep it corrected. A guitar roll is a **full pitch roll**:
//! the row is the sounding pitch, exactly as in MIDI mode. The *string*
//! rides on the note as an annotation, and the fret is derived from the
//! two — `row - tuning.open(string)`.
//!
//! The earlier model put the string on the vertical axis and stored the
//! fret on the note. It is a tempting shape (a guitar has six strings,
//! a roll has rows) and it is wrong: a note whose stored fret and stored
//! pitch disagree is unplayable, and there is nothing to stop them
//! disagreeing. Every unit test agreed with the old model, four of them
//! asserted it directly, and it took scenario 4 of the demo project to
//! catch. So the assertions below are about the *invariant* — fret is a
//! function of pitch and string — rather than about any stored field.

use std::cell::RefCell;

use dioxus::prelude::*;
use dioxus_test::{by_testid, render};
use expression_editor_core::doc::{ExpressionDoc, Note, NoteId, TimeBase};
use expression_editor_core::menu::Command;
use expression_editor_core::rows::{self, StringTuning};
use expression_editor_core::{Editor, Mode, RowSpace, Tool, Viewport};
use expression_editor_ui::ExpressionEditor;

const PPQ: f64 = 960.0;

thread_local! {
    static STAGED: RefCell<Option<Editor>> = const { RefCell::new(None) };
}

fn stage(ed: Editor) {
    STAGED.with(|s| *s.borrow_mut() = Some(ed));
}

/// A guitar editor with one note fingered at `(string, fret)`.
fn guitar_editor(string: u8, fret: u8) -> (Editor, StringTuning) {
    let tuning = StringTuning::guitar_standard();
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 4.0);
    let mut n = Note::new(NoteId(1), 0.0, PPQ, tuning.pitch(string as usize, fret));
    n.string = Some(string);
    doc.push(n);
    doc.row_space = RowSpace::Strings(tuning.clone());

    let mut ed = Editor::new(doc, Viewport::new(900.0, 480.0));
    ed.set_mode(Mode::Guitar);
    ed.tool = Tool::Select;
    ed.reset_view();
    (ed, tuning)
}

#[component]
fn Surface() -> Element {
    let editor = use_signal(|| STAGED.with(|s| s.borrow_mut().take()).expect("staged"));
    let ed = editor.read();
    let n = ed.doc.notes.first();
    let readout = format!(
        "notes={} row={} string={} strings={}",
        ed.doc.notes.len(),
        n.map(|n| n.row).unwrap_or(-1),
        n.and_then(|n| n.string).map(|s| s as i32).unwrap_or(-1),
        match &ed.row_space {
            RowSpace::Strings(t) => t.strings(),
            _ => 0,
        },
    );
    drop(ed);
    rsx! {
        div { "data-testid": "readout", "{readout}" }
        ExpressionEditor { editor }
    }
}

fn field(html: &str, key: &str) -> i64 {
    html.split_whitespace()
        .find_map(|kv| kv.strip_prefix(&format!("{key}="))?.parse().ok())
        .unwrap_or_else(|| panic!("no `{key}` in readout: {html}"))
}

// ── the model ────────────────────────────────────────────────────────

#[test]
fn the_row_is_the_sounding_pitch_not_the_string() {
    // The reversal this file exists to prevent. Two notes on different
    // strings at the same pitch must sit on the same row.
    let tuning = StringTuning::guitar_standard();
    // E4 = 64: open high E, and 5th fret of the B string.
    let a = tuning.pitch(5, 0);
    let b = tuning.pitch(4, 5);
    assert_eq!(a, b, "these should be the same pitch");

    let (ed, _) = guitar_editor(5, 0);
    assert_eq!(
        ed.doc.notes[0].row, a,
        "the row is not the pitch — the string roll has been flipped back"
    );
}

#[test]
fn the_fret_is_derived_and_never_stored() {
    let (ed, tuning) = guitar_editor(2, 7);
    let n = &ed.doc.notes[0];
    assert_eq!(
        rows::fret_of(n, &tuning),
        Some(7),
        "the fret does not come back out of pitch and string"
    );
    // And it follows the pitch: transposing changes the fret without
    // anything having to update a stored copy.
    let mut ed = ed;
    let id = ed.doc.notes[0].id;
    ed.doc.note_mut(id).unwrap().row += 2;
    assert_eq!(rows::fret_of(&ed.doc.notes[0], &tuning), Some(9));
}

#[test]
fn a_note_with_no_string_has_no_fret_but_is_still_a_note() {
    // Legitimate: a plain MIDI note on a guitar track. It must not be a
    // parse error, and it must not claim a fret.
    let tuning = StringTuning::guitar_standard();
    let n = Note::new(NoteId(1), 0.0, PPQ, 60);
    assert_eq!(rows::fret_of(&n, &tuning), None);
}

// ── re-fingering ─────────────────────────────────────────────────────

#[test]
fn cycling_string_keeps_the_pitch_and_changes_the_fret() {
    // Re-fingering, not transposition: the same note, played somewhere
    // else on the neck. If the pitch moved, this would be a wrong-note
    // generator.
    let (mut ed, tuning) = guitar_editor(5, 5);
    let id = ed.doc.notes[0].id;
    let pitch = ed.doc.notes[0].row;
    let fret_before = rows::fret_of(&ed.doc.notes[0], &tuning);

    assert!(ed.run_command(&Command::CycleString(id), None), "no cycle");
    assert_eq!(ed.doc.notes[0].row, pitch, "re-fingering moved the pitch");
    assert_ne!(
        rows::fret_of(&ed.doc.notes[0], &tuning),
        fret_before,
        "the fret did not change, so nothing was re-fingered"
    );
}

#[test]
fn every_string_a_cycle_lands_on_can_actually_play_the_note() {
    // The bug this replaced: `string + 1` walked onto strings the pitch
    // cannot reach, where the fret is negative and the position is
    // imaginary.
    let (mut ed, tuning) = guitar_editor(5, 5);
    let id = ed.doc.notes[0].id;
    let pitch = ed.doc.notes[0].row;

    for _ in 0..12 {
        if !ed.run_command(&Command::CycleString(id), None) {
            break;
        }
        let fret = rows::fret_of(&ed.doc.notes[0], &tuning).expect("still fingered");
        assert!(
            (0..=tuning.frets as i32).contains(&fret),
            "cycled onto an unplayable position: fret {fret}"
        );
        assert_eq!(
            ed.doc.notes[0].row, pitch,
            "the pitch drifted while cycling"
        );
    }
}

#[test]
fn setting_a_fret_slides_up_the_neck() {
    // The other direction: the fret is what the user names, and the
    // pitch follows. Keeping the string is what makes it a slide rather
    // than a re-fingering.
    let (mut ed, tuning) = guitar_editor(2, 5);
    let id = ed.doc.notes[0].id;
    ed.selection.set_single(id);
    assert!(ed.set_fret_of_selection(9));

    let n = &ed.doc.notes[0];
    assert_eq!(n.string, Some(2), "the hand left the string");
    assert_eq!(
        n.row,
        tuning.pitch(2, 9),
        "the pitch did not follow the fret"
    );
}

// ── on the real surface ──────────────────────────────────────────────

#[tokio::test]
async fn the_string_roll_mounts_with_its_tuning_intact() -> dioxus_test::Result<()> {
    let (ed, tuning) = guitar_editor(3, 4);
    let row = ed.doc.notes[0].row;
    stage(ed);
    let tester = render(Surface).with_window_size(1000, 620).build();
    let html = tester
        .query(by_testid("readout"))
        .immediately()?
        .inner_html();

    assert_eq!(field(&html, "strings"), tuning.strings() as i64, "{html}");
    assert_eq!(
        field(&html, "string"),
        3,
        "the string annotation was lost: {html}"
    );
    assert_eq!(
        field(&html, "row"),
        row as i64,
        "the row changed on mount, which would move the pitch: {html}"
    );
    Ok(())
}

#[tokio::test]
async fn a_six_string_roll_is_still_a_full_pitch_roll() -> dioxus_test::Result<()> {
    // Six strings, not six rows: the vertical axis spans playable
    // pitches. A roll six rows tall would be the reversed model back.
    let (ed, _) = guitar_editor(0, 0);
    let (lo, hi) = ed.doc.row_space.bounds();
    assert!(
        hi - lo > 12,
        "the string roll is only {} rows tall — the axis is strings again",
        hi - lo + 1
    );
    stage(ed);
    let tester = render(Surface).with_window_size(1000, 620).build();
    let el = tester.query(by_testid("roll")).immediately()?;
    let (w, h) = el.size();
    assert!(w > 0.0 && h > 0.0);
    Ok(())
}
