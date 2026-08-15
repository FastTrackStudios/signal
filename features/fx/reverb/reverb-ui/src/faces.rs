//! The seven panels.
//!
//! A reverb's family is not a preset — it is a different machine, and the
//! panels say so. A hall is a warm wooden-cheeked box with a room drawn on it;
//! a plate is a bare steel sheet; a spring is a green amp chassis with the
//! tank visible; ambient is a lit halo with no walls at all. Same seven
//! parameters underneath, because that is what the DSP has, but you should
//! know which reverb you are looking at before you read a single word.
//!
//! Each panel's centrepiece is *live*: it is drawn from decay, size and
//! damping, so turning a knob changes the picture of the space rather than
//! just a number. That is the whole reason for drawing a space at all.

use dioxus::prelude::*;
use fts_audio_ui::hardware::knob::{HardwareKnob, KnobStyle};
use fts_audio_ui::hardware::panel::{Panel, PanelEnds, PanelSlot, PanelTexture, Silkscreen};
use fts_audio_ui::ParamHandle;

/// Panel drawing size — 2U, like the compressor's faces.
pub const W: f64 = 960.0;
pub const H: f64 = 300.0;

/// What a panel draws in the middle: the thing that makes it that reverb.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Centrepiece {
    /// A hall seen end-on: an arch, with the tail drawn as light under it.
    Arch,
    /// A suspended sheet of steel, ringing.
    Sheet,
    /// A small room in perspective, with its early reflections.
    Room,
    /// The tank: a coil whose turns stretch with decay.
    Coil,
    /// No walls — concentric light, brightest where the wash is densest.
    Halo,
    /// A ladder of lamps, for the ones that are effects rather than spaces.
    Ladder,
    /// The recorded thing itself: an impulse and its decay envelope.
    Waveform,
}

/// One control on a panel.
#[derive(Clone, Copy)]
pub struct KnobSpec {
    /// Which parameter it drives. Matches the field names on `ReverbParams`.
    pub param: &'static str,
    pub legend: &'static str,
    pub x: f64,
    pub y: f64,
    pub d: f64,
    pub style: KnobStyle,
}

/// A family's panel.
#[derive(Clone, Copy)]
pub struct SpaceDesign {
    pub family: &'static str,
    pub paint: &'static str,
    pub ink: &'static str,
    pub dim_ink: &'static str,
    pub chrome: &'static str,
    /// The colour the centrepiece is lit in, and what the rail badges.
    pub accent: &'static str,
    pub ends: PanelEnds,
    pub texture: PanelTexture,
    pub centre: Centrepiece,
    pub knobs: &'static [KnobSpec],
}

/// The control row every panel shares, at the sizes that family wants.
///
/// Five knobs under the centrepiece, one row, evenly spaced. Families differ
/// in which two they make big — a hall is chosen by decay and size, a plate by
/// tone, a spring by how hard you hit it.
const fn knob(param: &'static str, legend: &'static str, x: f64, d: f64, style: KnobStyle) -> KnobSpec {
    // The row sits high enough that the biggest knob's box *and* its legend
    // clear the bottom edge: a knob's box is ~1.8x its diameter, so a 66px
    // knob reaches 60px below this line and its legend another 20 after that.
    KnobSpec { param, legend, x, y: 206.0, d, style }
}

const HALL_KNOBS: &[KnobSpec] = &[
    knob("decay", "Decay", 118.0, 62.0, KnobStyle::Skirted),
    knob("size", "Size", 230.0, 62.0, KnobStyle::Skirted),
    knob("predelay", "Pre-Delay", 340.0, 44.0, KnobStyle::Metal),
    knob("damping", "Damping", 450.0, 44.0, KnobStyle::Metal),
    knob("tone", "Tone", 560.0, 44.0, KnobStyle::Metal),
    // A hall's warmth is the low band outlasting the top, and its size is
    // only convincing once the reflections are dense.
    knob("bass", "Bass", 670.0, 44.0, KnobStyle::Metal),
    knob("diffusion", "Diffusion", 780.0, 44.0, KnobStyle::Metal),
    knob("mix", "Mix", 872.0, 52.0, KnobStyle::Skirted),
];

const PLATE_KNOBS: &[KnobSpec] = &[
    knob("decay", "Decay", 118.0, 58.0, KnobStyle::Collet),
    knob("tone", "Tone", 230.0, 58.0, KnobStyle::Collet),
    knob("damping", "Damping", 340.0, 44.0, KnobStyle::Collet),
    knob("predelay", "Pre-Delay", 450.0, 44.0, KnobStyle::Collet),
    // A plate is a sheet under tension with a damper pressed against it,
    // and the modulation is what stops it ringing on one note.
    knob("modulation", "Motion", 560.0, 44.0, KnobStyle::Collet),
    knob("diffusion", "Density", 670.0, 44.0, KnobStyle::Collet),
    knob("width", "Width", 780.0, 44.0, KnobStyle::Collet),
    knob("mix", "Mix", 872.0, 52.0, KnobStyle::Collet),
];

const ROOM_KNOBS: &[KnobSpec] = &[
    knob("size", "Size", 118.0, 58.0, KnobStyle::Metal),
    knob("decay", "Decay", 230.0, 58.0, KnobStyle::Metal),
    knob("damping", "Damping", 340.0, 44.0, KnobStyle::Metal),
    knob("tone", "Tone", 450.0, 44.0, KnobStyle::Metal),
    // A small room's character is its early reflections: how many, and
    // how hard the walls are.
    knob("diffusion", "Diffusion", 560.0, 44.0, KnobStyle::Metal),
    knob("character_a", "Walls", 670.0, 44.0, KnobStyle::Metal),
    knob("width", "Width", 780.0, 44.0, KnobStyle::Metal),
    knob("mix", "Mix", 872.0, 52.0, KnobStyle::Metal),
];

const SPRING_KNOBS: &[KnobSpec] = &[
    knob("decay", "Dwell", 118.0, 62.0, KnobStyle::Bakelite),
    knob("tone", "Tone", 230.0, 62.0, KnobStyle::Bakelite),
    knob("damping", "Damp", 340.0, 44.0, KnobStyle::Bakelite),
    knob("size", "Tension", 450.0, 44.0, KnobStyle::Bakelite),
    // `extra_a` is the spring's modulation rate — the drip. This is the
    // control that makes it sound like a tank being kicked.
    knob("character_a", "Drip", 560.0, 44.0, KnobStyle::Bakelite),
    knob("modulation", "Warble", 670.0, 44.0, KnobStyle::Bakelite),
    knob("diffusion", "Boing", 780.0, 44.0, KnobStyle::Bakelite),
    knob("mix", "Blend", 872.0, 62.0, KnobStyle::Bakelite),
];

const AMBIENT_KNOBS: &[KnobSpec] = &[
    knob("decay", "Decay", 118.0, 62.0, KnobStyle::Marconi),
    knob("size", "Spread", 230.0, 62.0, KnobStyle::Marconi),
    knob("predelay", "Onset", 340.0, 44.0, KnobStyle::Marconi),
    knob("damping", "Air", 450.0, 44.0, KnobStyle::Marconi),
    // Cloud and bloom read `extra_a`/`extra_b` as how the wash is built —
    // tap spread and how much of the input it keeps.
    knob("character_a", "Shape", 560.0, 44.0, KnobStyle::Marconi),
    knob("character_b", "Grain", 670.0, 44.0, KnobStyle::Marconi),
    knob("width", "Width", 780.0, 44.0, KnobStyle::Marconi),
    knob("mix", "Mix", 872.0, 52.0, KnobStyle::Marconi),
];

const SPECIAL_KNOBS: &[KnobSpec] = &[
    knob("decay", "Decay", 118.0, 58.0, KnobStyle::SilverTop),
    knob("size", "Size", 230.0, 58.0, KnobStyle::SilverTop),
    knob("damping", "Damping", 340.0, 44.0, KnobStyle::SilverTop),
    knob("tone", "Tone", 450.0, 44.0, KnobStyle::SilverTop),
    // The two that make these what they are: shimmer's amount and pitch,
    // magneto's saturation, non-linear's gate shape. The legend changes
    // with the profile — see `legend_for`.
    knob("character_a", "Amount", 560.0, 48.0, KnobStyle::SilverTop),
    knob("character_b", "Pitch", 670.0, 48.0, KnobStyle::SilverTop),
    knob("modulation", "Motion", 780.0, 44.0, KnobStyle::SilverTop),
    knob("mix", "Mix", 872.0, 52.0, KnobStyle::SilverTop),
];

const IR_KNOBS: &[KnobSpec] = &[
    knob("predelay", "Pre-Delay", 118.0, 58.0, KnobStyle::MetalFluted),
    knob("decay", "Length", 230.0, 58.0, KnobStyle::MetalFluted),
    knob("tone", "Tone", 340.0, 44.0, KnobStyle::MetalFluted),
    knob("damping", "Damping", 450.0, 44.0, KnobStyle::MetalFluted),
    // A convolution has no reflections to diffuse, but it does have
    // motion and a tilt you can put on the recorded tail.
    knob("modulation", "Motion", 560.0, 44.0, KnobStyle::MetalFluted),
    knob("bass", "Bass", 670.0, 44.0, KnobStyle::MetalFluted),
    knob("width", "Width", 780.0, 44.0, KnobStyle::MetalFluted),
    knob("mix", "Mix", 872.0, 52.0, KnobStyle::MetalFluted),
];

/// Impulse response: a machine, not a room. Dark, instrument-like, with the
/// captured file drawn across it.
pub static IR: SpaceDesign = SpaceDesign {
    family: "ir",
    paint: "linear-gradient(178deg, #23262b 0%, #191c20 50%, #101215 100%)",
    ink: "#dfe6ee",
    dim_ink: "#8794a3",
    chrome: "#8b939c",
    accent: "#59c2e8",
    ends: PanelEnds::RackEars,
    texture: PanelTexture::Brushed { strength: 40 },
    centre: Centrepiece::Waveform,
    knobs: IR_KNOBS,
};

/// Hall: the concert-hall box, in wood and warm cream.
pub static HALL: SpaceDesign = SpaceDesign {
    family: "hall",
    paint: "linear-gradient(178deg, #e8dfcb 0%, #dcd2bb 48%, #c9bea6 100%)",
    ink: "#33291d",
    dim_ink: "#7a6c58",
    chrome: "#b6a889",
    accent: "#c8912f",
    ends: PanelEnds::Wood,
    texture: PanelTexture::Painted,
    centre: Centrepiece::Arch,
    knobs: HALL_KNOBS,
};

/// Plate: bare steel, because that is what a plate is.
pub static PLATE: SpaceDesign = SpaceDesign {
    family: "plate",
    paint: "linear-gradient(178deg, #cdd0d3 0%, #b9bdc1 46%, #9ba0a5 100%)",
    ink: "#20242a",
    dim_ink: "#5c646d",
    chrome: "#aeb3b8",
    accent: "#3f7fa8",
    ends: PanelEnds::RackEars,
    texture: PanelTexture::Brushed { strength: 100 },
    centre: Centrepiece::Sheet,
    knobs: PLATE_KNOBS,
};

/// Room: neutral, matte, unremarkable on purpose.
pub static ROOM: SpaceDesign = SpaceDesign {
    family: "room",
    paint: "linear-gradient(178deg, #4a4f55 0%, #3b4046 48%, #2c3137 100%)",
    ink: "#e3e8ee",
    dim_ink: "#98a2ad",
    chrome: "#7d858e",
    accent: "#7fbf8a",
    ends: PanelEnds::RackEars,
    texture: PanelTexture::Painted,
    centre: Centrepiece::Room,
    knobs: ROOM_KNOBS,
};

/// Spring: a guitar amp's chassis, tolex-dark with the tank on show.
pub static SPRING: SpaceDesign = SpaceDesign {
    family: "spring",
    paint: "linear-gradient(178deg, #1f3b30 0%, #172d25 50%, #0e1e18 100%)",
    ink: "#e9e0c4",
    dim_ink: "#9aa892",
    chrome: "#b9a97e",
    accent: "#d8b24a",
    ends: PanelEnds::Wood,
    texture: PanelTexture::Painted,
    centre: Centrepiece::Coil,
    knobs: SPRING_KNOBS,
};

/// Ambient: no walls. Deep blue with the wash lit from inside.
pub static AMBIENT: SpaceDesign = SpaceDesign {
    family: "ambient",
    paint: "linear-gradient(178deg, #241f3d 0%, #1b1730 50%, #120f22 100%)",
    ink: "#e6e2f6",
    dim_ink: "#9c95c2",
    chrome: "#8f88b5",
    accent: "#9b7bec",
    ends: PanelEnds::RackEars,
    texture: PanelTexture::Painted,
    centre: Centrepiece::Halo,
    knobs: AMBIENT_KNOBS,
};

/// Special: black, lamps, and no pretence of being a room.
pub static SPECIAL: SpaceDesign = SpaceDesign {
    family: "special",
    paint: "linear-gradient(178deg, #1c1c1f 0%, #141416 48%, #0c0c0e 100%)",
    ink: "#f0e9df",
    dim_ink: "#8d8880",
    chrome: "#9b968e",
    accent: "#e2603f",
    ends: PanelEnds::RackEars,
    texture: PanelTexture::Brushed { strength: 30 },
    centre: Centrepiece::Ladder,
    knobs: SPECIAL_KNOBS,
};

/// Controls that belong to one engine and nothing else.
///
/// The family panel is shared — Shimmer, Magneto and Non-Linear are all
/// "Special" — but they are not the same machine, and this is where they stop
/// looking like each other. Each is placed in the panel's top right, beside
/// the centrepiece, because it is the control you reach for *after* you have
/// chosen the space.
///
/// Empty for the engines whose personality the shared controls already cover.
pub fn extras_for(profile_id: &str) -> &'static [KnobSpec] {
    // Statics rather than a match arm building them: a `&[..]` literal inside
    // the arm is a temporary, and this has to outlive the call.
    const fn extra(param: &'static str, legend: &'static str) -> KnobSpec {
        // Top right, beside the centrepiece — the control you reach for after
        // choosing the space, not while choosing it.
        KnobSpec { param, legend, x: 800.0, y: 74.0, d: 38.0, style: KnobStyle::Metal }
    }
    static SHIMMER: &[KnobSpec] = &[extra("shimmer_interval", "Interval")];
    static SPRINGS: &[KnobSpec] = &[extra("springs", "Springs")];
    static BLOOM: &[KnobSpec] = &[extra("harmonics", "Harmonics")];
    static CHORALE: &[KnobSpec] = &[extra("singers", "Singers")];
    static MAGNETO: &[KnobSpec] = &[extra("regen", "Regen")];
    static NONLINEAR: &[KnobSpec] = &[extra("chop", "Chop")];
    match profile_id {
        // The interval is the whole effect. A shimmer at +12 is a different
        // instrument from one at +7, and the coarse mapping cannot say so.
        "shimmer" => SHIMMER,
        "spring_classic" | "spring_vintage" => SPRINGS,
        "bloom" => BLOOM,
        "chorale" => CHORALE,
        "magneto" => MAGNETO,
        "nonlinear" => NONLINEAR,
        _ => &[],
    }
}

/// What this profile's engine does with `character_a` / `character_b`.
///
/// `extra_a` and `extra_b` reach every algorithm, and each one reads them as
/// something else — shimmer's amount and pitch, magneto's saturation and wow,
/// non-linear's gate shape. A knob called "Character A" tells you nothing, so
/// the panel prints what it actually does here.
pub fn character_legends(profile_id: &str) -> (&'static str, &'static str) {
    match profile_id {
        "shimmer" => ("Amount", "Pitch"),
        "chorale" => ("Voices", "Detune"),
        "magneto" => ("Saturation", "Wow"),
        "nonlinear" => ("Shape", "Gate"),
        "reflections" => ("Pattern", "Spread"),
        "freeverb" => ("Room", "Damp"),
        "cloud" => ("Taps", "Blend"),
        "bloom" => ("Stages", "Harmonics"),
        "swell" => ("Rise", "Hold"),
        "velvet" => ("Density", "Softness"),
        "spring_classic" | "spring_vintage" => ("Drip", "Tank"),
        // Halls and plates take the pair as reflection shaping; the IR takes
        // it as how the recorded tail is re-shaped on the way out.
        "ir" => ("Stretch", "Tilt"),
        "hall_concert" | "hall_cathedral" | "hall_arena" => ("Reflections", "Balcony"),
        "plate_classic" | "plate_224" | "plate_progenitor" => ("Tension", "Damper"),
        "room_medium" | "room_chamber" | "room_studio" => ("Walls", "Furniture"),
        _ => ("Character", "Colour"),
    }
}

/// The panel a profile is drawn on.
///
/// Per family rather than per profile: a Cathedral and an Arena are the same
/// machine with different dimensions, and pretending otherwise would mean
/// seven more panels that differ only in their silkscreen. The profile's name
/// is printed on the panel, and its accent shifts with it, so you can still
/// tell at a glance which one you are on.
pub fn design_for(profile_id: &str) -> &'static SpaceDesign {
    match reverb_profiles::category_of(profile_id).map(|(c, _)| reverb_profiles::CATEGORIES[c].id) {
        Some("ir") => &IR,
        Some("hall") => &HALL,
        Some("plate") => &PLATE,
        Some("room") => &ROOM,
        Some("spring") => &SPRING,
        Some("ambient") => &AMBIENT,
        _ => &SPECIAL,
    }
}

/// How lit the centrepiece is for a given variant — the second plate is
/// brighter than the first, the Arena brighter than the Concert hall. Small,
/// but it means the variants are visibly different and not just re-labelled.
pub fn variant_lift(profile_id: &str) -> f64 {
    match reverb_profiles::category_of(profile_id) {
        Some((_, index)) => 1.0 + index as f64 * 0.22,
        None => 1.0,
    }
}

/// A drawn reverb: the panel, its centrepiece, and its row of controls.
#[component]
pub fn SpaceFace(
    /// The active profile's id.
    profile_id: String,
    /// Bound controls, by parameter name — see [`KnobSpec::param`].
    handles: std::collections::HashMap<String, ParamHandle>,
    /// The shell's redraw tick. Not read: its job is to change, so the panel
    /// re-renders against fresh parameter values instead of being memoized.
    frame: u64,
) -> Element {
    let _ = frame;
    let design = design_for(&profile_id);
    let profile = reverb_profiles::profile_by_id(&profile_id)
        .unwrap_or(&reverb_profiles::PROFILES[0]);
    let scale = fts_audio_ui::hardware::panel::panel_scale(W, H, crate::control_view::RAIL_W);

    // The picture is drawn from the controls, so it moves with them.
    let value = |name: &str| {
        handles
            .get(name)
            .map(|h| h.normalized() as f64)
            .unwrap_or(0.5)
    };
    let (decay, size, damping) = (value("decay"), value("size"), value("damping"));

    rsx! {
        Panel {
            design_w: W,
            design_h: H,
            scale,
            background: design.paint.to_string(),
            chrome: design.chrome.to_string(),
            ends: design.ends,
            texture: design.texture,

            // The IR family gets a browser where the others get nothing —
            // a convolution is only as good as the file in it.
            if design.centre == Centrepiece::Waveform {
                PanelSlot { scale, x: 742.0, y: 104.0, w: 320.0, h: 152.0,
                    IrBrowser { ink: design.ink.to_string(), accent: design.accent.to_string() }
                }
            }

            // The centrepiece, in its own slot so it scales with the panel.
            PanelSlot { scale, x: if design.centre == Centrepiece::Waveform { 340.0 } else { W / 2.0 },
                y: 104.0,
                w: if design.centre == Centrepiece::Waveform { 400.0 } else { 620.0 },
                h: 150.0,
                CentreView {
                    kind: design.centre,
                    accent: design.accent.to_string(),
                    ink: design.dim_ink.to_string(),
                    decay,
                    size,
                    damping,
                    lift: variant_lift(&profile_id),
                }
            }

            // What it is, top left, the way a unit is badged.
            Silkscreen {
                scale, x: 150.0, y: 40.0, width: 260.0,
                text: profile.name.to_string(), size: 15.0,
                color: design.ink.to_string(), weight: 800,
            }
            Silkscreen {
                scale, x: 150.0, y: 60.0, width: 260.0,
                text: reverb_profiles::CATEGORIES
                    .iter()
                    .find(|c| c.profiles.contains(&profile.id))
                    .map(|c| c.label)
                    .unwrap_or("Reverb")
                    .to_string(),
                size: 8.0, color: design.dim_ink.to_string(),
            }

            for (index , spec) in design
                .knobs
                .iter()
                .chain(extras_for(&profile_id).iter())
                .copied()
                .enumerate()
            {
                div {
                    // Keyed and uniform — a list whose entries change shape
                    // between two designs is what walks blitz's mutator off
                    // the end of a template path.
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
                        scale, x: spec.x, y: spec.y + spec.d * 0.92 + 10.0, width: 130.0,
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

/// The impulse-response browser.
///
/// Deliberately a list and not a file dialog: a native dialog inside a plugin
/// window is a different can of worms on every host, and what you actually
/// want when reaching for an IR is to see the ones you have. The library is
/// whatever is under [`ir_library_root`](crate::params::ir_library_root)
/// (`FTS_IR_DIR` overrides), scanned when the panel mounts and on Rescan.
///
/// Clicking a name does not load anything here. It leaves the path on the
/// shared state and the plugin's worker does the decoding — see
/// [`ReverbUiState::request_ir`](crate::params::ReverbUiState::request_ir).
#[component]
fn IrBrowser(ink: String, accent: String) -> Element {
    let shared = use_context::<nice_plug_dioxus::SharedState>();
    let ui = shared
        .get::<crate::control_view::ReverbUi>()
        .expect("the IR browser was mounted without its ReverbUi");

    // Scanned once per mount, and again when asked. Walking a directory is
    // not something to do sixty times a second.
    let mut entries = use_signal(Vec::<(String, std::path::PathBuf)>::new);
    let mut scanned = use_signal(|| false);
    if !*scanned.read() {
        scanned.set(true);
        let mut library = reverb_dsp::ir::IrLibrary::new(crate::params::ir_library_root());
        let _ = library.rescan();
        entries.set(
            library
                .entries()
                .iter()
                .map(|e| (e.name.clone(), e.path.clone()))
                .collect(),
        );
    }

    let state = ui.state.clone();
    let params = ui.params.clone();
    let loading = state.ir_loading.load(std::sync::atomic::Ordering::Relaxed);
    let loaded = state.ir_loaded.lock().clone();
    let error = state.ir_error.lock().clone();
    let list = entries.read().clone();
    let empty = list.is_empty();

    rsx! {
        div {
            "data-testid": "ir-browser",
            style: format!(
                "width:100%; height:100%; display:flex; flex-direction:column; \
                 gap:3px; padding:6px 8px; overflow:hidden; \
                 background:rgba(0,0,0,0.28); border:1px solid rgba(255,255,255,0.10); \
                 border-radius:3px; color:{ink};"
            ),

            // What is loaded, and what the loader is doing about it.
            div {
                style: "display:flex; justify-content:space-between; align-items:center;                         font-size:8px; letter-spacing:0.10em; text-transform:uppercase;                         opacity:0.75;",
                span { "Impulse Response" }
                span {
                    "data-testid": "ir-rescan",
                    style: "cursor:pointer; opacity:0.8;",
                    onclick: move |_| {
                        scanned.set(false);
                    },
                    "Rescan"
                }
            }
            div {
                "data-testid": "ir-current",
                style: format!(
                    "font-size:10px; font-weight:700; white-space:nowrap; \
                     overflow:hidden; color:{};",
                    if error.is_some() { "#e2603f".to_string() } else { accent.clone() },
                ),
                if loading {
                    "Loading…"
                } else if let Some(err) = error.clone() {
                    "{err}"
                } else if loaded.is_empty() {
                    "No impulse loaded"
                } else {
                    "{loaded}"
                }
            }

            // The library.
            div {
                style: "flex:1; min-height:0; display:flex; flex-direction:column;                         gap:1px; overflow:hidden;",
                if empty {
                    div {
                        style: "font-size:8px; opacity:0.6; line-height:1.5;",
                        "Nothing in {crate::params::ir_library_root().display()}"
                    }
                }
                for (name , path) in list.into_iter().take(7) {
                    div {
                        key: "{name}",
                        "data-testid": "ir-entry",
                        style: format!(
                            "font-size:9px; padding:1px 4px; border-radius:2px; \
                             white-space:nowrap; overflow:hidden; cursor:pointer; \
                             background:{};",
                            if loaded == name { format!("{accent}33") } else { "transparent".into() },
                        ),
                        onclick: {
                            let state = state.clone();
                            let params = params.clone();
                            let path = path.clone();
                            move |_| {
                                *params.ir_path.write() = path.display().to_string();
                                state.load_ir(path.clone());
                            }
                        },
                        "{name}"
                    }
                }
            }
        }
    }
}

/// The live picture of the space.
///
/// One SVG per family, drawn in the panel's accent and driven by decay, size
/// and damping. They are deliberately not the same shape with a different
/// colour: an arch, a sheet, a coil and a halo are different *ideas* about
/// what a reverb is, which is the thing the seven families are for.
#[component]
fn CentreView(
    kind: Centrepiece,
    accent: String,
    ink: String,
    decay: f64,
    size: f64,
    damping: f64,
    /// Brightness multiplier for the variant inside the family.
    lift: f64,
) -> Element {
    // Everything below draws in a 620x150 box, which is the slot it sits in.
    let (w, h) = (620.0, 150.0);
    let glow = (0.28 + decay * 0.5).min(0.92) * lift.min(1.5);
    let body = accent.to_string();

    let inner = match kind {
        // A hall, seen end-on: the arch of the ceiling, with the tail as
        // light pooling under it. Size widens the arch; decay fills it.
        Centrepiece::Arch => {
            let span = 150.0 + size * 300.0;
            let rise = 40.0 + size * 70.0;
            let (cx, base) = (w / 2.0, h - 18.0);
            let arch = format!(
                "M {:.1} {:.1} Q {:.1} {:.1} {:.1} {:.1}",
                cx - span / 2.0,
                base,
                cx,
                base - rise * 2.0,
                cx + span / 2.0,
                base,
            );
            rsx! {
                // The lit interior, brighter as the tail grows.
                path { d: "{arch} Z", fill: "{body}", opacity: "{glow * 0.30:.3}" }
                path { d: "{arch}", fill: "none", stroke: "{body}", stroke_width: "2.2" }
                // Reflections down the floor, spaced by size.
                for i in 0..7 {
                    line {
                        key: "{i}",
                        x1: "{cx - span / 2.0 + span * i as f64 / 6.0:.1}",
                        y1: "{base:.1}",
                        x2: "{cx - span / 2.0 + span * i as f64 / 6.0:.1}",
                        y2: "{base - (rise * 0.9) * (1.0 - (i as f64 - 3.0).abs() / 3.6) * decay:.1}",
                        stroke: "{body}",
                        stroke_width: "1.4",
                        opacity: "{(0.9 - damping * 0.6) * glow:.3}",
                    }
                }
                line { x1: "40", y1: "{base:.1}", x2: "{w - 40.0:.1}", y2: "{base:.1}", stroke: "{ink}", stroke_width: "1" }
            }
        }

        // A sheet of steel hung in a frame, ringing. Decay drives how many
        // standing waves cross it; damping flattens them.
        Centrepiece::Sheet => {
            let (x0, y0, sw, sh) = (110.0, 16.0, w - 220.0, h - 48.0);
            rsx! {
                rect {
                    x: "{x0:.1}", y: "{y0:.1}", width: "{sw:.1}", height: "{sh:.1}",
                    fill: "{body}", opacity: "{glow * 0.16:.3}",
                    stroke: "{body}", stroke_width: "1.6",
                }
                // Standing waves across the plate.
                for i in 1..9 {
                    path {
                        key: "{i}",
                        d: "M {x0:.1} {y0 + sh * i as f64 / 9.0:.1} \
                            Q {x0 + sw * 0.25:.1} {y0 + sh * i as f64 / 9.0 - (14.0 - damping * 11.0) * decay:.1} \
                            {x0 + sw * 0.5:.1} {y0 + sh * i as f64 / 9.0:.1} \
                            Q {x0 + sw * 0.75:.1} {y0 + sh * i as f64 / 9.0 + (14.0 - damping * 11.0) * decay:.1} \
                            {x0 + sw:.1} {y0 + sh * i as f64 / 9.0:.1}",
                        fill: "none", stroke: "{body}", stroke_width: "1",
                        opacity: "{glow * (1.0 - i as f64 / 11.0):.3}",
                    }
                }
                // The suspension points, which is how a plate is hung.
                for (i , cx) in [x0, x0 + sw].into_iter().enumerate() {
                    circle { key: "{i}", cx: "{cx:.1}", cy: "{y0:.1}", r: "4", fill: "{ink}" }
                }
            }
        }

        // A small room in perspective: the box, and the first reflections
        // bouncing inside it.
        Centrepiece::Room => {
            let depth = 26.0 + size * 46.0;
            let (x0, y0, rw, rh) = (170.0, 20.0, w - 340.0, h - 62.0);
            rsx! {
                rect { x: "{x0:.1}", y: "{y0:.1}", width: "{rw:.1}", height: "{rh:.1}",
                    fill: "none", stroke: "{ink}", stroke_width: "1.4" }
                rect { x: "{x0 + depth:.1}", y: "{y0 + depth * 0.5:.1}",
                    width: "{rw - depth * 2.0:.1}", height: "{rh - depth:.1}",
                    fill: "{body}", opacity: "{glow * 0.18:.3}",
                    stroke: "{body}", stroke_width: "1.4" }
                for (i , (x1 , y1 , x2 , y2)) in [
                    (x0, y0, x0 + depth, y0 + depth * 0.5),
                    (x0 + rw, y0, x0 + rw - depth, y0 + depth * 0.5),
                    (x0, y0 + rh, x0 + depth, y0 + rh - depth * 0.5),
                    (x0 + rw, y0 + rh, x0 + rw - depth, y0 + rh - depth * 0.5),
                ].into_iter().enumerate() {
                    line { key: "{i}", x1: "{x1:.1}", y1: "{y1:.1}", x2: "{x2:.1}", y2: "{y2:.1}",
                        stroke: "{ink}", stroke_width: "1" }
                }
                // The bounce: a ray from the source, folded off the walls.
                polyline {
                    points: "{x0 + depth + 8.0:.1},{y0 + rh * 0.62:.1} \
                             {x0 + rw * 0.42:.1},{y0 + depth * 0.7:.1} \
                             {x0 + rw * 0.68:.1},{y0 + rh * 0.8:.1} \
                             {x0 + rw - depth - 8.0:.1},{y0 + rh * 0.4:.1}",
                    fill: "none", stroke: "{body}", stroke_width: "1.6",
                    opacity: "{(0.95 - damping * 0.5) * glow:.3}",
                }
            }
        }

        // The tank. Turns stretch with decay, and the whole coil sags —
        // which is exactly what a spring reverb sounds like.
        Centrepiece::Coil => {
            let turns = 16;
            let (x0, span) = (70.0, w - 140.0);
            let step = span / turns as f64;
            let amp = 22.0 + decay * 30.0;
            let sag = 6.0 + (1.0 - size) * 14.0;
            let mut d = format!("M {:.1} {:.1}", x0, h / 2.0);
            let mut i = 0;
            while i < turns {
                let x = x0 + step * (i as f64 + 1.0);
                let dir = if i % 2 == 0 { -1.0 } else { 1.0 };
                let mid = h / 2.0 + -(sag * ((i as f64 / turns as f64) * 2.0 - 1.0).abs()) + sag;
                d.push_str(&format!(
                    " Q {:.1} {:.1} {:.1} {:.1}",
                    x - step / 2.0,
                    mid + dir * amp,
                    x,
                    mid,
                ));
                i += 1;
            }
            rsx! {
                // The rails the tank hangs from.
                for (i , y) in [22.0, h - 22.0].into_iter().enumerate() {
                    line { key: "{i}", x1: "{x0 - 24.0:.1}", y1: "{y:.1}", x2: "{x0 + span + 24.0:.1}", y2: "{y:.1}",
                        stroke: "{ink}", stroke_width: "1.2", opacity: "0.7" }
                }
                path { d: "{d}", fill: "none", stroke: "{body}", stroke_width: "2.4",
                    opacity: "{glow:.3}", stroke_linecap: "round" }
                // The transducers at each end.
                for (i , cx) in [x0 - 10.0, x0 + span + 10.0].into_iter().enumerate() {
                    circle { key: "{i}", cx: "{cx:.1}", cy: "{h / 2.0:.1}", r: "7",
                        fill: "none", stroke: "{ink}", stroke_width: "1.6" }
                }
            }
        }

        // No walls: concentric light. Size spreads it, decay brightens it,
        // damping eats the outer rings.
        Centrepiece::Halo => {
            let (cx, cy) = (w / 2.0, h / 2.0);
            rsx! {
                for i in 0..7 {
                    circle {
                        key: "{i}",
                        cx: "{cx:.1}", cy: "{cy:.1}",
                        r: "{(12.0 + i as f64 * (7.0 + size * 9.0)):.1}",
                        fill: "none", stroke: "{body}", stroke_width: "{2.4 - i as f64 * 0.24:.2}",
                        opacity: "{(glow * (1.0 - i as f64 / 7.0) * (1.0 - damping * i as f64 / 9.0)).max(0.0):.3}",
                    }
                }
                circle { cx: "{cx:.1}", cy: "{cy:.1}", r: "6", fill: "{body}", opacity: "{glow:.3}" }
                // A horizon line, so the halo reads as suspended rather than
                // as a target.
                line { x1: "60", y1: "{cy:.1}", x2: "{w - 60.0:.1}", y2: "{cy:.1}",
                    stroke: "{ink}", stroke_width: "0.8", opacity: "0.35" }
            }
        }

        // Lamps, because these are effects and an effect has a state, not a
        // shape. The lit run grows with decay.
        Centrepiece::Ladder => {
            let count = 14;
            let lit = ((decay * count as f64).round() as usize).min(count);
            let (x0, gap) = (70.0, (w - 140.0) / count as f64);
            rsx! {
                for i in 0..count {
                    rect {
                        key: "{i}",
                        x: "{x0 + gap * i as f64:.1}", y: "{h / 2.0 - 26.0:.1}",
                        width: "{gap * 0.6:.1}", height: "52",
                        rx: "2",
                        fill: if i < lit { "{body}" } else { "{ink}" },
                        opacity: if i < lit { "{glow:.3}" } else { "0.18" },
                    }
                }
                line { x1: "{x0:.1}", y1: "{h / 2.0 + 40.0:.1}", x2: "{w - 70.0:.1}", y2: "{h / 2.0 + 40.0:.1}",
                    stroke: "{ink}", stroke_width: "1", opacity: "0.5" }
            }
        }

        // The recorded thing: an impulse and the envelope it decays under.
        Centrepiece::Waveform => {
            let (x0, span) = (60.0, w - 120.0);
            let mid = h / 2.0;
            // A decaying envelope, sampled — deterministic, so the drawing is
            // stable between frames rather than crawling.
            let bars = 96;
            rsx! {
                for i in 0..bars {
                    {
                        let t = i as f64 / bars as f64;
                        let env = (-t * (7.0 - decay * 5.5)).exp();
                        // A fixed pseudo-random texture: the same every frame.
                        let jitter = ((i as f64 * 12.9898).sin() * 43758.5453).fract().abs();
                        let a = env * (0.45 + jitter * 0.55) * (1.0 - damping * 0.35);
                        rsx! {
                            line {
                                key: "{i}",
                                x1: "{x0 + span * t:.1}", y1: "{mid - a * 52.0:.1}",
                                x2: "{x0 + span * t:.1}", y2: "{mid + a * 52.0:.1}",
                                stroke: "{body}", stroke_width: "{span / bars as f64 * 0.7:.2}",
                                opacity: "{(0.35 + glow * 0.6):.3}",
                            }
                        }
                    }
                }
                line { x1: "{x0:.1}", y1: "{mid:.1}", x2: "{x0 + span:.1}", y2: "{mid:.1}",
                    stroke: "{ink}", stroke_width: "0.8", opacity: "0.5" }
                // The pre-delay gap, drawn as the silence before the impulse.
                line { x1: "{x0:.1}", y1: "{mid - 58.0:.1}", x2: "{x0:.1}", y2: "{mid + 58.0:.1}",
                    stroke: "{ink}", stroke_width: "1.2", opacity: "0.7" }
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

    /// A knob under a rack ear is a knob you cannot turn — the ear is drawn
    /// over it. The panels are 960 wide with 26px ears, and a knob's box is
    /// nearly twice its diameter.
    #[test]
    fn nothing_is_placed_under_a_rack_ear() {
        const EAR: f64 = 26.0;
        for design in [&IR, &HALL, &PLATE, &ROOM, &SPRING, &AMBIENT, &SPECIAL] {
            for spec in design.knobs.iter().chain(
                reverb_profiles::PROFILES
                    .iter()
                    .filter(|p| design_for(p.id).family == design.family)
                    .flat_map(|p| extras_for(p.id)),
            ) {
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

    /// …and a knob whose legend falls off the bottom is a knob you cannot
    /// name. Blitz clips rather than shrinking, so this is not cosmetic.
    #[test]
    fn every_legend_fits_on_the_panel() {
        for design in [&IR, &HALL, &PLATE, &ROOM, &SPRING, &AMBIENT, &SPECIAL] {
            for spec in design.knobs {
                let legend_y = spec.y + spec.d * 0.92 + 10.0;
                assert!(
                    legend_y + 6.0 <= H,
                    "{}'s {} legend sits at y={legend_y:.0} on a {H}px panel",
                    design.family,
                    spec.param,
                );
            }
        }
    }

    /// Every control a panel places has to be one the editor actually binds.
    /// A typo here is a knob that silently never appears.
    #[test]
    fn every_placed_control_is_one_the_editor_binds() {
        const BOUND: &[&str] = &[
            "decay", "size", "predelay", "damping", "tone", "width", "mix",
            "diffusion", "modulation", "bass", "character_a", "character_b",
            "shimmer_interval", "springs", "harmonics", "singers", "regen", "chop",
        ];
        for profile in reverb_profiles::PROFILES {
            for spec in extras_for(profile.id) {
                assert!(
                    BOUND.contains(&spec.param),
                    "{}'s extra {:?} is not a control the editor binds",
                    profile.id,
                    spec.param,
                );
            }
        }
        for design in [&IR, &HALL, &PLATE, &ROOM, &SPRING, &AMBIENT, &SPECIAL] {
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

    /// Only the IR panel carries a browser, and it is the only one that
    /// should: every other family generates its space rather than loading one.
    #[test]
    fn the_browser_belongs_to_the_ir_panel_and_nowhere_else() {
        for design in [&IR, &HALL, &PLATE, &ROOM, &SPRING, &AMBIENT, &SPECIAL] {
            assert_eq!(
                design.centre == Centrepiece::Waveform,
                design.family == "ir",
                "{} draws {:?}",
                design.family,
                design.centre,
            );
        }
    }

    /// The two engine controls are named per profile, and the fallback is a
    /// sign nobody decided what they do on that machine.
    #[test]
    fn every_profile_names_its_engine_controls() {
        for profile in reverb_profiles::PROFILES {
            let (a, b) = character_legends(profile.id);
            assert_ne!(
                (a, b),
                ("Character", "Colour"),
                "{} still has the placeholder legends",
                profile.id,
            );
        }
    }
}
