//! Isolating a drag panic in the primitive-backed slider.
//!
//! The end-to-end test (`midi-tools-daw/tests/reaper_velocity_gui.rs`)
//! found that dragging the amount slider panics with "RefCell already
//! borrowed" inside dioxus's native-dom event dispatch, while clicking
//! chips and dragging the hand-rolled bar editor are fine. This narrows
//! that down with no REAPER and no panel — just the widget.

use dioxus::prelude::*;
use dioxus_test::{by_testid, render};

/// A slider and nothing else.
#[component]
fn LoneSlider() -> Element {
    let mut value = use_signal(|| 0.0_f64);
    rsx! {
        div {
            "data-testid": "readout",
            style: "width:400px;",
            expression_editor_ui::test_support::Slider {
                testid: "s".to_string(),
                value: value(),
                min: 0.0,
                max: 1.0,
                width: 300.0,
                on_change: move |v| value.set(v),
            }
            "{value():.3}"
        }
    }
}

/// A press with no movement — does the panic need a drag, or just a down?
#[tokio::test]
async fn a_press_alone_does_not_panic() -> dioxus_test::Result<()> {
    let tester = render(LoneSlider).with_window_size(500, 200).build();
    let el = tester.query(by_testid("s")).immediately()?;
    let (ox, oy) = el.document_origin();
    let (w, h) = el.size();
    tester.pointer_down(ox + w as f64 * 0.5, oy + h as f64 / 2.0);
    let _ = tester.pump().await;
    tester.pointer_up(ox + w as f64 * 0.5, oy + h as f64 / 2.0);
    let _ = tester.pump().await;
    Ok(())
}

/// The failing gesture: press, move, release.
#[tokio::test]
async fn a_drag_moves_the_value() -> dioxus_test::Result<()> {
    let tester = render(LoneSlider).with_window_size(500, 200).build();
    let el = tester.query(by_testid("s")).immediately()?;
    let (ox, oy) = el.document_origin();
    let (w, h) = el.size();
    let y = oy + h as f64 / 2.0;

    tester.pointer_down(ox + w as f64 * 0.05, y);
    let _ = tester.pump().await;
    for i in 1..=6 {
        tester.pointer_move(ox + w as f64 * (0.05 + 0.9 * i as f64 / 6.0), y, true);
        let _ = tester.pump().await;
    }
    tester.pointer_up(ox + w as f64 * 0.95, y);
    let _ = tester.pump().await;

    let html = tester
        .query(by_testid("readout"))
        .immediately()?
        .inner_html();
    assert!(
        !html.contains("0.000"),
        "the drag should have moved the value, got: {html}"
    );
    Ok(())
}
