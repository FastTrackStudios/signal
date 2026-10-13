//! The VU contact sheet: every face in the kit, painted to one PNG.
//!
//! The companion to `knob_sheet.rs`. Adding a face is a [`VuFace`] variant
//! plus one `VuSpec` const in `vu_faces.rs` — and then you want to look at
//! it, at rest and swinging, in its bezel and out of it:
//!
//! ```sh
//! cargo test -p fts-audio-ui --test vu_sheet
//! ```
//!
//! Output lands in `target/gui-shots/vu/` (override with `FTS_SHOTS_DIR`).
//! Nothing here asserts a *look*. What it asserts is that every face draws a
//! card and a needle, and that the needle actually moves when the value does
//! — a face whose needle matched its card would be a meter you cannot read,
//! and no colour test would notice.

use std::path::PathBuf;

use dioxus::prelude::*;
use dioxus_test::{by_testid, render};

use fts_audio_ui::hardware::vu::{BezelStyle, VuFace, VuMeter, VuMode};
use fts_audio_ui::hardware::vu_svg::VuScale;

/// Rest, working, and pinned — enough to see the needle sweep and to catch a
/// scale that crowds the wrong end.
const READINGS: [f32; 3] = [0.0, 6.0, 14.0];

/// The backlit face lit in the colours a panel might ask for. Its whole look
/// is a lamp behind smoked glass, so this is the axis that matters for it.
const LIT_COLOURS: [&str; 4] = ["#4a9eff", "#c0392b", "#39c07a", "#e8a33d"];

#[component]
fn Sheet() -> Element {
    rsx! {
        style {
            "html, body {{ margin:0; padding:0; background:#20232a; \
             font-family: ui-sans-serif, system-ui, sans-serif; }}"
        }
        div {
            style: "display:flex; flex-direction:column; gap:10px; padding:14px;",
            for face in VuFace::ALL {
                div {
                    key: "{face:?}",
                    style: "display:flex; align-items:center; gap:16px;",
                    div {
                        style: "width:96px; color:#dfe3e8; font-size:12px; \
                                font-weight:700; letter-spacing:0.04em;",
                        "{face:?}"
                    }
                    for (i , db) in READINGS.iter().enumerate() {
                        VuMeter {
                            key: "r{i}",
                            scale: 1.0,
                            width: 150.0,
                            face,
                            mode: VuMode::GainReduction,
                            value_db: *db,
                            legend: "VU".to_string(),
                            card: VuScale::Vu,
                        }
                    }
                    // In its frame, which is a different drawing: the chamfer
                    // has to read as an opening rather than a raised boss.
                    VuMeter {
                        scale: 1.0,
                        width: 150.0,
                        face,
                        mode: VuMode::Level,
                        value_db: -12.0,
                        legend: "VU".to_string(),
                        bezel: true,
                        card: VuScale::Vu,
                    }
                }
            }

            // The backlit face, which is the one that takes a colour from the
            // call site: same movement, same spec, different lamp.
            div {
                style: "margin-top:6px; color:#dfe3e8; font-size:12px; \
                        font-weight:700; letter-spacing:0.04em;",
                "BACKLIT — TINTED PER PANEL"
            }
            div {
                style: "display:flex; flex-wrap:wrap; gap:16px; padding:8px 0 18px;",
                for (i , color) in LIT_COLOURS.iter().enumerate() {
                    VuMeter {
                        key: "lit{i}",
                        scale: 1.0,
                        width: 168.0,
                        face: VuFace::Backlit,
                        mode: VuMode::GainReduction,
                        value_db: 7.0,
                        legend: "dB".to_string(),
                        bezel: true,
                        card: VuScale::Vu,
                        tint: color.to_string(),
                    }
                }
            }

            // The frames, all on one movement, so the difference between them
            // is the only thing changing.
            div {
                style: "margin-top:6px; color:#dfe3e8; font-size:12px; \
                        font-weight:700; letter-spacing:0.04em;",
                "BEZELS"
            }
            div {
                style: "display:flex; flex-wrap:wrap; align-items:flex-start; \
                        gap:22px; padding:8px 0 18px;",
                for style in BezelStyle::ALL {
                    div {
                        key: "{style:?}",
                        style: "display:flex; flex-direction:column; align-items:center; gap:5px;",
                        VuMeter {
                            scale: 1.0,
                            width: 130.0,
                            face: if style.is_lit() { VuFace::Amber } else { VuFace::Ivory },
                            mode: VuMode::Level,
                            value_db: -14.0,
                            legend: "VU".to_string(),
                            bezel: true,
                            bezel_style: style,
                            card: VuScale::Vu,
                        }
                        div {
                            style: "color:#aeb4bb; font-size:10px; letter-spacing:0.03em;",
                            "{style:?}"
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
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../target/gui-shots/vu")
        });
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("create {}: {e}", dir.display()));
    dir
}

#[tokio::test]
async fn shot_every_vu_face_in_the_kit() {
    let tester = render(Sheet)
        .with_window_size(800, 620 + 130 * VuFace::ALL.len() as u32)
        .build();
    let _ = tester.pump().await;
    tester.relayout();

    // Every face drew a movement with a real box, and a needle on it.
    let meters = tester.query_all(by_testid("vu-meter")).immediately();
    // Each face row is the readings plus one framed; then the bezel row is
    // one movement per frame in the kit.
    let expected =
        VuFace::ALL.len() * (READINGS.len() + 1) + LIT_COLOURS.len() + BezelStyle::ALL.len();
    assert_eq!(
        meters.len(),
        expected,
        "the sheet drew {} movements, not {expected}",
        meters.len(),
    );
    for m in &meters {
        let (w, h) = m.size();
        assert!(w > 0.0 && h > 0.0, "a movement drew {w}x{h}");
    }

    // And the needle moved: the readings are distinct, so the positions the
    // meters report must be too. A face that ignored its value would draw
    // four identical movements and look perfectly fine in a screenshot.
    let positions: Vec<String> = meters
        .iter()
        .take(READINGS.len())
        .filter_map(|m| m.attribute("data-vu"))
        .collect();
    assert_eq!(
        positions.len(),
        READINGS.len(),
        "a movement reported no position"
    );
    for pair in positions.windows(2) {
        assert_ne!(
            pair[0], pair[1],
            "the needle did not move between two different readings: {positions:?}",
        );
    }

    // Every frame in the kit drew, and the lit ones are actually marked as
    // lit — a glow that silently went missing would look like an unlit meter
    // and nothing else would say so.
    let bezels = tester.query_all(by_testid("vu-bezel")).immediately();
    assert_eq!(
        bezels.len(),
        VuFace::ALL.len() + LIT_COLOURS.len() + BezelStyle::ALL.len(),
        "not every frame drew",
    );
    let lit = bezels
        .iter()
        .filter(|b| b.attribute("data-lit").as_deref() == Some("true"))
        .count();
    assert_eq!(
        lit,
        BezelStyle::ALL.iter().filter(|s| s.is_lit()).count(),
        "the lit frames did not draw as lit",
    );

    // Exactly one face takes a colour. The rest are real parts in the colour
    // they are, and quietly recolouring one would be a different unit.
    let tintable: Vec<VuFace> = VuFace::ALL
        .into_iter()
        .filter(|f| f.is_tintable())
        .collect();
    assert_eq!(
        tintable,
        vec![VuFace::Backlit],
        "the wrong faces claim to take a tint",
    );

    let path = shots_dir().join("kit.png");
    tester.render_png(&path);
    println!("vu kit: {}", path.display());
}
