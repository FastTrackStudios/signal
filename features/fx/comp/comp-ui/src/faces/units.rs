//! The eight hardware faces, as [`RackDesign`] tables.
//!
//! Each one is modelled on the real unit's panel — the layout, the controls it
//! actually has, and the numbers printed around them. Not its branding or its
//! artwork: the point is that a 1176 gives you INPUT and OUTPUT and no
//! threshold, that a dbx 160 gives you three knobs, that a Fairchild gives you
//! a six-position time constant. That behaviour is what the profile data
//! encodes and what these panels expose.
//!
//! Panels are 4:1 rack-unit drawings, so faces ask the host for a short
//! editor — see `crate::faces::preferred_editor_size`.

use crate::hardware::knob::KnobStyle;
use crate::hardware::rack::{RackDesign, RackItem, Ring};
use crate::hardware::vu::{VuFace, VuMode};

/// Panel drawing size shared by every rack face.
pub const W: f64 = 900.0;
pub const H: f64 = 300.0;
/// The row the controls sit on, and where their legends are printed.
const ROW: f64 = 152.0;

// ─────────────────────────────────────────────────────────────────────────
// FET — UREI 1176
// ─────────────────────────────────────────────────────────────────────────

/// Black FET panel: INPUT drives *into* compression (there is no threshold
/// knob), the ratio is four buttons, and ATTACK/RELEASE run backwards —
/// fastest fully clockwise.
pub static UREI_1176: RackDesign = RackDesign {
    id: "urei_1176",
    w: W,
    h: H,
    paint: "linear-gradient(178deg, #2b2b2f 0%, #1a1a1e 52%, #101013 100%)",
    ink: "#ded7c9",
    dim_ink: "#9a9384",
    chrome: "#8d8a84",
    vu: VuFace::Blue,
    knob: KnobStyle::Bakelite,
    items: &[
        RackItem::Vu {
            x: 186.0,
            y: 140.0,
            w: 228.0,
            mode: VuMode::GainReduction,
            legend: "Gain Reduction",
        },
        RackItem::Buttons {
            id: "ratio",
            legend: "Ratio",
            x: 352.0,
            y: ROW,
            labels: &["4", "8", "12", "20", "All"],
        },
        RackItem::Knob {
            id: "input",
            legend: "Input",
            x: 470.0,
            y: ROW,
            d: 58.0,
            ring: Ring::Linear {
                from: -48.0,
                to: 12.0,
                majors: 5,
            },
            tint: None,
        },
        RackItem::Knob {
            id: "output",
            legend: "Output",
            x: 583.0,
            y: ROW,
            d: 58.0,
            ring: Ring::Linear {
                from: -12.0,
                to: 24.0,
                majors: 5,
            },
            tint: None,
        },
        RackItem::Knob {
            id: "attack",
            legend: "Attack",
            x: 696.0,
            y: ROW,
            d: 58.0,
            ring: Ring::Linear {
                from: 1.0,
                to: 7.0,
                majors: 7,
            },
            tint: None,
        },
        RackItem::Knob {
            id: "release",
            legend: "Release",
            x: 809.0,
            y: ROW,
            d: 58.0,
            ring: Ring::Linear {
                from: 1.0,
                to: 7.0,
                majors: 7,
            },
            tint: None,
        },
        RackItem::Text {
            x: 600.0,
            y: 60.0,
            text: "Peak Limiter",
            size: 14.0,
            strong: true,
        },
        RackItem::Text {
            x: 600.0,
            y: 84.0,
            text: "FTS Comp · FET",
            size: 9.0,
            strong: false,
        },
    ],
};

// ─────────────────────────────────────────────────────────────────────────
// Opto — LA-2A, CL 1B
// ─────────────────────────────────────────────────────────────────────────

/// Cream leveling amplifier: two knobs and a switch, both knobs printed 0–10
/// because the operator's reference is the panel, not the engine's dB.
pub static LA2A: RackDesign = RackDesign {
    id: "la2a",
    w: W,
    h: H,
    paint: "linear-gradient(178deg, #efe6cf 0%, #e4d8bb 46%, #d6c9a9 100%)",
    ink: "#3a3228",
    dim_ink: "#6b6053",
    chrome: "#c9c2b0",
    vu: VuFace::Amber,
    knob: KnobStyle::Bakelite,
    items: &[
        RackItem::Vu {
            x: 218.0,
            y: 140.0,
            w: 240.0,
            mode: VuMode::GainReduction,
            legend: "Gain Reduction",
        },
        RackItem::Knob {
            id: "gain",
            legend: "Gain",
            x: 470.0,
            y: ROW,
            d: 66.0,
            ring: Ring::Linear {
                from: 0.0,
                to: 10.0,
                majors: 6,
            },
            tint: None,
        },
        RackItem::Knob {
            id: "peak_reduction",
            legend: "Peak Reduction",
            x: 650.0,
            y: ROW,
            d: 66.0,
            ring: Ring::Linear {
                from: 0.0,
                to: 10.0,
                majors: 6,
            },
            tint: None,
        },
        RackItem::Switch {
            id: "mode",
            legend: "Mode",
            x: 800.0,
            y: ROW,
            labels: ["Comp", "Limit"],
        },
        RackItem::Text {
            x: 640.0,
            y: 62.0,
            text: "Leveling Amplifier",
            size: 15.0,
            strong: true,
        },
        RackItem::Text {
            x: 640.0,
            y: 88.0,
            text: "FTS Comp · Optical",
            size: 9.0,
            strong: false,
        },
    ],
};

/// The other optical tube unit: same physics, every control unlocked. Blue
/// panel, five large black knobs, VU on the left.
pub static CL1B: RackDesign = RackDesign {
    id: "cl1b",
    w: W,
    h: H,
    paint: "linear-gradient(178deg, #2e5f86 0%, #1f4665 52%, #16334a 100%)",
    ink: "#eef4fa",
    dim_ink: "#a9c3d8",
    chrome: "#9fb4c6",
    vu: VuFace::Amber,
    knob: KnobStyle::Bakelite,
    items: &[
        RackItem::Vu {
            x: 168.0,
            y: 140.0,
            w: 212.0,
            mode: VuMode::GainReduction,
            legend: "Gain Reduction",
        },
        RackItem::Knob {
            id: "threshold",
            legend: "Threshold",
            x: 350.0,
            y: ROW,
            d: 54.0,
            ring: Ring::Linear {
                from: -40.0,
                to: 0.0,
                majors: 5,
            },
            tint: None,
        },
        RackItem::Knob {
            id: "ratio",
            legend: "Ratio",
            x: 462.0,
            y: ROW,
            d: 54.0,
            ring: Ring::Linear {
                from: 2.0,
                to: 10.0,
                majors: 5,
            },
            tint: None,
        },
        RackItem::Knob {
            id: "attack",
            legend: "Attack",
            x: 574.0,
            y: ROW,
            d: 54.0,
            ring: Ring::Plain { majors: 6 },
            tint: None,
        },
        RackItem::Knob {
            id: "release",
            legend: "Release",
            x: 686.0,
            y: ROW,
            d: 54.0,
            ring: Ring::Plain { majors: 6 },
            tint: None,
        },
        RackItem::Knob {
            id: "gain",
            legend: "Gain",
            x: 806.0,
            y: ROW,
            d: 54.0,
            ring: Ring::Linear {
                from: 0.0,
                to: 24.0,
                majors: 5,
            },
            tint: None,
        },
        RackItem::Text {
            x: 560.0,
            y: 58.0,
            text: "Opto Compressor",
            size: 14.0,
            strong: true,
        },
        RackItem::Text {
            x: 560.0,
            y: 82.0,
            text: "FTS Comp · Optical Tube",
            size: 9.0,
            strong: false,
        },
    ],
};

// ─────────────────────────────────────────────────────────────────────────
// Variable-Mu — Fairchild 670, Manley
// ─────────────────────────────────────────────────────────────────────────

/// The valve limiter: input attenuator, threshold, and the six-position TIME
/// CONSTANT switch that is the whole personality of the unit — positions 5 and
/// 6 are program-dependent, which is where the "glue" comes from.
pub static FAIRCHILD_670: RackDesign = RackDesign {
    id: "fairchild670",
    w: W,
    h: H,
    paint: "linear-gradient(178deg, #cfcabc 0%, #b9b3a4 50%, #a19a8b 100%)",
    ink: "#2a2620",
    dim_ink: "#5d564b",
    chrome: "#8f887a",
    vu: VuFace::Amber,
    knob: KnobStyle::Bakelite,
    items: &[
        RackItem::Vu {
            x: 148.0,
            y: 138.0,
            w: 176.0,
            mode: VuMode::GainReduction,
            legend: "Gain Reduction",
        },
        RackItem::Vu {
            x: 336.0,
            y: 138.0,
            w: 176.0,
            mode: VuMode::Level,
            legend: "Output",
        },
        RackItem::Knob {
            id: "input_gain",
            legend: "Input Gain",
            x: 520.0,
            y: ROW,
            d: 52.0,
            ring: Ring::Linear {
                from: -20.0,
                to: 0.0,
                majors: 5,
            },
            tint: None,
        },
        RackItem::Knob {
            id: "threshold",
            legend: "Threshold",
            x: 618.0,
            y: ROW,
            d: 52.0,
            ring: Ring::Linear {
                from: -40.0,
                to: 0.0,
                majors: 5,
            },
            tint: None,
        },
        RackItem::Knob {
            id: "time_constant",
            legend: "Time Const",
            x: 716.0,
            y: ROW,
            d: 52.0,
            ring: Ring::Detents(&["1", "2", "3", "4", "5", "6"]),
            tint: None,
        },
        RackItem::Knob {
            id: "output",
            legend: "Output",
            x: 814.0,
            y: ROW,
            d: 52.0,
            ring: Ring::Linear {
                from: 0.0,
                to: 20.0,
                majors: 5,
            },
            tint: None,
        },
        RackItem::Text {
            x: 660.0,
            y: 56.0,
            text: "Tube Limiter",
            size: 14.0,
            strong: true,
        },
        RackItem::Text {
            x: 660.0,
            y: 80.0,
            text: "FTS Comp · Variable-Mu",
            size: 9.0,
            strong: false,
        },
    ],
};

/// The modern mix-bus variable-mu: two meters, continuous threshold and
/// attack, stepped recovery, COMP/LIMIT, and the 100 Hz sidechain filter that
/// stops the kick from working the whole mix.
pub static MANLEY_VARI_MU: RackDesign = RackDesign {
    id: "manley_vari_mu",
    w: W,
    h: H,
    paint: "linear-gradient(178deg, #26262a 0%, #191a1d 52%, #101114 100%)",
    ink: "#e8e4da",
    dim_ink: "#9c968a",
    chrome: "#b8b2a6",
    vu: VuFace::Amber,
    knob: KnobStyle::Metal,
    items: &[
        RackItem::Vu {
            x: 150.0,
            y: 138.0,
            w: 180.0,
            mode: VuMode::GainReduction,
            legend: "Gain Reduction",
        },
        RackItem::Knob {
            id: "input",
            legend: "Input",
            x: 306.0,
            y: ROW,
            d: 50.0,
            ring: Ring::Linear {
                from: -8.0,
                to: 8.0,
                majors: 5,
            },
            tint: None,
        },
        RackItem::Knob {
            id: "threshold",
            legend: "Threshold",
            x: 402.0,
            y: ROW,
            d: 50.0,
            ring: Ring::Linear {
                from: -40.0,
                to: 0.0,
                majors: 5,
            },
            tint: None,
        },
        RackItem::Knob {
            id: "attack",
            legend: "Attack",
            x: 498.0,
            y: ROW,
            d: 50.0,
            ring: Ring::Plain { majors: 6 },
            tint: None,
        },
        RackItem::Knob {
            id: "recovery",
            legend: "Recovery",
            x: 594.0,
            y: ROW,
            d: 50.0,
            ring: Ring::Detents(&["1", "2", "3", "4", "5"]),
            tint: None,
        },
        RackItem::Knob {
            id: "output",
            legend: "Output",
            x: 690.0,
            y: ROW,
            d: 50.0,
            ring: Ring::Linear {
                from: -6.0,
                to: 18.0,
                majors: 5,
            },
            tint: None,
        },
        RackItem::Switch {
            id: "mode",
            legend: "Mode",
            x: 760.0,
            y: ROW,
            labels: ["Comp", "Limit"],
        },
        // The sidechain filter is a small two-position rotary in the corner:
        // a second bat switch beside MODE does not fit inside the rack ears.
        RackItem::Knob {
            id: "hp_sidechain",
            legend: "HP SC",
            x: 838.0,
            y: ROW,
            d: 34.0,
            ring: Ring::Detents(&["Out", "100"]),
            tint: None,
        },
        RackItem::Text {
            x: 560.0,
            y: 56.0,
            text: "Vari-Mu Limiter",
            size: 14.0,
            strong: true,
        },
        RackItem::Text {
            x: 560.0,
            y: 80.0,
            text: "FTS Comp · Variable-Mu",
            size: 9.0,
            strong: false,
        },
    ],
};

// ─────────────────────────────────────────────────────────────────────────
// VCA — SSL bus, dbx 160
// ─────────────────────────────────────────────────────────────────────────

/// The console centre-section strip. RATIO, ATTACK and RELEASE are rotary
/// *switches*, so their rings print detent legends — including AUTO at the
/// release stop.
pub static SSL_BUS: RackDesign = RackDesign {
    id: "ssl_bus",
    w: W,
    h: H,
    paint: "linear-gradient(178deg, #4a4d52 0%, #34373b 50%, #26282c 100%)",
    ink: "#e6e8ea",
    dim_ink: "#9aa0a6",
    chrome: "#a9adb2",
    vu: VuFace::Blue,
    knob: KnobStyle::Metal,
    items: &[
        RackItem::Vu {
            x: 180.0,
            y: 140.0,
            w: 218.0,
            mode: VuMode::GainReduction,
            legend: "Compression",
        },
        RackItem::Knob {
            id: "threshold",
            legend: "Threshold",
            x: 360.0,
            y: 150.0,
            d: 54.0,
            ring: Ring::Linear {
                from: -30.0,
                to: 0.0,
                majors: 7,
            },
            tint: None,
        },
        RackItem::Knob {
            id: "ratio",
            legend: "Ratio",
            x: 470.0,
            y: 150.0,
            d: 54.0,
            ring: Ring::Detents(&["2", "4", "10"]),
            tint: None,
        },
        RackItem::Knob {
            id: "attack",
            legend: "Attack ms",
            x: 580.0,
            y: 150.0,
            d: 54.0,
            ring: Ring::Detents(&["0.1", "0.3", "1", "3", "10", "30"]),
            tint: None,
        },
        RackItem::Knob {
            id: "release",
            legend: "Release s",
            x: 690.0,
            y: 150.0,
            d: 54.0,
            ring: Ring::Detents(&["0.1", "0.3", "0.6", "1.2", "A"]),
            tint: None,
        },
        RackItem::Knob {
            id: "makeup",
            legend: "Makeup",
            x: 776.0,
            y: 150.0,
            d: 54.0,
            ring: Ring::Linear {
                from: 0.0,
                to: 18.0,
                majors: 7,
            },
            tint: None,
        },
        RackItem::Knob {
            id: "mix",
            legend: "Mix",
            x: 838.0,
            y: 76.0,
            d: 34.0,
            ring: Ring::None,
            tint: None,
        },
        RackItem::Text {
            x: 560.0,
            y: 56.0,
            text: "Bus Compressor",
            size: 14.0,
            strong: true,
        },
        RackItem::Text {
            x: 560.0,
            y: 80.0,
            text: "FTS Comp · VCA",
            size: 9.0,
            strong: false,
        },
    ],
};

/// Three knobs and a VU, and that really is the whole front panel.
pub static DBX_160: RackDesign = RackDesign {
    id: "dbx160",
    w: W,
    h: H,
    paint: "linear-gradient(178deg, #d3d5d8 0%, #b6b9bd 50%, #9ca0a5 100%)",
    ink: "#25272a",
    dim_ink: "#5a5e63",
    chrome: "#8b8f94",
    vu: VuFace::Amber,
    knob: KnobStyle::Bakelite,
    items: &[
        RackItem::Vu {
            x: 210.0,
            y: 140.0,
            w: 236.0,
            mode: VuMode::GainReduction,
            legend: "Gain Reduction",
        },
        RackItem::Knob {
            id: "threshold",
            legend: "Threshold",
            x: 470.0,
            y: ROW,
            d: 60.0,
            ring: Ring::Linear {
                from: -40.0,
                to: 0.0,
                majors: 5,
            },
            tint: None,
        },
        RackItem::Knob {
            id: "compression",
            legend: "Compression",
            x: 640.0,
            y: ROW,
            d: 60.0,
            ring: Ring::Linear {
                from: 1.0,
                to: 20.0,
                majors: 5,
            },
            tint: None,
        },
        RackItem::Knob {
            id: "output",
            legend: "Output Gain",
            x: 810.0,
            y: ROW,
            d: 60.0,
            ring: Ring::Linear {
                from: -20.0,
                to: 20.0,
                majors: 5,
            },
            tint: None,
        },
        RackItem::Text {
            x: 640.0,
            y: 58.0,
            text: "Compressor / Limiter",
            size: 14.0,
            strong: true,
        },
        RackItem::Text {
            x: 640.0,
            y: 82.0,
            text: "FTS Comp · VCA",
            size: 9.0,
            strong: false,
        },
    ],
};

// ─────────────────────────────────────────────────────────────────────────
// Hybrid — Distressor
// ─────────────────────────────────────────────────────────────────────────

/// Digitally-controlled analogue: the ratio switch is a mode selector as much
/// as a number (NUKE is not "32:1", it is the sound), with the audio and
/// detector modes beside it.
pub static DISTRESSOR: RackDesign = RackDesign {
    id: "distressor",
    w: W,
    h: H,
    paint: "linear-gradient(178deg, #1e2a35 0%, #16202a 52%, #0e161d 100%)",
    ink: "#dfe7ee",
    dim_ink: "#8fa1b1",
    chrome: "#7d8b98",
    vu: VuFace::Blue,
    knob: KnobStyle::Bakelite,
    items: &[
        RackItem::Vu {
            x: 150.0,
            y: 136.0,
            w: 180.0,
            mode: VuMode::GainReduction,
            legend: "Gain Reduction",
        },
        RackItem::Buttons {
            id: "ratio",
            legend: "Ratio",
            x: 306.0,
            y: 146.0,
            labels: &["1", "2", "3", "4", "6", "10", "20", "Nuke"],
        },
        RackItem::Knob {
            id: "input",
            legend: "Input",
            x: 420.0,
            y: ROW,
            d: 52.0,
            ring: Ring::Plain { majors: 6 },
            tint: None,
        },
        RackItem::Knob {
            id: "attack",
            legend: "Attack",
            x: 524.0,
            y: ROW,
            d: 52.0,
            ring: Ring::Plain { majors: 6 },
            tint: None,
        },
        RackItem::Knob {
            id: "release",
            legend: "Release",
            x: 628.0,
            y: ROW,
            d: 52.0,
            ring: Ring::Plain { majors: 6 },
            tint: None,
        },
        RackItem::Knob {
            id: "output",
            legend: "Output",
            x: 732.0,
            y: ROW,
            d: 52.0,
            ring: Ring::Linear {
                from: -12.0,
                to: 24.0,
                majors: 5,
            },
            tint: None,
        },
        RackItem::Knob {
            id: "audio_mode",
            legend: "Audio",
            x: 828.0,
            y: 112.0,
            d: 40.0,
            ring: Ring::Detents(&["Cln", "D2", "D3"]),
            tint: None,
        },
        RackItem::Knob {
            id: "detector",
            legend: "Detector",
            x: 828.0,
            y: 214.0,
            d: 40.0,
            ring: Ring::Detents(&["Off", "HP", "HP+"]),
            tint: None,
        },
        RackItem::Text {
            x: 560.0,
            y: 54.0,
            text: "Distressor",
            size: 14.0,
            strong: true,
        },
        RackItem::Text {
            x: 560.0,
            y: 78.0,
            text: "FTS Comp · Hybrid",
            size: 9.0,
            strong: false,
        },
    ],
};

/// The design for a profile id, if that profile has a hardware face.
pub fn design_for(profile_id: &str) -> Option<&'static RackDesign> {
    Some(match profile_id {
        "urei_1176" => &UREI_1176,
        "la2a" => &LA2A,
        "cl1b" => &CL1B,
        "fairchild670" => &FAIRCHILD_670,
        "manley_vari_mu" => &MANLEY_VARI_MU,
        "ssl_bus" => &SSL_BUS,
        "dbx160" => &DBX_160,
        "distressor" => &DISTRESSOR,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use comp_profiles::{all_profiles, Profile};

    /// Every control a panel places has to exist on that unit's profile —
    /// otherwise the knob mounts and does nothing, which is the one failure a
    /// screenshot will not show you.
    #[test]
    fn every_placed_control_exists_on_its_profile() {
        for profile in all_profiles() {
            let Some(design) = design_for(profile.id()) else {
                continue;
            };
            for item in design.items {
                let id = match item {
                    RackItem::Knob { id, .. }
                    | RackItem::Buttons { id, .. }
                    | RackItem::Switch { id, .. } => *id,
                    _ => continue,
                };
                assert!(
                    profile.controls().iter().any(|c| c.id == id),
                    "{} places a control {id} its profile does not have",
                    profile.id(),
                );
            }
        }
    }

    /// …and the reverse: a control the unit has but the panel never places is
    /// unreachable. The Distressor's `audio_mode`/`detector` are on the panel
    /// as small rotaries, so nothing is exempt.
    #[test]
    fn every_profile_control_is_reachable_on_its_panel() {
        for profile in all_profiles() {
            let Some(design) = design_for(profile.id()) else {
                continue;
            };
            for control in profile.controls() {
                let placed = design.items.iter().any(|item| {
                    matches!(
                        item,
                        RackItem::Knob { id, .. }
                            | RackItem::Buttons { id, .. }
                            | RackItem::Switch { id, .. }
                        if *id == control.id
                    )
                });
                assert!(
                    placed,
                    "{}'s {} control is not on its panel",
                    profile.id(),
                    control.id,
                );
            }
        }
    }

    /// Every hardware profile has a panel — a rail entry with no face would
    /// mount an empty surface.
    #[test]
    fn every_hardware_profile_has_a_face() {
        for profile in all_profiles() {
            if profile.id() == "control" {
                continue;
            }
            assert!(
                design_for(profile.id()).is_some(),
                "{} has no rack design",
                profile.id()
            );
        }
    }

    /// Nothing may hang off the panel: the rack ears own the outer 26 px, and
    /// a control drawn under them is unclickable.
    #[test]
    fn nothing_is_placed_under_the_rack_ears() {
        const EAR: f64 = 26.0;
        for profile in all_profiles() {
            let Some(design) = design_for(profile.id()) else {
                continue;
            };
            for item in design.items {
                let (x, half) = match item {
                    RackItem::Knob { x, d, .. } => (*x, *d),
                    RackItem::Vu { x, w, .. } => (*x, (w + 14.0) / 2.0),
                    RackItem::Buttons { x, .. } => (*x, 45.0),
                    RackItem::Switch { x, .. } => (*x, 60.0),
                    // Console idioms — the EQ's panels place these, no
                    // compressor face does.
                    RackItem::Text { .. }
                    | RackItem::Lever { .. }
                    | RackItem::Readout { .. }
                    | RackItem::Lamp { .. }
                    | RackItem::Button { .. }
                    | RackItem::LedMeter { .. }
                    | RackItem::Divider { .. } => continue,
                };
                assert!(
                    x - half >= EAR && x + half <= design.w - EAR,
                    "{}: an item at x={x} (half-width {half}) runs under a rack ear",
                    profile.id(),
                );
            }
        }
    }
}
