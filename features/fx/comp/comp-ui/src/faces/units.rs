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
    vu: VuFace::Ivory,
    vu_bezel: false,
    // Documented: black plastic bodies with brushed silver tops and
    // clear plastic collars — the large pair for INPUT and OUTPUT, the
    // smaller for ATTACK and RELEASE.
    knob: KnobStyle::SilverTop,
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
            style: None,
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
            style: None,
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
            style: None,
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
            style: None,
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

/// The LA-2A, laid out from the unit.
///
/// The panel is **grey**, not cream — the cream is the meter card behind its
/// bezel, and mistaking one for the other is how every LA-2A pastiche gives
/// itself away. Red Teletronix script at the left, the movement centred in a
/// heavy bezel, and the two big pointer knobs either side of it with their
/// scales printed **0–100** on the panel.
///
/// The knobs are plain black with a moulded nose: you read the nose against
/// the panel, which is why there is no skirt and nothing printed on the knob.
///
/// **Not yet wired**: the meter-mode selector (gain reduction / +10 / +4) and
/// POWER. They draw and do nothing.
pub static LA2A: RackDesign = RackDesign {
    id: "la2a",
    w: 940.0,
    h: 254.0,
    // Brushed grey panel, lit from above.
    paint: "linear-gradient(178deg, #cfd0cd 0%, #bcbdba 46%, #a7a8a5 100%)",
    ink: "#2e2f2d",
    dim_ink: "#6a6b68",
    chrome: "#b9bab6",
    vu: VuFace::Amber,
    vu_bezel: false,
    knob: KnobStyle::Pointer,
    items: &[
        // ── Identity ─────────────────────────────────────────────────────
        RackItem::Text { x: 168.0, y: 62.0, text: "FTS", size: 17.0, strong: true },
        RackItem::Text { x: 168.0, y: 84.0, text: "Audio", size: 7.5, strong: false },
        RackItem::Text { x: 296.0, y: 64.0, text: "Leveling Amplifier", size: 8.5, strong: false },
        RackItem::Text { x: 296.0, y: 82.0, text: "Optical · Tube", size: 8.5, strong: false },

        // ── Mode, far left, as the unit has it ───────────────────────────
        RackItem::Switch {
            id: "mode",
            legend: "",
            x: 104.0,
            y: 150.0,
            labels: ["Limit", "Compress"],
        },

        // ── Gain ─────────────────────────────────────────────────────────
        RackItem::Knob {
            id: "gain",
            legend: "Gain",
            x: 286.0,
            y: 150.0,
            d: 68.0,
            ring: Ring::Numerals(&[
                "0", "10", "20", "30", "40", "50", "60", "70", "80", "90", "100",
            ]),
            tint: None,
            style: None,
        },

        // ── The movement ─────────────────────────────────────────────────
        RackItem::Vu {
            x: 478.0,
            y: 122.0,
            w: 198.0,
            mode: VuMode::GainReduction,
            legend: "VU Level Indicator",
        },

        // ── Peak reduction ───────────────────────────────────────────────
        RackItem::Knob {
            id: "peak_reduction",
            legend: "Peak Reduction",
            x: 660.0,
            y: 150.0,
            d: 68.0,
            ring: Ring::Numerals(&[
                "0", "10", "20", "30", "40", "50", "60", "70", "80", "90", "100",
            ]),
            tint: None,
            style: None,
        },

        // ── Meter mode + power, top right ────────────────────────────────
        RackItem::Text { x: 830.0, y: 44.0, text: "Gain Reduction", size: 7.5, strong: false },
        RackItem::Text { x: 776.0, y: 62.0, text: "Output +10", size: 7.0, strong: false },
        RackItem::Text { x: 884.0, y: 62.0, text: "Output +4", size: 7.0, strong: false },
        RackItem::Knob {
            id: "",
            legend: "",
            x: 830.0,
            y: 106.0,
            d: 42.0,
            ring: Ring::None,
            tint: None,
            style: None,
        },
        RackItem::Switch {
            id: "",
            legend: "",
            x: 830.0,
            y: 190.0,
            labels: ["On", "Power"],
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
    vu: VuFace::Ivory,
    vu_bezel: false,
    // Five large plain black knobs on the blue face — no skirt, no
    // flutes, which is what makes the CL 1B look modern beside an LA-2A.
    knob: KnobStyle::Skirted,
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
            style: None,
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
            style: None,
        },
        RackItem::Knob {
            id: "attack",
            legend: "Attack",
            x: 574.0,
            y: ROW,
            d: 54.0,
            ring: Ring::Plain { majors: 6 },
            tint: None,
            style: None,
        },
        RackItem::Knob {
            id: "release",
            legend: "Release",
            x: 686.0,
            y: ROW,
            d: 54.0,
            ring: Ring::Plain { majors: 6 },
            tint: None,
            style: None,
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
            style: None,
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

/// The valve limiter, laid out from the unit.
///
/// Black panel with white silkscreen and the channel labels picked out in
/// red — the 670 is a two-channel unit and the colour is how you keep track
/// of which half you are touching. Both movements are mounted through the
/// panel in bezels, stacked at the left, amber-lit.
///
/// **One channel is drawn, not two.** The unit has a full set of controls per
/// channel; this plugin has one set. Drawing a second row that moved the same
/// parameters would look right and lie, so the panel carries Channel A and
/// says so.
///
/// The TIME CONSTANT switch is the unit's personality: six positions pairing
/// an attack with a release, the last two program-dependent — which is where
/// the glue comes from, and why it drives inertia rather than just release.
///
/// **Not yet wired**: the meter selector (bypass / VU / GR / balance) and the
/// M-S link. They draw and do nothing.
pub static FAIRCHILD_670: RackDesign = RackDesign {
    id: "fairchild670",
    w: 960.0,
    h: 330.0,
    paint: "linear-gradient(178deg, #232426 0%, #191a1c 48%, #0f1012 100%)",
    ink: "#eceef0",
    dim_ink: "#9aa0a6",
    chrome: "#a9adb2",
    vu: VuFace::Amber,
    vu_bezel: true,
    knob: KnobStyle::Pointer,
    items: &[
        // ── Power, far left ──────────────────────────────────────────────
        RackItem::Text { x: 92.0, y: 40.0, text: "On", size: 8.0, strong: false },
        RackItem::Switch { id: "", legend: "", x: 92.0, y: 84.0, labels: ["On", "Off"] },
        RackItem::Lamp { x: 92.0, y: 176.0, color: "#e0483a" },

        // ── The two movements ────────────────────────────────────────────
        RackItem::Vu {
            x: 236.0,
            y: 96.0,
            w: 150.0,
            mode: VuMode::GainReduction,
            legend: "Gain Reduction",
        },
        RackItem::Vu {
            x: 236.0,
            y: 236.0,
            w: 150.0,
            mode: VuMode::Level,
            legend: "Output",
        },

        // ── Channel A ────────────────────────────────────────────────────
        RackItem::Text { x: 388.0, y: 34.0, text: "Channel A", size: 8.5, strong: true },
        RackItem::TintedText { x: 470.0, y: 34.0, text: "Left / M", size: 8.0, color: "#d8483a" },

        RackItem::Text { x: 358.0, y: 56.0, text: "Meter", size: 7.5, strong: false },
        RackItem::Knob {
            id: "",
            legend: "",
            x: 358.0,
            y: 124.0,
            d: 46.0,
            ring: Ring::Numerals(&["Byp", "VU", "GR", "Bal"]),
            tint: None,
            style: None,
        },
        RackItem::Text { x: 500.0, y: 56.0, text: "Input Gain", size: 7.5, strong: false },
        RackItem::Knob {
            id: "input_gain",
            legend: "",
            x: 500.0,
            y: 124.0,
            d: 60.0,
            ring: Ring::Numerals(&["0", "4", "8", "12", "16", "20"]),
            tint: None,
            style: None,
        },
        RackItem::Text { x: 646.0, y: 56.0, text: "Threshold", size: 7.5, strong: false },
        RackItem::Knob {
            id: "threshold",
            legend: "",
            x: 646.0,
            y: 124.0,
            d: 60.0,
            ring: Ring::Numerals(&["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"]),
            tint: None,
            style: None,
        },
        RackItem::Text { x: 800.0, y: 56.0, text: "Time Constant", size: 7.5, strong: false },
        RackItem::Knob {
            id: "time_constant",
            legend: "",
            x: 800.0,
            y: 124.0,
            d: 52.0,
            ring: Ring::Numerals(&["1", "2", "3", "4", "5", "6"]),
            tint: None,
            style: None,
        },
        RackItem::Text { x: 898.0, y: 106.0, text: "Var", size: 7.0, strong: false },

        // ── Output, and the link the unit switches its channels with ─────
        RackItem::Text { x: 500.0, y: 216.0, text: "Output", size: 7.5, strong: false },
        RackItem::Knob {
            id: "output",
            legend: "",
            x: 500.0,
            y: 268.0,
            d: 54.0,
            ring: Ring::Numerals(&["0", "4", "8", "12", "16", "20"]),
            tint: None,
            style: None,
        },
        RackItem::Text { x: 660.0, y: 216.0, text: "M-S · Link · Ind", size: 7.5, strong: false },
        RackItem::Switch { id: "", legend: "", x: 660.0, y: 268.0, labels: ["M-S", "Link"] },

        // ── Panel marks ──────────────────────────────────────────────────
        RackItem::Text { x: 800.0, y: 250.0, text: "Tube Limiter", size: 12.0, strong: true },
        RackItem::Text { x: 800.0, y: 274.0, text: "FTS Comp · Variable-Mu", size: 8.0, strong: false },
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
    vu_bezel: false,
    // Brushed metal, the modern boutique idiom.
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
            style: None,
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
            style: None,
        },
        RackItem::Knob {
            id: "attack",
            legend: "Attack",
            x: 498.0,
            y: ROW,
            d: 50.0,
            ring: Ring::Plain { majors: 6 },
            tint: None,
            style: None,
        },
        RackItem::Knob {
            id: "recovery",
            legend: "Recovery",
            x: 594.0,
            y: ROW,
            d: 50.0,
            ring: Ring::Detents(&["1", "2", "3", "4", "5"]),
            tint: None,
            style: None,
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
            style: None,
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
            style: None,
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
    vu: VuFace::Ivory,
    vu_bezel: false,
    // The console's collet caps: the bus compressor is a centre-section
    // module, so it wears the same knob as the channel strip.
    knob: KnobStyle::Collet,
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
            style: None,
        },
        RackItem::Knob {
            id: "ratio",
            legend: "Ratio",
            x: 470.0,
            y: 150.0,
            d: 54.0,
            ring: Ring::Detents(&["2", "4", "10"]),
            tint: None,
            style: None,
        },
        RackItem::Knob {
            id: "attack",
            legend: "Attack ms",
            x: 580.0,
            y: 150.0,
            d: 54.0,
            ring: Ring::Detents(&["0.1", "0.3", "1", "3", "10", "30"]),
            tint: None,
            style: None,
        },
        RackItem::Knob {
            id: "release",
            legend: "Release s",
            x: 690.0,
            y: 150.0,
            d: 54.0,
            ring: Ring::Detents(&["0.1", "0.3", "0.6", "1.2", "A"]),
            tint: None,
            style: None,
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
            style: None,
        },
        RackItem::Knob {
            id: "mix",
            legend: "Mix",
            x: 838.0,
            y: 76.0,
            d: 34.0,
            ring: Ring::None,
            tint: None,
            style: None,
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
    vu: VuFace::Ivory,
    vu_bezel: false,
    knob: KnobStyle::Skirted,
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
            style: None,
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
            style: None,
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
            style: None,
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

/// The Distressor, laid out from the unit.
///
/// Two bordered sections, as the panel has: everything that *reports* up top —
/// the gain-reduction ladder, the ratio row, the mode buttons — and the four
/// big dials plus MIX below, with their legends printed above them rather than
/// underneath.
///
/// The knobs are the point. A Distressor's numerals are printed on a wide
/// brushed skirt that *turns*, read against a fixed index on the panel; that
/// is why the panel around each knob is bare where every other unit here
/// prints a scale. And it does not have a meter movement at all: gain
/// reduction is an LED ladder reading right to left, 1 dB at the right.
///
/// **Not yet wired**: BYPASS, POWER, the detector Link, and the HR trim. They
/// draw and do nothing — see the note on the SSL face.
pub static DISTRESSOR: RackDesign = RackDesign {
    id: "distressor",
    w: 940.0,
    h: 340.0,
    paint: "linear-gradient(178deg, #26282b 0%, #191b1d 48%, #0f1113 100%)",
    ink: "#e8eaec",
    dim_ink: "#9aa1a8",
    chrome: "#8b9096",
    vu: VuFace::Ivory,
    vu_bezel: false,
    knob: KnobStyle::Dial,
    items: &[
        // ── Upper section: what the unit reports ─────────────────────────
        RackItem::Frame { x: 470.0, y: 96.0, w: 856.0, h: 150.0 },
        RackItem::Text { x: 168.0, y: 96.0, text: "FTS", size: 20.0, strong: true },
        RackItem::Text { x: 168.0, y: 116.0, text: "FTS Comp · Hybrid", size: 8.0, strong: false },

        // Gain reduction: 1 dB at the right, deepest at the left.
        RackItem::LedBar {
            x: 508.0,
            y: 48.0,
            steps: &[26.0, 23.0, 20.0, 17.0, 14.0, 12.0, 10.0, 9.0, 8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0],
            pitch: 27.0,
        },
        RackItem::Text { x: 508.0, y: 86.0, text: "Gain Reduction", size: 8.5, strong: false },

        // Ratio, as the row of lamps it is on the unit — the button beside it
        // steps through them, and so does clicking a lamp.
        RackItem::LedSelect {
            id: "ratio",
            x: 476.0,
            y: 120.0,
            labels: &["1:1", "2:1", "3:1", "4:1", "6:1", "10:1", "20:1", "Nuke"],
            pitch: 54.0,
        },
        RackItem::Text { x: 476.0, y: 156.0, text: "Ratio", size: 8.5, strong: false },
        RackItem::Button {
            id: "",
            label: "BY PASS",
            x: 232.0,
            y: 128.0,
            color: "#8d9298",
            ink: "#15171a",
            led: "#e0483a",
        },
        RackItem::Button {
            id: "ratio",
            label: "RATIO",
            x: 734.0,
            y: 128.0,
            color: "#8d9298",
            ink: "#15171a",
            led: "",
        },
        RackItem::Button {
            id: "detector",
            label: "DET",
            x: 800.0,
            y: 128.0,
            color: "#8d9298",
            ink: "#15171a",
            led: "",
        },
        RackItem::Button {
            id: "audio_mode",
            label: "AUDIO",
            x: 866.0,
            y: 128.0,
            color: "#8d9298",
            ink: "#15171a",
            led: "",
        },
        RackItem::Text { x: 800.0, y: 156.0, text: "Detector", size: 8.0, strong: false },
        RackItem::Text { x: 866.0, y: 156.0, text: "Audio", size: 8.0, strong: false },

        // ── Lower section: the dials ─────────────────────────────────────
        RackItem::Frame { x: 470.0, y: 250.0, w: 856.0, h: 150.0 },
        RackItem::Text { x: 152.0, y: 180.0, text: "Input", size: 9.5, strong: true },
        RackItem::Knob {
            id: "input",
            legend: "",
            x: 152.0,
            y: 254.0,
            d: 92.0,
            ring: Ring::Numerals(&["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10"]),
            tint: None,
            style: None,
        },
        RackItem::Text { x: 320.0, y: 180.0, text: "Attack", size: 9.5, strong: true },
        RackItem::Knob {
            id: "attack",
            legend: "",
            x: 320.0,
            y: 254.0,
            d: 92.0,
            ring: Ring::Numerals(&["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10"]),
            tint: None,
            style: None,
        },
        RackItem::Text { x: 488.0, y: 180.0, text: "Release", size: 9.5, strong: true },
        RackItem::Knob {
            id: "release",
            legend: "",
            x: 488.0,
            y: 254.0,
            d: 92.0,
            ring: Ring::Numerals(&["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10"]),
            tint: None,
            style: None,
        },
        RackItem::Text { x: 656.0, y: 180.0, text: "Output", size: 9.5, strong: true },
        RackItem::Knob {
            id: "output",
            legend: "",
            x: 656.0,
            y: 254.0,
            d: 92.0,
            ring: Ring::Numerals(&["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10"]),
            tint: None,
            style: None,
        },
        RackItem::Text { x: 800.0, y: 186.0, text: "Mix", size: 9.5, strong: true },
        RackItem::Knob {
            id: "mix",
            legend: "",
            x: 800.0,
            y: 250.0,
            d: 44.0,
            ring: Ring::None,
            tint: Some("#26282b"),
            style: None,
        },
        RackItem::Text { x: 772.0, y: 292.0, text: "Dry", size: 7.5, strong: false },
        RackItem::Text { x: 832.0, y: 292.0, text: "Comp", size: 7.5, strong: false },
        RackItem::Text { x: 878.0, y: 254.0, text: "", size: 9.0, strong: false },
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

    /// Every control id a panel places, excluding the deliberately unwired.
    fn wired_ids(design: &RackDesign) -> Vec<&'static str> {
        design
            .items
            .iter()
            .filter_map(|item| match item {
                RackItem::Knob { id, .. }
                | RackItem::Buttons { id, .. }
                | RackItem::Switch { id, .. }
                | RackItem::Button { id, .. }
                | RackItem::LedSelect { id, .. } => Some(*id),
                _ => None,
            })
            .filter(|id| !id.is_empty())
            .collect()
    }

    /// Controls a panel draws with nothing behind them.
    fn unwired_count(design: &RackDesign) -> usize {
        design
            .items
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    RackItem::Knob { id, .. }
                        | RackItem::Button { id, .. }
                        | RackItem::Switch { id, .. }
                    if id.is_empty()
                )
            })
            .count()
    }

    /// The unwired controls are a known, counted debt — a panel may draw what
    /// the DSP does not have yet, but not by accident. See the EQ's twin.
    #[test]
    fn the_unwired_controls_are_the_ones_we_know_about() {
        for profile in all_profiles() {
            let Some(design) = design_for(profile.id()) else {
                continue;
            };
            let expected = match profile.id() {
                // The LA-2A's meter-mode selector and POWER; the Fairchild's
                // power, meter selector and M-S link; the Distressor's BYPASS.
                // Everything else is wired.
                "la2a" => 2,
                "fairchild670" => 3,
                "distressor" => 1,
                _ => 0,
            };
            assert_eq!(
                unwired_count(design),
                expected,
                "{} grew an unwired control",
                profile.id(),
            );
        }
    }

    /// Every control a panel places has to exist on that unit's profile —
    /// otherwise the knob mounts and does nothing, which is the one failure a
    /// screenshot will not show you.
    #[test]
    fn every_placed_control_exists_on_its_profile() {
        for profile in all_profiles() {
            let Some(design) = design_for(profile.id()) else {
                continue;
            };
            for id in wired_ids(design) {
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
                let placed = wired_ids(design).contains(&control.id);
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
                    | RackItem::TintedText { .. }
                    | RackItem::Lever { .. }
                    | RackItem::Readout { .. }
                    | RackItem::Lamp { .. }
                    | RackItem::Button { .. }
                    | RackItem::LedMeter { .. }
                    | RackItem::Divider { .. }
                    | RackItem::Frame { .. }
                    | RackItem::LedBar { .. }
                    | RackItem::LedSelect { .. } => continue,
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
