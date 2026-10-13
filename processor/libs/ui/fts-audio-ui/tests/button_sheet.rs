//! The button contact sheet: every part in the kit, up and down.
//!
//! Third of the kit sheets, after `knob_sheet` and `vu_sheet`. Adding a
//! button is a [`ButtonStyle`] variant plus one `ButtonSpec` const in
//! `button_parts.rs` — and then you want to look at it, in both states,
//! rather than wiring it onto a faceplate:
//!
//! ```sh
//! cargo test -p fts-audio-ui --test button_sheet
//! ```
//!
//! Output lands in `target/gui-shots/buttons/` (override with
//! `FTS_SHOTS_DIR`). Nothing here asserts a *look*. What it asserts is that
//! every part draws in both states and that pressing one actually changes the
//! parameter behind it — a button that draws beautifully and does nothing is
//! the failure a screenshot cannot see.

use std::path::PathBuf;

use dioxus::prelude::*;
use dioxus_test::{by_testid, render};

use fts_audio_ui::hardware::button::{ButtonStyle, PanelButton};
use fts_audio_ui::param::ParamHandle;

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

// Where the sheet's live handles park their value, so a test can read what a
// press actually wrote.
//
// The DOM cannot answer this: the sheet's handles are plain atomics rather
// than signals, so writing one does not re-render the button and `data-on`
// stays where it was. That is a property of this harness, not of the widget
// — in a plugin the handle is signal-backed — and the *parameter* is the
// thing worth asserting on anyway. A button that redraws but writes nothing
// is the failure that matters.
// Thread-local, not global: the tests in this file each render their own copy
// of the sheet and cargo runs them in parallel, so a shared map would have one
// test reading the other's handles — which looked exactly like the buttons
// failing to latch.
thread_local! {
    static CELLS: RefCell<HashMap<String, Arc<AtomicU32>>> = RefCell::new(HashMap::new());
}

/// What `key`'s handle holds now.
fn value_of(key: &str) -> f32 {
    CELLS.with(|c| {
        let map = c.borrow();
        let cell = map
            .get(key)
            .unwrap_or_else(|| panic!("no handle registered for {key}"));
        f32::from_bits(cell.load(Ordering::Relaxed))
    })
}

/// A handle that actually remembers what was written to it.
///
/// `ParamHandle::inert` reads a fixed position and drops writes — which is
/// exactly right for a control the DSP does not back yet, and useless for
/// asking "did pressing this change anything". Stored as bits in an atomic so
/// the closures stay `Send + Sync`.
fn live(key: &str, name: &str, start: f32) -> ParamHandle {
    let cell = Arc::new(AtomicU32::new(start.to_bits()));
    CELLS.with(|c| c.borrow_mut().insert(key.to_string(), cell.clone()));
    let read = cell.clone();
    let write = cell.clone();
    let shown = name.to_string();
    let named = name.to_string();
    ParamHandle::new(
        move || f32::from_bits(read.load(Ordering::Relaxed)),
        || {},
        move |v: f32| write.store(v.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed),
        || {},
        move || shown.clone(),
        move || named.clone(),
        |_| None,
    )
}

/// The colours a panel would actually order these in.
const CAP: &str = "#e6e2d4";
const LED: &str = "#43d17a";

#[component]
fn Sheet() -> Element {
    rsx! {
        style {
            "html, body {{ margin:0; padding:0; background:#20232a; \
             font-family: ui-sans-serif, system-ui, sans-serif; }}"
        }
        div {
            style: "display:flex; flex-direction:column; gap:14px; padding:16px;",
            for style in ButtonStyle::ALL {
                div {
                    key: "{style:?}",
                    style: "display:flex; align-items:center; gap:26px;",
                    div {
                        style: "width:110px; color:#dfe3e8; font-size:12px; \
                                font-weight:700; letter-spacing:0.04em;",
                        "{style:?}"
                    }
                    // Off, on, and unwired — the three states a face can put
                    // one of these in.
                    for (i , (pos , wired)) in
                        [(0.0_f32, true), (1.0, true), (0.0, false)].iter().enumerate()
                    {
                        div {
                            key: "s{i}",
                            style: "display:flex; flex-direction:column; \
                                    align-items:center; gap:6px;",
                            PanelButton {
                                handle: wired.then(|| live(&format!("{style:?}-{i}"), "In", *pos)),
                                testid: format!("{style:?}-{i}").to_lowercase(),
                                scale: 1.0,
                                label: "IN".to_string(),
                                color: CAP.to_string(),
                                led: LED.to_string(),
                                style,
                            }
                            div {
                                style: "color:#8b929b; font-size:9px;",
                                {if !wired { "unwired" } else if *pos > 0.5 { "on" } else { "off" }}
                            }
                        }
                    }
                    // And with the lamp suppressed: a face saying "ours has
                    // no lamp" overrides the part, whatever it would do.
                    PanelButton {
                        handle: Some(live(&format!("{style:?}-nolamp"), "In", 1.0)),
                        testid: format!("{style:?}-nolamp").to_lowercase(),
                        scale: 1.0,
                        label: "IN".to_string(),
                        color: CAP.to_string(),
                        led: String::new(),
                        style,
                    }
                }
            }
        }
    }
}

fn shots_dir() -> PathBuf {
    let dir = std::env::var("FTS_SHOTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../target/gui-shots/buttons")
        });
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("create {}: {e}", dir.display()));
    dir
}

#[tokio::test]
async fn shot_every_button_in_the_kit() {
    let tester = render(Sheet)
        .with_window_size(620, 40 + 92 * ButtonStyle::ALL.len() as u32)
        .build();
    let _ = tester.pump().await;
    tester.relayout();

    for style in ButtonStyle::ALL {
        for i in 0..3 {
            let id = format!("hw-button-{style:?}-{i}").to_lowercase();
            let el = tester
                .query(by_testid(&id))
                .immediately()
                .unwrap_or_else(|e| panic!("{style:?} did not draw in state {i}: {e:?}"));
            let (w, h) = el.size();
            assert!(w > 0.0 && h > 0.0, "{style:?} drew {w}x{h} in state {i}");
        }
        // The cap is the part that is pressed, so it has to have a box of its
        // own — a surround that swallowed it would leave nothing to click.
        let cap = tester
            .query(by_testid(
                &format!("hw-button-{style:?}-0-cap").to_lowercase(),
            ))
            .immediately()
            .expect("no cap");
        let (cw, ch) = cap.size();
        assert!(cw > 0.0 && ch > 0.0, "{style:?}'s cap drew {cw}x{ch}");
    }

    let path = shots_dir().join("kit.png");
    tester.render_png(&path);
    println!("button kit: {}", path.display());
}

/// A hit-tested press on the cap toggles the parameter behind it — for every
/// part in the kit, including the ones whose cap sits inside a surround,
/// where a badly sized surround would swallow the click.
#[tokio::test]
async fn every_button_in_the_kit_can_actually_be_pressed() -> dioxus_test::Result<()> {
    let tester = render(Sheet)
        .with_window_size(620, 40 + 92 * ButtonStyle::ALL.len() as u32)
        .build();
    let _ = tester.pump().await;
    tester.relayout();

    for style in ButtonStyle::ALL {
        let key = format!("{style:?}-0");
        assert_eq!(value_of(&key), 0.0, "{style:?} started on");

        let cap = tester
            .query(by_testid(&format!("hw-button-{key}-cap").to_lowercase()))
            .immediately()?;
        let (x, y) = cap.document_origin();
        let (w, h) = cap.size();
        tester.pointer_down(x + w as f64 / 2.0, y + h as f64 / 2.0);
        let _ = tester.pump().await;
        tester.pointer_up(x + w as f64 / 2.0, y + h as f64 / 2.0);
        let _ = tester.pump().await;

        assert_eq!(
            value_of(&key),
            1.0,
            "{style:?} did not latch when its cap was pressed",
        );
    }
    Ok(())
}
