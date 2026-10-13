//! Control profile — full parametric access to all compressor parameters.
//!
//! No constraints, all parameters exposed. This is the "advanced view."

use crate::{Constraint, ParamMapping, Profile, ProfileControl};

pub struct ControlProfile;

static CONTROLS: &[ProfileControl] = &[
    direct("threshold_db", "Threshold", "threshold_db", -60.0..=0.0),
    direct("ratio", "Ratio", "ratio", 1.0..=20.0),
    direct("attack_ms", "Attack", "attack_ms", 0.01..=250.0),
    direct("release_ms", "Release", "release_ms", 5.0..=3000.0),
    direct("knee_db", "Knee", "knee_db", 0.0..=36.0),
    direct("range_db", "Range", "range_db", 0.0..=60.0),
    direct("input_gain_db", "Input", "input_gain_db", -24.0..=24.0),
    direct("output_gain_db", "Output", "output_gain_db", -24.0..=24.0),
    direct("mix", "Mix", "fold", 0.0..=1.0),
    direct(
        "multiband_amount",
        "Multiband",
        "multiband_amount",
        0.0..=1.0,
    ),
    direct(
        "expander_threshold_db",
        "Gate Threshold",
        "expander_threshold_db",
        -100.0..=0.0,
    ),
    direct("expander_ratio", "Gate Ratio", "expander_ratio", 1.0..=20.0),
    direct(
        "upward_threshold_db",
        "Up Threshold",
        "upward_threshold_db",
        -100.0..=0.0,
    ),
    direct("upward_ratio", "Up Ratio", "upward_ratio", 1.0..=20.0),
    direct("feedback", "Feedback", "feedback", 0.0..=1.0),
    direct("channel_link", "Stereo Link", "channel_link", 0.0..=1.0),
    direct(
        "detector_rms_mix",
        "Detector RMS",
        "detector_rms_mix",
        0.0..=1.0,
    ),
    direct("sidechain_freq", "SC HPF", "sidechain_freq", 20.0..=1000.0),
    direct(
        "sidechain_lowpass_freq",
        "SC LPF",
        "sidechain_lowpass_freq",
        0.0..=20_000.0,
    ),
    direct("lookahead_ms", "Lookahead", "lookahead_ms", 0.0..=20.0),
    direct("hold_ms", "Hold", "hold_ms", 0.0..=500.0),
    direct("inertia", "Inertia", "inertia", 0.0..=1.0),
    direct("inertia_decay", "Inertia Decay", "inertia_decay", 0.0..=1.0),
    direct("ceiling", "Ceiling", "ceiling", 0.01..=1.0),
    direct("drive", "Drive", "drive", 0.0..=1.0),
    ProfileControl {
        id: "character_mode",
        label: "Character",
        mapping: ParamMapping::Stepped {
            param: "character_mode",
            values: &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            labels: &["Tanh", "Tube", "Diode", "Bright", "Cubic", "Clip", "Asym"],
        },
    },
    direct("auto_makeup", "Auto Gain", "auto_makeup", 0.0..=1.0),
    ProfileControl {
        id: "style",
        label: "Style",
        mapping: ParamMapping::Stepped {
            param: "style",
            values: &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0],
            labels: &["Clean", "Classic", "Opto", "FET", "Punch", "Smooth"],
        },
    },
];

const fn direct(
    id: &'static str,
    label: &'static str,
    param: &'static str,
    range: std::ops::RangeInclusive<f64>,
) -> ProfileControl {
    ProfileControl {
        id,
        label,
        mapping: ParamMapping::Direct { param, range },
    }
}

impl Profile for ControlProfile {
    fn id(&self) -> &'static str {
        "control"
    }

    fn name(&self) -> &'static str {
        "Control"
    }

    fn controls(&self) -> &[ProfileControl] {
        CONTROLS
    }

    fn constraints(&self) -> &[Constraint] {
        &[]
    }
}
