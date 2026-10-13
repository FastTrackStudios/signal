//! The other half of a knob: what it says.
//!
//! [`plugin_knobs`](../plugin_knobs.rs) answers "what does the dial look
//! like". This answers the questions you only see once a value is under it —
//! does a four-character readout fit the same box as `-inf dB`, does a
//! two-word legend wrap or truncate, does a bipolar knob read as centred, is
//! a disabled control legible enough to still be read.
//!
//! ```sh
//! cargo test -p kit-sheet --test knob_readouts
//! ```
//!
//! Lands in `target/gui-shots/knob-readouts/` (override with `FTS_SHOTS_DIR`).
//! Nothing here asserts a look. What it asserts is that every scenario drew
//! its readout — a value that formats to nothing is a blank knob, and blank
//! is exactly what you cannot see in a screenshot you did not take.

use std::path::PathBuf;

use dioxus::prelude::*;
use dioxus_test::{by_testid, render};

use fts_audio_ui::controls::knob::{Knob, KnobSize};
use fts_audio_ui::drag::DragProvider;
use fts_audio_ui::hardware::knob::{HardwareKnob, KnobStyle};
use fts_audio_ui::hardware::knob_svg::{detent_ring, linear_scale_label, scale_ring, ScaleMark};
use fts_audio_ui::param::ParamHandle;

/// A parameter that shows what you tell it to.
///
/// [`ParamHandle::inert`] displays its own *name*, which is right for a
/// hardware knob (the panel is the readout) and useless here — the whole
/// point is the string under the dial.
fn shown(name: &str, position: f32, display: &str) -> ParamHandle {
    let name = name.to_string();
    let display = display.to_string();
    let position = position.clamp(0.0, 1.0);
    ParamHandle::new(
        move || position,
        || {},
        |_| {},
        || {},
        move || display.clone(),
        move || name.clone(),
        |_| None,
    )
}

/// A section heading, and the note saying what the row is for.
#[component]
fn Head(title: String, note: String) -> Element {
    rsx! {
        div {
            style: "margin:22px 0 8px; padding-bottom:6px; \
                    border-bottom:1px solid rgba(255,255,255,0.09);",
            div {
                style: "color:#e8ecf1; font-size:13px; font-weight:700; \
                        letter-spacing:0.02em;",
                "{title}"
            }
            div { style: "color:#8b93a1; font-size:10px; margin-top:2px;", "{note}" }
        }
    }
}

/// A labelled cell, so the picture says which case it is.
/// A labelled cell, so the picture says which case it is.
///
/// Fixed height, bottom-aligned: a strip mixing a 44 px dial with a 72 px one
/// otherwise steps up and down across the row, and the thing you are trying
/// to compare — where the readout sits under the dial — is the thing that
/// moves.
#[component]
fn Cell(caption: String, #[props(default = 104)] h: u32, children: Element) -> Element {
    rsx! {
        div {
            style: "display:flex; flex-direction:column; align-items:center; \
                    justify-content:flex-end; gap:6px; width:104px; height:{h}px;",
            {children}
            div {
                style: "color:#7d8593; font-size:8px; text-align:center; \
                        line-height:1.3; letter-spacing:0.02em;",
                "{caption}"
            }
        }
    }
}

#[component]
fn Strip(#[props(default)] label: Option<String>, children: Element) -> Element {
    rsx! {
        div {
            style: "display:flex; align-items:stretch; gap:8px; padding:10px; \
                    background:#191c22; border-radius:8px; margin-bottom:6px;",
            if let Some(label) = label {
                div {
                    style: "width:64px; flex:none; color:#9aa3b2; font-size:9px; \
                            font-weight:700; letter-spacing:0.05em; \
                            text-transform:uppercase; display:flex; \
                            align-items:center;",
                    "{label}"
                }
            }
            div {
                style: "flex:1; display:flex; flex-wrap:wrap; align-items:stretch; gap:8px;",
                {children}
            }
        }
    }
}

/// The value strings the product actually formats, including the two that
/// break a fixed-width readout: a silence that prints as `-inf` and a
/// frequency that has run out of room for its decimal.
const VALUES: &[(&str, f32, &str, &str)] = &[
    ("Gain", 0.62, "-6.0 dB", "decibels, signed"),
    ("Freq", 0.55, "1.20 kHz", "frequency, scaled unit"),
    ("Mix", 0.48, "48 %", "percent"),
    ("Time", 0.30, "120 ms", "milliseconds"),
    ("Ratio", 0.40, "4:1", "ratio, no space"),
    ("Sync", 0.35, "1/8 D", "note division"),
    ("Gain", 0.00, "-inf dB", "the floor"),
    ("Pitch", 0.70, "+7 st", "bipolar, signed"),
    ("Mode", 0.66, "Vintage", "a word, not a number"),
    ("Freq", 1.00, "22.05 kHz", "the widest string"),
];

/// Legends from one character to the longest a face prints, against the
/// readout's own `min-width:52px`.
const LEGENDS: &[&str] = &[
    "Q",
    "Mix",
    "Drive",
    "Pre-Delay",
    "Attack (ms)",
    "Low Frequency",
];

#[component]
fn Sheet() -> Element {
    rsx! {
        style {
            "html, body {{ margin:0; padding:0; background:#0e1014; \
             font-family: ui-sans-serif, system-ui, sans-serif; }}"
        }
        DragProvider {
            div {
                style: "padding:16px 18px 26px; --foreground:#dfe3e8; \
                        --muted-foreground:#8b93a1;",

                Head {
                    title: "Sizes and states",
                    note: "The same parameter at each size, through every state a knob has.",
                }
                for size in [KnobSize::Small, KnobSize::Medium, KnobSize::Large] {
                    Strip {
                        key: "s{size:?}",
                        label: format!("{size:?}"),
                        for (name , tag , pos , bipolar , modu , disabled , colour) in STATES {
                            Cell {
                                key: "{size:?}-{tag}",
                                caption: "{tag}",
                                h: 120,
                                Knob {
                                    handle: state_handle(name, *pos, *bipolar),
                                    size,
                                    mod_min: modu.map(|(lo, _)| lo),
                                    mod_max: modu.map(|(_, hi)| hi),
                                    disabled: *disabled,
                                    color: colour.map(str::to_string),
                                }
                            }
                        }
                    }
                }

                Head {
                    title: "Readouts",
                    note: "Every unit the plugins format, at one size, so the widths compare.",
                }
                Strip {
                    for (i , (name , pos , value , note)) in VALUES.iter().enumerate() {
                        Cell {
                            key: "v{i}",
                            caption: "{note}",
                            Knob {
                                handle: shown(name, *pos, value),
                                size: KnobSize::Medium,
                            }
                        }
                    }
                }

                Head {
                    title: "Labels",
                    note: "One character to the longest legend a face prints. The readout box is \
                           52 px wide; the legend under it is not, and runs past the dial.",
                }
                for size in [KnobSize::Small, KnobSize::Large] {
                    Strip {
                        key: "l{size:?}",
                        label: format!("{size:?}"),
                        for (i , legend) in LEGENDS.iter().enumerate() {
                            Cell {
                                key: "l{i}-{size:?}",
                                caption: if legend.len() == 1 { "1 char".to_string() } else { format!("{} chars", legend.len()) },
                                h: 120,
                                Knob {
                                    handle: shown(legend, 0.5, "0.0 dB"),
                                    size,
                                }
                            }
                        }
                    }
                }

                Head {
                    title: "Printed scales",
                    note: "The hardware idiom reads off the panel, so the ring IS the readout.",
                }
                Strip {
                    for (i , (caption , style , marks , ticks , dots)) in rings().into_iter().enumerate() {
                        Cell {
                            key: "r{i}",
                            caption: "{caption}",
                            HardwareKnob {
                                handle: ParamHandle::inert(caption.clone(), 0.62),
                                testid: format!("ring-{i}"),
                                scale: 0.72,
                                diameter: 58.0,
                                style,
                                ink: "#c9ced5".to_string(),
                                marks,
                                ticks,
                                dots,
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The states a knob has, in the order a reviewer wants to compare them.
type State = (
    &'static str,
    &'static str,
    f32,
    bool,
    Option<(f64, f64)>,
    bool,
    Option<&'static str>,
);
const STATES: &[State] = &[
    ("Drive", "plain", 0.62, false, None, false, None),
    ("Tilt", "bipolar centre", 0.5, true, None, false, None),
    ("Tilt", "bipolar up", 0.78, true, None, false, None),
    ("Cutoff", "modulated", 0.44, false, Some((0.30, 0.72)), false, None),
    ("Mix", "accent", 0.66, false, None, false, Some("#e0703a")),
    ("Sag", "disabled", 0.35, false, None, true, None),
];

fn state_handle(name: &str, pos: f32, bipolar: bool) -> ParamHandle {
    let value = if bipolar {
        format!("{:+.1} dB", (pos as f64 - 0.5) * 24.0)
    } else {
        format!("{:.0} %", pos as f64 * 100.0)
    };
    shown(name, pos, &value).with_bipolar(bipolar)
}

/// The ring variants the faces ask for: a plain 0–10 silkscreen, the 1176's
/// signed scale, a stepped legend, the 1073's dots, and numerals with no
/// ticks at all.
#[allow(clippy::type_complexity)]
fn rings() -> Vec<(String, KnobStyle, Vec<ScaleMark>, bool, usize)> {
    vec![
        (
            "0–10, 4 minors".to_string(),
            KnobStyle::Bakelite,
            scale_ring(5, 4, linear_scale_label(0.0, 10.0)),
            true,
            0,
        ),
        (
            "-48…+12 dB".to_string(),
            KnobStyle::SilverTop,
            scale_ring(6, 1, linear_scale_label(-48.0, 12.0)),
            true,
            0,
        ),
        (
            "stepped legend".to_string(),
            KnobStyle::Collet,
            detent_ring(&["0.1", "0.3", "1", "3", "10", "All"]),
            true,
            0,
        ),
        (
            "dots, no ticks".to_string(),
            KnobStyle::Neve,
            Vec::new(),
            false,
            11,
        ),
        (
            "numerals only".to_string(),
            KnobStyle::Pointer,
            scale_ring(5, 0, linear_scale_label(0.0, 10.0)),
            false,
            0,
        ),
        (
            "bare panel".to_string(),
            KnobStyle::Dial,
            Vec::new(),
            false,
            0,
        ),
    ]
}

fn shots_dir() -> PathBuf {
    let dir = std::env::var("FTS_SHOTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../../target/gui-shots/knob-readouts")
        });
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("create {}: {e}", dir.display()));
    dir
}

#[tokio::test]
async fn shot_every_readout_label_and_scale() {
    let tester = render(Sheet).with_window_size(1020, 1420).build();
    let _ = tester.pump().await;
    tester.relayout();

    // Every value string has to have made it onto the sheet. A readout that
    // formats to nothing draws a knob with a gap under it, and a gap is the
    // one thing a contact sheet cannot show you.
    for (name, _, _, _) in VALUES {
        let id = format!("knob-{name}-readout");
        tester
            .query(by_testid(&id))
            .immediately()
            .unwrap_or_else(|e| panic!("{name} drew no readout: {e:?}"));
    }

    let path = shots_dir().join("readouts.png");
    tester.render_png(&path);
    println!("knob readouts: {}", path.display());
}
