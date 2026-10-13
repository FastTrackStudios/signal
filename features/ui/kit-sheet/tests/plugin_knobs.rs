//! Every knob in the product, on the panel it actually sits on.
//!
//! The kit sheet in `fts-audio-ui` shows the eleven styles side by side on
//! one neutral background, which answers "does this style draw" and not the
//! question you actually have, which is "does the 1176's knob look right on
//! the 1176's panel, next to the LA-2A's on its". Knob art is judged by
//! comparison: the same style at two diameters over two panel colours, with
//! the legends the faces really print.
//!
//! ```sh
//! cargo test -p kit-sheet --test plugin_knobs
//! ```
//!
//! Lands in `target/gui-shots/plugin-knobs/` (override with `FTS_SHOTS_DIR`).
//! Nothing here asserts a look — a wrong-looking knob is a picture you have
//! to look at. It asserts that every face in every plugin draws all of its
//! knobs, which is the thing that silently stops being true when a design
//! gains a control.

use std::path::PathBuf;

use dioxus::prelude::*;
use dioxus_test::render;

use fts_audio_ui::drag::DragProvider;
use fts_audio_ui::hardware::knob::{HardwareKnob, KnobStyle};
use fts_audio_ui::hardware::knob_svg::{linear_scale_label, scale_ring};
use fts_audio_ui::hardware::rack::RackItem;
use fts_audio_ui::param::ParamHandle;

/// One knob as a face asks for it.
#[derive(Clone, PartialEq)]
struct Knob {
    param: String,
    legend: &'static str,
    diameter: f64,
    style: KnobStyle,
    tint: Option<String>,
    /// A dual-concentric knob's inner cap. The 1073's frequency collar and
    /// the gain cap inside it are one placement and two controls, and the
    /// pair is what you have to look at — a cap that reads as a separate
    /// small knob is the failure mode.
    inner: bool,
}

/// One face: a panel colour, an ink, and the knobs printed on it.
#[derive(Clone, PartialEq)]
struct Face {
    plugin: &'static str,
    name: String,
    paint: String,
    ink: String,
    knobs: Vec<Knob>,
}

/// Positions chosen so a row is not a line of identical pointers: an index
/// that only looks right at twelve o'clock gives itself away when its
/// neighbours are elsewhere.
fn position(i: usize) -> f32 {
    const SWEEP: [f32; 5] = [0.12, 0.34, 0.5, 0.72, 0.9];
    SWEEP[i % SWEEP.len()]
}

/// How many knobs sit on one line of a face's row.
///
/// Fixed rather than `flex-wrap`, because the sheet's height has to be known
/// before it is rendered — a row that wraps one line further than the window
/// was sized for silently pushes every face below it off the bottom, which is
/// exactly the face you were trying to look at.
const COLS: usize = 12;
/// Design px per screen px. The faces span 20 px (the Distressor's trims) to
/// 92 px (a Pultec boost), and the widest has to fit a cell.
const SCALE: f64 = 0.5;
/// The knob's box is wider than the knob: the printed ring is drawn outside
/// the body, so the `-55..55` viewBox spans 110 design px for a 60 px body.
const BOX_RATIO: f64 = 110.0 / 60.0;
/// Room under the biggest knob for the legend and the style name.
const CAPTION_H: f64 = 26.0;
/// The row's own padding, plus the gap to the next row.
const ROW_PAD: f64 = 22.0;

fn box_px(diameter: f64) -> f64 {
    diameter * BOX_RATIO * SCALE
}

/// One line of a face's row: the tallest knob on the face, plus its caption.
fn cell_h(f: &Face) -> f64 {
    let tallest = f
        .knobs
        .iter()
        .map(|k| box_px(k.diameter))
        .fold(0.0_f64, f64::max);
    tallest + CAPTION_H
}

fn row_h(f: &Face) -> f64 {
    let lines = f.knobs.len().div_ceil(COLS).max(1) as f64;
    lines * cell_h(f) + ROW_PAD
}

fn saturate_faces() -> Vec<Face> {
    saturate_profiles::PROFILES
        .iter()
        .map(|p| {
            let d = saturate_ui::faces::design_for(p.id);
            Face {
                plugin: "Saturate",
                name: p.name.to_string(),
                paint: d.paint.to_string(),
                ink: d.ink.to_string(),
                knobs: d
                    .knobs
                    .iter()
                    .map(|k| Knob {
                        param: k.param.to_string(),
                        legend: k.legend,
                        diameter: k.d,
                        style: k.style,
                        tint: None,
                        inner: false,
                    })
                    .collect(),
            }
        })
        .collect()
}

fn delay_faces() -> Vec<Face> {
    delay_profiles::PROFILES
        .iter()
        .map(|p| {
            let d = delay_ui::faces::design_for(p.id);
            Face {
                plugin: "Delay",
                name: p.name.to_string(),
                paint: d.paint.to_string(),
                ink: d.ink.to_string(),
                knobs: d
                    .knobs
                    .iter()
                    .map(|k| Knob {
                        param: k.param.to_string(),
                        legend: k.legend,
                        diameter: k.d,
                        style: k.style,
                        tint: None,
                        inner: false,
                    })
                    .collect(),
            }
        })
        .collect()
}

fn reverb_faces() -> Vec<Face> {
    reverb_profiles::PROFILES
        .iter()
        .map(|p| {
            let d = reverb_ui::faces::design_for(p.id);
            Face {
                plugin: "Reverb",
                name: p.name.to_string(),
                paint: d.paint.to_string(),
                ink: d.ink.to_string(),
                knobs: d
                    .knobs
                    .iter()
                    .chain(reverb_ui::faces::extras_for(p.id).iter())
                    .map(|k| Knob {
                        param: k.param.to_string(),
                        legend: k.legend,
                        diameter: k.d,
                        style: k.style,
                        tint: None,
                        inner: false,
                    })
                    .collect(),
            }
        })
        .collect()
}

/// The knobs on a rack panel, in the order it prints them.
///
/// A face names one style for the panel and an item may override it: the
/// Pultec's five big boost/atten knobs and its three small pointer knobs are
/// different parts, and a sheet that flattened them to one style would show a
/// panel nobody built.
fn rack_knobs(d: &'static fts_audio_ui::hardware::rack::RackDesign) -> Vec<Knob> {
    d.items
        .iter()
        .filter_map(|item| match item {
            RackItem::Knob {
                id,
                legend,
                d: dia,
                tint,
                style,
                ..
            } => Some(Knob {
                param: (*id).to_string(),
                legend,
                diameter: *dia,
                style: style.unwrap_or(d.knob),
                tint: tint.map(str::to_string),
                inner: false,
            }),
            RackItem::Concentric {
                outer_id,
                legend,
                d: dia,
                tint,
                ..
            } => Some(Knob {
                param: (*outer_id).to_string(),
                legend,
                diameter: *dia,
                style: d.knob,
                tint: tint.map(str::to_string),
                inner: true,
            }),
            _ => None,
        })
        .collect()
}

/// The EQ's emulated units. Keyed by the `model` parameter's value rather
/// than a profile id — the EQ's faces predate the profile table.
fn eq_faces() -> Vec<Face> {
    (1..=5)
        .filter_map(|model| {
            let d = eq_ui::faces::units::design_for(model)?;
            Some(Face {
                plugin: "EQ",
                name: d.id.replace('_', " "),
                paint: d.paint.to_string(),
                ink: d.ink.to_string(),
                knobs: rack_knobs(d),
            })
        })
        // 4 and 5 are the same SSL panel.
        .fold(Vec::new(), |mut acc: Vec<Face>, f| {
            if !acc.iter().any(|seen| seen.name == f.name) {
                acc.push(f);
            }
            acc
        })
}

/// The compressor's emulated units. A rack face names one knob style for the
/// panel and each item may override it — the Pultec's five big boost knobs
/// and its three small pointer knobs are different parts — so the row is
/// whatever the item asked for, at the size it asked for.
fn comp_faces() -> Vec<Face> {
    comp_profiles::all_profiles()
        .iter()
        .filter_map(|p| {
            let d = comp_ui::faces::units::design_for(p.id())?;
            Some(Face {
                plugin: "Comp",
                name: p.name().to_string(),
                paint: d.paint.to_string(),
                ink: d.ink.to_string(),
                knobs: rack_knobs(d),
            })
        })
        .collect()
}

#[component]
fn Row(face: ReadSignal<Face>) -> Element {
    let f = face.read();
    let h = row_h(&f);
    let cell = cell_h(&f);
    rsx! {
        div {
            style: "display:flex; align-items:stretch; border-radius:6px; \
                    overflow:hidden; margin-bottom:8px; height:{h}px;",
            div {
                style: "width:148px; flex:none; padding:8px 12px; background:#15171c; \
                        color:#dfe3e8; font-size:11px; font-weight:700; \
                        letter-spacing:0.03em; display:flex; flex-direction:column; \
                        justify-content:center; gap:3px;",
                div { style: "opacity:0.5; font-size:9px; font-weight:600;", "{f.plugin}" }
                div { "{f.name}" }
            }
            // The panel itself, so the knob is judged against the colour it
            // will actually sit on rather than a neutral grey.
            div {
                style: "flex:1; background:{f.paint}; padding:0 14px; \
                        display:grid; grid-template-columns:repeat({COLS}, 1fr); \
                        align-content:center;",
                for (i , k) in f.knobs.iter().enumerate() {
                    div {
                        key: "{i}",
                        style: "height:{cell}px; display:flex; flex-direction:column; \
                                align-items:center; justify-content:flex-end; gap:1px;",
                        HardwareKnob {
                            handle: ParamHandle::inert(k.legend.to_string(), position(i)),
                            testid: format!("{}-{}-{}", f.plugin, f.name, k.param)
                                .to_lowercase()
                                .replace([' ', '/', '.', '(', ')', '\u{b7}'], "-"),
                            scale: SCALE,
                            diameter: k.diameter,
                            style: k.style,
                            ink: f.ink.clone(),
                            tint: k.tint.clone(),
                            marks: scale_ring(5, 1, linear_scale_label(0.0, 10.0)),
                            inner_handle: k.inner.then(|| {
                                ParamHandle::inert(k.legend.to_string(), 1.0 - position(i))
                            }),
                        }
                        div {
                            style: "color:{f.ink}; font-size:8px; font-weight:700; \
                                    letter-spacing:0.04em; opacity:0.9; line-height:1;",
                            "{k.legend.to_uppercase()}"
                        }
                        div {
                            style: "color:{f.ink}; font-size:7px; opacity:0.42; line-height:1.4;",
                            { if k.inner { format!("{:?} \u{b7} {:.0} \u{b7} dual", k.style, k.diameter) } else { format!("{:?} \u{b7} {:.0}", k.style, k.diameter) } }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn Sheet() -> Element {
    let faces: Vec<Face> = use_context();
    rsx! {
        style {
            "html, body {{ margin:0; padding:0; background:#0e1014; \
             font-family: ui-sans-serif, system-ui, sans-serif; }}"
        }
        DragProvider {
            div {
                style: "padding:14px;",
                for (i , f) in faces.into_iter().enumerate() {
                    Row { key: "{i}", face: f }
                }
            }
        }
    }
}

fn shots_dir() -> PathBuf {
    let dir = std::env::var("FTS_SHOTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../target/gui-shots/plugin-knobs")
        });
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("create {}: {e}", dir.display()));
    dir
}

/// How many knobs one page may carry.
///
/// Not a taste call. Vello rasterizes into a bump-allocated tile buffer, and
/// a scene past its capacity does not fail — it *garbles*, drawing a knob's
/// tiles over its neighbours, which reads as a rendering bug in the knob art
/// rather than a sheet that was asked for too much at once. The whole product
/// on one page (≈450 knobs, 6300 px tall) reliably tips over; a page of this
/// size does not.
const KNOBS_PER_PAGE: usize = 36;

/// Split a plugin's faces into pages, never breaking a face across two.
fn paginate(faces: Vec<Face>) -> Vec<Vec<Face>> {
    let mut pages: Vec<Vec<Face>> = Vec::new();
    let mut page: Vec<Face> = Vec::new();
    let mut budget = 0usize;
    for f in faces {
        if !page.is_empty() && budget + f.knobs.len() > KNOBS_PER_PAGE {
            pages.push(std::mem::take(&mut page));
            budget = 0;
        }
        budget += f.knobs.len();
        page.push(f);
    }
    if !page.is_empty() {
        pages.push(page);
    }
    pages
}

async fn shoot(name: &str, faces: Vec<Face>) -> usize {
    assert!(!faces.is_empty(), "{name}: no faces — the design table went missing");
    let knobs: usize = faces.iter().map(|f| f.knobs.len()).sum();
    let pages = paginate(faces);
    let many = pages.len() > 1;

    for (i, page) in pages.into_iter().enumerate() {
        // Sized from the rows themselves: a face whose knobs wrap onto a
        // second line is taller, and guessing one height for all of them is
        // how the bottom of the sheet goes missing.
        let height = 28.0 + page.iter().map(|f| row_h(f) + 8.0).sum::<f64>();
        let tester = render(Sheet)
            .with_root_context(page)
            .with_window_size(1180, height.ceil() as u32)
            .build();
        let _ = tester.pump().await;
        tester.relayout();

        let file = if many {
            format!("{name}-{}.png", i + 1)
        } else {
            format!("{name}.png")
        };
        let path = shots_dir().join(&file);
        tester.render_png(&path);
        println!("{}", path.display());
    }
    knobs
}

#[tokio::test]
async fn shot_every_knob_on_the_panel_it_sits_on() {
    let mut total = 0;
    total += shoot("saturate", saturate_faces()).await;
    total += shoot("eq", eq_faces()).await;
    total += shoot("comp", comp_faces()).await;
    total += shoot("delay", delay_faces()).await;
    total += shoot("reverb", reverb_faces()).await;
    println!("{total} knobs across every face in the product");
    assert!(total > 100, "only {total} knobs — a plugin stopped reporting its faces");
}
