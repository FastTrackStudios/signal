//! The five panels.
//!
//! A modulator's family is not a preset — it is a different machine, and the
//! panels say so. A chorus is a rack unit; a vibrato is a wooden-ended amp
//! chassis; a wah is a pedal. Same controls underneath, because that is what
//! the DSP has, but you should know which one you are looking at before you
//! read a single word.
//!
//! Each panel draws its own **modulation shape**, live, sampled from the
//! engine that will process the audio (see
//! [`modulation_profiles::shape`]). That is the honest picture of a
//! modulator: what it moves and when. A tremolo with groove on it draws a
//! lopsided wave because it *is* lopsided; a tape chorus wanders because its
//! wow is not locked to its rate; an envelope wah draws a flat line because
//! nothing periodic drives it at all.

use dioxus::prelude::*;
use fts_audio_ui::hardware::knob::{HardwareKnob, KnobStyle};
use fts_audio_ui::hardware::panel::{Panel, PanelEnds, PanelSlot, PanelTexture, Silkscreen};
use fts_audio_ui::ParamHandle;
use modulation_profiles::Character;

/// Panel drawing size — 2U, like the saturator's faces.
pub const W: f64 = 960.0;
pub const H: f64 = 300.0;

/// What a panel draws around its shape — the machine itself.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Machine {
    /// A rack chorus: two lamps and a stereo pair.
    Rack,
    /// A flanger: the comb, drawn as the notches it actually makes.
    Comb,
    /// An amp's vibrato circuit: a valve and a wooden cabinet.
    Chassis,
    /// A tremolo: the lamp and photocell that made it.
    Optical,
    /// A wah pedal: the treadle, at the angle the position knob implies.
    Treadle,
}

/// A circuit's panel.
#[derive(Clone, Copy)]
pub struct ModDesign {
    pub family: &'static str,
    pub paint: &'static str,
    pub ink: &'static str,
    pub dim_ink: &'static str,
    pub chrome: &'static str,
    pub accent: &'static str,
    pub ends: PanelEnds,
    pub texture: PanelTexture,
    pub machine: Machine,
    pub knob: KnobStyle,
}

/// Chorus: a cool blue rack unit.
pub static CHORUS: ModDesign = ModDesign {
    family: "chorus",
    paint: "linear-gradient(178deg, #2f3d4c 0%, #263340 48%, #1b242e 100%)",
    ink: "#e6eef6",
    dim_ink: "#93a5b6",
    chrome: "#8b98a6",
    accent: "#57b6e8",
    ends: PanelEnds::RackEars,
    texture: PanelTexture::Brushed { strength: 40 },
    machine: Machine::Rack,
    knob: KnobStyle::Collet,
};

/// Flanger: the same rack, wound tighter and coloured for it.
pub static FLANGER: ModDesign = ModDesign {
    family: "flanger",
    paint: "linear-gradient(178deg, #3a3350 0%, #2e2842 48%, #221d33 100%)",
    ink: "#ece7f8",
    dim_ink: "#a599bf",
    chrome: "#9489ab",
    accent: "#b07de8",
    ends: PanelEnds::RackEars,
    texture: PanelTexture::Brushed { strength: 50 },
    machine: Machine::Comb,
    knob: KnobStyle::SilverTop,
};

/// Vibrato: an amp chassis, wooden-ended.
pub static VIBRATO: ModDesign = ModDesign {
    family: "vibrato",
    paint: "linear-gradient(178deg, #e3d6bb 0%, #d4c6a8 48%, #bfae8c 100%)",
    ink: "#33291a",
    dim_ink: "#7d6c51",
    chrome: "#b3a488",
    accent: "#d8892f",
    ends: PanelEnds::Wood,
    texture: PanelTexture::Painted,
    machine: Machine::Chassis,
    knob: KnobStyle::Bakelite,
};

/// Tremolo: brownface, and wooden-ended for the same reason.
pub static TREMOLO: ModDesign = ModDesign {
    family: "tremolo",
    paint: "linear-gradient(178deg, #6b5238 0%, #57422c 48%, #402f1f 100%)",
    ink: "#f2e4cc",
    dim_ink: "#b39c7d",
    chrome: "#a08a6c",
    accent: "#e0a24a",
    ends: PanelEnds::Wood,
    texture: PanelTexture::Painted,
    machine: Machine::Optical,
    knob: KnobStyle::Pointer,
};

/// Wah: a pedal, and it should look like one.
pub static WAH: ModDesign = ModDesign {
    family: "wah",
    paint: "linear-gradient(178deg, #26262a 0%, #1c1c20 48%, #131316 100%)",
    ink: "#eceef2",
    dim_ink: "#8d919b",
    chrome: "#8b8f99",
    accent: "#e8a33c",
    ends: PanelEnds::RackEars,
    texture: PanelTexture::Painted,
    machine: Machine::Treadle,
    knob: KnobStyle::Daka,
};

pub fn design_for(profile_id: &str) -> &'static ModDesign {
    match modulation_profiles::category_of(profile_id)
        .map(|(c, _)| modulation_profiles::CATEGORIES[c].id)
    {
        Some("chorus") => &CHORUS,
        Some("flanger") => &FLANGER,
        Some("vibrato") => &VIBRATO,
        Some("tremolo") => &TREMOLO,
        _ => &WAH,
    }
}

/// What this profile's circuit calls its four knobs.
///
/// The *roles* live in `modulation-profiles`; these are only the words. A
/// slot the profile leaves unwired gets no name because it gets no knob.
pub fn knob_legends(profile_id: &str) -> [&'static str; 4] {
    match profile_id {
        "juno" => ["Voices", "Brightness", "Feedback", "Width"],
        "bbd" => ["Voices", "Clock", "Feedback", "Width"],
        "tape" => ["Voices", "Age", "Feedback", "Width"],
        "orbit" => ["Voices", "Orbit", "Feedback", "Width"],
        "cubic" => ["Voices", "Feedback", "Width", ""],
        "flanger" => ["Feedback", "Tone", "Width", "Voices"],
        "flanger_bbd" => ["Feedback", "Clock", "Width", "Voices"],
        "vibrato" => ["Tone", "Width", "Voices", ""],
        "vibrato_juno" => ["Brightness", "Width", "Voices", ""],
        "trem_opto" => ["Groove", "Feel", "Accent", "Analog"],
        "trem_stereo" => ["Groove", "Feel", "Accent", "Analog"],
        "trem_harmonic" => ["Groove", "Feel", "Crossover", "Analog"],
        "wah_auto" => ["Position", "Resonance", "Sensitivity", "Shape"],
        "wah_pedal" => ["Position", "Resonance", "Stages", "Shape"],
        "wah_pattern" => ["Position", "Resonance", "Pattern", "Shape"],
        _ => ["", "", "", ""],
    }
}

/// One control on a panel: which parameter, what it is called.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub param: &'static str,
    pub legend: String,
    pub x: f64,
    pub d: f64,
}

/// Lay the controls this profile actually has across the panel.
///
/// Generated rather than tabulated, because which circuit knobs exist is a
/// per-profile fact: a Clean chorus has three and a Juno has four, and a
/// static table would eventually place a knob the profile does not wire.
/// [`modulation_profiles::Voicing::knobs`] is the single source for that.
pub fn placed_controls(profile_id: &str) -> Vec<Placed> {
    let profile = modulation_profiles::profile_by_id(profile_id)
        .unwrap_or(&modulation_profiles::PROFILES[0]);
    let legends = knob_legends(profile_id);

    let mut controls: Vec<(&'static str, String)> = vec![
        ("rate", "Rate".to_string()),
        ("depth", "Depth".to_string()),
    ];
    for (i, knob) in profile.voicing.knobs.iter().enumerate() {
        if knob.role == Character::None {
            continue;
        }
        let param = match i {
            0 => "knob_a",
            1 => "knob_b",
            2 => "knob_c",
            _ => "knob_d",
        };
        controls.push((param, legends[i].to_string()));
    }
    // A wet-only circuit gets no Mix knob rather than one that does nothing.
    if !profile.voicing.wet_only {
        controls.push(("mix", "Mix".to_string()));
    }
    controls.push(("output", "Output".to_string()));

    // Even spacing inside the rack ears, widest knobs first so Rate and
    // Depth read as the two that matter.
    let n = controls.len().max(1);
    let (first, last) = (118.0, 878.0);
    let step = if n > 1 {
        (last - first) / (n - 1) as f64
    } else {
        0.0
    };
    controls
        .into_iter()
        .enumerate()
        .map(|(i, (param, legend))| Placed {
            param,
            legend,
            x: first + step * i as f64,
            d: if i < 2 { 54.0 } else { 46.0 },
        })
        .collect()
}

/// A drawn modulator: the panel, its shape, and its row of controls.
#[component]
pub fn ModFace(
    profile_id: String,
    handles: std::collections::HashMap<String, ParamHandle>,
    /// The shell's redraw tick. Not read; its job is to change, so the panel
    /// re-renders against fresh parameter values instead of being memoized.
    frame: u64,
) -> Element {
    let _ = frame;
    let design = design_for(&profile_id);
    let profile = modulation_profiles::profile_by_id(&profile_id)
        .unwrap_or(&modulation_profiles::PROFILES[0]);
    let scale = fts_audio_ui::hardware::panel::panel_scale(W, H, fts_audio_ui::shell::RAIL_W);

    let value = |name: &str, fallback: f32| {
        handles
            .get(name)
            .map(|h| h.normalized())
            .unwrap_or(fallback)
    };
    let defaults = modulation_profiles::Controls::default();
    let controls = modulation_profiles::Controls {
        rate: value("rate", defaults.rate),
        depth: value("depth", defaults.depth),
        mix: value("mix", defaults.mix),
        knobs: [
            value("knob_a", 0.5),
            value("knob_b", 0.5),
            value("knob_c", 0.5),
            value("knob_d", 0.5),
        ],
    };

    let placed = placed_controls(&profile_id);

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
                ShapeView {
                    machine: design.machine,
                    accent: design.accent.to_string(),
                    ink: design.dim_ink.to_string(),
                    profile_id: profile_id.clone(),
                    controls,
                }
            }

            Silkscreen {
                scale, x: 150.0, y: 40.0, width: 280.0,
                text: profile.name.to_string(), size: 15.0,
                color: design.ink.to_string(), weight: 800,
            }
            Silkscreen {
                scale, x: 150.0, y: 60.0, width: 280.0,
                text: modulation_profiles::CATEGORIES
                    .iter()
                    .find(|c| c.profiles.contains(&profile.id))
                    .map(|c| c.label)
                    .unwrap_or("Modulation")
                    .to_string(),
                size: 8.0, color: design.dim_ink.to_string(),
            }

            for (index , spec) in placed.iter().cloned().enumerate() {
                div {
                    key: "{design.family}-{index}",
                    if let Some(handle) = handles.get(spec.param) {
                        PanelSlot { scale, x: spec.x, y: 206.0, w: spec.d * 2.0, h: spec.d * 2.0,
                            HardwareKnob {
                                handle: handle.clone(),
                                testid: spec.param.replace('_', "-"),
                                scale,
                                diameter: spec.d,
                                style: design.knob,
                                ink: design.ink.to_string(),
                            }
                        }
                    }
                    Silkscreen {
                        scale, x: spec.x, y: 206.0 + spec.d * 0.92 + 10.0, width: 120.0,
                        text: spec.legend.clone(),
                        size: 9.0,
                        color: design.ink.to_string(),
                    }
                }
            }
        }
    }
}

/// The modulation shape, live, with the machine drawn beside it.
///
/// One cycle of what the circuit moves, sampled from the engine that will
/// process the audio — see [`modulation_profiles::shape`] for what is being
/// plotted on each family and why it differs.
#[component]
fn ShapeView(
    machine: Machine,
    accent: String,
    ink: String,
    profile_id: String,
    controls: modulation_profiles::Controls,
) -> Element {
    let (w, h) = (620.0, 150.0);
    let (cx, cy) = (w / 2.0, h / 2.0);
    let half = 52.0;
    let body = accent.clone();
    let depth = controls.depth as f64;
    let glow = (0.4 + depth * 0.5).min(0.95);

    let profile = modulation_profiles::profile_by_id(&profile_id)
        .unwrap_or(&modulation_profiles::PROFILES[0]);
    let mut samples = [0.0f64; 180];
    modulation_profiles::shape(profile, &controls, &mut samples);

    // Two cycles, so a shape with a groove on it reads as repeating rather
    // than as one odd-looking hump.
    let span = half * 2.6;
    let mut path = String::new();
    let mut fill = String::new();
    let total = samples.len() * 2;
    for i in 0..=total {
        let s = samples[i % samples.len()];
        let x = cx - span + (i as f64 / total as f64) * span * 2.0;
        let y = cy + half - (s * half * 2.0);
        if i == 0 {
            path.push_str(&format!("M {x:.1} {y:.1}"));
            fill.push_str(&format!("M {x:.1} {:.1} L {x:.1} {y:.1}", cy + half));
        } else {
            path.push_str(&format!(" L {x:.1} {y:.1}"));
            fill.push_str(&format!(" L {x:.1} {y:.1}"));
        }
    }
    fill.push_str(&format!(" L {:.1} {:.1} Z", cx + span, cy + half));

    rsx! {
        svg {
            view_box: "0 0 {w} {h}",
            style: "width:100%; height:100%; display:block;",

            // The rest line: what the circuit does when it is not moving.
            line {
                x1: "{cx - span:.1}", y1: "{cy:.1}", x2: "{cx + span:.1}", y2: "{cy:.1}",
                stroke: "{ink}", stroke_width: "0.8", opacity: "0.35",
                stroke_dasharray: "3 4",
            }
            line {
                x1: "{cx - span:.1}", y1: "{cy + half:.1}",
                x2: "{cx + span:.1}", y2: "{cy + half:.1}",
                stroke: "{ink}", stroke_width: "0.8", opacity: "0.25",
            }

            path { d: "{fill}", fill: "{body}", opacity: "{glow * 0.14:.3}" }
            path {
                d: "{path}", fill: "none", stroke: "{body}", stroke_width: "2.2",
                opacity: "{glow:.3}", stroke_linecap: "round", stroke_linejoin: "round",
            }

            // The machine, drawn beside it.
            match machine {
                // A rack unit: two lamps, one per channel.
                Machine::Rack => rsx! {
                    for (i , dx) in [-16.0_f64, 16.0].into_iter().enumerate() {
                        circle {
                            key: "{i}",
                            cx: "{w - 66.0 + dx:.1}", cy: "{cy - 26.0:.1}", r: "7",
                            fill: "{body}", opacity: "{glow * (1.0 - i as f64 * 0.35):.3}",
                            stroke: "{ink}", stroke_width: "1.0",
                        }
                    }
                    rect {
                        x: "{w - 96.0:.1}", y: "{cy - 6.0:.1}", width: "60", height: "34", rx: "3",
                        fill: "none", stroke: "{ink}", stroke_width: "1.3", opacity: "0.7",
                    }
                },
                // The comb itself: notches, and they move.
                Machine::Comb => rsx! {
                    for i in 0..7 {
                        line {
                            key: "{i}",
                            x1: "{w - 104.0 + i as f64 * 13.0:.1}", y1: "{cy - 34.0:.1}",
                            x2: "{w - 104.0 + i as f64 * 13.0:.1}",
                            y2: "{cy + 34.0 - i as f64 * 6.0:.1}",
                            stroke: "{body}", stroke_width: "2.0",
                            opacity: "{glow * (1.0 - i as f64 * 0.1):.3}",
                        }
                    }
                },
                // An amp chassis: a valve behind the shape.
                Machine::Chassis => rsx! {
                    ellipse {
                        cx: "{w - 70.0:.1}", cy: "{cy:.1}", rx: "22", ry: "34",
                        fill: "{body}", opacity: "{glow * 0.16:.3}",
                        stroke: "{ink}", stroke_width: "1.3",
                    }
                    path {
                        d: "M {w - 78.0:.1} {cy + 14.0:.1} L {w - 70.0:.1} {cy - 12.0:.1} L {w - 62.0:.1} {cy + 14.0:.1}",
                        fill: "none", stroke: "{body}", stroke_width: "2.0",
                        opacity: "{(0.4 + depth * 0.6):.3}",
                    }
                },
                // Lamp and photocell — the parts that made a tremolo.
                Machine::Optical => rsx! {
                    circle {
                        cx: "{w - 86.0:.1}", cy: "{cy:.1}", r: "11",
                        fill: "{body}", opacity: "{glow * 0.8:.3}",
                        stroke: "{ink}", stroke_width: "1.2",
                    }
                    rect {
                        x: "{w - 60.0:.1}", y: "{cy - 12.0:.1}", width: "16", height: "24", rx: "2",
                        fill: "none", stroke: "{ink}", stroke_width: "1.4", opacity: "0.8",
                    }
                    for i in 0..3 {
                        line {
                            key: "{i}",
                            x1: "{w - 73.0:.1}", y1: "{cy - 6.0 + i as f64 * 6.0:.1}",
                            x2: "{w - 62.0:.1}", y2: "{cy - 6.0 + i as f64 * 6.0:.1}",
                            stroke: "{body}", stroke_width: "1.2", opacity: "{glow:.3}",
                        }
                    }
                },
                // A treadle, tilted to where the pedal is sitting.
                Machine::Treadle => rsx! {
                    path {
                        d: "M {w - 108.0:.1} {cy + 26.0:.1} L {w - 30.0:.1} {cy + 26.0 - 40.0 * depth:.1} L {w - 30.0:.1} {cy + 34.0 - 40.0 * depth:.1} L {w - 108.0:.1} {cy + 34.0:.1} Z",
                        fill: "{body}", opacity: "{glow * 0.5:.3}",
                        stroke: "{ink}", stroke_width: "1.2",
                    }
                    circle {
                        cx: "{w - 108.0:.1}", cy: "{cy + 30.0:.1}", r: "4",
                        fill: "{ink}", opacity: "0.7",
                    }
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use modulation_profiles::{profile_by_id, Character, CATEGORIES, PROFILES};

    const ALL: [&ModDesign; 5] = [&CHORUS, &FLANGER, &VIBRATO, &TREMOLO, &WAH];

    #[test]
    fn every_family_has_a_panel() {
        for category in CATEGORIES {
            for id in category.profiles {
                assert_eq!(
                    design_for(id).family,
                    category.id,
                    "{id} draws the wrong panel",
                );
            }
        }
        assert_eq!(ALL.len(), CATEGORIES.len());
    }

    #[test]
    fn nothing_is_placed_under_a_rack_ear() {
        const EAR: f64 = 26.0;
        for profile in PROFILES {
            for spec in placed_controls(profile.id) {
                let half = spec.d * (110.0 / 60.0) / 2.0;
                assert!(
                    spec.x - half >= EAR && spec.x + half <= W - EAR,
                    "{}'s {} at x={} runs under an ear",
                    profile.id,
                    spec.param,
                    spec.x,
                );
            }
        }
    }

    #[test]
    fn every_legend_fits_on_the_panel() {
        for profile in PROFILES {
            for spec in placed_controls(profile.id) {
                let y = 206.0 + spec.d * 0.92 + 10.0;
                assert!(y + 6.0 <= H, "{}'s {} legend falls off", profile.id, spec.param);
            }
        }
    }

    /// Rate, depth and output are on every modulator ever built.
    #[test]
    fn every_panel_has_rate_depth_and_output() {
        for profile in PROFILES {
            let placed = placed_controls(profile.id);
            for required in ["rate", "depth", "output"] {
                assert!(
                    placed.iter().any(|p| p.param == required),
                    "{} has no {required}",
                    profile.id,
                );
            }
        }
    }

    /// Mix appears only where there is a dry path to mix. On vibrato the
    /// control has no effect at all, and a knob that does nothing is worse
    /// than no knob: it teaches you the plugin is lying.
    #[test]
    fn mix_is_offered_only_where_there_is_a_dry_path() {
        for profile in PROFILES {
            let has_mix = placed_controls(profile.id).iter().any(|p| p.param == "mix");
            assert_eq!(
                has_mix, !profile.voicing.wet_only,
                "{} disagrees with its voicing about Mix",
                profile.id,
            );
        }
    }

    /// A knob the panel places must be one the profile wires. This is the
    /// drift the voicing table exists to prevent, and the reason the layout
    /// is generated from it rather than tabulated beside it.
    #[test]
    fn every_placed_circuit_knob_is_wired_by_its_profile() {
        for profile in PROFILES {
            let placed = placed_controls(profile.id);
            for (i, param) in ["knob_a", "knob_b", "knob_c", "knob_d"].iter().enumerate() {
                let drawn = placed.iter().any(|p| p.param == *param);
                let wired = profile.voicing.knobs[i].role != Character::None;
                assert_eq!(
                    drawn, wired,
                    "{} draws {param}={drawn} but wires it {wired}",
                    profile.id,
                );
            }
        }
    }

    /// …and it must have a name. An unnamed knob on a hardware panel is a
    /// hole someone forgot to silkscreen.
    #[test]
    fn every_placed_control_has_a_legend() {
        for profile in PROFILES {
            for spec in placed_controls(profile.id) {
                assert!(
                    !spec.legend.trim().is_empty(),
                    "{}'s {} has no legend",
                    profile.id,
                    spec.param,
                );
            }
        }
    }

    /// Two knobs with the same word under them is a panel you cannot read.
    #[test]
    fn no_profile_shows_the_same_legend_twice() {
        for profile in PROFILES {
            let mut seen: Vec<String> = Vec::new();
            for spec in placed_controls(profile.id) {
                assert!(
                    !seen.contains(&spec.legend),
                    "{} shows {:?} twice",
                    profile.id,
                    spec.legend,
                );
                seen.push(spec.legend);
            }
        }
    }

    /// The legend table is written by hand and the roles are not — so a
    /// profile that grows a knob must not keep an empty name for it.
    #[test]
    fn every_wired_knob_has_a_word_in_the_legend_table() {
        for profile in PROFILES {
            let legends = knob_legends(profile.id);
            for (i, knob) in profile.voicing.knobs.iter().enumerate() {
                if knob.role == Character::None {
                    continue;
                }
                assert!(
                    !legends[i].is_empty(),
                    "{} wires knob {i} as {:?} and does not name it",
                    profile.id,
                    knob.role,
                );
            }
        }
        assert!(profile_by_id("juno").is_some());
    }
}
