//! UREI 1176 profile.
//!
//! Maps the classic 1176 controls to the compressor DSP chain:
//! - Input (drive into compression)
//! - Output (makeup gain)
//! - Attack / Release (both fast, continuous)
//! - Ratio buttons (4:1, 8:1, 12:1, 20:1, all-buttons)

use crate::{Constraint, ParamMapping, Profile, ProfileControl};

pub struct Urei1176Profile;

static CONTROLS: &[ProfileControl] = &[
    ProfileControl {
        id: "input",
        label: "Input",
        mapping: ParamMapping::Compound {
            mappings: &[
                ("input_gain_db", input_gain),
                ("threshold_db", input_threshold),
                ("drive", input_drive),
            ],
            range: 0.0..=1.0,
        },
    },
    ProfileControl {
        id: "output",
        label: "Output",
        mapping: ParamMapping::Direct {
            param: "output_gain_db",
            range: -12.0..=24.0,
        },
    },
    ProfileControl {
        id: "attack",
        label: "Attack",
        mapping: ParamMapping::Direct {
            param: "attack_ms",
            range: 0.02..=0.8,
        },
    },
    ProfileControl {
        id: "release",
        label: "Release",
        mapping: ParamMapping::Direct {
            param: "release_ms",
            range: 50.0..=1100.0,
        },
    },
    ProfileControl {
        id: "ratio",
        label: "Ratio",
        mapping: ParamMapping::Stepped {
            param: "ratio",
            values: &[4.0, 8.0, 12.0, 20.0, 32.0],
            labels: &["4", "8", "12", "20", "All"],
        },
    },
];

static CONSTRAINTS: &[Constraint] = &[
    Constraint::Fixed {
        param: "style",
        value: 3.0,
    },
    Constraint::Fixed {
        param: "knee_db",
        value: 0.0,
    },
    Constraint::Clamped {
        param: "range_db",
        range: 0.0..=30.0,
    },
    Constraint::Clamped {
        param: "feedback",
        range: 0.35..=1.0,
    },
    Constraint::Fixed {
        param: "detector_rms_mix",
        value: 0.0,
    },
    Constraint::Clamped {
        param: "drive",
        range: 0.0..=0.8,
    },
    Constraint::Fixed {
        param: "character_mode",
        value: 2.0,
    },
];

fn input_gain(x: f64) -> f64 {
    -6.0 + x.clamp(0.0, 1.0) * 30.0
}

fn input_threshold(x: f64) -> f64 {
    -8.0 - x.clamp(0.0, 1.0) * 36.0
}

fn input_drive(x: f64) -> f64 {
    x.clamp(0.0, 1.0) * 0.8
}

impl Profile for Urei1176Profile {
    fn id(&self) -> &'static str {
        "urei_1176"
    }

    fn name(&self) -> &'static str {
        "UREI 1176"
    }

    fn controls(&self) -> &[ProfileControl] {
        CONTROLS
    }

    fn constraints(&self) -> &[Constraint] {
        CONSTRAINTS
    }
}
