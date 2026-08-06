//! The seven panels.
//!
//! A saturator's family is not a preset — it is a different machine, and the
//! panels say so. A tape has reels and a head stack; an analog saturator is a
//! chip and its clock; a rhythmic one is a grid of taps. Same controls
//! underneath, because that is what the DSP has, but you should know which
//! saturator you are looking at before you read a single word.
//!
//! Each panel draws its own **transfer curve**, live, from drive and bias.
//! That is the honest picture of a saturator: the shape of the curve is the
//! sound, its asymmetry is where the even harmonics come from, and you can
//! watch a tube's curve lean as you move its bias in a way no number shows.

use dioxus::prelude::*;
use fts_ui_audio::hardware::knob::{HardwareKnob, KnobStyle};
use fts_ui_audio::hardware::panel::{Panel, PanelEnds, PanelSlot, PanelTexture, Silkscreen};
use fts_ui_audio::ParamHandle;

/// Panel drawing size — 2U, like the compressor's faces.
pub const W: f64 = 960.0;
pub const H: f64 = 300.0;



/// What a panel draws around its transfer curve — the circuit itself.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Circuit {
    /// A valve, lit, with the curve leaning the way its bias is set.
    Valve,
    /// Tape over a head, and the knee it goes into.
    Tape,
    /// A core with windings, blooming on transients.
    Core,
    /// A transistor's sharp corner, drawn as sharp as it sounds.
    Solid,
    /// Quantisation: a staircase instead of a curve.
    Steps,
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

/// A circuit's panel.
#[derive(Clone, Copy)]
pub struct SatDesign {
    pub family: &'static str,
    pub paint: &'static str,
    pub ink: &'static str,
    pub dim_ink: &'static str,
    pub chrome: &'static str,
    pub accent: &'static str,
    pub ends: PanelEnds,
    pub texture: PanelTexture,
    pub circuit: Circuit,
    pub knobs: &'static [KnobSpec],
}

const fn knob(param: &'static str, legend: &'static str, x: f64, d: f64, style: KnobStyle) -> KnobSpec {
    KnobSpec { param, legend, x, y: 206.0, d, style }
}

/// Drive, mix and output are on all five — a saturator without them is not
/// one. Bias is only offered where it does something: on a symmetric curve it
/// is a knob that lies.
const TUBE_KNOBS: &[KnobSpec] = &[
    knob("drive", "Drive", 130.0, 62.0, KnobStyle::Bakelite),
    knob("bias", "Bias", 262.0, 62.0, KnobStyle::Bakelite),
    knob("sag", "Sag", 390.0, 46.0, KnobStyle::Bakelite),
    knob("character_a", "Heat", 510.0, 46.0, KnobStyle::Bakelite),
    knob("tilt", "Tilt", 630.0, 46.0, KnobStyle::Bakelite),
    knob("mix", "Mix", 750.0, 46.0, KnobStyle::Bakelite),
    knob("output", "Output", 866.0, 54.0, KnobStyle::Bakelite),
];

const TAPE_KNOBS: &[KnobSpec] = &[
    knob("drive", "Level", 130.0, 62.0, KnobStyle::Skirted),
    knob("character_a", "Bias", 262.0, 62.0, KnobStyle::Skirted),
    knob("character_b", "Speed", 390.0, 46.0, KnobStyle::Skirted),
    knob("tilt", "Emphasis", 510.0, 46.0, KnobStyle::Skirted),
    knob("sag", "Compression", 630.0, 46.0, KnobStyle::Skirted),
    knob("mix", "Mix", 750.0, 46.0, KnobStyle::Skirted),
    knob("output", "Output", 866.0, 54.0, KnobStyle::Skirted),
];

const XFMR_KNOBS: &[KnobSpec] = &[
    knob("drive", "Drive", 130.0, 62.0, KnobStyle::Marconi),
    knob("character_a", "Core", 262.0, 62.0, KnobStyle::Marconi),
    knob("tilt", "Low Bloom", 390.0, 46.0, KnobStyle::Marconi),
    knob("character_b", "Hysteresis", 510.0, 46.0, KnobStyle::Marconi),
    knob("sag", "Sag", 630.0, 46.0, KnobStyle::Marconi),
    knob("mix", "Mix", 750.0, 46.0, KnobStyle::Marconi),
    knob("output", "Output", 866.0, 54.0, KnobStyle::Marconi),
];

const SS_KNOBS: &[KnobSpec] = &[
    knob("drive", "Gain", 130.0, 62.0, KnobStyle::SilverTop),
    knob("character_a", "Edge", 262.0, 62.0, KnobStyle::SilverTop),
    knob("character_b", "Gate", 390.0, 46.0, KnobStyle::SilverTop),
    knob("tilt", "Tone", 510.0, 46.0, KnobStyle::SilverTop),
    knob("sag", "Sag", 630.0, 46.0, KnobStyle::SilverTop),
    knob("mix", "Mix", 750.0, 46.0, KnobStyle::SilverTop),
    knob("output", "Output", 866.0, 54.0, KnobStyle::SilverTop),
];

// The two circuit knobs carry this family's real pair — Ceiling/Knee on the
// clipper, Bits/Rate on the crusher — so the shared legends here must not
// claim either of those names, or Bitcrush shows "Rate" twice.
const DIGITAL_KNOBS: &[KnobSpec] = &[
    knob("drive", "Drive", 130.0, 58.0, KnobStyle::Collet),
    knob("character_a", "Ceiling", 262.0, 58.0, KnobStyle::Collet),
    knob("character_b", "Bits", 390.0, 46.0, KnobStyle::Collet),
    knob("tilt", "Tone", 510.0, 46.0, KnobStyle::Collet),
    knob("sag", "Dither", 630.0, 46.0, KnobStyle::Collet),
    knob("mix", "Mix", 750.0, 46.0, KnobStyle::Collet),
    knob("output", "Output", 866.0, 54.0, KnobStyle::Collet),
];

/// Tube: warm cream and a lit valve.
pub static TUBE: SatDesign = SatDesign {
    family: "tube",
    paint: "linear-gradient(178deg, #e7dcc4 0%, #d9cdb2 48%, #c3b697 100%)",
    ink: "#33281a",
    dim_ink: "#7c6c53",
    chrome: "#b5a586",
    accent: "#e08a2e",
    ends: PanelEnds::Wood,
    texture: PanelTexture::Painted,
    circuit: Circuit::Valve,
    knobs: TUBE_KNOBS,
};

/// Tape: oxide brown on a machine's face.
pub static TAPE: SatDesign = SatDesign {
    family: "tape",
    paint: "linear-gradient(178deg, #4a423a 0%, #3a332d 48%, #2b2621 100%)",
    ink: "#efe6d4",
    dim_ink: "#a3968180",
    chrome: "#9c8f7c",
    accent: "#c98a45",
    ends: PanelEnds::RackEars,
    texture: PanelTexture::Brushed { strength: 35 },
    circuit: Circuit::Tape,
    knobs: TAPE_KNOBS,
};

/// Transformer: iron and copper.
pub static TRANSFORMER: SatDesign = SatDesign {
    family: "transformer",
    paint: "linear-gradient(178deg, #3a4048 0%, #2c313a 48%, #1f242b 100%)",
    ink: "#e8edf3",
    dim_ink: "#94a0ad",
    chrome: "#8a939d",
    accent: "#c0763f",
    ends: PanelEnds::RackEars,
    texture: PanelTexture::Brushed { strength: 55 },
    circuit: Circuit::Core,
    knobs: XFMR_KNOBS,
};

/// Transistor: modern, hard, unromantic.
pub static TRANSISTOR: SatDesign = SatDesign {
    family: "transistor",
    paint: "linear-gradient(178deg, #23262a 0%, #1a1d21 48%, #111417 100%)",
    ink: "#eaeef2",
    dim_ink: "#8b959f",
    chrome: "#8e969e",
    accent: "#4fd1a5",
    ends: PanelEnds::RackEars,
    texture: PanelTexture::Brushed { strength: 30 },
    circuit: Circuit::Solid,
    knobs: SS_KNOBS,
};

/// Digital: not saturation at all, and it should not pretend.
pub static DIGITAL: SatDesign = SatDesign {
    family: "digital",
    paint: "linear-gradient(178deg, #1c1f26 0%, #15181e 48%, #0d1015 100%)",
    ink: "#e9edf6",
    dim_ink: "#8a92a4",
    chrome: "#868d9c",
    accent: "#e0507a",
    ends: PanelEnds::RackEars,
    texture: PanelTexture::Painted,
    circuit: Circuit::Steps,
    knobs: DIGITAL_KNOBS,
};

pub fn design_for(profile_id: &str) -> &'static SatDesign {
    match saturate_profiles::category_of(profile_id).map(|(c, _)| saturate_profiles::CATEGORIES[c].id) {
        Some("tube") => &TUBE,
        Some("tape") => &TAPE,
        Some("transformer") => &TRANSFORMER,
        Some("transistor") => &TRANSISTOR,
        _ => &DIGITAL,
    }
}

pub fn variant_lift(profile_id: &str) -> f64 {
    match saturate_profiles::category_of(profile_id) {
        Some((_, index)) => 1.0 + index as f64 * 0.25,
        None => 1.0,
    }
}

/// What this profile's circuit does with `character_a` / `character_b`.
pub fn character_legends(profile_id: &str) -> (&'static str, &'static str) {
    match profile_id {
        "triode" => ("Heat", "Grid"),
        "pentode" => ("Heat", "Screen"),
        "tape" => ("Bias", "Speed"),
        "tape_hot" => ("Bias", "Print"),
        "transformer" => ("Core", "Hysteresis"),
        "transistor" => ("Edge", "Gate"),
        "fuzz" => ("Fuzz", "Starve"),
        "clip" => ("Ceiling", "Knee"),
        "crush" => ("Bits", "Rate"),
        _ => ("Character", "Colour"),
    }
}
/// A drawn saturator: the panel, its centrepiece, and its row of controls.
#[component]
pub fn SatFace(
    profile_id: String,
    handles: std::collections::HashMap<String, ParamHandle>,
    /// The shell's redraw tick. Not read; its job is to change, so the panel
    /// re-renders against fresh parameter values instead of being memoized.
    frame: u64,
) -> Element {
    let _ = frame;
    let design = design_for(&profile_id);
    let profile =
        saturate_profiles::profile_by_id(&profile_id).unwrap_or(&saturate_profiles::PROFILES[0]);
    let scale = fts_ui_audio::hardware::panel::panel_scale(W, H, crate::control_view::RAIL_W);

    let value = |name: &str, fallback: f32| {
        handles
            .get(name)
            .map(|h| h.normalized())
            .unwrap_or(fallback)
    };
    // The curve is drawn from what actually shapes it — the same struct the
    // plugin hands to `saturate_profiles::apply` on the audio thread.
    let defaults = saturate_profiles::Controls::default();
    let controls = saturate_profiles::Controls {
        drive: value("drive", defaults.drive),
        bias: value("bias", defaults.bias),
        sag: value("sag", defaults.sag),
        tilt: value("tilt", defaults.tilt),
        character_a: value("character_a", defaults.character_a),
        character_b: value("character_b", defaults.character_b),
        mix: value("mix", defaults.mix),
    };

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
                CurveView {
                    circuit: design.circuit,
                    accent: design.accent.to_string(),
                    ink: design.dim_ink.to_string(),
                    profile_id: profile_id.clone(),
                    controls,
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
                text: saturate_profiles::CATEGORIES
                    .iter()
                    .find(|c| c.profiles.contains(&profile.id))
                    .map(|c| c.label)
                    .unwrap_or("Saturate")
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


/// The transfer curve, live, with the circuit drawn around it.
///
/// This is the honest picture of a saturator, and it is honest because it is
/// **the engine's own curve** — [`saturate_profiles::apply`] voices a stage
/// exactly as the audio thread does, and this samples
/// [`ClassAPreamp::transfer`] through it. A panel that drew a stylised
/// impression of what a tube does would be a nicer drawing and a worse
/// instrument: the whole reason to put a curve on a saturator is that its
/// shape is the sound and its **asymmetry** is where even harmonics come
/// from. Move a tube's Bias and watch the curve lean; move a transistor's and
/// watch nothing happen, which is also true and also worth seeing.
///
/// Tilt is the one control that cannot appear here — it is a filter pair, so
/// it has no single transfer curve — and rate reduction cannot either, since
/// a held sample is a fact about time rather than about level. The step
/// graphic beside the curve is what says so.
#[component]
fn CurveView(
    circuit: Circuit,
    accent: String,
    ink: String,
    profile_id: String,
    controls: saturate_profiles::Controls,
    lift: f64,
) -> Element {
    let (w, h) = (620.0, 150.0);
    let (cx, cy) = (w / 2.0, h / 2.0);
    let half = 62.0;
    let drive = controls.drive as f64;
    let mix = controls.mix as f64;
    let glow = (0.35 + drive * 0.5).min(0.95) * lift.min(1.5);
    let body = accent.clone();

    let profile = saturate_profiles::profile_by_id(&profile_id)
        .unwrap_or(&saturate_profiles::PROFILES[0]);
    let mut pre = saturate_dsp::preamp::ClassAPreamp::new(48_000.0);
    let mut quantiser = saturate_dsp::digital::DigitalStage::new();
    saturate_profiles::apply(profile, &controls, &mut pre, &mut quantiser);
    // Rate reduction is time-domain and dither is noise; neither belongs in a
    // static curve. Word length does — a crushed signal really is a staircase.
    quantiser.rate = 1.0;
    quantiser.dither = 0.0;

    // The stage compensates its own drive (`process` divides the shaped
    // signal by it), so the drawing has to as well — otherwise turning drive
    // up would draw a steeper line rather than a flatter one, which is the
    // opposite of what you hear.
    let compensate = pre.drive.max(1.0e-3);

    let mut path = String::new();
    let samples = 200;
    for i in 0..=samples {
        let x = -1.0 + 2.0 * i as f64 / samples as f64;
        let shaped = pre.transfer(x as f32) / compensate;
        let y = (quantiser.process(0, shaped) as f64).clamp(-1.0, 1.0);
        let (px, py) = (cx + x * half * 2.0, cy - y * half);
        path.push_str(&format!("{}{:.1} {:.1}", if i == 0 { "M " } else { " L " }, px, py));
    }
    // The corner markers on the solid-state panel sit where the curve gives
    // up, which is the drive it takes to reach the rail.
    let gain = compensate as f64;

    rsx! {
        svg {
            view_box: "0 0 {w} {h}",
            style: "width:100%; height:100%; display:block;",

            // Axes: the unity line is what the curve is bending away from.
            line { x1: "{cx - half * 2.0:.1}", y1: "{cy:.1}", x2: "{cx + half * 2.0:.1}", y2: "{cy:.1}",
                stroke: "{ink}", stroke_width: "0.8", opacity: "0.45" }
            line { x1: "{cx:.1}", y1: "{cy - half:.1}", x2: "{cx:.1}", y2: "{cy + half:.1}",
                stroke: "{ink}", stroke_width: "0.8", opacity: "0.45" }
            line {
                x1: "{cx - half * 2.0:.1}", y1: "{cy + half:.1}",
                x2: "{cx + half * 2.0:.1}", y2: "{cy - half:.1}",
                stroke: "{ink}", stroke_width: "0.7", opacity: "0.22",
                stroke_dasharray: "3 4",
            }

            // The curve itself.
            path { d: "{path}", fill: "none", stroke: "{body}", stroke_width: "2.4",
                opacity: "{glow:.3}", stroke_linecap: "round" }

            // The circuit, drawn beside it.
            match circuit {
                // A valve: envelope, plate, and a heater that glows with drive.
                Circuit::Valve => rsx! {
                    ellipse { cx: "{w - 74.0:.1}", cy: "{cy:.1}", rx: "26", ry: "40",
                        fill: "{body}", opacity: "{glow * 0.16:.3}",
                        stroke: "{ink}", stroke_width: "1.4" }
                    rect { x: "{w - 88.0:.1}", y: "{cy + 34.0:.1}", width: "28", height: "12", rx: "2",
                        fill: "{ink}", opacity: "0.5" }
                    // The heater — the part that is actually orange.
                    path {
                        d: "M {w - 82.0:.1} {cy + 18.0:.1} L {w - 74.0:.1} {cy - 14.0:.1} L {w - 66.0:.1} {cy + 18.0:.1}",
                        fill: "none", stroke: "{body}", stroke_width: "2.2",
                        opacity: "{(0.35 + drive * 0.65):.3}",
                    }
                },
                // Tape over a head.
                Circuit::Tape => rsx! {
                    path {
                        d: "M {w - 122.0:.1} {cy - 30.0:.1} Q {w - 74.0:.1} {cy - 6.0:.1} {w - 26.0:.1} {cy - 30.0:.1}",
                        fill: "none", stroke: "{ink}", stroke_width: "2.0", opacity: "0.65",
                    }
                    rect { x: "{w - 84.0:.1}", y: "{cy - 14.0:.1}", width: "20", height: "30", rx: "3",
                        fill: "{body}", opacity: "{glow * 0.7:.3}", stroke: "{ink}", stroke_width: "1.2" }
                },
                // A core with windings through it.
                Circuit::Core => rsx! {
                    rect { x: "{w - 104.0:.1}", y: "{cy - 34.0:.1}", width: "64", height: "68", rx: "3",
                        fill: "none", stroke: "{ink}", stroke_width: "1.6" }
                    for i in 0..5 {
                        path {
                            key: "{i}",
                            d: "M {w - 110.0:.1} {cy - 24.0 + i as f64 * 12.0:.1} \
                                q 10 -8 20 0 q 10 8 20 0 q 10 -8 20 0",
                            fill: "none", stroke: "{body}", stroke_width: "1.6",
                            opacity: "{glow * (1.0 - i as f64 * 0.12):.3}",
                        }
                    }
                },
                // The corner, called out.
                Circuit::Solid => rsx! {
                    for (i , sign) in [-1.0_f64, 1.0].into_iter().enumerate() {
                        circle {
                            key: "{i}",
                            cx: "{cx + sign * (half * 2.0 / gain).min(half * 2.0):.1}",
                            cy: "{cy - sign * half * 0.92:.1}",
                            r: "4", fill: "{body}", opacity: "{glow:.3}",
                        }
                    }
                },
                // The steps, counted out along the ceiling.
                Circuit::Steps => rsx! {
                    for i in 0..8 {
                        rect {
                            key: "{i}",
                            x: "{w - 108.0 + i as f64 * 11.0:.1}",
                            y: "{cy - 30.0 + i as f64 * 6.0:.1}",
                            width: "9", height: "5",
                            fill: "{body}",
                            opacity: "{glow * (1.0 - i as f64 * 0.09):.3}",
                        }
                    }
                },
            }

            // How much of it you are actually hearing.
            rect {
                x: "{cx - half * 2.0:.1}", y: "{h - 10.0:.1}",
                width: "{half * 4.0 * mix:.1}", height: "3", rx: "1.5",
                fill: "{body}", opacity: "{glow * 0.8:.3}",
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [&SatDesign; 5] = [&TUBE, &TAPE, &TRANSFORMER, &TRANSISTOR, &DIGITAL];

    #[test]
    fn nothing_is_placed_under_a_rack_ear() {
        const EAR: f64 = 26.0;
        for design in ALL {
            for spec in design.knobs {
                let half = spec.d * (110.0 / 60.0) / 2.0;
                assert!(
                    spec.x - half >= EAR && spec.x + half <= W - EAR,
                    "{}'s {} at x={} runs under an ear",
                    design.family, spec.param, spec.x,
                );
            }
        }
    }

    #[test]
    fn every_legend_fits_on_the_panel() {
        for design in ALL {
            for spec in design.knobs {
                let y = spec.y + spec.d * 0.92 + 10.0;
                assert!(y + 6.0 <= H, "{}'s {} legend falls off", design.family, spec.param);
            }
        }
    }

    #[test]
    fn every_placed_control_is_one_the_editor_binds() {
        const BOUND: &[&str] = &[
            "drive", "bias", "sag", "tilt", "mix", "output", "character_a", "character_b",
        ];
        for design in ALL {
            for spec in design.knobs {
                assert!(BOUND.contains(&spec.param), "{} places {:?}", design.family, spec.param);
            }
        }
    }

    /// Drive, mix and output are on every saturator ever built. One that hides
    /// any of them is a panel you cannot use.
    #[test]
    fn every_panel_has_drive_mix_and_output() {
        for design in ALL {
            for required in ["drive", "mix", "output"] {
                assert!(
                    design.knobs.iter().any(|k| k.param == required),
                    "{} has no {required}",
                    design.family,
                );
            }
        }
    }

    /// Bias only appears where it does something. On a symmetric curve the
    /// control has no audible effect, and a knob that does nothing is worse
    /// than no knob: it teaches you the plugin is lying.
    #[test]
    fn bias_is_offered_only_where_the_curve_is_asymmetric() {
        for design in ALL {
            let has_bias = design.knobs.iter().any(|k| k.param == "bias");
            let asymmetric = matches!(design.circuit, Circuit::Valve | Circuit::Tape);
            assert!(
                !has_bias || asymmetric,
                "{} offers Bias on a symmetric curve",
                design.family,
            );
        }
    }

    /// …and now that a profile names its own shapers, "asymmetric" is a fact
    /// about the engine rather than about which panel it draws. A family that
    /// offers Bias must be built on a stage where moving the operating point
    /// actually changes the harmonics.
    #[test]
    fn a_panel_that_offers_bias_is_built_on_an_asymmetric_stage() {
        for category in saturate_profiles::CATEGORIES {
            let design = design_for(category.profiles[0]);
            if !design.knobs.iter().any(|k| k.param == "bias") {
                continue;
            }
            for id in category.profiles {
                let v = saturate_profiles::profile_by_id(id).unwrap().voicing;
                assert!(
                    v.positive != v.negative || v.skew != 0.0,
                    "{id} offers Bias but its two halves are the same circuit",
                );
            }
        }
    }

    /// Two knobs with the same word under them is a panel you cannot read.
    /// The circuit knobs are renamed per profile, so the clash only appears
    /// on one of the nine — which is exactly the kind of thing a static
    /// legend table hides.
    #[test]
    fn no_profile_shows_the_same_legend_twice() {
        for profile in saturate_profiles::PROFILES {
            let design = design_for(profile.id);
            let (a, b) = character_legends(profile.id);
            let mut seen: Vec<&str> = Vec::new();
            for spec in design.knobs {
                let legend = match spec.param {
                    "character_a" => a,
                    "character_b" => b,
                    _ => spec.legend,
                };
                assert!(
                    !seen.contains(&legend),
                    "{} shows {legend:?} twice",
                    profile.id,
                );
                seen.push(legend);
            }
        }
    }

    /// The panel draws the engine, so a control the panel places must be one
    /// the profile has actually wired to something. A knob drawn on a circuit
    /// that ignores it is the drift the voicing table exists to prevent.
    #[test]
    fn every_circuit_knob_a_panel_places_is_wired_by_its_profile() {
        use saturate_profiles::Character;
        for profile in saturate_profiles::PROFILES {
            let design = design_for(profile.id);
            for (param, role) in [
                ("character_a", profile.voicing.character_a),
                ("character_b", profile.voicing.character_b),
            ] {
                if design.knobs.iter().any(|k| k.param == param) {
                    assert_ne!(
                        role,
                        Character::None,
                        "{} draws {param} and wires it to nothing",
                        profile.id,
                    );
                }
            }
        }
    }

    #[test]
    fn every_profile_names_its_circuit_controls() {
        for profile in saturate_profiles::PROFILES {
            let (a, b) = character_legends(profile.id);
            assert_ne!((a, b), ("Character", "Colour"), "{} has placeholders", profile.id);
        }
    }

    #[test]
    fn every_family_has_a_panel() {
        for category in saturate_profiles::CATEGORIES {
            for id in category.profiles {
                assert_eq!(design_for(id).family, category.id, "{id} draws the wrong panel");
            }
        }
    }
}
