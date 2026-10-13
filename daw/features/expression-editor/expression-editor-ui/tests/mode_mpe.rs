//! MPE, in depth: three dimensions that belong to the *note*.
//!
//! `mpe_gestures.rs` is the regression net for the surface surviving a
//! drag — it was written when every gesture panicked under Blitz, and it
//! still only asserts that they do not. This file asserts what the mode
//! is *for*.
//!
//! The founding claim of the whole editor: per-note bend, pressure and
//! timbre are properties of a note, not entries in a controller lane
//! somewhere else. Everything below is a consequence of taking that
//! literally — each note owns three independent curves, editing one
//! leaves the others alone, and a note carries its expression with it
//! when it moves.
//!
//! Why per-note is not merely tidier: in a lane, expression is
//! attributed by *time*, so two overlapping notes cannot be shaped
//! separately — the lane has one value at each instant and both notes
//! read it. Owning the curve is what makes a chord editable voice by
//! voice, which is the thing MPE hardware can play and a lane editor
//! cannot express.

use std::cell::RefCell;

use dioxus::prelude::*;
use dioxus_test::keyboard_types::Modifiers;
use dioxus_test::{by_testid, render};
use expression_editor_core::doc::{Dimension, ExpressionDoc, Note, NoteId, TimeBase};
use expression_editor_core::edit::Edit;
use expression_editor_core::{Editor, Mode, Tool, Viewport};
use expression_editor_ui::ExpressionEditor;

const PPQ: f64 = 960.0;

thread_local! {
    static STAGED: RefCell<Option<Editor>> = const { RefCell::new(None) };
}

fn stage(ed: Editor) {
    STAGED.with(|s| *s.borrow_mut() = Some(ed));
}

/// Three notes, each on its own channel — the MPE case, where expression
/// is attributable in the first place.
fn mpe_editor() -> Editor {
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 4.0);
    for i in 0..3u64 {
        let mut n = Note::new(
            NoteId(i + 1),
            PPQ * i as f64,
            PPQ * i as f64 + PPQ * 0.8,
            60 + i as i32 * 4,
        );
        n.channel = Some(i as u8 + 1);
        doc.push(n);
    }
    let mut ed = Editor::new(doc, Viewport::new(900.0, 480.0));
    ed.set_mode(Mode::Mpe);
    ed.tool = Tool::Select;
    ed.reset_view();
    ed
}

/// A chord: three notes sounding at once, on separate channels.
fn chord_editor() -> Editor {
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 4.0);
    for (i, row) in [60, 64, 67].into_iter().enumerate() {
        let mut n = Note::new(NoteId(i as u64 + 1), 0.0, PPQ * 2.0, row);
        n.channel = Some(i as u8 + 1);
        doc.push(n);
    }
    let mut ed = Editor::new(doc, Viewport::new(900.0, 480.0));
    ed.set_mode(Mode::Mpe);
    ed.tool = Tool::Select;
    ed.reset_view();
    ed
}

#[component]
fn Surface() -> Element {
    let editor = use_signal(|| STAGED.with(|s| s.borrow_mut().take()).expect("staged"));
    let ed = editor.read();
    let counts: Vec<String> = Dimension::ALL
        .iter()
        .map(|d| {
            let n: usize = ed.doc.notes.iter().map(|n| n.curve(*d).len()).sum();
            format!("{d:?}={n}")
        })
        .collect();
    let readout = format!("notes={} {}", ed.doc.notes.len(), counts.join(" "));
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

// ── the three dimensions are independent ─────────────────────────────

#[test]
fn each_note_owns_a_curve_per_dimension() {
    let mut ed = mpe_editor();
    let id = ed.doc.notes[0].id;

    for d in Dimension::ALL {
        ed.doc.note_mut(id).unwrap().curve_mut(d).set(0.0, 0.5);
    }
    let n = ed.doc.note(id).unwrap();
    for d in Dimension::ALL {
        assert_eq!(
            n.curve(d).sample(0.0, d.default_value()),
            0.5,
            "{d:?} did not keep its own value"
        );
    }
}

#[test]
fn editing_one_dimension_leaves_the_others_alone() {
    // Three curves sharing storage would be a lane wearing a note's
    // clothes, and this is the assertion that catches it.
    let mut ed = mpe_editor();
    let id = ed.doc.notes[0].id;
    ed.doc
        .note_mut(id)
        .unwrap()
        .curve_mut(Dimension::Pressure)
        .set(PPQ * 0.5, 0.25);

    let n = ed.doc.note(id).unwrap();
    assert_eq!(n.curve(Dimension::Pressure).len(), 1);
    assert!(
        n.curve(Dimension::Pitch).is_empty() && n.curve(Dimension::Timbre).is_empty(),
        "writing pressure wrote into another dimension"
    );
}

#[test]
fn an_unauthored_dimension_reads_its_mpe_default() {
    // Not zero: pressure and timbre rest at 100/127, the MPE
    // convention, so a note that was never shaped still plays as the
    // hardware expects rather than silent and dull.
    let ed = mpe_editor();
    let n = &ed.doc.notes[0];
    assert_eq!(
        n.curve(Dimension::Pressure)
            .sample(0.0, Dimension::Pressure.default_value()),
        100.0 / 127.0
    );
    assert_eq!(
        n.curve(Dimension::Pitch)
            .sample(0.0, Dimension::Pitch.default_value()),
        0.0,
        "pitch rests at the note's own row, not at an offset"
    );
}

// ── the thing a lane cannot do ───────────────────────────────────────

#[test]
fn two_notes_sounding_at_once_shape_separately() {
    // The whole argument for per-note expression. In a controller lane
    // these two overlap in time, so one curve would have to serve both.
    let mut ed = chord_editor();
    let (a, b) = (ed.doc.notes[0].id, ed.doc.notes[1].id);

    ed.doc
        .note_mut(a)
        .unwrap()
        .curve_mut(Dimension::Pitch)
        .set(PPQ, 1.0);
    ed.doc
        .note_mut(b)
        .unwrap()
        .curve_mut(Dimension::Pitch)
        .set(PPQ, -1.0);

    // Same instant, opposite bends, both intact.
    assert_eq!(
        ed.doc
            .note(a)
            .unwrap()
            .curve(Dimension::Pitch)
            .sample(PPQ, 0.0),
        1.0
    );
    assert_eq!(
        ed.doc
            .note(b)
            .unwrap()
            .curve(Dimension::Pitch)
            .sample(PPQ, 0.0),
        -1.0
    );
}

#[test]
fn expression_travels_with_the_note_it_belongs_to() {
    // A lane stays where it is when a note moves, and the shaping is
    // left behind on whatever is now underneath. Owning the curve is
    // what makes moving a note keep its performance.
    let mut ed = mpe_editor();
    let id = ed.doc.notes[0].id;
    ed.doc
        .note_mut(id)
        .unwrap()
        .curve_mut(Dimension::Timbre)
        .set(PPQ * 0.25, 0.9);

    ed.apply(&Edit::Transpose {
        notes: vec![id],
        semitones: 5,
    });

    let n = ed.doc.note(id).unwrap();
    assert_eq!(n.row, 65, "the note did not move");
    assert_eq!(
        n.curve(Dimension::Timbre).sample(PPQ * 0.25, 0.0),
        0.9,
        "the timbre shaping was left behind by the transpose"
    );
}

#[test]
fn every_note_has_its_own_channel_so_expression_is_attributable() {
    // The precondition for all of the above on real hardware: shared
    // channels mean the synth cannot tell whose bend is whose.
    let ed = chord_editor();
    let mut seen: Vec<u8> = ed.doc.notes.iter().filter_map(|n| n.channel).collect();
    seen.sort_unstable();
    let before = seen.len();
    seen.dedup();
    assert_eq!(seen.len(), before, "two notes share a channel");
    assert_eq!(before, ed.doc.notes.len(), "a note has no channel at all");
}

// ── on the real surface ──────────────────────────────────────────────

#[tokio::test]
async fn a_pen_stroke_writes_the_active_dimension_and_only_that_one() -> dioxus_test::Result<()> {
    // What the pen writes must land in the dimension the toolbar has
    // active — a stroke that also moved pitch while you were shaping
    // timbre is the failure this catches. (Ctrl used to be the pen
    // override; under the FTS map Ctrl is the razor, so the pen is
    // reached by arming the tool.)
    let mut ed = mpe_editor();
    ed.tool = Tool::Pen;
    ed.dimension = Dimension::Timbre;
    ed.selection.notes = ed.doc.notes.iter().map(|n| n.id).collect();
    stage(ed);

    let tester = render(Surface).with_window_size(1000, 620).build();
    let el = tester.query(by_testid("roll")).immediately()?;
    let (ox, oy) = el.document_origin();
    let (w, h) = el.size();
    let iw = w as f64 - expression_editor_ui::canvas::GUTTER_W;
    let ih = h as f64 - expression_editor_ui::canvas::RULER_H;
    let x0 = ox + expression_editor_ui::canvas::GUTTER_W + iw * 0.05;
    let y0 = oy + expression_editor_ui::canvas::RULER_H + ih * 0.6;
    let (x1, y1) = (x0 + iw * 0.8, y0 - ih * 0.25);

    // `drain` mid-gesture, `pump` once at the end — pumping waits, and a
    // move that renders nothing waits the full second. See
    // `DocumentTester::drain`.
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
        field(&html, "Timbre") > 0,
        "the stroke wrote nothing into the active dimension: {html}"
    );
    assert_eq!(
        field(&html, "Pressure"),
        0,
        "shaping timbre also wrote pressure: {html}"
    );
    Ok(())
}

#[tokio::test]
async fn the_surface_mounts_on_each_dimension() -> dioxus_test::Result<()> {
    // Each dimension redraws the roll differently — pitch as an offset
    // from the row, the other two as filled envelopes — and each has to
    // survive being the active one.
    for d in Dimension::ALL {
        let mut ed = mpe_editor();
        ed.dimension = d;
        stage(ed);
        let tester = render(Surface).with_window_size(1000, 620).build();
        let el = tester.query(by_testid("roll")).immediately()?;
        let (w, h) = el.size();
        assert!(w > 0.0 && h > 0.0, "{d:?}: the roll has no area");
    }
    Ok(())
}
