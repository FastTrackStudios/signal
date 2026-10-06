//! Presses as Blitz delivers them to the rigs' controls: a click goes to
//! what the press and the release share, and a dropdown hung from a
//! z-indexed bar inside a z-indexed header — centred with a transform —
//! takes its own presses rather than passing them to what lies beneath.

use std::sync::atomic::{AtomicU32, Ordering};

use dioxus::prelude::*;
use dioxus_test::render;

static A: AtomicU32 = AtomicU32::new(0);
static B: AtomicU32 = AtomicU32::new(0);

#[component]
fn TwoButtons() -> Element {
    rsx! {
        div { style: "display: flex; width: 400px; height: 100px;",
            button { style: "width: 200px; height: 100px;", onclick: |_| { A.fetch_add(1, Ordering::SeqCst); }, "A" }
            button { style: "width: 200px; height: 100px;", onclick: |_| { B.fetch_add(1, Ordering::SeqCst); }, "B" }
        }
    }
}

/// A knob dragged and let go over a footswitch must not press it: the
/// release is not a click on whatever it ended over.
#[tokio::test]
async fn a_press_released_over_another_control_clicks_neither() {
    let t = render(TwoButtons).build();
    t.pointer_down(50.0, 50.0);
    t.pointer_up(300.0, 50.0);
    t.pump().await.ok();
    assert_eq!(A.load(Ordering::SeqCst), 0, "pressed A, released over B: not a click on A");
    assert_eq!(B.load(Ordering::SeqCst), 0, "pressed A, released over B: not a click on B");
    // Pressed and released on B: a click on B.
    t.pointer_down(300.0, 50.0);
    t.pointer_up(300.0, 50.0);
    t.pump().await.ok();
    assert_eq!(B.load(Ordering::SeqCst), 1);
}

static PANEL: AtomicU32 = AtomicU32::new(0);
static UNDER: AtomicU32 = AtomicU32::new(0);

#[component]
fn HeaderWithDropdown() -> Element {
    rsx! {
        div { style: "position: relative; width: 800px; height: 600px;",
            // The header (z 3), the bar in it (z 40), a cell, and the
            // panel hung under the cell, centred on it (z 50).
            div { style: "position: relative; z-index: 3; height: 60px; display: flex;",
                div { style: "position: relative; z-index: 40; width: 100%; height: 60px;",
                    div { style: "position: relative; width: 100px; height: 60px; margin-left: 300px;",
                        div { style: "position: absolute; top: 100%; left: 50%; transform: translateX(-50%); z-index: 50; width: 300px; height: 100px; display: flex;",
                            button { style: "width: 100px; height: 100px;", onclick: |_| { PANEL.fetch_add(1, Ordering::SeqCst); }, "left" }
                        }
                    }
                }
            }
            // What the panel hangs over: the footswitches.
            button { style: "display: block; width: 800px; height: 540px;", onclick: |_| { UNDER.fetch_add(1, Ordering::SeqCst); }, "under" }
        }
    }
}

/// The panel spans x 200–500 (its cell at 300–400, centred), y 60–160. A
/// press on its left half is the panel's, not the switch's beneath.
#[tokio::test]
async fn a_centred_dropdown_from_a_nested_layer_takes_its_presses() {
    let t = render(HeaderWithDropdown).with_window_size(800, 600).build();
    t.pointer_down(250.0, 110.0);
    t.pointer_up(250.0, 110.0);
    t.pump().await.ok();
    assert_eq!(UNDER.load(Ordering::SeqCst), 0, "the press went through the panel");
    assert_eq!(PANEL.load(Ordering::SeqCst), 1, "the panel's button took it");
}
