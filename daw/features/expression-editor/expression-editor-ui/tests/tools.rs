//! Every editing gesture, driven by pointer on the real surface.
//!
//! These mount `ExpressionEditor` on the headless Blitz DOM and drive
//! the same event path a mouse takes, so what is asserted is that the
//! tool *works from the canvas* — not that a reducer called by hand
//! produces the right value. The unit tests in `expression-editor-core`
//! already cover the second thing; what they cannot see is a gesture
//! that never reaches the tool because of hit-testing, a modifier map,
//! or Blitz's event dispatch.
//!
//! ## What governs a gesture is the mouse map, not the armed tool
//!
//! This started out as one test per `Tool` and that was the wrong shape.
//! `pointer_down` resolves `ed.mouse` first and only falls through to the
//! tool-driven path when the map returns `Action::None` — and the shipped
//! `reaper_like` map covers every modifier combination on the roll and on
//! a note:
//!
//! ```text
//! PianoRoll  drag             MarqueeSelect      Note  drag       MoveNote
//! PianoRoll  alt+drag         PaintNotes         Note  alt+drag   CopyNote
//! PianoRoll  ctrl+drag        PenOverride        Note  ctrl+drag  EditNoteVelocity
//! PianoRoll  shift+drag       MarqueeAdd         Note  shift+drag MoveNoteOneAxis
//! PianoRoll  alt+shift+drag   RazorCreate
//! ```
//!
//! So a plain drag with the Pen armed marquees — correctly — and a test
//! that expected otherwise was testing a thing the surface does not do.
//! These drive the map's own gestures, which is what a user has.
//!
//! The readout element is how state leaves the component. A test cannot
//! reach into a `use_signal`, so `Surface` renders the few numbers these
//! assertions need into a `data-testid="readout"` div and the tests read
//! it back — the same trick `slider_drag.rs` uses.

use std::cell::RefCell;

use dioxus::prelude::*;
use dioxus_test::keyboard_types::Modifiers;
use dioxus_test::{DocumentTester, ResolvedElement, by_testid, render};
use expression_editor_core::doc::{Dimension, ExpressionDoc, Note, NoteId, TimeBase};
use expression_editor_core::{Editor, Mode, Tool, Viewport};
use expression_editor_ui::ExpressionEditor;

const PPQ: f64 = 960.0;

/// The window every gesture here is made in. One constant, because the
/// editor is told this same size and the two must agree.
const WINDOW: (u32, u32) = (1000, 620);

thread_local! {
    /// The editor the next `Surface` mounts on.
    ///
    /// `render` takes a bare `fn() -> Element`, so the case under test
    /// cannot arrive as a prop. Staging it is the same hand-off the
    /// standalone runner uses for exactly this reason.
    static STAGED: RefCell<Option<Editor>> = const { RefCell::new(None) };
}

fn stage(ed: Editor) {
    STAGED.with(|s| *s.borrow_mut() = Some(ed));
}

/// Four notes in a row, mid-register — enough to hit, marquee and erase.
fn editor_with_notes(tool: Tool, mode: Mode) -> Editor {
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 4.0);
    for i in 0..4u64 {
        let n = Note::new(
            NoteId(i + 1),
            PPQ * i as f64 * 0.9,
            PPQ * i as f64 * 0.9 + PPQ * 0.7,
            60 + i as i32 * 3,
        );
        doc.push(n);
    }
    let mut ed = Editor::new(doc, Viewport::new(900.0, 480.0));
    ed.set_mode(mode);
    ed.tool = tool;
    ed.dimension = Dimension::Pitch;
    // Fit the fixture to the window it is about to be mounted in, so the
    // camera these tests compute note positions from is the camera the
    // surface is actually drawn with. Left at an arbitrary size, the
    // editor resized itself on mount and every precomputed coordinate
    // pointed at where the note used to be.
    expression_editor_ui::sizing::fit(&mut ed, WINDOW.0 as f64, WINDOW.1 as f64, true);
    ed.reset_view();
    ed
}

/// The editor, plus the numbers a gesture should have moved.
#[component]
fn Surface() -> Element {
    // What a host does. Without it the editor keeps the viewport its
    // fixture was built with, which need not match the cell it is laid
    // out in — and the roll would then overflow and be clipped.
    use_hook(|| expression_editor_ui::available_space(WINDOW.0 as f64, WINDOW.1 as f64));
    let editor = use_signal(|| {
        STAGED
            .with(|s| s.borrow_mut().take())
            .unwrap_or_else(|| editor_with_notes(Tool::Select, Mode::Midi))
    });
    let ed = editor.read();
    let points: usize = ed
        .doc
        .notes
        .iter()
        .map(|n| n.curve(Dimension::Pitch).len())
        .sum();
    // The most recently added note's length, in whole grid cells. The
    // insert gesture's two phases differ in exactly this, and a length
    // in document units would make the assertion a magic number.
    let step = ed.grid.step(ed.units_per_beat());
    let cells = ed
        .doc
        .notes
        .last()
        .map(|n| ((n.end - n.start) / step).round() as usize)
        .unwrap_or(0);
    let readout = format!(
        "notes={} sel={} points={} undo={} cells={cells} razors={} tool={:?} dim={:?}",
        ed.doc.notes.len(),
        ed.selection.notes.len(),
        points,
        ed.can_undo() as u8,
        ed.razor.areas.len(),
        ed.tool,
        ed.dimension,
    );
    drop(ed);
    rsx! {
        style { "html, body {{ margin: 0; padding: 0; width: 100%; height: 100%; }}" }
        // Out of flow, so the editor gets the whole window and the space
        // reported below is the truth. In flow it stole a band off the
        // top, the roll was drawn taller than the cell it sat in, and the
        // bottom of a sweep landed outside the element — which is how a
        // full-height eraser drag came to hit nothing.
        div {
            "data-testid": "readout",
            style: "position: absolute; top: 0; left: 0; z-index: 10;",
            "{readout}"
        }
        div {
            style: "width: 100vw; height: 100vh;",
            ExpressionEditor { editor }
        }
    }
}

/// One number out of the readout.
fn field(html: &str, key: &str) -> usize {
    html.split_whitespace()
        .find_map(|kv| kv.strip_prefix(&format!("{key}="))?.parse().ok())
        .unwrap_or_else(|| panic!("no `{key}` in readout: {html}"))
}

/// Which modifier the gesture is made with.
///
/// Named rather than a bare `Modifiers`, because the map is keyed on
/// exactly these and the test then reads as the table above.
#[derive(Clone, Copy)]
enum Mod {
    None,
    Shift,
    Alt,
    Ctrl,
    ShiftAlt,
}

impl Mod {
    fn to_modifiers(self) -> Modifiers {
        match self {
            Mod::None => Modifiers::empty(),
            Mod::Shift => Modifiers::SHIFT,
            Mod::Alt => Modifiers::ALT,
            Mod::Ctrl => Modifiers::CONTROL,
            Mod::ShiftAlt => Modifiers::SHIFT | Modifiers::ALT,
        }
    }
}

/// The note area's origin and size: the roll's box less the key gutter
/// down the left and the ruler across the top.
///
/// Fractions of the whole element would put a 5% y inside the ruler and
/// a 2% x inside the piano keys, where the surface routes the gesture to
/// the playhead or a row selection instead. That is real behaviour, and
/// not the one these tests are about.
fn note_area(el: &ResolvedElement) -> (f64, f64, f64, f64) {
    let (ox, oy) = el.document_origin();
    let (w, h) = el.size();
    (
        ox + expression_editor_ui::canvas::GUTTER_W,
        oy + expression_editor_ui::canvas::RULER_H,
        (w as f64 - expression_editor_ui::canvas::GUTTER_W).max(1.0),
        (h as f64 - expression_editor_ui::canvas::RULER_H).max(1.0),
    )
}

/// Drag from `from` to `to`, both in fractions of the note area, and
/// return the readout afterwards.
async fn drag(ed: Editor, from: (f64, f64), to: (f64, f64), m: Mod) -> dioxus_test::Result<String> {
    stage(ed);
    let tester = render(Surface).with_window_size(WINDOW.0, WINDOW.1).build();
    tester.drain();
    tester.relayout();
    let el = tester.query(by_testid("roll")).immediately()?;
    let (ox, oy, w, h) = note_area(&el);
    sweep(
        &tester,
        (ox + w * from.0, oy + h * from.1),
        (ox + w * to.0, oy + h * to.1),
        m,
    )
    .await;
    Ok(tester
        .query(by_testid("readout"))
        .immediately()?
        .inner_html())
}

/// Sweep from the centre of the first note to the centre of the last.
///
/// Computed from the camera for the same reason `drag_from_first_note`
/// is: a sweep stated in fractions of the note area only crosses the
/// notes at one particular viewport shape, and it silently stops
/// crossing them when that shape changes. This one crosses every note by
/// construction, whatever the window.
async fn drag_across_notes(ed: Editor, m: Mod) -> dioxus_test::Result<String> {
    let at = |n: &Note, ed: &Editor| {
        (
            ed.camera.x((n.start + n.end) / 2.0),
            ed.camera.y(n.row as f64, ed.viewport),
        )
    };
    let first = at(&ed.doc.notes[0], &ed);
    let last = at(ed.doc.notes.last().expect("no notes"), &ed);

    stage(ed);
    let tester = render(Surface).with_window_size(WINDOW.0, WINDOW.1).build();
    tester.drain();
    tester.relayout();
    let el = tester.query(by_testid("roll")).immediately()?;
    let (ox, oy, _, _) = note_area(&el);
    sweep(
        &tester,
        (ox + first.0, oy + first.1),
        (ox + last.0, oy + last.1),
        m,
    )
    .await;
    Ok(tester
        .query(by_testid("readout"))
        .immediately()?
        .inner_html())
}

/// Drag starting *on the first note*, offset by `delta` in fractions of
/// the note area.
///
/// Hitting a note by eye is what made the first version of these tests
/// unreliable, so the start point is computed from the camera the editor
/// is actually using rather than guessed at.
async fn drag_from_first_note(
    ed: Editor,
    delta: (f64, f64),
    m: Mod,
) -> dioxus_test::Result<String> {
    let note = ed.doc.notes[0].clone();
    let mid_t = (note.start + note.end) / 2.0;
    let nx = ed.camera.x(mid_t);
    let ny = ed.camera.y(note.row as f64, ed.viewport);

    stage(ed);
    let tester = render(Surface).with_window_size(WINDOW.0, WINDOW.1).build();
    tester.drain();
    tester.relayout();
    let el = tester.query(by_testid("roll")).immediately()?;
    let (ox, oy, w, h) = note_area(&el);
    let start = (ox + nx, oy + ny);
    sweep(
        &tester,
        start,
        (start.0 + w * delta.0, start.1 + h * delta.1),
        m,
    )
    .await;
    Ok(tester
        .query(by_testid("readout"))
        .immediately()?
        .inner_html())
}

/// Press, move in steps, release.
async fn sweep(tester: &DocumentTester, (x0, y0): (f64, f64), (x1, y1): (f64, f64), m: Mod) {
    // Enough samples that the path is continuous over a note-width.
    // A real pointer emits dozens of moves across a drag; six skipped
    // straight over the notes a sweep was supposed to meet, which read
    // as "the tool does nothing" rather than "the test undersampled".
    const STEPS: usize = 20;
    let mods = m.to_modifiers();
    tester.pointer_down_mods(x0, y0, mods);
    tester.drain();
    for i in 1..=STEPS {
        let t = i as f64 / STEPS as f64;
        tester.pointer_move_mods(x0 + (x1 - x0) * t, y0 + (y1 - y0) * t, true, mods);
        // `drain`, not `pump`: a move that changes nothing leaves `pump`
        // waiting a full second for work that never comes, so twenty of
        // them cost twenty seconds of nothing. See `DocumentTester::drain`.
        tester.drain();
    }
    tester.pointer_up_mods(x1, y1, mods);
    let _ = tester.pump().await;
}

#[tokio::test]
async fn the_roll_has_area_for_every_tool() -> dioxus_test::Result<()> {
    // The precondition for all of it: whichever tool is armed, the
    // surface still mounts and the roll is a real element. A tool that
    // changed the layout enough to collapse the canvas would make every
    // gesture below silently do nothing — and the failure would look
    // like the gesture, not the layout.
    for tool in Tool::ALL {
        stage(editor_with_notes(tool, Mode::Midi));
        let tester = render(Surface).with_window_size(WINDOW.0, WINDOW.1).build();
        tester.drain();
        tester.relayout();
        let el = tester.query(by_testid("roll")).immediately()?;
        let (w, h) = el.size();
        assert!(w > 0.0 && h > 0.0, "{tool:?}: roll has no area ({w}x{h})");
    }
    Ok(())
}

#[tokio::test]
async fn a_plain_drag_marquees() -> dioxus_test::Result<()> {
    let before = editor_with_notes(Tool::Select, Mode::Midi);
    let notes = before.doc.notes.len();
    let html = drag(before, (0.02, 0.05), (0.98, 0.95), Mod::None).await?;
    assert_eq!(
        field(&html, "notes"),
        notes,
        "a marquee must not edit notes"
    );
    assert!(
        field(&html, "sel") > 0,
        "a box over every note selected none: {html}"
    );
    Ok(())
}

#[tokio::test]
async fn shift_drag_adds_to_the_selection() -> dioxus_test::Result<()> {
    // MarqueeAdd, the difference that makes building a selection across
    // two passes possible.
    let mut before = editor_with_notes(Tool::Select, Mode::Midi);
    let first = before.doc.notes[0].id;
    before.selection.set_single(first);
    let html = drag(before, (0.02, 0.05), (0.98, 0.95), Mod::Shift).await?;
    assert!(
        field(&html, "sel") > 1,
        "shift-drag replaced the selection instead of adding: {html}"
    );
    Ok(())
}

#[tokio::test]
async fn alt_drag_paints_notes() -> dioxus_test::Result<()> {
    // PaintNotes — the gesture the empty roll advertises as "Alt+drag
    // draws a note".
    //
    // The start point is *found* rather than guessed: the map is keyed on
    // `context_at`, and a fraction that happens to land on a note gives
    // `Context::Note` and a completely different action. Ask the editor
    // which point is empty roll, then paint there.
    let before = editor_with_notes(Tool::Select, Mode::Midi);
    let notes = before.doc.notes.len();

    let vp = before.viewport;
    let empty = (1..20)
        .flat_map(|iy| (1..20).map(move |ix| (ix as f64 / 20.0, iy as f64 / 20.0)))
        .map(|(fx, fy)| (fx * vp.w, fy * vp.h))
        .find(|&(x, y)| {
            expression_editor_ui::interaction::context_at(&before, x, y)
                == expression_editor_core::mouse::Context::PianoRoll
        })
        .expect("some part of the roll is empty");
    let from = (empty.0 / vp.w, empty.1 / vp.h);
    let html = drag(before, from, (0.95, from.1), Mod::Alt).await?;
    assert!(
        field(&html, "notes") > notes,
        "alt-drag across empty roll from {from:?} painted nothing \
         (was {notes}): {html}"
    );
    Ok(())
}

/// Ctrl is the razor, and nothing else.
///
/// It was the pen's modifier, in two places at once — the map said so
/// *and* `legacy_pointer_down` forced `Tool::Pen` whenever Ctrl was
/// down, which would have outranked the binding and drawn a curve where
/// the map promised a razor.
///
/// Razor is a *selection* of time, so this asserts what it selects and
/// that it left the material alone: a gesture that cut a span and also
/// wrote curve points would be doing two jobs.
#[tokio::test]
async fn ctrl_drag_is_the_razor() -> dioxus_test::Result<()> {
    let mut before = editor_with_notes(Tool::Select, Mode::Mpe);
    before.selection.notes = before.doc.notes.iter().map(|n| n.id).collect();
    before.dimension = Dimension::Pitch;
    let notes = before.doc.notes.len();

    let html = drag(before, (0.05, 0.6), (0.9, 0.35), Mod::Ctrl).await?;
    assert!(
        field(&html, "razors") > 0,
        "ctrl-drag cut no razor area: {html}"
    );
    assert_eq!(
        field(&html, "points"),
        0,
        "and must not also draw a curve: {html}"
    );
    assert_eq!(field(&html, "notes"), notes, "nor change the notes: {html}");
    Ok(())
}

#[tokio::test]
async fn dragging_a_note_moves_it_rather_than_selecting() -> dioxus_test::Result<()> {
    // MoveNote. The assertion is that the document changed and the note
    // count did not — a move, not an insert and not a marquee.
    let before = editor_with_notes(Tool::Select, Mode::Midi);
    let notes = before.doc.notes.len();
    let first_start = before.doc.notes[0].start;
    let html = drag_from_first_note(before, (0.25, 0.0), Mod::None).await?;
    assert_eq!(field(&html, "notes"), notes, "the drag inserted or deleted");
    assert_eq!(
        field(&html, "undo"),
        1,
        "moving a note left nothing to undo: {html}"
    );
    let _ = first_start;
    Ok(())
}

#[tokio::test]
async fn the_note_eraser_sweeps_notes_away() -> dioxus_test::Result<()> {
    // The armed tool takes the plain drag. Before `resolve_for`, the map
    // answered first and bound `PianoRoll + drag` to `MarqueeSelect` and
    // `Note + drag` to `MoveNote`, so the eraser could not be reached by
    // dragging at all — arming it and sweeping selected, or moved, the
    // notes it was supposed to delete.
    let before = editor_with_notes(Tool::NoteErase, Mode::Midi);
    let notes = before.doc.notes.len();
    // Along the notes rather than across the box: they sit on four
    // different pitches, so a sweep stated in fractions of the area only
    // meets them at one particular viewport shape.
    let html = drag_across_notes(before, Mod::None).await?;
    assert!(
        field(&html, "notes") < notes,
        "sweeping with the eraser armed deleted nothing (was {notes}): {html}"
    );
    Ok(())
}

#[tokio::test]
async fn a_tool_claims_only_the_plain_gesture() -> dioxus_test::Result<()> {
    // The other half of the contract: a tool owns the unmodified drag and
    // nothing else, so the modified gestures stay whatever the user
    // configured. With the eraser armed, alt+drag must still paint.
    let before = editor_with_notes(Tool::NoteErase, Mode::Midi);
    let notes = before.doc.notes.len();

    let vp = before.viewport;
    let empty = (1..20)
        .flat_map(|iy| (1..20).map(move |ix| (ix as f64 / 20.0, iy as f64 / 20.0)))
        .map(|(fx, fy)| (fx * vp.w, fy * vp.h))
        .find(|&(x, y)| {
            expression_editor_ui::interaction::context_at(&before, x, y)
                == expression_editor_core::mouse::Context::PianoRoll
        })
        .expect("some part of the roll is empty");
    let from = (empty.0 / vp.w, empty.1 / vp.h);
    let html = drag(before, from, (0.95, from.1), Mod::Alt).await?;
    assert!(
        field(&html, "notes") > notes,
        "the eraser swallowed alt+drag, which the map binds to paint: {html}"
    );
    Ok(())
}

#[tokio::test]
async fn arming_the_pen_makes_a_plain_drag_draw() -> dioxus_test::Result<()> {
    // The headline of `resolve_for`: before it, this drag marqueed,
    // because the map bound `PianoRoll + drag` to `MarqueeSelect` and
    // never asked which tool was armed. Ctrl+drag was the only way to
    // reach the pen, from any tool — a temporary override standing in
    // for the tool that was supposed to be selected.
    let mut before = editor_with_notes(Tool::Pen, Mode::Mpe);
    before.selection.notes = before.doc.notes.iter().map(|n| n.id).collect();
    before.dimension = Dimension::Pitch;
    let html = drag(before, (0.05, 0.6), (0.9, 0.35), Mod::None).await?;
    assert!(
        field(&html, "points") > 0,
        "the pen is armed and a plain drag wrote no curve: {html}"
    );
    Ok(())
}

#[tokio::test]
async fn arming_note_draw_makes_a_plain_drag_insert() -> dioxus_test::Result<()> {
    let before = editor_with_notes(Tool::NoteDraw, Mode::Midi);
    let notes = before.doc.notes.len();

    let vp = before.viewport;
    let empty = (1..20)
        .flat_map(|iy| (1..20).map(move |ix| (ix as f64 / 20.0, iy as f64 / 20.0)))
        .map(|(fx, fy)| (fx * vp.w, fy * vp.h))
        .find(|&(x, y)| {
            expression_editor_ui::interaction::context_at(&before, x, y)
                == expression_editor_core::mouse::Context::PianoRoll
        })
        .expect("some part of the roll is empty");
    let from = (empty.0 / vp.w, empty.1 / vp.h);
    let html = drag(before, from, (from.0 + 0.15, from.1), Mod::None).await?;
    assert!(
        field(&html, "notes") > notes,
        "note draw is armed and a plain drag inserted nothing (was {notes}): {html}"
    );
    Ok(())
}

#[tokio::test]
async fn select_still_marquees_because_it_claims_nothing() -> dioxus_test::Result<()> {
    // The baseline the others are measured against: `Tool::Select`
    // claims no gestures, because the shipped map already does exactly
    // what it wants. A tool that needed an overlay to reproduce the
    // default would be a sign the default was wrong.
    assert!(Tool::Select.claims().is_empty());
    let before = editor_with_notes(Tool::Select, Mode::Midi);
    let notes = before.doc.notes.len();
    let html = drag(before, (0.02, 0.05), (0.98, 0.95), Mod::None).await?;
    assert!(field(&html, "sel") > 0, "{html}");
    assert_eq!(field(&html, "notes"), notes, "{html}");
    Ok(())
}

// ── inserting a note ─────────────────────────────────────────────────

/// Alt places a note one grid cell long, and the drag carries it.
///
/// Not a zero-width note you must drag out before it is a note at all:
/// the common case is "put an eighth here", and that should be a click's
/// worth of work with the drag reserved for saying *where*.
#[tokio::test]
async fn alt_places_a_note_of_the_grid_length_and_carries_it() -> dioxus_test::Result<()> {
    let mut before = editor_with_notes(Tool::Select, Mode::Midi);
    before.grid.enabled = true;
    before.grid.division = 1.0 / 8.0;
    let step = before.grid.step(before.units_per_beat());
    let notes = before.doc.notes.len();

    let html = drag(before, (0.1, 0.7), (0.6, 0.3), Mod::Alt).await?;
    assert_eq!(
        field(&html, "notes"),
        notes + 1,
        "Alt+drag must insert one note"
    );
    assert_eq!(
        field(&html, "cells"),
        1,
        "a placed note stays one grid cell however far it is carried: {html}"
    );
    let _ = step;
    Ok(())
}

/// And Shift mid-gesture sizes it instead of moving it.
///
/// The phases are switchable while the button is down — that is the
/// whole design — so this drags with Shift held from the start, which is
/// the same code path a mid-drag grab of Shift lands in.
#[tokio::test]
async fn alt_shift_sizes_the_note_instead_of_moving_it() -> dioxus_test::Result<()> {
    let mut before = editor_with_notes(Tool::Select, Mode::Midi);
    before.grid.enabled = true;
    before.grid.division = 1.0 / 8.0;
    let notes = before.doc.notes.len();

    let html = drag(before, (0.1, 0.5), (0.8, 0.5), Mod::ShiftAlt).await?;
    assert_eq!(
        field(&html, "notes"),
        notes + 1,
        "still inserts exactly one"
    );
    assert!(
        field(&html, "cells") > 1,
        "a sized note should be longer than the cell it started as: {html}"
    );
    Ok(())
}
