//! The touch rule, on the real event path (Blitz hit-testing, panning and
//! click synthesis, headless): **a scroll or a drag never fires an action,
//! and only a row's ⋯ opens its menu.**
//!
//! A list of [`RowMenu`] rows — each a tappable button and a ⋯
//! ([`MoreButton`]) — in a scroll container, in touch mode. A finger that
//! rests on a row and then scrolls the list (the gesture that used to open
//! the row's long-press menu mid-scroll) must open nothing and tap nothing;
//! a finger held still and lifted is a plain tap; the ⋯ opens the menu.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use dioxus::prelude::*;
use dioxus_test::{DocumentTester, render};
use signal_rig_perform_ui::menu::{Item, MoreButton, Picked, RowMenu};
use signal_widgets::{PopupHost, PopupLayer, Touch};

thread_local! {
    static TAPS: AtomicUsize = const { AtomicUsize::new(0) };
    static PICKS: AtomicUsize = const { AtomicUsize::new(0) };
}

fn taps() -> usize {
    TAPS.with(|t| t.load(Ordering::SeqCst))
}

fn picks() -> usize {
    PICKS.with(|t| t.load(Ordering::SeqCst))
}

/// Longer than any press-and-hold ever was (500 ms).
const HOLD: Duration = Duration::from_millis(700);

#[component]
fn Setlist() -> Element {
    Touch::provide_with(true);
    PopupHost::provide();
    rsx! {
        div { style: "position: relative; width: 420px; height: 360px; display: flex; flex-direction: column;",
            div { "data-testid": "list", style: "flex: 1; min-height: 0; overflow-y: auto;",
                for i in 0..20usize {
                    RowMenu {
                        key: "{i}",
                        items: vec![Item::head(format!("Song {i}")), Item::run("go", "Play")],
                        on_pick: move |_: Picked| {
                            PICKS.with(|p| p.fetch_add(1, Ordering::SeqCst));
                        },
                        style: "position: relative; display: flex; align-items: center; height: 64px;",
                        button {
                            "data-testid": "row-{i}",
                            style: "flex: 1; height: 64px; border: none; background: transparent;",
                            onclick: move |_| {
                                TAPS.with(|t| t.fetch_add(1, Ordering::SeqCst));
                            },
                            "Song {i}"
                        }
                        MoreButton {
                            label: format!("Song {i} actions"),
                            items: vec![Item::head(format!("Song {i}")), Item::run("go", "Play")],
                            on_pick: move |_: Picked| {
                            PICKS.with(|p| p.fetch_add(1, Ordering::SeqCst));
                        },
                        }
                    }
                }
            }
            PopupLayer {}
        }
    }
}

fn tester() -> DocumentTester {
    TAPS.with(|t| t.store(0, Ordering::SeqCst));
    PICKS.with(|t| t.store(0, Ordering::SeqCst));
    render(Setlist).with_window_size(420, 360).build()
}

/// Run the handlers (and anything they spawned), then lay out again.
async fn settle(t: &DocumentTester) {
    let _ = t.pump().await;
    t.relayout();
}

fn centre(t: &DocumentTester, sel: &str) -> (f64, f64) {
    let el = t.query(sel).immediately().expect("element in the DOM");
    let (x, y) = el.document_origin();
    let (w, h) = el.size();
    (x + f64::from(w) / 2.0, y + f64::from(h) / 2.0)
}

fn menu_open(t: &DocumentTester) -> bool {
    t.query("[role=menu]").immediately().is_ok()
}

#[tokio::test(flavor = "current_thread")]
async fn a_scroll_over_a_row_opens_no_menu_and_fires_no_action() {
    let t = tester();
    settle(&t).await;
    let (x, y) = centre(&t, "[data-testid='row-2']");
    // Rest on the row past the old hold time, then scroll the list up.
    t.finger_down(x, y);
    settle(&t).await;
    tokio::time::sleep(HOLD).await;
    settle(&t).await;
    for step in 1..=12 {
        t.finger_move(x, y - 15.0 * f64::from(step));
        settle(&t).await;
    }
    t.finger_up(x, y - 180.0);
    settle(&t).await;
    tokio::time::sleep(HOLD).await;
    settle(&t).await;

    assert!(!menu_open(&t), "a scroll opened a menu");
    assert_eq!(taps(), 0, "a scroll tapped a row");
    assert_eq!(picks(), 0, "a scroll picked a menu item");
}

#[tokio::test(flavor = "current_thread")]
async fn a_finger_held_still_is_a_tap_not_a_menu() {
    let t = tester();
    settle(&t).await;
    let (x, y) = centre(&t, "[data-testid='row-3']");
    t.finger_down(x, y);
    settle(&t).await;
    tokio::time::sleep(HOLD).await;
    settle(&t).await;
    assert!(!menu_open(&t), "holding still opened a menu");
    t.finger_up(x, y);
    settle(&t).await;
    assert!(!menu_open(&t), "the hold's release opened a menu");
    assert_eq!(taps(), 1, "lifting a still finger is one tap");
}

#[tokio::test(flavor = "current_thread")]
async fn the_more_button_opens_the_rows_menu() {
    let t = tester();
    settle(&t).await;
    assert!(!menu_open(&t));
    let (x, y) = centre(&t, "[aria-label='Song 1 actions']");
    t.finger_down(x, y);
    settle(&t).await;
    t.finger_up(x, y);
    settle(&t).await;
    settle(&t).await;
    assert!(menu_open(&t), "the ⋯ did not open the row's menu");
    assert_eq!(taps(), 0, "the ⋯ also tapped its row");
}
