//! The hand gestures on a role lane's hits, driven on the real surface.
//!
//! The claims: a drag on a hit line emits `Slip` in SPLIT mode and
//! `Stretch` in WARP mode; a click selects, and the arrows then nudge
//! by the grid (Shift = 1 ms); Alt+click adds a hit; Delete removes the
//! selected one; a press away from any hit still switches lanes rather
//! than gesturing. The daw write is the host's; the gestures' contract
//! is these enums.

use std::cell::RefCell;

use dioxus::prelude::*;
use dioxus_test::keyboard_types::{Key, Modifiers};
use dioxus_test::{by_testid, render};
use expression_editor_core::kit::LaneRole;
use expression_editor_core::{Editor, ExpressionDoc, Mode, Note, NoteId, TimeBase, Viewport};
use expression_editor_ui::stack::HitGesture;
use expression_editor_ui::{canvas, stack};

const VP_W: f64 = 1000.0;
const VP_H: f64 = 400.0;
const RATE: f64 = 100.0;
/// One grid division handed to the view, seconds.
const GRID: f64 = 0.25;

thread_local! {
    static STAGED: RefCell<Option<(Editor, bool)>> = const { RefCell::new(None) };
    static GOT: RefCell<Vec<HitGesture>> = const { RefCell::new(Vec::new()) };
}

/// A one-lane kit: a kick with hits at 1.0 s and 2.0 s in a 4 s doc.
fn kit_editor() -> Editor {
    let mut doc = ExpressionDoc::new(TimeBase::Frames { frame_rate: RATE }, 0.0, RATE * 4.0);
    doc.push(Note::new(NoteId(1), RATE, RATE * 2.0, 1));
    doc.push(Note::new(NoteId(2), RATE * 2.0, RATE * 3.0, 1));
    let mut ed = Editor::new(doc, Viewport::new(VP_W, VP_H));
    ed.set_mode(Mode::UnpitchedAudio);
    ed.tracks.rename(0, "Kick In");
    let guid = ed.tracks.track(0).unwrap().guid.clone();
    ed.tracks.fold_roles(&[(guid, LaneRole::Kick)]);
    ed.stacked = true;
    ed.reset_view();
    ed
}

#[component]
fn Surface() -> Element {
    let (staged, warp) = STAGED
        .with(|s| s.borrow_mut().take())
        .expect("an editor was staged");
    let editor = use_signal(|| staged);
    rsx! {
        div {
            style: "width: {VP_W + canvas::GUTTER_W}px; height: {VP_H + canvas::RULER_H}px;",
            "data-testid": "stack",
            stack::StackView {
                editor,
                warp,
                grid_secs: GRID,
                on_hit: move |g: HitGesture| {
                    GOT.with(|got| got.borrow_mut().push(g));
                },
            }
        }
    }
}

struct Stage {
    tester: dioxus_test::DocumentTester,
    /// Element origin in window space.
    origin: (f64, f64),
    /// The first hit's x, window space.
    hit_x: f64,
    /// Mid-lane y, window space.
    y: f64,
}

/// Mount the surface around `kit_editor()` and locate the first hit.
fn stage(warp: bool) -> Stage {
    let ed = kit_editor();
    let views = stack::lanes(&ed, 1.15, 24.0);
    let first = views
        .iter()
        .flat_map(|l| l.notes.iter())
        .min_by(|a, b| a.at_secs.total_cmp(&b.at_secs))
        .expect("a hit to drag");
    assert!((first.at_secs - 1.0).abs() < 1e-6);
    let hit_x = canvas::GUTTER_W + first.x;
    STAGED.with(|s| *s.borrow_mut() = Some((ed, warp)));
    GOT.with(|g| g.borrow_mut().clear());

    let tester = render(Surface)
        .with_window_size(
            (VP_W + canvas::GUTTER_W) as u32,
            (VP_H + canvas::RULER_H) as u32,
        )
        .build();
    tester.drain();
    tester.relayout();
    // The div is not at the window origin (body margin); pointer
    // coordinates are window-space, the handler's are element-space.
    let origin = tester
        .query(by_testid("stack"))
        .immediately()
        .expect("the stack mounted")
        .document_origin();
    Stage {
        y: origin.1 + canvas::RULER_H + VP_H / 2.0,
        hit_x: hit_x + origin.0,
        origin,
        tester,
    }
}

/// Drag from the first hit `dx` pixels right, with `mods` held.
fn drag(s: &Stage, dx: f64, mods: Modifiers) {
    s.tester.pointer_down_mods(s.hit_x + 1.0, s.y, mods);
    s.tester.drain();
    for i in 1..=10 {
        let t = i as f64 / 10.0;
        s.tester
            .pointer_move_mods(s.hit_x + 1.0 + dx * t, s.y, true, mods);
        s.tester.drain();
    }
    s.tester.pointer_up_mods(s.hit_x + 1.0 + dx, s.y, mods);
    s.tester.drain();
}

fn got() -> Vec<HitGesture> {
    GOT.with(|g| g.borrow().clone())
}

// r[verify drums.manual.slip]
#[tokio::test]
async fn dragging_a_hit_emits_the_slip() -> dioxus_test::Result<()> {
    let s = stage(false);
    drag(&s, 40.0, Modifiers::empty());
    let _ = s.tester.pump().await;

    let all = got();
    let Some(HitGesture::Slip { hit, next, delta }) = all.first() else {
        panic!("the drag emitted a slip, got {all:?}");
    };
    assert!((hit - 1.0).abs() < 0.02, "hit at 1.0 s, got {hit}");
    assert!((next - 2.0).abs() < 0.02, "next at 2.0 s, got {next}");
    // 40 px over a 4 s view in 1000 px ≈ 0.16 s, dragged later.
    assert!(
        (delta - 0.16).abs() < 0.02,
        "delta ≈ 0.16 s later, got {delta}"
    );
    Ok(())
}

// r[verify drums.manual.stretch]
#[tokio::test]
async fn in_warp_mode_the_same_drag_stretches() -> dioxus_test::Result<()> {
    let s = stage(true);
    drag(&s, 40.0, Modifiers::empty());
    let _ = s.tester.pump().await;

    let all = got();
    let Some(HitGesture::Stretch {
        hit,
        prev,
        next,
        delta,
        both,
    }) = all.first()
    else {
        panic!("the drag emitted a stretch, got {all:?}");
    };
    assert!((hit - 1.0).abs() < 0.02, "hit at 1.0 s, got {hit}");
    assert!(
        prev.is_infinite() && prev.is_sign_negative(),
        "no earlier hit, got {prev}"
    );
    assert!((next - 2.0).abs() < 0.02, "next at 2.0 s, got {next}");
    assert!((delta - 0.16).abs() < 0.02, "delta ≈ 0.16 s, got {delta}");
    assert!(!both, "no Shift, neighbours pinned");
    Ok(())
}

// r[verify drums.manual.stretch]
#[tokio::test]
async fn shift_makes_the_stretch_both_sided() -> dioxus_test::Result<()> {
    let s = stage(true);
    drag(&s, 40.0, Modifiers::SHIFT);
    let _ = s.tester.pump().await;

    let all = got();
    let Some(HitGesture::Stretch { both, .. }) = all.first() else {
        panic!("the drag emitted a stretch, got {all:?}");
    };
    assert!(both, "Shift held, the BothStretch law");
    Ok(())
}

// r[verify drums.manual.nudge]
#[tokio::test]
async fn a_selected_hit_nudges_by_the_grid_and_finely_with_shift() -> dioxus_test::Result<()> {
    let s = stage(false);
    // A click (no travel) selects.
    s.tester.pointer_down(s.hit_x + 1.0, s.y);
    s.tester.drain();
    s.tester.pointer_up(s.hit_x + 1.0, s.y);
    s.tester.drain();
    assert!(got().is_empty(), "a click alone edits nothing");

    let cell = s.tester.query(by_testid("stack-cell")).immediately()?;
    cell.focus();
    s.tester.press_key(Key::ArrowRight, Modifiers::empty());
    s.tester.drain();
    s.tester.press_key(Key::ArrowLeft, Modifiers::SHIFT);
    s.tester.drain();
    let _ = s.tester.pump().await;

    let all = got();
    assert_eq!(all.len(), 2, "two nudges, got {all:?}");
    let Some(HitGesture::Slip { hit, delta, .. }) = all.first() else {
        panic!("a nudge slips in SPLIT mode, got {all:?}");
    };
    assert!((hit - 1.0).abs() < 0.02, "the selected hit, got {hit}");
    assert!((delta - GRID).abs() < 1e-9, "one division, got {delta}");
    let Some(HitGesture::Slip { hit, delta, .. }) = all.get(1) else {
        panic!("the fine nudge slips too, got {all:?}");
    };
    assert!(
        (hit - (1.0 + GRID)).abs() < 0.02,
        "selection followed the first nudge, got {hit}"
    );
    assert!((delta + 0.001).abs() < 1e-9, "Shift = 1 ms, got {delta}");
    Ok(())
}

// r[verify drums.manual.add-remove]
#[tokio::test]
async fn alt_click_adds_and_delete_removes() -> dioxus_test::Result<()> {
    let s = stage(false);
    // Alt+click well away from either hit: an Add at the click's time.
    let far_x = s.origin.0 + canvas::GUTTER_W + VP_W * (3.0 / 4.0);
    s.tester.pointer_down_mods(far_x, s.y, Modifiers::ALT);
    s.tester.drain();
    s.tester.pointer_up_mods(far_x, s.y, Modifiers::ALT);
    s.tester.drain();

    // Select the first hit, then throw it out.
    s.tester.pointer_down(s.hit_x + 1.0, s.y);
    s.tester.drain();
    s.tester.pointer_up(s.hit_x + 1.0, s.y);
    s.tester.drain();
    let cell = s.tester.query(by_testid("stack-cell")).immediately()?;
    cell.focus();
    s.tester.press_key(Key::Delete, Modifiers::empty());
    let _ = s.tester.pump().await;

    let all = got();
    let Some(HitGesture::Add { lane, at }) = all.first() else {
        panic!("Alt+click added, got {all:?}");
    };
    assert_eq!(lane, "Kick");
    // The exact time is the host's to refine; the view only promises
    // the neighbourhood of the click (the view spans a little more
    // than the doc, so ¾ of the width is not exactly ¾ of 4 s).
    assert!((at - 3.0).abs() < 0.1, "near the click's time, got {at}");
    let Some(HitGesture::Remove { lane, hit }) = all.get(1) else {
        panic!("Delete removed, got {all:?}");
    };
    assert_eq!(lane, "Kick");
    assert!((hit - 1.0).abs() < 0.02, "the selected hit, got {hit}");
    Ok(())
}

// r[verify drums.manual.nudge]
#[tokio::test]
async fn double_click_snaps_the_hit_to_the_grid() -> dioxus_test::Result<()> {
    // Move the first hit off-grid so the snap has somewhere to go: the
    // view's hits sit at 1.0 s, and with a 0.25 s grid that is *on*
    // grid — so stage a doc whose first hit is at 1.1 s instead.
    let mut ed = kit_editor();
    ed.doc.notes[0].start = RATE * 1.1;
    STAGED.with(|s| *s.borrow_mut() = Some((ed.clone(), false)));
    GOT.with(|g| g.borrow_mut().clear());
    let views = stack::lanes(&ed, 1.15, 24.0);
    let first = views
        .iter()
        .flat_map(|l| l.notes.iter())
        .min_by(|a, b| a.at_secs.total_cmp(&b.at_secs))
        .expect("a hit");
    let tester = render(Surface)
        .with_window_size(
            (VP_W + canvas::GUTTER_W) as u32,
            (VP_H + canvas::RULER_H) as u32,
        )
        .build();
    tester.drain();
    tester.relayout();
    let (ox, oy) = tester
        .query(by_testid("stack"))
        .immediately()?
        .document_origin();
    let x = ox + canvas::GUTTER_W + first.x + 1.0;
    let y = oy + canvas::RULER_H + VP_H / 2.0;

    for _ in 0..2 {
        tester.pointer_down(x, y);
        tester.drain();
        tester.pointer_up(x, y);
        tester.drain();
    }
    let _ = tester.pump().await;

    let all = got();
    let Some(HitGesture::Slip { hit, delta, .. }) = all.first() else {
        panic!("the double click snapped, got {all:?}");
    };
    assert!((hit - 1.1).abs() < 0.02, "the off-grid hit, got {hit}");
    // Nearest division of 0.25 from 1.1 is 1.0 → delta ≈ −0.1.
    assert!((delta + 0.1).abs() < 0.02, "snap to 1.0 s, got {delta}");
    Ok(())
}

// r[verify drums.manual.slip]
#[tokio::test]
async fn a_press_away_from_any_hit_does_not_gesture() -> dioxus_test::Result<()> {
    let s = stage(false);
    let far_x = s.origin.0 + canvas::GUTTER_W + 20.0;
    s.tester.pointer_down(far_x, s.y);
    s.tester.drain();
    s.tester.pointer_move(far_x + 40.0, s.y, true);
    s.tester.drain();
    s.tester.pointer_up(far_x + 40.0, s.y);
    let _ = s.tester.pump().await;

    assert!(
        got().is_empty(),
        "no hit under the press, no gesture: {:?}",
        got()
    );
    Ok(())
}

/// A two-mic kick, for the gutter's mic selector.
fn two_mic_editor() -> Editor {
    let doc = ExpressionDoc::new(TimeBase::Frames { frame_rate: RATE }, 0.0, RATE * 4.0);
    let mut ed = Editor::new(doc.clone(), Viewport::new(VP_W, VP_H));
    ed.set_mode(Mode::UnpitchedAudio);
    ed.tracks.rename(0, "In");
    let first = ed.tracks.track(0).unwrap().guid.clone();
    let second = {
        let t = expression_editor_core::tracks::Track::in_mode("Out", doc, Mode::UnpitchedAudio);
        let g = t.guid.clone();
        ed.tracks.push(t);
        g
    };
    ed.tracks
        .fold_roles(&[(first, LaneRole::Kick), (second, LaneRole::Kick)]);
    ed.stacked = true;
    ed.reset_view();
    ed
}

/// The gutter chip and its menu own their presses: opening the list
/// and picking a member never leaks into the hit-gesture or
/// lane-switch paths. (The switch itself is a `switch_track` call the
/// core already tests; the surface's job is routing the press.)
#[tokio::test]
async fn the_gutter_chip_picks_the_lanes_mic() -> dioxus_test::Result<()> {
    let ed = two_mic_editor();
    assert_eq!(ed.tracks.active(), 0, "opens on In");
    let views = stack::lanes(&ed, 1.15, 24.0);
    let lane = views.iter().find(|l| l.is_role).expect("the kick lane");
    assert_eq!(lane.members.len(), 2);
    let (chip_x, chip_y) = (10.0, canvas::RULER_H + lane.y + 22.0);
    // The second member's menu row, in element space.
    let item_y = canvas::RULER_H + lane.y + 36.0 + 16.0 + 8.0;
    STAGED.with(|s| *s.borrow_mut() = Some((ed, false)));
    GOT.with(|g| g.borrow_mut().clear());

    let tester = render(Surface)
        .with_window_size(
            (VP_W + canvas::GUTTER_W) as u32,
            (VP_H + canvas::RULER_H) as u32,
        )
        .build();
    tester.drain();
    tester.relayout();
    let origin = tester
        .query(by_testid("stack"))
        .immediately()?
        .document_origin();

    // Open the menu…
    tester.pointer_down(origin.0 + chip_x, origin.1 + chip_y);
    tester.drain();
    tester.pointer_up(origin.0 + chip_x, origin.1 + chip_y);
    tester.drain();
    tester.relayout();
    // …and pick "Out".
    tester.pointer_down(origin.0 + 20.0, origin.1 + item_y);
    tester.drain();
    tester.pointer_up(origin.0 + 20.0, origin.1 + item_y);
    let _ = tester.pump().await;

    assert!(got().is_empty(), "a menu press is not a hit gesture");
    Ok(())
}
