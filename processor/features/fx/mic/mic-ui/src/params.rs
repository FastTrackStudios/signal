//! The parameter trees of FTS Mic and FTS Mic 180, and the state their
//! editors read. Ids are the plugins' since their first release: hosts
//! restore sessions by them.

use std::sync::Arc;

use nice_plug::prelude::*;

/// The modelled mics, in the reference's order.
pub const MICS: [&str; 40] = [
    "LD-47K", "LD-49K", "LD-67", "LD-67 NOS", "LD-87 Vintage", "LD-87 Modern", "LD-87 TK", "LD-103", "LD-12",
    "LD-251", "LD-800", "LD-37A", "LD-37P", "LD-BV1", "LD-414 Brass", "LD-414 Nylon", "LD-414 US", "LD-414 T2",
    "LD-563", "LD-017T", "SD-451", "SD-416", "RB-4038", "RB-77DX Satin", "RB-77DX Umber", "RB-121", "RB-160",
    "DN-57", "DN-7", "DN-20", "DN-409N", "DN-409U", "DN-421N", "DN-421S", "DN-421B", "DN-12A", "DN-12E",
    "Sphere Linear", "Sphere Diffuse", "Sphere Direct",
];
pub const PATTERNS: [&str; 9] =
    ["Omni", "Sub-Omni", "Wide-Card", "Sub-Card", "Cardioid", "Super-Card", "Hyper-Card", "Sub-8", "Figure-8"];
pub const FILTERS: [&str; 4] = ["Off", "Low", "Med", "High"];
pub const SOURCES: [&str; 2] = ["L22/DLX", "LX"];
pub const SOLOS: [&str; 3] = ["Off", "Mic1", "Mic2"];

type ToText = Arc<dyn Fn(i32) -> String + Send + Sync>;
type FromText = Arc<dyn Fn(&str) -> Option<i32> + Send + Sync>;

fn named(names: &'static [&'static str]) -> (ToText, FromText) {
    (
        Arc::new(move |v| usize::try_from(v).ok().and_then(|i| names.get(i)).map_or_else(String::new, |s| (*s).to_string())),
        Arc::new(move |s| names.iter().position(|n| n.eq_ignore_ascii_case(s.trim())).and_then(|i| i32::try_from(i).ok())),
    )
}

/// A named choice: prints its name and parses it back.
#[must_use]
pub fn choice(name: &str, default: i32, names: &'static [&'static str]) -> IntParam {
    let (to_s, from_s) = named(names);
    let max = i32::try_from(names.len()).unwrap_or(1).saturating_sub(1);
    IntParam::new(name, default, IntRange::Linear { min: 0, max }).with_value_to_string(to_s).with_string_to_value(from_s)
}

/// A linear control with a unit; parses what it prints, unit or not.
#[must_use]
pub fn float(name: &str, default: f32, min: f32, max: f32, unit: &'static str, digits: usize) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min, max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(digits))
        .with_string_to_value(Arc::new(|s: &str| s.trim().trim_end_matches(|c: char| c.is_alphabetic() || c == '%' || c == '°').trim().parse().ok()))
}

/// A choice's current index.
#[must_use]
pub fn index(p: &IntParam) -> usize {
    usize::try_from(p.value()).unwrap_or(0)
}

#[derive(Params)]
pub struct MicParams {
    #[id = "type1"]
    pub type1: IntParam,
    #[id = "pattern1"]
    pub pattern1: IntParam,
    #[id = "filter1"]
    pub filter1: IntParam,
    #[id = "axis1"]
    pub axis1: FloatParam,
    #[id = "dual"]
    pub dual: BoolParam,
    #[id = "mix"]
    pub mix: FloatParam,
    #[id = "type2"]
    pub type2: IntParam,
    #[id = "pattern2"]
    pub pattern2: IntParam,
    #[id = "filter2"]
    pub filter2: IntParam,
    #[id = "axis2"]
    pub axis2: FloatParam,
    #[id = "align"]
    pub align: FloatParam,
    #[id = "solo"]
    pub solo: IntParam,
    #[id = "proximity"]
    pub proximity: FloatParam,
    #[id = "output"]
    pub output: FloatParam,
    #[id = "phase"]
    pub phase: BoolParam,
    #[id = "rear_trim"]
    pub rear_trim: FloatParam,
    #[id = "swap"]
    pub swap: BoolParam,
    #[id = "source"]
    pub source: IntParam,
    /// The 180 variant's stereo pair: mic 1 forward on the left, mic 2
    /// facing back on the right (instead of dual mode's mix).
    #[id = "stereo180"]
    pub stereo180: BoolParam,
    /// 180: mic 2 follows mic 1's type, pattern, filter and axis.
    #[id = "link"]
    pub link: BoolParam,
    #[id = "pan"]
    pub pan: FloatParam,
    #[id = "width"]
    pub width: FloatParam,
}

impl Default for MicParams {
    fn default() -> Self {
        Self {
            type1: choice("Mic1 Type", 0, &MICS),
            pattern1: choice("Mic1 Pattern", 4, &PATTERNS),
            filter1: choice("Mic1 Filter", 0, &FILTERS),
            axis1: float("Mic1 Axis", 0.0, 0.0, 180.0, "°", 1),
            dual: BoolParam::new("Dual", false),
            mix: float("Mic Mix", 0.0, 0.0, 100.0, "%", 1),
            type2: choice("Mic2 Type", 2, &MICS),
            pattern2: choice("Mic2 Pattern", 4, &PATTERNS),
            filter2: choice("Mic2 Filter", 0, &FILTERS),
            axis2: float("Mic2 Axis", 0.0, 0.0, 180.0, "°", 1),
            align: float("Mic2 Align", 0.0, -2.0, 2.0, " cm", 2),
            solo: choice("Mic Solo", 0, &SOLOS),
            proximity: float("Proximity", 0.0, -100.0, 100.0, "%", 1),
            output: float("Output", 0.0, -12.0, 12.0, " dB", 1),
            phase: BoolParam::new("Phase Invert", false),
            rear_trim: float("Rear Trim", 0.0, -6.0, 6.0, " dB", 2),
            swap: BoolParam::new("Swap Capsules", false),
            source: choice("Source Mic", 0, &SOURCES),
            stereo180: BoolParam::new("180 Stereo", false),
            link: BoolParam::new("Mic Link", true),
            pan: float("Mic Pan", 0.0, -100.0, 100.0, "%", 1),
            width: float("Stereo Width", 100.0, 0.0, 200.0, "%", 1),
        }
    }
}

/// What FTS Mic's editor reads.
pub struct MicUiState {
    pub params: Arc<MicParams>,
}

impl MicUiState {
    #[must_use]
    pub const fn new(params: Arc<MicParams>) -> Self {
        Self { params }
    }
}
