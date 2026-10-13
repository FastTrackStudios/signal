//! A drag has to survive the pointer leaving the control.
//!
//! Blitz has no pointer capture: it hit-tests every move and dispatches to
//! whatever is under the cursor. [`DragProvider`] is the answer — it wraps
//! the editor root, so a move anywhere in the window bubbles to it and
//! reaches whichever widget started the drag. These tests are that claim,
//! stated: press a knob, move the pointer somewhere else entirely, and the
//! parameter must still follow.
//!
//! ```sh
//! cargo test -p fts-audio-ui --test drag_capture
//! ```

use std::cell::Cell;

use dioxus::prelude::*;
use dioxus_test::{by_testid, render};

use fts_audio_ui::controls::knob::{Knob, KnobSize};
use fts_audio_ui::drag::DragProvider;
use fts_audio_ui::param::ParamHandle;

/// The parameter under test.
///
/// Thread-local, not a global: cargo runs these tests concurrently and each
/// tester drives its own DOM on its own thread. A shared static means one
/// test's `mount()` resets the value out from under another's drag, which
/// reads as a knob that stops halfway — a very convincing bug report about
/// the code under test rather than about the fixture.
thread_local! {
    static VALUE: Cell<f32> = const { Cell::new(0.5) };
}

fn value() -> f32 {
    VALUE.with(Cell::get)
}

fn handle() -> ParamHandle {
    ParamHandle::new(
        || value(),
        || {},
        |v| VALUE.with(|c| c.set(v)),
        || {},
        || format!("{:.3}", value()),
        || "Drive".to_string(),
        |_| None,
    )
    // Centred and bipolar, so the soft detent at the default is in play —
    // which is what the last test in this file is about.
    .with_default(0.5)
    .with_bipolar(true)
}

#[component]
fn Sheet() -> Element {
    rsx! {
        style { "html, body {{ margin:0; padding:0; }}" }
        DragProvider {
            div {
                // The knob sits in the middle, so there is room to drag well
                // clear of it in any direction without leaving the window.
                style: "width:900px; height:700px; display:flex; \
                        align-items:center; justify-content:center;",
                Knob { handle: handle(), size: KnobSize::Large }
            }
        }
    }
}

struct Fx {
    tester: dioxus_test::DocumentTester,
}

impl Fx {
    async fn settle(&self) {
        let _ = self.tester.pump().await;
        self.tester.relayout();
    }

    /// The centre of the knob's gesture surface.
    fn dial(&self) -> (f64, f64) {
        let el = self
            .tester
            .query(by_testid("knob-Drive-dial"))
            .immediately()
            .expect("the knob did not draw its gesture surface");
        let (x, y) = el.document_origin();
        let (w, h) = el.size();
        (x + f64::from(w) / 2.0, y + f64::from(h) / 2.0)
    }
}

async fn mount() -> Fx {
    VALUE.with(|c| c.set(0.5));
    let tester = render(Sheet).with_window_size(900, 700).build();
    let fx = Fx { tester };
    fx.settle().await;
    fx
}

/// The baseline: a drag that stays on the dial moves the parameter.
#[tokio::test]
async fn a_drag_on_the_dial_moves_the_parameter() {
    let fx = mount().await;
    let (x, y) = fx.dial();

    fx.tester.pointer_down(x, y);
    fx.settle().await;
    fx.tester.pointer_move(x, y - 20.0, true);
    fx.settle().await;
    fx.tester.pointer_up(x, y - 20.0);
    fx.settle().await;

    assert!(
        value() > 0.5,
        "a 20 px drag up left the value at {}",
        value()
    );
}

/// THE regression: once a drag is in flight the pointer belongs to the knob,
/// wherever in the window it goes. Blitz hit-tests every move and dispatches
/// to whatever is under the cursor, so a move that lands on some other
/// element only reaches the drag layer because `DragProvider` is above it in
/// the tree. Anything that breaks that path makes a knob usable only while
/// the cursor is still inside its own 72 px box, which is not how a knob
/// works — you set a knob by dragging *away* from it.
#[tokio::test]
async fn a_drag_survives_the_pointer_leaving_the_control() {
    let fx = mount().await;
    let (x, y) = fx.dial();

    fx.tester.pointer_down(x, y);
    fx.settle().await;

    // Up and across, well clear of the knob but still inside the window.
    for step in 1..=8 {
        let t = f64::from(step) / 8.0;
        fx.tester.pointer_move(x + 220.0 * t, y - 120.0 * t, true);
        fx.settle().await;
    }

    // 120 px up at the knob's 150 px sweep is +0.8.
    let held = value();
    assert!(
        held > 0.95,
        "a 120 px drag up that left the knob only reached {held}",
    );

    fx.tester.pointer_up(x + 220.0, y - 120.0);
    fx.settle().await;
}

/// And it survives the pointer leaving the *window*.
///
/// A hand setting a knob runs out of screen long before it runs out of
/// range. On macOS the view that took the mouse-down keeps receiving drag
/// events wherever the cursor goes, so the events exist — the question is
/// whether anything downstream throws them away. Headless, the harness will
/// not hit-test a point outside the viewport at all, which is the same
/// failure by a different route: `#[ignore]`d as a statement of intent until
/// the host path can deliver it.
#[tokio::test]
#[ignore = "needs pointer capture: blitz hit-tests every move and drops the ones outside the viewport"]
async fn a_drag_survives_the_pointer_leaving_the_window() {
    let fx = mount().await;
    let (x, y) = fx.dial();

    fx.tester.pointer_down(x, y);
    fx.settle().await;
    for step in 1..=8 {
        fx.tester.pointer_move(x, y - f64::from(step) * 60.0, true);
        fx.settle().await;
    }

    assert!(
        value() > 0.99,
        "a drag that left the top of the window stopped at {}",
        value(),
    );
    fx.tester.pointer_up(x, y - 480.0);
    fx.settle().await;
}

/// Leaving the bipolar detent must not jump.
///
/// The dead zone holds the value at centre while the pointer is inside it.
/// What it must not do is hand back the raw travel on the way out: that makes
/// the first pixel past the zone worth the whole width of the zone, and on a
/// ±30 dB band gain there was no way to ask for anything between 0 and
/// 2.4 dB.
#[tokio::test]
async fn leaving_the_centre_detent_is_continuous() {
    let fx = mount().await;
    let (x, y) = fx.dial();

    // A bipolar parameter centred where it starts.
    fx.tester.pointer_down(x, y);
    fx.settle().await;

    let mut last = value();
    let mut biggest_step = 0.0_f32;
    for step in 1..=40 {
        fx.tester.pointer_move(x, y - f64::from(step) * 1.0, true);
        fx.settle().await;
        let now = value();
        biggest_step = biggest_step.max((now - last).abs());
        last = now;
    }
    fx.tester.pointer_up(x, y - 40.0);
    fx.settle().await;

    // One pixel of travel is 1/150 of the sweep. Anything much past that is
    // the dead zone being handed back in one lump.
    assert!(
        biggest_step < 0.012,
        "a 1 px move stepped the value by {biggest_step} — the detent jumped",
    );
    assert!(last > 0.5, "the drag did not leave the detent at all");
}

/// The whole range has to stay reachable through the detent.
///
/// The dead zone shifts the value by its own width, so it has to be applied
/// to the raw travel and clamped afterwards. Clamp first and the raw value
/// pins at 1.0 while the shift keeps taking the zone off it — the top of the
/// range goes missing. On the EQ's ±30 dB band gain that showed up as a knob
/// that stopped at 26.4 dB.
#[tokio::test]
async fn a_full_sweep_still_reaches_the_end_of_a_detented_range() {
    let fx = mount().await;
    let (x, y) = fx.dial();

    fx.tester.pointer_down(x, y);
    fx.settle().await;
    // Half a sweep is 75 px; the detent costs six more.
    for step in 1..=30 {
        fx.tester.pointer_move(x, y - f64::from(step) * 4.0, true);
        fx.settle().await;
    }
    fx.tester.pointer_up(x, y - 120.0);
    fx.settle().await;

    assert!(
        value() >= 1.0,
        "a full sweep up stopped at {} — the detent ate the top of the range",
        value(),
    );
}
