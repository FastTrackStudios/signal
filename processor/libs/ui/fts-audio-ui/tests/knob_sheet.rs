//! The knob contact sheet: every style in the kit, painted to one PNG.
//!
//! Adding a knob is a [`KnobStyle`] variant plus one `KnobSpec` const in
//! `knob_parts.rs` — and then you want to *look* at it, which used to mean
//! wiring it onto a faceplate and shooting that. This paints the whole kit
//! instead, each style plain and tinted, at the size a panel actually asks
//! for:
//!
//! ```sh
//! cargo test -p fts-audio-ui --test knob_sheet
//! ```
//!
//! Output lands in `target/gui-shots/knobs/` (override with `FTS_SHOTS_DIR`).
//! Nothing here asserts a *look* — a wrong-looking knob is a picture you have
//! to look at. What it does assert is that every style in the kit draws
//! something, at three positions, without panicking.

use std::path::PathBuf;

use dioxus::prelude::*;
use dioxus_test::{by_testid, render};

use fts_audio_ui::drag::DragProvider;
use fts_audio_ui::hardware::knob::{HardwareKnob, KnobStyle};
use fts_audio_ui::hardware::knob_svg::{linear_scale_label, scale_ring};
use fts_audio_ui::param::ParamHandle;

/// Three positions across the sweep, so an index that only looks right at
/// twelve o'clock gives itself away.
const POSITIONS: [f32; 3] = [0.0, 0.5, 0.88];

/// The colour a console would code a band with, to exercise the tint path.
const TINT: &str = "#2f6f9e";

/// Height of one style's row on the sheet.
const ROW_H: u32 = 112;

#[component]
fn Sheet() -> Element {
    rsx! {
        style {
            "html, body {{ margin:0; padding:0; background:#20232a; \
             font-family: ui-sans-serif, system-ui, sans-serif; }}"
        }
        DragProvider {
            div {
                style: "display:flex; flex-direction:column; gap:4px; padding:14px;",
                for style in KnobStyle::ALL {
                    div {
                        key: "{style:?}",
                        style: "display:flex; align-items:center; gap:18px;",
                        div {
                            style: "width:112px; color:#dfe3e8; font-size:12px; \
                                    font-weight:700; letter-spacing:0.04em;",
                            "{style:?}"
                        }
                        for (i , pos) in POSITIONS.iter().enumerate() {
                            HardwareKnob {
                                key: "p{i}",
                                handle: ParamHandle::inert(format!("{style:?}"), *pos),
                                testid: format!("{style:?}-{i}").to_lowercase(),
                                scale: 1.0,
                                diameter: 58.0,
                                style,
                                ink: "#c9ced5".to_string(),
                                marks: scale_ring(5, 1, linear_scale_label(0.0, 10.0)),
                            }
                        }
                        // The same knob in a band colour, so the tint path is
                        // on the sheet too — a finish that ignores its tint is
                        // invisible until a console asks for one.
                        HardwareKnob {
                            handle: ParamHandle::inert(format!("{style:?} tinted"), 0.62),
                            testid: format!("{style:?}-tint").to_lowercase(),
                            scale: 1.0,
                            diameter: 58.0,
                            style,
                            ink: "#c9ced5".to_string(),
                            marks: scale_ring(5, 1, linear_scale_label(0.0, 10.0)),
                            tint: TINT.to_string(),
                        }
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
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../target/gui-shots/knobs")
        });
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("create {}: {e}", dir.display()));
    dir
}

/// Paint the kit, and check that every style in it actually drew a knob at
/// every position — a spec with no tiers, or one whose radii collapse, is a
/// blank space on a panel and nothing else would catch it.
#[tokio::test]
async fn shot_every_knob_in_the_kit() {
    let tester = render(Sheet)
        // A row is the knob box (58 px scaled by the 110/60 viewBox) plus
        // its gap; short by even a little and the last styles fall off the
        // sheet, which is exactly when you would want to see them.
        .with_window_size(560, 40 + ROW_H * KnobStyle::ALL.len() as u32)
        .build();
    let _ = tester.pump().await;
    tester.relayout();

    for style in KnobStyle::ALL {
        for i in 0..POSITIONS.len() {
            let id = format!("hw-knob-{style:?}-{i}").to_lowercase();
            let el = tester
                .query(by_testid(&id))
                .immediately()
                .unwrap_or_else(|e| panic!("{style:?} did not draw at position {i}: {e:?}"));
            let (w, h) = el.size();
            assert!(
                w > 0.0 && h > 0.0,
                "{style:?} drew a {w}x{h} knob at position {i}",
            );
        }
    }

    let path = shots_dir().join("kit.png");
    tester.render_png(&path);
    println!("knob kit: {}", path.display());
}
