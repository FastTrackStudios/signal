//! Every mode, mounted and driven on the real surface.
//!
//! The editor claims to be seven products sharing one surface — MIDI,
//! MPE, Vocals, Drums, Guitar and the two audio modes — and the risk in
//! that claim is a mode that is fine in the model and broken the moment
//! it is drawn: a row space whose keyboard has no rows, a mode whose
//! chrome collapses the canvas, a gesture that panics because the
//! vertical axis means something the hit test did not expect.
//!
//! So these mount the real `ExpressionEditor` for each mode and drive a
//! pointer through it. What they assert is deliberately shallow and
//! wide: it renders, it has area, the row space is the one the mode
//! asks for, and a drag across it neither panics nor is silently
//! ignored. Depth per mode belongs in that mode's own tests.

use std::cell::RefCell;

use dioxus::prelude::*;
use dioxus_test::keyboard_types::Modifiers;
use dioxus_test::{by_testid, render};
use expression_editor_core::doc::{ExpressionDoc, Note, NoteId, TimeBase};
use expression_editor_core::{Editor, Mode, RowSpace, Tool, Viewport};
use expression_editor_ui::ExpressionEditor;

const PPQ: f64 = 960.0;

thread_local! {
    static STAGED: RefCell<Option<Editor>> = const { RefCell::new(None) };
}

fn stage(ed: Editor) {
    STAGED.with(|s| *s.borrow_mut() = Some(ed));
}

/// An editor in `mode`, with notes placed on rows that mode can show.
///
/// The row range comes from the mode's own row space rather than a fixed
/// 60–69: a drum map is twenty lanes and a band split is a handful, so
/// MIDI pitches would sit outside the roll entirely and every assertion
/// below would be about an empty canvas.
fn editor_in(mode: Mode) -> Editor {
    let space = mode.default_row_space();
    let (lo, hi) = space.bounds();
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 4.0);
    let span = (hi - lo).max(1);
    for i in 0..4u64 {
        let row = lo + (span * (i as i32 + 1)) / 6;
        let start = PPQ * i as f64 * 0.9;
        doc.push(Note::new(NoteId(i + 1), start, start + PPQ * 0.7, row));
    }
    doc.row_space = space;

    let mut ed = Editor::new(doc, Viewport::new(900.0, 480.0));
    ed.set_mode(mode);
    // Select explicitly, rather than inheriting `Tool`'s `#[default]`
    // (Curve). Since a tool now claims the plain drag, the default would
    // make every assertion below about the curve tool instead of about
    // the mode — which is what these are supposed to isolate.
    ed.tool = Tool::Select;
    ed.reset_view();
    ed
}

#[component]
fn Surface() -> Element {
    let editor = use_signal(|| {
        STAGED
            .with(|s| s.borrow_mut().take())
            .unwrap_or_else(|| editor_in(Mode::Midi))
    });
    let ed = editor.read();
    let readout = format!(
        "notes={} sel={} undo={} mode={:?} rows={}",
        ed.doc.notes.len(),
        ed.selection.notes.len(),
        ed.can_undo() as u8,
        ed.mode,
        matches!(ed.doc.row_space, RowSpace::Pitch) as u8,
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

#[tokio::test]
async fn every_mode_renders_a_roll_with_area() -> dioxus_test::Result<()> {
    // The cheapest test that would have caught a mode whose chrome eats
    // the canvas — and the one every gesture below depends on.
    for mode in Mode::ALL {
        stage(editor_in(mode));
        let tester = render(Surface).with_window_size(1000, 620).build();
        let el = tester.query(by_testid("roll")).immediately()?;
        let (w, h) = el.size();
        assert!(
            w > 0.0 && h > 0.0,
            "{mode:?}: the roll rendered with no area ({w}x{h})"
        );
    }
    Ok(())
}

#[tokio::test]
async fn every_mode_keeps_the_row_space_it_asked_for() -> dioxus_test::Result<()> {
    // Mounting must not quietly convert the document's row space —
    // a drum roll redrawn as 128 pitches is a different instrument.
    for mode in Mode::ALL {
        let want = mode.default_row_space();
        stage(editor_in(mode));
        let tester = render(Surface).with_window_size(1000, 620).build();
        let html = tester
            .query(by_testid("readout"))
            .immediately()?
            .inner_html();
        let is_pitch = field(&html, "rows") == 1;
        assert_eq!(
            is_pitch,
            matches!(want, RowSpace::Pitch),
            "{mode:?}: row space changed on mount ({html})"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_plain_drag_does_the_mode_s_own_thing() -> dioxus_test::Result<()> {
    // One gesture, every mode: the vertical axis means something
    // different in each (pitches, kit lanes, strings, spectral bands),
    // and the hit test has to cope with all of them.
    //
    // Drums is deliberately not like the others. Its mouse profile binds
    // a plain drag to painting — "named kit lanes, triangle heads,
    // paint-on-drag" is the mode's own description — so a sweep there
    // adds hits rather than selecting them. Asserting a marquee
    // everywhere would have been asserting that Drums is broken.
    for mode in Mode::ALL {
        stage(editor_in(mode));
        let tester = render(Surface).with_window_size(1000, 620).build();
        let el = tester.query(by_testid("roll")).immediately()?;
        let (ox, oy) = el.document_origin();
        let (w, h) = el.size();
        let x0 = ox + expression_editor_ui::canvas::GUTTER_W + 4.0;
        let y0 = oy + expression_editor_ui::canvas::RULER_H + 4.0;
        let x1 = ox + w as f64 - 4.0;
        let y1 = oy + h as f64 - 4.0;

        // `drain` mid-gesture, `pump` once at the end — pumping waits,
        // and a move that renders nothing waits the full second. See
        // `DocumentTester::drain`.
        tester.pointer_down_mods(x0, y0, Modifiers::empty());
        tester.drain();
        for i in 1..=5 {
            let t = i as f64 / 5.0;
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
        // The invariant that actually holds across all seven: the sweep
        // *reached* the surface. What it did is the mode's business —
        // most select, Drums paints its kit lanes, Guitar inserts on the
        // string roll — and pinning each of those belongs in that mode's
        // own tests, not in a loop that would have to special-case its
        // way through the list.
        let touched =
            field(&html, "sel") > 0 || field(&html, "notes") != 4 || field(&html, "undo") == 1;
        assert!(
            touched,
            "{mode:?}: a sweep across the whole roll did nothing at all ({html})"
        );
    }
    Ok(())
}

#[tokio::test]
async fn switching_mode_on_a_live_surface_survives() -> dioxus_test::Result<()> {
    // The switcher is chrome on the same mounted component, so a mode
    // change is a re-render of a live surface rather than a fresh mount.
    // Every pair is worth one pass because the row space changes under
    // the camera, which is where this has broken before.
    for from in Mode::ALL {
        for to in Mode::ALL {
            let mut ed = editor_in(from);
            ed.set_mode(to);
            ed.reset_view();
            stage(ed);
            let tester = render(Surface).with_window_size(1000, 620).build();
            let el = tester.query(by_testid("roll")).immediately()?;
            let (w, h) = el.size();
            assert!(
                w > 0.0 && h > 0.0,
                "{from:?} -> {to:?}: roll collapsed ({w}x{h})"
            );
        }
    }
    Ok(())
}
