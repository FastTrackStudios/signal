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

use fts_ui_audio::hardware::panel::{PanelEnds, PanelTexture};
use fts_ui_audio::hardware::vu_svg::VuScale;
use fts_ui_audio::hardware::knob::KnobStyle;
use fts_ui_audio::hardware::rack::{FilterGlyph, RackDesign, RackItem, Ring};
use fts_ui_audio::hardware::vu::VuFace;

/// Panel drawing size shared by the EQ faces. Taller than the compressor's
/// because these units are 2–3U and carry two rows of controls; the ratio is
/// the Pultec's own 2.72:1.
pub const W: f64 = 900.0;
pub const H: f64 = 331.0;

/// The SSL is a channel strip: wider and shorter than the outboard faces, and
/// the ratio is the unit's own.
const SSL_W: f64 = 1060.0;
const SSL_H: f64 = 296.0;

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
    vu_bezel: false,
    vu_frame: None,
    vu_card: VuScale::Vu,
    ends: PanelEnds::RackEars,
    texture: PanelTexture::Painted,
    // Daka-Ware phenolic, as the unit wears: a ridged 1⅛" body on a 1½" skirt,
    // with the index engraved into the body and filled white.
    knob: KnobStyle::Daka,
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
            tint: None,
            style: None,
        },
        RackItem::Readout { id: "low_atten", x: 349.0, y: 22.0 },
        RackItem::Knob {
            id: "low_atten",
            legend: "Atten",
            x: 349.0,
            y: 98.0,
            d: 96.0,
            ring: Ring::Numerals(&["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10"]),
            tint: None,
            style: None,
        },
        RackItem::Readout { id: "high_boost", x: 553.0, y: 22.0 },
        RackItem::Knob {
            id: "high_boost",
            legend: "Boost",
            x: 553.0,
            y: 98.0,
            d: 96.0,
            ring: Ring::Numerals(&["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10"]),
            tint: None,
            style: None,
        },
        RackItem::Readout { id: "high_atten", x: 688.0, y: 22.0 },
        RackItem::Knob {
            id: "high_atten",
            legend: "Atten",
            x: 688.0,
            y: 98.0,
            d: 96.0,
            ring: Ring::Numerals(&["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10"]),
            tint: None,
            style: None,
        },
        // The high-attenuation frequency: a small knob at the top right.
        RackItem::Knob {
            id: "high_atten_freq",
            legend: "Atten Sel",
            x: 810.0,
            y: 94.0,
            d: 42.0,
            ring: Ring::Numerals(&["5", "10", "20"]),
            tint: None,
            style: Some(KnobStyle::Pointer),
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
            tint: None,
            style: None,
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
            tint: None,
            style: Some(KnobStyle::Pointer),
        },
        RackItem::Readout { id: "trim", x: 810.0, y: 196.0 },
        RackItem::Knob {
            id: "trim",
            legend: "Output",
            x: 810.0,
            y: 240.0,
            d: 46.0,
            ring: Ring::Numerals(&["0", "2", "4", "6", "8", "10"]),
            tint: None,
            style: Some(KnobStyle::Pointer),
        },
        // ── Panel marks ──────────────────────────────────────────────────
        RackItem::Text {
            x: 108.0,
            y: 302.0,
            text: "Program Equalizer",
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

/// The console channel EQ, laid out from the unit's own panel.
///
/// Sections left to right — filters, LF, LMF, HMF, HF, then the switching and
/// the output metering — divided by hairlines, exactly as the strip is. Two
/// rows: gains and switching up top, frequencies and Qs beneath, so a band
/// reads as a column.
///
/// The band colours are not decoration. A console is operated by reaching for
/// "the blue one" without reading anything, and the E and G both colour-code
/// LMF blue, HMF green and HF magenta.
///
/// **Not yet wired.** The panel carries controls this plugin has no parameter
/// for — the LMF and HMF Q knobs, the ÷3 and ×3 range switches, FLTR IN, the
/// phase invert, ANALOG. They draw and do nothing, on purpose: the panel is
/// the specification for what the DSP still owes, and hiding them would hide
/// the debt. Everything with an id behind it works.
///
/// E and G share this panel and these parameters; the model value picks the
/// curves, which is the whole difference between them.
pub static SSL: RackDesign = RackDesign {
    id: "eq_ssl_e",
    w: SSL_W,
    h: SSL_H,
    paint: "linear-gradient(178deg, #34373b 0%, #26282c 46%, #1b1d20 100%)",
    ink: "#eceef0",
    dim_ink: "#9aa0a6",
    chrome: "#8f9398",
    vu: VuFace::Blue,
    vu_bezel: false,
    vu_frame: None,
    vu_card: VuScale::Vu,
    ends: PanelEnds::RackEars,
    texture: PanelTexture::Painted,
    // Collet caps: flat top, fluted rim, one white bar. The panel prints the
    // travel as dots around them rather than the knob carrying a skirt.
    knob: KnobStyle::Collet,
    items: &[
        // ── Filters ──────────────────────────────────────────────────────
        RackItem::Text { x: 66.0, y: 22.0, text: "Analog", size: 9.0, strong: false },
        RackItem::Text { x: 46.0, y: 118.0, text: "Off", size: 8.0, strong: false },
        RackItem::Button {
            id: "",
            label: "ON",
            x: 46.0,
            y: 62.0,
            color: "#1c1e21",
            ink: "#e8eaec",
            led: "#43d17a",
        },
        RackItem::Text { x: 128.0, y: 24.0, text: "HP", size: 11.0, strong: true },
        RackItem::Button {
            id: "",
            label: "FLTR IN",
            x: 152.0,
            y: 62.0,
            color: "#e6e2d4",
            ink: "#23252a",
            led: "#43d17a",
        },
        RackItem::Knob {
            id: "hpf",
            legend: "Hz",
            x: 86.0,
            y: 182.0,
            d: 42.0,
            ring: Ring::Dots(&["16", "20", "70", "120", "200", "300", "350"]),
            tint: Some("#d8d4c6"),
            style: None,
        },
        RackItem::Knob {
            id: "lpf",
            legend: "kHz",
            x: 172.0,
            y: 182.0,
            d: 42.0,
            ring: Ring::Dots(&["3", "5", "8", "12", "16", "20", "22"]),
            tint: Some("#d8d4c6"),
            style: None,
        },
        RackItem::Divider { x: 218.0, y: 140.0, h: 236.0 },

        // ── LF ───────────────────────────────────────────────────────────
        RackItem::Text { x: 268.0, y: 24.0, text: "LF", size: 11.0, strong: true },
        RackItem::Knob {
            id: "lf_gain",
            legend: "",
            x: 288.0,
            y: 74.0,
            d: 44.0,
            ring: Ring::Dots(&["-", "", "", "", "", "0", "", "", "", "", "+"]),
            tint: Some("#2b2d31"),
            style: None,
        },
        RackItem::Knob {
            id: "lf_freq",
            legend: "Hz",
            x: 288.0,
            y: 182.0,
            d: 42.0,
            ring: Ring::Dots(&["30", "50", "100", "200", "300", "450"]),
            tint: Some("#2b2d31"),
            style: None,
        },
        RackItem::Divider { x: 340.0, y: 140.0, h: 236.0 },

        // ── LMF ──────────────────────────────────────────────────────────
        RackItem::Text { x: 392.0, y: 24.0, text: "LMF", size: 11.0, strong: true },
        RackItem::Knob {
            id: "lmf_gain",
            legend: "",
            x: 396.0,
            y: 74.0,
            d: 44.0,
            ring: Ring::Dots(&["-", "", "", "", "", "0", "", "", "", "", "+"]),
            tint: Some("#2b7fc0"),
            style: None,
        },
        RackItem::Button {
            id: "",
            label: "/3",
            x: 462.0,
            y: 68.0,
            color: "#e6e2d4",
            ink: "#1f5f96",
            led: "#43d17a",
        },
        RackItem::Knob {
            id: "",
            legend: "",
            x: 380.0,
            y: 186.0,
            d: 40.0,
            ring: Ring::Dots(&["3", "2", "1.5", "1", ".5"]),
            tint: Some("#2b7fc0"),
            style: None,
        },
        RackItem::Knob {
            id: "lmf_freq",
            legend: "kHz",
            x: 470.0,
            y: 186.0,
            d: 40.0,
            ring: Ring::Dots(&[".2", ".3", ".8", "1", "1.5", "2", "2.5"]),
            tint: Some("#2b7fc0"),
            style: None,
        },
        RackItem::Divider { x: 516.0, y: 140.0, h: 236.0 },

        // ── HMF ──────────────────────────────────────────────────────────
        RackItem::Text { x: 578.0, y: 24.0, text: "HMF", size: 11.0, strong: true },
        RackItem::Button {
            id: "",
            label: "×3",
            x: 552.0,
            y: 68.0,
            color: "#e6e2d4",
            ink: "#1d6b46",
            led: "#43d17a",
        },
        RackItem::Knob {
            id: "hmf_gain",
            legend: "",
            x: 622.0,
            y: 74.0,
            d: 44.0,
            ring: Ring::Dots(&["-", "", "", "", "", "0", "", "", "", "", "+"]),
            tint: Some("#2c8f5a"),
            style: None,
        },
        RackItem::Knob {
            id: "",
            legend: "",
            x: 566.0,
            y: 186.0,
            d: 40.0,
            ring: Ring::Dots(&["3", "2", "1.5", "1", ".5"]),
            tint: Some("#2c8f5a"),
            style: None,
        },
        RackItem::Knob {
            id: "hmf_freq",
            legend: "kHz",
            x: 656.0,
            y: 186.0,
            d: 40.0,
            ring: Ring::Dots(&[".6", ".8", "1.5", "3", "4.5", "6", "7"]),
            tint: Some("#2c8f5a"),
            style: None,
        },
        RackItem::Divider { x: 702.0, y: 140.0, h: 236.0 },

        // ── HF ───────────────────────────────────────────────────────────
        RackItem::Text { x: 742.0, y: 24.0, text: "HF", size: 11.0, strong: true },
        RackItem::Knob {
            id: "hf_gain",
            legend: "",
            x: 752.0,
            y: 74.0,
            d: 44.0,
            ring: Ring::Dots(&["-", "", "", "", "", "0", "", "", "", "", "+"]),
            tint: Some("#a8438f"),
            style: None,
        },
        RackItem::Knob {
            id: "hf_freq",
            legend: "kHz",
            x: 752.0,
            y: 186.0,
            d: 42.0,
            ring: Ring::Dots(&["1.5", "2", "5", "8", "10", "14", "16"]),
            tint: Some("#a8438f"),
            style: None,
        },
        RackItem::Divider { x: 800.0, y: 140.0, h: 236.0 },

        // ── Switching + output ───────────────────────────────────────────
        RackItem::Button {
            id: "eq_in",
            label: "EQ IN",
            x: 838.0,
            y: 68.0,
            color: "#e6e2d4",
            ink: "#23252a",
            led: "#43d17a",
        },
        RackItem::Button {
            id: "",
            label: "Ø",
            x: 906.0,
            y: 68.0,
            color: "#c2382c",
            ink: "#f4e9e7",
            led: "#e0483a",
        },
        RackItem::Knob {
            id: "trim",
            legend: "Output",
            x: 872.0,
            y: 186.0,
            d: 42.0,
            ring: Ring::Dots(&["-24", "", "", "0", "", "", "+12"]),
            tint: Some("#d8d4c6"),
            style: None,
        },
        RackItem::Knob {
            id: "drive",
            legend: "Drive",
            x: 946.0,
            y: 186.0,
            d: 34.0,
            ring: Ring::Dots(&["", "", "", "", ""]),
            tint: Some("#d8d4c6"),
            style: None,
        },
        RackItem::Text { x: 978.0, y: 48.0, text: "0", size: 7.0, strong: false },
        RackItem::Text { x: 978.0, y: 84.0, text: "-10", size: 7.0, strong: false },
        RackItem::Text { x: 978.0, y: 132.0, text: "-20", size: 7.0, strong: false },
        RackItem::Text { x: 978.0, y: 186.0, text: "-30", size: 7.0, strong: false },
        RackItem::Text { x: 978.0, y: 222.0, text: "-40", size: 7.0, strong: false },
        RackItem::LedMeter { x: 1006.0, y: 132.0, h: 190.0, right: false },
        RackItem::LedMeter { x: 1032.0, y: 132.0, h: 190.0, right: true },

        RackItem::Text { x: 520.0, y: 268.0, text: "Console", size: 9.0, strong: false },
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
    vu_bezel: false,
    vu_frame: None,
    vu_card: VuScale::Vu,
    ends: PanelEnds::RackEars,
    texture: PanelTexture::Painted,
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
            text: "Proportional Q",
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
            tint: None,
            style: None,
        },
        RackItem::Knob {
            id: "mid_freq",
            legend: "Mid Freq",
            x: 380.0,
            y: ROW_A,
            d: 52.0,
            ring: Ring::Detents(&["400", "800", "1.5k", "3k", "5k"]),
            tint: None,
            style: None,
        },
        RackItem::Knob {
            id: "high_freq",
            legend: "High Freq",
            x: 592.0,
            y: ROW_A,
            d: 52.0,
            ring: Ring::Detents(&["5k", "7k", "10k", "12.5k", "15k"]),
            tint: None,
            style: None,
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
            tint: None,
            style: None,
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
            tint: None,
            style: None,
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
            tint: None,
            style: None,
        },
        RackItem::Knob {
            id: "drive",
            legend: "Drive",
            x: 730.0,
            y: ROW_B,
            d: 42.0,
            ring: Ring::Plain { majors: 5 },
            tint: None,
            style: None,
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
            tint: None,
            style: None,
        },
    ],
};

// ─────────────────────────────────────────────────────────────────────────
// Neve 1073
// ─────────────────────────────────────────────────────────────────────────

/// The console channel: a fixed 12 kHz top, a stepped mid, a stepped low
/// shelf, a stepped high-pass. Four decisions and no continuous frequency
/// anywhere, which is why it is fast to use and hard to make sound bad.
/// The 1073's own panel proportion. The module is a tall channel strip, but
/// its controls are one row along its length — laid out here the way the unit
/// is photographed and the way it reads on a desk: wide, short, one row.
const NEVE_W: f64 = 1240.0;
const NEVE_H: f64 = 300.0;

/// Where the 1073's single row of controls sits, and the arc of its glyphs.
const NEVE_ROW: f64 = 170.0;
const NEVE_GLYPH_Y: f64 = 74.0;

/// The 1073, laid out from the module's own front: one row, a red gain switch
/// at the left, the bands' knobs each inside a printed ring of dots with the
/// filter's *shape* drawn above it, and the latching buttons stacked on a pale
/// strip at the right.
///
/// The details that are the unit rather than decoration:
///
/// - The rings are **dots**, not ticks. It is the first thing you see of a
///   1073 from across a room and the reason its knobs read as haloed.
/// - Neve prints the band's **shape** over the control instead of naming it,
///   which is why the module is readable with no English on it at all.
/// - The gain switch is red, the high-pass blue, the bands grey. The colour
///   is the index — you find the control you want before reading anything.
/// - The frequencies are the module's own stepped values (0·36–7·2 kHz mid,
///   35–220 Hz low, 50–300 Hz high-pass), because a 1073 does not sweep.
pub static NEVE_1073: RackDesign = RackDesign {
    id: "eq_neve_1073",
    w: NEVE_W,
    h: NEVE_H,
    // Near-black with the blue in it that the unit's paint has under light.
    paint: "linear-gradient(178deg, #2c3139 0%, #232830 54%, #191d24 100%)",
    ink: "#eef1f4",
    dim_ink: "#9aa1aa",
    chrome: "#aeb4ba",
    vu: VuFace::Amber,
    vu_bezel: false,
    vu_frame: None,
    vu_card: VuScale::Vu,
    ends: PanelEnds::RackEars,
    texture: PanelTexture::Painted,
    // Moulded caps with a dark index, read against the printed dots.
    knob: KnobStyle::Neve,
    items: &[
        // ── The gain switch: red, and the biggest thing on the panel ──────
        RackItem::Knob {
            id: "drive",
            legend: "dB",
            x: 140.0,
            y: NEVE_ROW,
            d: 84.0,
            ring: Ring::Dots(&["0", "2", "4", "6", "8", "10"]),
            // Neve red, and a wing rather than a cap: the gain switch is the
            // one control on the module you find without looking.
            tint: Some("#9c1f27"),
            style: Some(KnobStyle::Marconi),
        },
        // ── High shelf: fixed at 12 kHz, so gain alone ───────────────────
        RackItem::Glyph {
            x: 322.0,
            y: NEVE_GLYPH_Y,
            shape: FilterGlyph::HighShelf,
            w: 30.0,
        },
        RackItem::Knob {
            id: "high_gain",
            legend: "12 kHz",
            x: 322.0,
            y: NEVE_ROW,
            d: 84.0,
            ring: Ring::Dots(&["-16", "-8", "0", "+8", "+16"]),
            tint: Some("#7f858c"),
            style: None,
        },
        // ── Mid: one control. Collar picks the frequency, cap sets the
        //    gain — which is how the module is built and why its panel has
        //    room for a whole EQ in one row.
        RackItem::Glyph {
            x: 504.0,
            y: NEVE_GLYPH_Y,
            shape: FilterGlyph::Bell,
            w: 30.0,
        },
        RackItem::Concentric {
            outer_id: "mid_freq",
            inner_id: "mid_gain",
            legend: "KHz",
            x: 504.0,
            y: NEVE_ROW,
            d: 84.0,
            ring: Ring::Dots(&["Off", "0·36", "0·7", "1·6", "3·2", "4·8", "7·2"]),
            tint: Some("#7f858c"),
        },
        // The cap's direction, printed where the module prints it: the inner
        // knob boosts one way and cuts the other.
        RackItem::Text { x: 452.0, y: 104.0, text: "+", size: 11.0, strong: true },
        RackItem::Text { x: 556.0, y: 104.0, text: "−", size: 11.0, strong: true },
        // ── Low shelf: the same pair ─────────────────────────────────────
        RackItem::Glyph {
            x: 686.0,
            y: NEVE_GLYPH_Y,
            shape: FilterGlyph::LowShelf,
            w: 30.0,
        },
        RackItem::Concentric {
            outer_id: "low_freq",
            inner_id: "low_gain",
            legend: "Hz",
            x: 686.0,
            y: NEVE_ROW,
            d: 84.0,
            ring: Ring::Dots(&["Off", "35", "60", "110", "220"]),
            tint: Some("#7f858c"),
        },
        RackItem::Text { x: 634.0, y: 104.0, text: "+", size: 11.0, strong: true },
        RackItem::Text { x: 738.0, y: 104.0, text: "−", size: 11.0, strong: true },
        // ── High pass: the blue one ──────────────────────────────────────
        RackItem::Glyph {
            x: 868.0,
            y: NEVE_GLYPH_Y,
            shape: FilterGlyph::HighPass,
            w: 30.0,
        },
        RackItem::Knob {
            id: "hpf",
            legend: "Hz",
            x: 868.0,
            y: NEVE_ROW,
            d: 84.0,
            ring: Ring::Dots(&["Off", "50", "80", "160", "300"]),
            // Neve blue, and the same wing as the gain switch — the two
            // coloured controls on the module are the two shaped ones.
            tint: Some("#2b4a6d"),
            style: Some(KnobStyle::Marconi),
        },
        RackItem::Knob {
            id: "trim",
            legend: "Trim",
            x: 1022.0,
            y: NEVE_ROW,
            d: 58.0,
            ring: Ring::Dots(&["-10", "0", "+10"]),
            tint: Some("#7f858c"),
            style: None,
        },
        // ── The pale strip at the right, and what is printed on it ───────
        RackItem::Button {
            id: "phase",
            label: "Phase",
            x: 1160.0,
            y: 104.0,
            color: "#e6e4dc",
            ink: "#20242b",
            led: "#d24a3a",
        },
        RackItem::Button {
            id: "eq_in",
            label: "EQL",
            x: 1160.0,
            y: 204.0,
            color: "#e6e4dc",
            ink: "#20242b",
            led: "#d24a3a",
        },
        RackItem::Text {
            x: 1160.0,
            y: 268.0,
            text: "1073",
            size: 10.0,
            strong: true,
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

    /// Ids placed on a panel, excluding the deliberately unwired ones.
    fn wired_ids(design: &RackDesign) -> Vec<&'static str> {
        design
            .items
            .iter()
            .flat_map(|item| match item {
                RackItem::Knob { id, .. }
                | RackItem::Buttons { id, .. }
                | RackItem::Switch { id, .. }
                | RackItem::Lever { id, .. }
                | RackItem::Button { id, .. } => vec![*id],
                // One placement, two controls.
                RackItem::Concentric { outer_id, inner_id, .. } => vec![*outer_id, *inner_id],
                _ => Vec::new(),
            })
            .filter(|id| !id.is_empty())
            .collect()
    }

    /// How many controls a panel draws with nothing behind them.
    fn unwired_count(design: &RackDesign) -> usize {
        design
            .items
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    RackItem::Knob { id, .. } | RackItem::Button { id, .. } if id.is_empty()
                )
            })
            .count()
    }

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
            for id in wired_ids(design) {
                assert!(
                    control_ptr(&params, model, id).is_some(),
                    "model {model} places {id}, which resolves to no parameter",
                );
            }
        }
    }

    /// The unwired controls are a known, counted debt.
    ///
    /// A panel may draw a control the DSP does not have yet — that is how the
    /// panel states what it still owes — but it may not do so by accident. A
    /// mistyped id would otherwise land here silently and read as "not wired
    /// yet" forever, so the number is pinned and moving it is a deliberate
    /// edit.
    #[test]
    fn the_unwired_controls_are_the_ones_we_know_about() {
        // SSL: ANALOG, FLTR IN, ÷3, ×3, phase, and the two mid-band Q knobs.
        assert_eq!(unwired_count(design_for(4).unwrap()), 7);
        assert_eq!(unwired_count(design_for(5).unwrap()), 7, "E and G share a panel");
        // Every other model is fully wired.
        for model in [1, 2, 3] {
            assert_eq!(
                unwired_count(design_for(model).unwrap()),
                0,
                "model {model} grew an unwired control",
            );
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
                    RackItem::Knob { x, d, .. }
                    | RackItem::Concentric { x, d, .. } => (*x, *d),
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

    /// The 1073's kit and its wiring, which together are the module:
    ///
    /// - the two *coloured* controls are wing knobs, and a wing knob has to
    ///   carry a colour, because on this panel the wing IS the colour;
    /// - the swept bands are **concentric** — one placement carrying the
    ///   band's frequency on the collar and its gain on the cap, which is
    ///   what lets a whole EQ sit in one row;
    /// - everything else is a collar knob, and nothing wears a part from
    ///   another console.
    #[test]
    fn the_1073_wears_wings_on_its_coloured_controls_and_pairs_its_swept_bands() {
        let mut wings = Vec::new();
        let mut collars = Vec::new();
        let mut pairs = Vec::new();
        for item in NEVE_1073.items {
            match item {
                RackItem::Knob { id, style, tint, .. } => {
                    match style.unwrap_or(NEVE_1073.knob) {
                        KnobStyle::Marconi => {
                            assert!(
                                tint.is_some(),
                                "{id} is a wing knob with no colour — the wing IS the colour",
                            );
                            wings.push(*id);
                        }
                        KnobStyle::Neve => collars.push(*id),
                        other => panic!("{id} wears {other:?}, which is not 1073 kit"),
                    }
                }
                RackItem::Concentric { outer_id, inner_id, .. } => {
                    pairs.push((*outer_id, *inner_id));
                }
                _ => {}
            }
        }
        assert_eq!(wings, vec!["drive", "hpf"], "wrong controls wear wings");
        assert_eq!(
            collars,
            vec!["high_gain", "trim"],
            "wrong controls wear collars",
        );
        // Frequency on the collar, gain on the cap — never the other way
        // round: the collar is the one with detents, and the printed ring is
        // its scale.
        assert_eq!(
            pairs,
            vec![("mid_freq", "mid_gain"), ("low_freq", "low_gain")],
            "the swept bands are not paired the way the module pairs them",
        );
    }

    /// SSL E and G are one panel driven by two model values, so both must
    /// resolve every control on it — the shared parameter set is the point.
    #[test]
    fn both_ssl_variants_drive_the_same_panel() {
        assert_eq!(design_for(4).unwrap().id, design_for(5).unwrap().id);
    }
}
