//! The EQ faces, as [`RackDesign`] tables.
//!
//! Modelled on the real units' panels — the controls they have and the numbers
//! printed around them, not anyone's branding. What that buys is the thing
//! that makes these EQs different from each other and from a parametric:
//!
//! - The **Pultec** has separate BOOST and ATTEN on the same low band, which is
//!   why the "Pultec trick" (boosting and cutting at once) exists at all, and a
//!   BANDWIDTH control rather than a Q number.
//! - The **SSL** is four bands with fixed shapes and swept frequencies, plus
//!   the filters — you steer it, you do not design it.
//! - The **API** is three bands of stepped frequencies and 2 dB gain steps: a
//!   proportional-Q console EQ where the choices are deliberately coarse.
//! - The **1073** gives you a fixed 12 kHz top, a stepped mid, a stepped low
//!   shelf and a stepped high-pass. Four decisions, no continuous frequency.
//!
//! Panel drawings are wide and short, so a face asks the host for a shorter
//! editor — see [`crate::faces::preferred_editor_size`].

use fts_ui_audio::hardware::knob::KnobStyle;
use fts_ui_audio::hardware::rack::{RackDesign, RackItem, Ring};
use fts_ui_audio::hardware::vu::VuFace;

/// Panel drawing size shared by the EQ faces. Taller than the compressor's
/// because these units are 2–3U and carry two rows of controls; the ratio is
/// the Pultec's own 2.72:1.
pub const W: f64 = 900.0;
pub const H: f64 = 331.0;

/// The two control rows.
const ROW_A: f64 = 132.0;
const ROW_B: f64 = 268.0;

// ─────────────────────────────────────────────────────────────────────────
// Pultec EQP-1A
// ─────────────────────────────────────────────────────────────────────────

/// The EQP-1A, laid out from the unit's own panel rather than from a grid.
///
/// Coordinates are the photograph's, scaled to this design space (the panel is
/// 2.72:1, and everything below sits where it sits on the real thing): the two
/// low-band knobs and the two high-band knobs across the top with their values
/// printed above them, the two frequency *levers* below, BANDWIDTH between
/// them, and ATTEN SEL / OUTPUT stacked at the right edge.
///
/// The details that are the unit rather than decoration:
///
/// - BOOST and ATTEN are separate controls on the *same* band, which is the
///   only reason the "Pultec trick" exists.
/// - The frequency selectors are levers, not knobs — four positions and seven,
///   read at a glance.
/// - The knobs are printed 0–10 and nothing else. The number above each is the
///   position on that scale, which is how the unit is actually documented and
///   recalled.
pub static PULTEC: RackDesign = RackDesign {
    id: "eq_pultec_eqp1a",
    w: W,
    h: H,
    // Dusty blue-grey, lit from above.
    paint: "linear-gradient(176deg, #5b7e94 0%, #4e7188 46%, #3f5f74 100%)",
    ink: "#e8eef2",
    dim_ink: "#a9bfcc",
    chrome: "#b9c6cf",
    vu: VuFace::Blue,
    knob: KnobStyle::Skirted,
    items: &[
        // ── Top row: the two bands' boost and atten ──────────────────────
        RackItem::Readout { id: "low_boost", x: 214.0, y: 22.0 },
        RackItem::Knob {
            id: "low_boost",
            legend: "Boost",
            x: 214.0,
            y: 98.0,
            d: 96.0,
            ring: Ring::Numerals(&["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10"]),
        },
        RackItem::Readout { id: "low_atten", x: 349.0, y: 22.0 },
        RackItem::Knob {
            id: "low_atten",
            legend: "Atten",
            x: 349.0,
            y: 98.0,
            d: 96.0,
            ring: Ring::Numerals(&["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10"]),
        },
        RackItem::Readout { id: "high_boost", x: 553.0, y: 22.0 },
        RackItem::Knob {
            id: "high_boost",
            legend: "Boost",
            x: 553.0,
            y: 98.0,
            d: 96.0,
            ring: Ring::Numerals(&["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10"]),
        },
        RackItem::Readout { id: "high_atten", x: 688.0, y: 22.0 },
        RackItem::Knob {
            id: "high_atten",
            legend: "Atten",
            x: 688.0,
            y: 98.0,
            d: 96.0,
            ring: Ring::Numerals(&["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10"]),
        },
        // The high-attenuation frequency: a small knob at the top right.
        RackItem::Knob {
            id: "high_atten_freq",
            legend: "Atten Sel",
            x: 810.0,
            y: 94.0,
            d: 42.0,
            ring: Ring::Numerals(&["5", "10", "20"]),
        },
        // ── Bottom row: the levers, bandwidth, bypass and output ─────────
        RackItem::Lever {
            id: "low_freq",
            legend: "Low Frequency",
            unit: "CPS",
            x: 289.0,
            y: 222.0,
            labels: &["20", "30", "60", "100"],
        },
        RackItem::Readout { id: "bandwidth", x: 447.0, y: 176.0 },
        RackItem::Knob {
            id: "bandwidth",
            legend: "Bandwidth",
            x: 447.0,
            y: 240.0,
            d: 88.0,
            ring: Ring::Numerals(&["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10"]),
        },
        RackItem::Lever {
            id: "high_boost_freq",
            legend: "High Frequency",
            unit: "KCS",
            x: 609.0,
            y: 222.0,
            labels: &["3", "4", "5", "8", "10", "12", "16"],
        },
        RackItem::Lamp {
            x: 746.0,
            y: 168.0,
            color: "#ff4a32",
        },
        RackItem::Text {
            x: 690.0,
            y: 196.0,
            text: "Off",
            size: 9.0,
            strong: true,
        },
        RackItem::Text {
            x: 744.0,
            y: 214.0,
            text: "On",
            size: 9.0,
            strong: true,
        },
        RackItem::Knob {
            id: "eq_in",
            legend: "",
            x: 709.0,
            y: 236.0,
            d: 44.0,
            ring: Ring::None,
        },
        RackItem::Readout { id: "trim", x: 810.0, y: 196.0 },
        RackItem::Knob {
            id: "trim",
            legend: "Output",
            x: 810.0,
            y: 240.0,
            d: 46.0,
            ring: Ring::Numerals(&["0", "2", "4", "6", "8", "10"]),
        },
        // ── Panel marks ──────────────────────────────────────────────────
        RackItem::Text {
            x: 108.0,
            y: 302.0,
            text: "EQP-1A",
            size: 13.0,
            strong: true,
        },
        RackItem::Text {
            x: 700.0,
            y: 306.0,
            text: "Vintage Program Equalizer",
            size: 10.0,
            strong: false,
        },
    ],
};

// ─────────────────────────────────────────────────────────────────────────
// SSL channel EQ (E and G)
// ─────────────────────────────────────────────────────────────────────────

/// Four bands with swept frequencies and the console's filters. E and G share
/// this panel and these parameters — the difference between them is the curves
/// the DSP runs, which is exactly what the model value selects.
pub static SSL: RackDesign = RackDesign {
    id: "eq_ssl_e",
    w: W,
    h: H,
    paint: "linear-gradient(178deg, #43464b 0%, #303337 50%, #232528 100%)",
    ink: "#e9ebee",
    dim_ink: "#9ba1a8",
    chrome: "#a5a9ae",
    vu: VuFace::Blue,
    knob: KnobStyle::Metal,
    items: &[
        RackItem::Text {
            x: 450.0,
            y: 44.0,
            text: "Channel Equalizer",
            size: 15.0,
            strong: true,
        },
        RackItem::Text {
            x: 450.0,
            y: 68.0,
            text: "FTS EQ · Console",
            size: 9.0,
            strong: false,
        },
        // Frequencies on the top row, gains beneath them: the console layout,
        // and the one that lets you read a band as a column.
        RackItem::Knob {
            id: "hpf",
            legend: "HPF",
            x: 112.0,
            y: ROW_A,
            d: 46.0,
            ring: Ring::Plain { majors: 5 },
        },
        RackItem::Knob {
            id: "lf_freq",
            legend: "LF Freq",
            x: 230.0,
            y: ROW_A,
            d: 46.0,
            ring: Ring::Plain { majors: 5 },
        },
        RackItem::Knob {
            id: "lmf_freq",
            legend: "LMF Freq",
            x: 348.0,
            y: ROW_A,
            d: 46.0,
            ring: Ring::Plain { majors: 5 },
        },
        RackItem::Knob {
            id: "hmf_freq",
            legend: "HMF Freq",
            x: 466.0,
            y: ROW_A,
            d: 46.0,
            ring: Ring::Plain { majors: 5 },
        },
        RackItem::Knob {
            id: "hf_freq",
            legend: "HF Freq",
            x: 584.0,
            y: ROW_A,
            d: 46.0,
            ring: Ring::Plain { majors: 5 },
        },
        RackItem::Knob {
            id: "lpf",
            legend: "LPF",
            x: 702.0,
            y: ROW_A,
            d: 46.0,
            ring: Ring::Plain { majors: 5 },
        },
        RackItem::Switch {
            id: "eq_in",
            legend: "EQ",
            x: 818.0,
            y: ROW_A,
            labels: ["Out", "In"],
        },
        RackItem::Knob {
            id: "lf_gain",
            legend: "LF",
            x: 230.0,
            y: ROW_B,
            d: 46.0,
            ring: Ring::Linear {
                from: -18.0,
                to: 18.0,
                majors: 5,
            },
        },
        RackItem::Knob {
            id: "lmf_gain",
            legend: "LMF",
            x: 348.0,
            y: ROW_B,
            d: 46.0,
            ring: Ring::Linear {
                from: -18.0,
                to: 18.0,
                majors: 5,
            },
        },
        RackItem::Knob {
            id: "hmf_gain",
            legend: "HMF",
            x: 466.0,
            y: ROW_B,
            d: 46.0,
            ring: Ring::Linear {
                from: -18.0,
                to: 18.0,
                majors: 5,
            },
        },
        RackItem::Knob {
            id: "hf_gain",
            legend: "HF",
            x: 584.0,
            y: ROW_B,
            d: 46.0,
            ring: Ring::Linear {
                from: -18.0,
                to: 18.0,
                majors: 5,
            },
        },
        RackItem::Knob {
            id: "drive",
            legend: "Drive",
            x: 702.0,
            y: ROW_B,
            d: 40.0,
            ring: Ring::Plain { majors: 5 },
        },
        RackItem::Knob {
            id: "trim",
            legend: "Trim",
            x: 112.0,
            y: ROW_B,
            d: 40.0,
            ring: Ring::Linear {
                from: -24.0,
                to: 24.0,
                majors: 5,
            },
        },
    ],
};

// ─────────────────────────────────────────────────────────────────────────
// API 550A
// ─────────────────────────────────────────────────────────────────────────

/// Three bands, stepped frequencies, coarse gain steps. The coarseness is the
/// design: a console EQ you set rather than dial.
pub static API_550A: RackDesign = RackDesign {
    id: "eq_api_550a",
    w: W,
    h: H,
    paint: "linear-gradient(178deg, #b9bcc0 0%, #a0a4a9 50%, #86898e 100%)",
    ink: "#23262a",
    dim_ink: "#565b61",
    chrome: "#7e8288",
    vu: VuFace::Amber,
    knob: KnobStyle::Bakelite,
    items: &[
        RackItem::Text {
            x: 450.0,
            y: 44.0,
            text: "Discrete Equalizer",
            size: 15.0,
            strong: true,
        },
        RackItem::Text {
            x: 450.0,
            y: 68.0,
            text: "FTS EQ · Proportional Q",
            size: 9.0,
            strong: false,
        },
        RackItem::Knob {
            id: "low_freq",
            legend: "Low Freq",
            x: 168.0,
            y: ROW_A,
            d: 52.0,
            ring: Ring::Detents(&["50", "100", "200", "400"]),
        },
        RackItem::Knob {
            id: "mid_freq",
            legend: "Mid Freq",
            x: 380.0,
            y: ROW_A,
            d: 52.0,
            ring: Ring::Detents(&["400", "800", "1.5k", "3k", "5k"]),
        },
        RackItem::Knob {
            id: "high_freq",
            legend: "High Freq",
            x: 592.0,
            y: ROW_A,
            d: 52.0,
            ring: Ring::Detents(&["5k", "7k", "10k", "12.5k", "15k"]),
        },
        RackItem::Switch {
            id: "eq_in",
            legend: "EQ",
            x: 800.0,
            y: ROW_A,
            labels: ["Out", "In"],
        },
        RackItem::Knob {
            id: "low_gain",
            legend: "Low",
            x: 168.0,
            y: ROW_B,
            d: 52.0,
            ring: Ring::Linear {
                from: -12.0,
                to: 12.0,
                majors: 7,
            },
        },
        RackItem::Knob {
            id: "mid_gain",
            legend: "Mid",
            x: 380.0,
            y: ROW_B,
            d: 52.0,
            ring: Ring::Linear {
                from: -12.0,
                to: 12.0,
                majors: 7,
            },
        },
        RackItem::Knob {
            id: "high_gain",
            legend: "High",
            x: 592.0,
            y: ROW_B,
            d: 52.0,
            ring: Ring::Linear {
                from: -12.0,
                to: 12.0,
                majors: 7,
            },
        },
        RackItem::Knob {
            id: "drive",
            legend: "Drive",
            x: 730.0,
            y: ROW_B,
            d: 42.0,
            ring: Ring::Plain { majors: 5 },
        },
        RackItem::Knob {
            id: "trim",
            legend: "Trim",
            x: 828.0,
            y: ROW_B,
            d: 42.0,
            ring: Ring::Linear {
                from: -24.0,
                to: 24.0,
                majors: 5,
            },
        },
    ],
};

// ─────────────────────────────────────────────────────────────────────────
// Neve 1073
// ─────────────────────────────────────────────────────────────────────────

/// The console channel: a fixed 12 kHz top, a stepped mid, a stepped low
/// shelf, a stepped high-pass. Four decisions and no continuous frequency
/// anywhere, which is why it is fast to use and hard to make sound bad.
pub static NEVE_1073: RackDesign = RackDesign {
    id: "eq_neve_1073",
    w: W,
    h: H,
    paint: "linear-gradient(178deg, #3a3c40 0%, #2a2c30 50%, #1e2023 100%)",
    ink: "#ece8e0",
    dim_ink: "#9d9a94",
    chrome: "#9fa3a8",
    vu: VuFace::Amber,
    knob: KnobStyle::Bakelite,
    items: &[
        RackItem::Text {
            x: 450.0,
            y: 44.0,
            text: "Channel Amplifier",
            size: 15.0,
            strong: true,
        },
        RackItem::Text {
            x: 450.0,
            y: 68.0,
            text: "FTS EQ · Class A",
            size: 9.0,
            strong: false,
        },
        RackItem::Knob {
            id: "high_gain",
            legend: "High 12k",
            x: 150.0,
            y: ROW_A,
            d: 52.0,
            ring: Ring::Linear {
                from: -16.0,
                to: 16.0,
                majors: 5,
            },
        },
        RackItem::Knob {
            id: "mid_freq",
            legend: "Mid Freq",
            x: 330.0,
            y: ROW_A,
            d: 52.0,
            ring: Ring::Detents(&["Off", "360", "700", "1.6k", "3.2k", "4.8k", "7.2k"]),
        },
        RackItem::Knob {
            id: "mid_gain",
            legend: "Mid",
            x: 510.0,
            y: ROW_A,
            d: 52.0,
            ring: Ring::Linear {
                from: -16.0,
                to: 16.0,
                majors: 5,
            },
        },
        RackItem::Knob {
            id: "drive",
            legend: "Drive",
            x: 676.0,
            y: ROW_A,
            d: 46.0,
            ring: Ring::Plain { majors: 5 },
        },
        RackItem::Switch {
            id: "eq_in",
            legend: "EQ",
            x: 800.0,
            y: ROW_A,
            labels: ["Out", "In"],
        },
        RackItem::Knob {
            id: "low_freq",
            legend: "Low Freq",
            x: 150.0,
            y: ROW_B,
            d: 52.0,
            ring: Ring::Detents(&["Off", "35", "60", "110", "220"]),
        },
        RackItem::Knob {
            id: "low_gain",
            legend: "Low",
            x: 330.0,
            y: ROW_B,
            d: 52.0,
            ring: Ring::Linear {
                from: -16.0,
                to: 16.0,
                majors: 5,
            },
        },
        RackItem::Knob {
            id: "hpf",
            legend: "High Pass",
            x: 510.0,
            y: ROW_B,
            d: 52.0,
            ring: Ring::Detents(&["Off", "50", "80", "160", "300"]),
        },
        RackItem::Knob {
            id: "trim",
            legend: "Trim",
            x: 676.0,
            y: ROW_B,
            d: 46.0,
            ring: Ring::Plain { majors: 5 },
        },
        RackItem::Switch {
            id: "phase",
            legend: "Phase",
            x: 800.0,
            y: ROW_B,
            labels: ["Norm", "Ø"],
        },
    ],
};

/// The panel for a `model` value, if that model has one.
pub fn design_for(model: i32) -> Option<&'static RackDesign> {
    Some(match model {
        1 => &PULTEC,
        2 => &NEVE_1073,
        3 => &API_550A,
        4 | 5 => &SSL,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::faces::params_map::control_ptr;
    use crate::params::FtsEqParams;

    /// Every control a panel places has to resolve to a parameter for that
    /// model — otherwise the knob mounts and does nothing, which is the one
    /// failure a screenshot will not show you. (`EqRackFace` panics on a miss;
    /// this is what makes that panic a test failure instead of a crash in a
    /// DAW.)
    #[test]
    fn every_placed_control_resolves_to_a_parameter() {
        let params = FtsEqParams::default();
        for model in 1..=5 {
            let Some(design) = design_for(model) else {
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
                    control_ptr(&params, model, id).is_some(),
                    "model {model} places {id}, which resolves to no parameter",
                );
            }
        }
    }

    /// Every model the parameter can hold has a panel.
    #[test]
    fn every_hardware_model_has_a_face() {
        for model in 1..=5 {
            assert!(design_for(model).is_some(), "model {model} has no panel");
        }
        assert!(design_for(0).is_none(), "the curve editor is not a panel");
    }

    /// Nothing may hang off the panel: the rack ears own the outer 26 px.
    #[test]
    fn nothing_is_placed_under_the_rack_ears() {
        const EAR: f64 = 26.0;
        for model in 1..=5 {
            let Some(design) = design_for(model) else {
                continue;
            };
            for item in design.items {
                let (x, half) = match item {
                    RackItem::Knob { x, d, .. } => (*x, *d),
                    RackItem::Buttons { x, .. } => (*x, 45.0),
                    RackItem::Switch { x, .. } => (*x, 55.0),
                    _ => continue,
                };
                assert!(
                    x - half >= EAR && x + half <= design.w - EAR,
                    "model {model}: an item at x={x} (half-width {half}) runs under a rack ear",
                );
            }
        }
    }

    /// SSL E and G are one panel driven by two model values, so both must
    /// resolve every control on it — the shared parameter set is the point.
    #[test]
    fn both_ssl_variants_drive_the_same_panel() {
        assert_eq!(design_for(4).unwrap().id, design_for(5).unwrap().id);
    }
}
