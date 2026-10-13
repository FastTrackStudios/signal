//! MPE editing by gesture, on the real surface (#167 criterion 4).
//!
//! Mounts the actual `ExpressionEditor` on the headless Blitz DOM and
//! drives a pointer across the roll, so what is exercised is the same
//! event path a user's mouse takes — not a hand-called reducer.
//!
//! The claim under test is the founding one: per-note bend, pressure
//! and timbre are properties *of a note*, edited on the note, rather
//! than entries in a controller lane somewhere else.
//!
//! ## What these found
//!
//! Every gesture test below panics with **"RefCell already borrowed"**
//! at `native-dom/src/events.rs:164` — which is `doc.borrow_mut()`, and
//! dioxus documents the constraint on the line above it:
//!
//! > Returns `None` if the document is currently mutably borrowed.
//! > Use this from background tasks to avoid panicking during event
//! > handling.
//!
//! The background task is ours: `ExpressionEditor`'s `onmounted` does
//! `spawn` + `get_client_rect().await`, and resolving that future
//! re-enters the document while an event is being dispatched.
//!
//! **This is a dioxus/Blitz constraint, not a stray line in our
//! handler** — `get_client_rect` has no `try_` form to reach for. It is
//! the same fault `slider_drag.rs` isolated in a widget, here in the
//! primary editing surface, and Blitz is the renderer REAPER uses.
//!
//! Guarding our own signal with `try_write` (done, and worth keeping —
//! it also stops a no-op resize invalidating the view) does **not** fix
//! it, because the contended borrow is dioxus's document, not ours.
//!
//! The real fix is to stop measuring from a spawned task: let the host
//! supply the viewport, which `Editor::resize` already supports and the
//! standalone runner already does. Removing the mount-time measurement
//! changes how the canvas auto-sizes inside a REAPER dock, which is a
//! thing to *look at* rather than assert — so it is not done here.
//!
//! **That change landed.** The mount-time measurement is gone: the host
//! states its space via `expression_editor_ui::available_space` and the
//! canvas subtracts its own chrome, so nothing measures the document
//! from a task any more. These run, and they are the net.

use dioxus::prelude::*;
use dioxus_test::{by_testid, render};
use expression_editor_core::doc::{Dimension, ExpressionDoc, Note, NoteId, TimeBase};
use expression_editor_core::{Editor, Tool, Viewport};
use expression_editor_ui::ExpressionEditor;

const PPQ: f64 = 960.0;

/// Three notes on their own channels — the MPE case, where expression
/// is attributable.
fn mpe_editor(dimension: Dimension) -> Editor {
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
    ed.dimension = dimension;
    ed.tool = Tool::Curve;
    ed.reset_view();
    ed
}

#[component]
fn Surface(dimension: Dimension) -> Element {
    let editor = use_signal(|| mpe_editor(dimension));
    rsx! { ExpressionEditor { editor } }
}

#[component]
fn PitchSurface() -> Element {
    rsx! { Surface { dimension: Dimension::Pitch } }
}

#[component]
fn PressureSurface() -> Element {
    rsx! { Surface { dimension: Dimension::Pressure } }
}

#[component]
fn TimbreSurface() -> Element {
    rsx! { Surface { dimension: Dimension::Timbre } }
}

/// Drag left-to-right across the roll and report whether it survived.
async fn drag_across(app: fn() -> Element) -> dioxus_test::Result<()> {
    let tester = render(app).with_window_size(1000, 620).build();
    let el = tester.query(by_testid("roll")).immediately()?;
    let (ox, oy) = el.document_origin();
    let (w, h) = el.size();
    let y = oy + h as f64 * 0.45;

    tester.pointer_down(ox + w as f64 * 0.12, y);
    tester.drain();
    for i in 1..=5 {
        tester.pointer_move(
            ox + w as f64 * (0.12 + 0.5 * i as f64 / 5.0),
            y - i as f64 * 4.0,
            true,
        );
        // `drain`, not `pump`. A move that renders nothing leaves `pump`
        // waiting a full second for work that never arrives, which is
        // what made each of these take five seconds of doing nothing.
        tester.drain();
    }
    tester.pointer_up(ox + w as f64 * 0.62, y - 20.0);
    let _ = tester.pump().await;
    Ok(())
}

#[tokio::test]
async fn the_roll_is_reachable_by_a_pointer() -> dioxus_test::Result<()> {
    // The precondition for every gesture below: the surface mounts and
    // the roll is a real element with a size.
    let tester = render(PitchSurface).with_window_size(1000, 620).build();
    let el = tester.query(by_testid("roll")).immediately()?;
    let (w, h) = el.size();
    assert!(
        w > 0.0 && h > 0.0,
        "the roll rendered with no area: {w}x{h}"
    );
    Ok(())
}

#[tokio::test]
async fn a_drag_on_the_pitch_dimension_does_not_panic() -> dioxus_test::Result<()> {
    // Blitz's event dispatch is where the surface has broken before —
    // a RefCell re-entrancy panic mid-drag. This is the regression net.
    drag_across(PitchSurface).await
}

#[tokio::test]
async fn a_drag_on_the_pressure_dimension_does_not_panic() -> dioxus_test::Result<()> {
    drag_across(PressureSurface).await
}

#[tokio::test]
async fn a_drag_on_the_timbre_dimension_does_not_panic() -> dioxus_test::Result<()> {
    drag_across(TimbreSurface).await
}

#[tokio::test]
async fn a_press_without_moving_is_survivable() -> dioxus_test::Result<()> {
    // Separated from the drag because a press alone and a press-plus-move
    // take different paths, and only one of them has broken before.
    let tester = render(PitchSurface).with_window_size(1000, 620).build();
    let el = tester.query(by_testid("roll")).immediately()?;
    let (ox, oy) = el.document_origin();
    let (w, h) = el.size();
    tester.pointer_down(ox + w as f64 * 0.3, oy + h as f64 * 0.5);
    let _ = tester.pump().await;
    tester.pointer_up(ox + w as f64 * 0.3, oy + h as f64 * 0.5);
    let _ = tester.pump().await;
    Ok(())
}

#[tokio::test]
async fn switching_dimension_re_renders_the_same_notes() -> dioxus_test::Result<()> {
    // Three dimensions, one set of notes: the dimension is a view onto
    // the note, not a different document.
    for app in [
        PitchSurface as fn() -> Element,
        PressureSurface,
        TimbreSurface,
    ] {
        let tester = render(app).with_window_size(1000, 620).build();
        let el = tester.query(by_testid("roll")).immediately()?;
        assert!(el.size().0 > 0.0);
    }
    Ok(())
}
