//! The seven panels.
//!
//! A delay's family is not a preset — it is a different machine, and the
//! panels say so. A tape has reels and a head stack; an analog delay is a
//! chip and its clock; a rhythmic one is a grid of taps. Same controls
//! underneath, because that is what the DSP has, but you should know which
//! delay you are looking at before you read a single word.
//!
//! Each panel's centrepiece is *live*: it is drawn from feedback, time and
//! tone, so turning a knob changes the picture of the repeats rather than
//! just a number. Feedback is the one you can see best — the repeats it draws
//! are the repeats you will hear.

use dioxus::prelude::*;
use fts_ui_audio::hardware::knob::{HardwareKnob, KnobStyle};
use fts_ui_audio::hardware::panel::{Panel, PanelEnds, PanelSlot, PanelTexture, Silkscreen};
use fts_ui_audio::ParamHandle;

/// Panel drawing size — 2U, like the compressor's faces.
pub const W: f64 = 960.0;
pub const H: f64 = 300.0;


/// What a panel draws in the middle: the thing that makes it that delay.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Centrepiece {
    /// Exact repeats on a ruled line — a digital delay is arithmetic.
    Ticks,
    /// Two reels and the head stack between them.
    Reels,
    /// The chip and its clock: a DIP package with the bucket line running
    /// through it.
    Chip,
    /// Repeats climbing (or falling) an interval at a time.
    Staircase,
    /// The tap grid — where in the bar each repeat lands.
    Grid,
    /// A repeat that is no longer one: turned around, smeared, dissolved.
    Smear,
}

/// One control on a panel.
#[derive(Clone, Copy)]
pub struct KnobSpec {
    pub param: &'static str,
    pub legend: &'static str,
    pub x: f64,
    pub y: f64,
    pub d: f64,
    pub style: KnobStyle,
}

/// A family's panel.
#[derive(Clone, Copy)]
pub struct EchoDesign {
    pub family: &'static str,
    pub paint: &'static str,
    pub ink: &'static str,
    pub dim_ink: &'static str,
    pub chrome: &'static str,
    pub accent: &'static str,
    pub ends: PanelEnds,
    pub texture: PanelTexture,
    pub centre: Centrepiece,
    pub knobs: &'static [KnobSpec],
}

/// The control row. Eight across, on the line that leaves room for the
/// biggest knob's box *and* its legend above the bottom edge.
const fn knob(param: &'static str, legend: &'static str, x: f64, d: f64, style: KnobStyle) -> KnobSpec {
    KnobSpec { param, legend, x, y: 206.0, d, style }
}

/// Time, feedback and mix are on every delay ever made, so they are in the
/// same place on every panel: the two big ones on the left, mix on the right.
/// What sits between them is what the family is about.
const DIGITAL_KNOBS: &[KnobSpec] = &[
    knob("time_l", "Time L", 118.0, 58.0, KnobStyle::Collet),
    knob("time_r", "Time R", 230.0, 58.0, KnobStyle::Collet),
    knob("feedback", "Feedback", 340.0, 58.0, KnobStyle::Collet),
    knob("tone", "Tone", 450.0, 44.0, KnobStyle::Collet),
    knob("character_a", "Filter", 560.0, 44.0, KnobStyle::Collet),
    knob("character_b", "Resonance", 670.0, 44.0, KnobStyle::Collet),
    knob("duck", "Duck", 780.0, 44.0, KnobStyle::Collet),
    knob("mix", "Mix", 872.0, 52.0, KnobStyle::Collet),
];

const TAPE_KNOBS: &[KnobSpec] = &[
    knob("time_l", "Time", 118.0, 62.0, KnobStyle::Bakelite),
    knob("feedback", "Repeats", 230.0, 62.0, KnobStyle::Bakelite),
    knob("drive", "Drive", 340.0, 48.0, KnobStyle::Bakelite),
    knob("wow", "Wow", 450.0, 44.0, KnobStyle::Bakelite),
    knob("flutter", "Flutter", 560.0, 44.0, KnobStyle::Bakelite),
    knob("tone", "Tone", 670.0, 44.0, KnobStyle::Bakelite),
    knob("character_a", "Heads", 780.0, 44.0, KnobStyle::Bakelite),
    knob("mix", "Mix", 872.0, 52.0, KnobStyle::Bakelite),
];

const ANALOG_KNOBS: &[KnobSpec] = &[
    knob("time_l", "Time", 118.0, 62.0, KnobStyle::Skirted),
    knob("feedback", "Repeats", 230.0, 62.0, KnobStyle::Skirted),
    knob("tone", "Tone", 340.0, 48.0, KnobStyle::Skirted),
    // The bucket brigade's own two: how far the clock has drifted and how
    // much of the top has fallen off the end of the chain.
    knob("character_a", "Clock", 450.0, 44.0, KnobStyle::Skirted),
    knob("character_b", "Degrade", 560.0, 44.0, KnobStyle::Skirted),
    knob("wow", "Warble", 670.0, 44.0, KnobStyle::Skirted),
    knob("duck", "Duck", 780.0, 44.0, KnobStyle::Skirted),
    knob("mix", "Mix", 872.0, 52.0, KnobStyle::Skirted),
];

const PITCH_KNOBS: &[KnobSpec] = &[
    knob("time_l", "Time", 118.0, 58.0, KnobStyle::SilverTop),
    knob("feedback", "Feedback", 230.0, 58.0, KnobStyle::SilverTop),
    // The interval is the effect. Everything else is a delay.
    knob("character_a", "Interval", 340.0, 58.0, KnobStyle::SilverTop),
    knob("character_b", "Spread", 450.0, 44.0, KnobStyle::SilverTop),
    knob("tone", "Tone", 560.0, 44.0, KnobStyle::SilverTop),
    knob("drive", "Drive", 670.0, 44.0, KnobStyle::SilverTop),
    knob("duck", "Duck", 780.0, 44.0, KnobStyle::SilverTop),
    knob("mix", "Mix", 872.0, 52.0, KnobStyle::SilverTop),
];

const RHYTHMIC_KNOBS: &[KnobSpec] = &[
    knob("time_l", "Time", 118.0, 58.0, KnobStyle::Metal),
    knob("time_r", "Offset", 230.0, 58.0, KnobStyle::Metal),
    knob("feedback", "Feedback", 340.0, 58.0, KnobStyle::Metal),
    knob("character_a", "Pattern", 450.0, 44.0, KnobStyle::Metal),
    knob("character_b", "Spread", 560.0, 44.0, KnobStyle::Metal),
    knob("tone", "Tone", 670.0, 44.0, KnobStyle::Metal),
    knob("duck", "Duck", 780.0, 44.0, KnobStyle::Metal),
    knob("mix", "Mix", 872.0, 52.0, KnobStyle::Metal),
];

const SPECIAL_KNOBS: &[KnobSpec] = &[
    knob("time_l", "Time", 118.0, 58.0, KnobStyle::MetalFluted),
    knob("feedback", "Feedback", 230.0, 58.0, KnobStyle::MetalFluted),
    knob("character_a", "Shape", 340.0, 58.0, KnobStyle::MetalFluted),
    knob("character_b", "Smear", 450.0, 44.0, KnobStyle::MetalFluted),
    knob("tone", "Tone", 560.0, 44.0, KnobStyle::MetalFluted),
    knob("drive", "Drive", 670.0, 44.0, KnobStyle::MetalFluted),
    knob("duck", "Duck", 780.0, 44.0, KnobStyle::MetalFluted),
    knob("mix", "Mix", 872.0, 52.0, KnobStyle::MetalFluted),
];

/// Digital: a modern box. Nothing on the panel pretends to be old.
pub static DIGITAL: EchoDesign = EchoDesign {
    family: "digital",
    paint: "linear-gradient(178deg, #2a2e35 0%, #1e2229 50%, #14181e 100%)",
    ink: "#e6ecf3",
    dim_ink: "#8d99a7",
    chrome: "#8f98a2",
    accent: "#48c9b0",
    ends: PanelEnds::RackEars,
    texture: PanelTexture::Brushed { strength: 35 },
    centre: Centrepiece::Ticks,
    knobs: DIGITAL_KNOBS,
};

/// Tape: a machine with reels on it, in cream and oxide brown.
pub static TAPE: EchoDesign = EchoDesign {
    family: "tape",
    paint: "linear-gradient(178deg, #e4d9c2 0%, #d6cab1 48%, #c0b399 100%)",
    ink: "#2f2417",
    dim_ink: "#7b6b53",
    chrome: "#b3a68c",
    accent: "#a8642a",
    ends: PanelEnds::Wood,
    texture: PanelTexture::Painted,
    centre: Centrepiece::Reels,
    knobs: TAPE_KNOBS,
};

/// Analog: a pedal-shop green with the chip drawn on it.
pub static ANALOG: EchoDesign = EchoDesign {
    family: "analog",
    paint: "linear-gradient(178deg, #24463a 0%, #1b362d 50%, #122720 100%)",
    ink: "#e8e2cd",
    dim_ink: "#93a596",
    chrome: "#a8b0a4",
    accent: "#e2b33f",
    ends: PanelEnds::RackEars,
    texture: PanelTexture::Painted,
    centre: Centrepiece::Chip,
    knobs: ANALOG_KNOBS,
};

/// Pitch: cold and electronic, because the effect is not natural.
pub static PITCH: EchoDesign = EchoDesign {
    family: "pitch",
    paint: "linear-gradient(178deg, #262133 0%, #1c1828 50%, #12101c 100%)",
    ink: "#ece7fa",
    dim_ink: "#9d95bd",
    chrome: "#8f89ab",
    accent: "#7aa2f7",
    ends: PanelEnds::RackEars,
    texture: PanelTexture::Brushed { strength: 30 },
    centre: Centrepiece::Staircase,
    knobs: PITCH_KNOBS,
};

/// Rhythmic: a grid, on a panel that looks like a sequencer.
pub static RHYTHMIC: EchoDesign = EchoDesign {
    family: "rhythmic",
    paint: "linear-gradient(178deg, #3d4046 0%, #2f3238 48%, #23262b 100%)",
    ink: "#eef1f5",
    dim_ink: "#9aa3ae",
    chrome: "#848c95",
    accent: "#f0803c",
    ends: PanelEnds::RackEars,
    texture: PanelTexture::Painted,
    centre: Centrepiece::Grid,
    knobs: RHYTHMIC_KNOBS,
};

/// Special: black, for the ones that do not give your signal back.
pub static SPECIAL: EchoDesign = EchoDesign {
    family: "special",
    paint: "linear-gradient(178deg, #1b1b1e 0%, #141416 48%, #0b0b0d 100%)",
    ink: "#f1ece4",
    dim_ink: "#8b867e",
    chrome: "#99948c",
    accent: "#e0538a",
    ends: PanelEnds::RackEars,
    texture: PanelTexture::Brushed { strength: 30 },
    centre: Centrepiece::Smear,
    knobs: SPECIAL_KNOBS,
};

/// The panel a profile is drawn on — per family, with the profile's name
/// silkscreened and its accent shifted so the variants are still distinct.
pub fn design_for(profile_id: &str) -> &'static EchoDesign {
    match delay_profiles::category_of(profile_id).map(|(c, _)| delay_profiles::CATEGORIES[c].id) {
        Some("digital") => &DIGITAL,
        Some("tape") => &TAPE,
        Some("analog") => &ANALOG,
        Some("pitch") => &PITCH,
        Some("rhythmic") => &RHYTHMIC,
        _ => &SPECIAL,
    }
}

/// How lit the centrepiece is for a variant inside its family.
pub fn variant_lift(profile_id: &str) -> f64 {
    match delay_profiles::category_of(profile_id) {
        Some((_, index)) => 1.0 + index as f64 * 0.22,
        None => 1.0,
    }
}

/// What this profile's engine does with `character_a` / `character_b`.
///
/// The pair reaches every engine and each reads it differently. A knob called
/// "Character A" tells you nothing, so the panel prints what it does here.
pub fn character_legends(profile_id: &str) -> (&'static str, &'static str) {
    match profile_id {
        "digital" => ("Width", "Sync"),
        "filter" => ("Filter", "Resonance"),
        "tape" => ("Heads", "Age"),
        "oilcan" => ("Smear", "Chorus"),
        "bbd" => ("Clock", "Degrade"),
        "lofi" => ("Bits", "Crush"),
        "pitch" => ("Interval", "Spread"),
        "shimmer" => ("Octave", "Regen"),
        "multitap" => ("Taps", "Spread"),
        "rhythm" => ("Division", "Swing"),
        "drum" => ("Groove", "Accent"),
        "reverse" => ("Window", "Overlap"),
        "spectral" => ("Tilt", "Bands"),
        "reverb_delay" => ("Diffusion", "Size"),
        _ => ("Character", "Colour"),
    }
}

/// A drawn delay: the panel, its centrepiece, and its row of controls.
#[component]
pub fn EchoFace(
    profile_id: String,
    handles: std::collections::HashMap<String, ParamHandle>,
    /// The shell's redraw tick. Not read; its job is to change, so the panel
    /// re-renders against fresh parameter values instead of being memoized.
    frame: u64,
) -> Element {
    let _ = frame;
    let design = design_for(&profile_id);
    let profile =
        delay_profiles::profile_by_id(&profile_id).unwrap_or(&delay_profiles::PROFILES[0]);
    let scale = fts_ui_audio::hardware::panel::panel_scale(W, H, crate::control_view::RAIL_W);

    let value = |name: &str| {
        handles
            .get(name)
            .map(|h| h.normalized() as f64)
            .unwrap_or(0.5)
    };
    let (feedback, time, tone) = (value("feedback"), value("time_l"), value("tone"));

    rsx! {
        Panel {
            design_w: W,
            design_h: H,
            scale,
            background: design.paint.to_string(),
            chrome: design.chrome.to_string(),
            ends: design.ends,
            texture: design.texture,

            PanelSlot { scale, x: W / 2.0, y: 104.0, w: 620.0, h: 150.0,
                CentreView {
                    kind: design.centre,
                    accent: design.accent.to_string(),
                    ink: design.dim_ink.to_string(),
                    feedback,
                    time,
                    tone,
                    lift: variant_lift(&profile_id),
                }
            }

            Silkscreen {
                scale, x: 150.0, y: 40.0, width: 280.0,
                text: profile.name.to_string(), size: 15.0,
                color: design.ink.to_string(), weight: 800,
            }
            Silkscreen {
                scale, x: 150.0, y: 60.0, width: 280.0,
                text: delay_profiles::CATEGORIES
                    .iter()
                    .find(|c| c.profiles.contains(&profile.id))
                    .map(|c| c.label)
                    .unwrap_or("Delay")
                    .to_string(),
                size: 8.0, color: design.dim_ink.to_string(),
            }

            for (index , spec) in design.knobs.iter().copied().enumerate() {
                div {
                    key: "{design.family}-{index}",
                    if let Some(handle) = handles.get(spec.param) {
                        PanelSlot { scale, x: spec.x, y: spec.y, w: spec.d * 2.0, h: spec.d * 2.0,
                            HardwareKnob {
                                handle: handle.clone(),
                                testid: spec.param.replace('_', "-"),
                                scale,
                                diameter: spec.d,
                                style: spec.style,
                                ink: design.ink.to_string(),
                            }
                        }
                    }
                    Silkscreen {
                        scale, x: spec.x, y: spec.y + spec.d * 0.92 + 10.0, width: 120.0,
                        text: match spec.param {
                            "character_a" => character_legends(&profile_id).0.to_string(),
                            "character_b" => character_legends(&profile_id).1.to_string(),
                            _ => spec.legend.to_string(),
                        },
                        size: 9.0,
                        color: design.ink.to_string(),
                    }
                }
            }
        }
    }
}

/// The live picture of the repeats.
///
/// Drawn from feedback, time and tone. Feedback is the one that shows: the
/// number of repeats it draws is the number you will hear, so the panel tells
/// you what a setting does before you play a note through it.
#[component]
fn CentreView(
    kind: Centrepiece,
    accent: String,
    ink: String,
    feedback: f64,
    time: f64,
    tone: f64,
    lift: f64,
) -> Element {
    let (w, h) = (620.0, 150.0);
    let body = accent.clone();
    let glow = (0.30 + feedback * 0.55).min(0.95) * lift.min(1.5);
    // How many repeats survive. Feedback near the top runs away, which is a
    // real setting on most of these and worth showing as "more than fits".
    let repeats = ((feedback * 11.0).round() as usize).clamp(1, 11);
    // Where they land: a long time setting spaces them out.
    let spacing = 26.0 + time * 46.0;

    let inner = match kind {
        // Exact repeats on a ruled line — a digital delay is arithmetic, and
        // the picture should be as plain as the sound.
        Centrepiece::Ticks => rsx! {
            line { x1: "40", y1: "{h / 2.0:.1}", x2: "{w - 40.0:.1}", y2: "{h / 2.0:.1}",
                stroke: "{ink}", stroke_width: "1", opacity: "0.45" }
            for i in 0..repeats {
                rect {
                    key: "{i}",
                    x: "{50.0 + spacing * i as f64:.1}",
                    y: "{h / 2.0 - (46.0 * (1.0 - i as f64 / (repeats as f64 + 1.0))):.1}",
                    width: "3",
                    height: "{92.0 * (1.0 - i as f64 / (repeats as f64 + 1.0)):.1}",
                    fill: "{body}",
                    opacity: "{glow * (1.0 - i as f64 / (repeats as f64 + 2.0)):.3}",
                }
            }
        },

        // Two reels and the head stack between them. The reels are drawn at
        // the sizes a tape machine's are: supply full, take-up filling.
        Centrepiece::Reels => {
            let (cy, r) = (h / 2.0, 46.0);
            rsx! {
                for (i , cx) in [120.0_f64, w - 120.0].into_iter().enumerate() {
                    circle { key: "{i}", cx: "{cx:.1}", cy: "{cy:.1}", r: "{r:.1}",
                        fill: "none", stroke: "{ink}", stroke_width: "1.6" }
                }
                for (i , cx) in [120.0_f64, w - 120.0].into_iter().enumerate() {
                    circle { key: "h{i}", cx: "{cx:.1}", cy: "{cy:.1}",
                        r: "{(r * (0.35 + time * 0.4)):.1}",
                        fill: "{body}", opacity: "{glow * 0.35:.3}",
                        stroke: "{body}", stroke_width: "1.2" }
                }
                // The tape path across the heads.
                line { x1: "{120.0 + r:.1}", y1: "{cy - r + 8.0:.1}", x2: "{w - 120.0 - r:.1}", y2: "{cy - r + 8.0:.1}",
                    stroke: "{ink}", stroke_width: "1.4" }
                // The head stack: one per repeat tap the machine offers.
                for i in 0..4 {
                    rect {
                        key: "s{i}",
                        x: "{w / 2.0 - 54.0 + i as f64 * 30.0:.1}", y: "{cy - r + 2.0:.1}",
                        width: "10", height: "22", rx: "2",
                        fill: if (i as f64) < 1.0 + feedback * 3.0 { "{body}" } else { "{ink}" },
                        opacity: if (i as f64) < 1.0 + feedback * 3.0 { "{glow:.3}" } else { "0.30" },
                    }
                }
            }
        },

        // The chip and its bucket line: charge handed down the chain, one
        // stage at a time, losing a little at every step.
        Centrepiece::Chip => {
            let (x0, y0, cw, ch) = (w / 2.0 - 140.0, h / 2.0 - 34.0, 280.0, 68.0);
            rsx! {
                rect { x: "{x0:.1}", y: "{y0:.1}", width: "{cw:.1}", height: "{ch:.1}", rx: "4",
                    fill: "rgba(0,0,0,0.35)", stroke: "{ink}", stroke_width: "1.4" }
                // Pins.
                for i in 0..8 {
                    rect { key: "p{i}", x: "{x0 + 18.0 + i as f64 * 34.0:.1}", y: "{y0 - 10.0:.1}",
                        width: "8", height: "10", fill: "{ink}", opacity: "0.55" }
                }
                for i in 0..8 {
                    rect { key: "q{i}", x: "{x0 + 18.0 + i as f64 * 34.0:.1}", y: "{y0 + ch:.1}",
                        width: "8", height: "10", fill: "{ink}", opacity: "0.55" }
                }
                // The buckets, dimming down the chain — this is the sound.
                for i in 0..repeats {
                    circle {
                        key: "b{i}",
                        cx: "{x0 + 22.0 + (cw - 44.0) * i as f64 / (repeats.max(2) - 1) as f64:.1}",
                        cy: "{h / 2.0:.1}",
                        r: "{(7.0 - i as f64 * 0.35).max(2.5):.1}",
                        fill: "{body}",
                        opacity: "{(glow * (1.0 - i as f64 / (repeats as f64 + 1.0)) * (0.4 + tone * 0.6)):.3}",
                    }
                }
            }
        },

        // Repeats climbing an interval at a time. The staircase is the effect
        // drawn literally, which is the clearest it can be put.
        Centrepiece::Staircase => rsx! {
            line { x1: "40", y1: "{h - 24.0:.1}", x2: "{w - 40.0:.1}", y2: "{h - 24.0:.1}",
                stroke: "{ink}", stroke_width: "1", opacity: "0.4" }
            for i in 0..repeats {
                rect {
                    key: "{i}",
                    x: "{54.0 + spacing * i as f64:.1}",
                    y: "{h - 34.0 - (i as f64 + 1.0) * (86.0 / (repeats as f64 + 1.0)):.1}",
                    width: "{(spacing * 0.55).min(26.0):.1}",
                    height: "10", rx: "2",
                    fill: "{body}",
                    opacity: "{glow * (1.0 - i as f64 / (repeats as f64 + 2.0)):.3}",
                }
            }
        },

        // The bar, and where in it each tap lands.
        Centrepiece::Grid => {
            let steps = 16;
            let (x0, span) = (54.0, w - 108.0);
            let step = span / steps as f64;
            rsx! {
                for i in 0..=steps {
                    line {
                        key: "g{i}",
                        x1: "{x0 + step * i as f64:.1}", y1: "{h / 2.0 - 40.0:.1}",
                        x2: "{x0 + step * i as f64:.1}", y2: "{h / 2.0 + 40.0:.1}",
                        stroke: "{ink}",
                        stroke_width: if i % 4 == 0 { "1.4" } else { "0.7" },
                        opacity: if i % 4 == 0 { "0.55" } else { "0.25" },
                    }
                }
                // Taps: spaced by the time setting, lit by feedback.
                for i in 0..repeats {
                    rect {
                        key: "t{i}",
                        x: "{x0 + step * (((i as f64 + 1.0) * (1.0 + time * 3.0)).round() % steps as f64):.1}",
                        y: "{h / 2.0 - 34.0:.1}",
                        width: "{step * 0.7:.1}", height: "68", rx: "2",
                        fill: "{body}",
                        opacity: "{glow * (1.0 - i as f64 / (repeats as f64 + 2.0)):.3}",
                    }
                }
            }
        },

        // A repeat that stopped being one: the envelope runs backwards and
        // smears out as it goes.
        Centrepiece::Smear => {
            let bars = 72;
            rsx! {
                line { x1: "40", y1: "{h / 2.0:.1}", x2: "{w - 40.0:.1}", y2: "{h / 2.0:.1}",
                    stroke: "{ink}", stroke_width: "0.8", opacity: "0.4" }
                for i in 0..bars {
                    {
                        let t = i as f64 / bars as f64;
                        // Backwards: quiet first, loudest at the end.
                        let env = t.powf(1.6 + (1.0 - feedback) * 1.5);
                        let jitter = ((i as f64 * 7.3319).sin() * 21_337.0).fract().abs();
                        let a = env * (0.5 + jitter * 0.5) * (0.5 + tone * 0.5);
                        rsx! {
                            line {
                                key: "{i}",
                                x1: "{54.0 + (w - 108.0) * t:.1}", y1: "{h / 2.0 - a * 56.0:.1}",
                                x2: "{54.0 + (w - 108.0) * t:.1}", y2: "{h / 2.0 + a * 56.0:.1}",
                                stroke: "{body}", stroke_width: "{(w - 108.0) / bars as f64 * 0.72:.2}",
                                opacity: "{(0.30 + glow * 0.6):.3}",
                            }
                        }
                    }
                }
            }
        }
    };

    rsx! {
        svg {
            view_box: "0 0 {w} {h}",
            style: "width:100%; height:100%; display:block;",
            {inner}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_placed_under_a_rack_ear() {
        const EAR: f64 = 26.0;
        for design in [&DIGITAL, &TAPE, &ANALOG, &PITCH, &RHYTHMIC, &SPECIAL] {
            for spec in design.knobs {
                let half = spec.d * (110.0 / 60.0) / 2.0;
                assert!(
                    spec.x - half >= EAR && spec.x + half <= W - EAR,
                    "{}'s {} at x={} (half-box {half:.1}) runs under an ear",
                    design.family,
                    spec.param,
                    spec.x,
                );
            }
        }
    }

    #[test]
    fn every_legend_fits_on_the_panel() {
        for design in [&DIGITAL, &TAPE, &ANALOG, &PITCH, &RHYTHMIC, &SPECIAL] {
            for spec in design.knobs {
                let legend_y = spec.y + spec.d * 0.92 + 10.0;
                assert!(legend_y + 6.0 <= H, "{}'s {} legend falls off", design.family, spec.param);
            }
        }
    }

    #[test]
    fn every_placed_control_is_one_the_editor_binds() {
        const BOUND: &[&str] = &[
            "time_l", "time_r", "feedback", "tone", "drive", "wow", "flutter",
            "duck", "mix", "character_a", "character_b",
        ];
        for design in [&DIGITAL, &TAPE, &ANALOG, &PITCH, &RHYTHMIC, &SPECIAL] {
            for spec in design.knobs {
                assert!(
                    BOUND.contains(&spec.param),
                    "{} places {:?}, which the editor does not bind",
                    design.family,
                    spec.param,
                );
            }
        }
    }

    /// Time, feedback and mix are on every delay ever built, so they are on
    /// every panel — a family that hides one is a panel you cannot use.
    #[test]
    fn every_panel_has_time_feedback_and_mix() {
        for design in [&DIGITAL, &TAPE, &ANALOG, &PITCH, &RHYTHMIC, &SPECIAL] {
            for required in ["time_l", "feedback", "mix"] {
                assert!(
                    design.knobs.iter().any(|k| k.param == required),
                    "{} has no {required}",
                    design.family,
                );
            }
        }
    }

    #[test]
    fn every_profile_names_its_engine_controls() {
        for profile in delay_profiles::PROFILES {
            let (a, b) = character_legends(profile.id);
            assert_ne!((a, b), ("Character", "Colour"), "{} has placeholders", profile.id);
        }
    }

    #[test]
    fn every_family_has_a_panel_and_every_panel_a_family() {
        for category in delay_profiles::CATEGORIES {
            for id in category.profiles {
                assert_eq!(design_for(id).family, category.id, "{id} draws the wrong panel");
            }
        }
    }
}
