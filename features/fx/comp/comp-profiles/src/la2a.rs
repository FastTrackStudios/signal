//! LA-2A profile.
//!
//! Maps the simple LA-2A controls to the compressor DSP chain:
//! - Peak Reduction (compound: drives threshold + ratio + knee)
//! - Gain (makeup)
//! - Compress / Limit switch (changes ratio curve)

use crate::{Constraint, ParamMapping, Profile, ProfileControl};

pub struct La2aProfile;

static CONTROLS: &[ProfileControl] = &[
    ProfileControl {
        id: "peak_reduction",
        label: "Peak Reduction",
        mapping: ParamMapping::Compound {
            mappings: &[
                ("threshold_db", peak_reduction_threshold),
                ("ratio", peak_reduction_ratio),
                ("knee_db", peak_reduction_knee),
                ("range_db", peak_reduction_range),
                ("drive", peak_reduction_drive),
            ],
            range: 0.0..=1.0,
        },
    },
    ProfileControl {
        id: "gain",
        label: "Gain",
        mapping: ParamMapping::Direct {
            param: "output_gain_db",
            range: 0.0..=24.0,
        },
    },
    ProfileControl {
        id: "mode",
        label: "Mode",
        mapping: ParamMapping::Stepped {
            param: "ratio",
            values: &[3.0, 10.0],
            labels: &["Compress", "Limit"],
        },
    },
];

static CONSTRAINTS: &[Constraint] = &[
    Constraint::Fixed {
        param: "style",
        value: 2.0,
    },
    Constraint::Fixed {
        param: "attack_ms",
        value: 10.0,
    },
    Constraint::Clamped {
        param: "release_ms",
        range: 80.0..=3000.0,
    },
    Constraint::Fixed {
        param: "channel_link",
        value: 1.0,
    },
    Constraint::Fixed {
        param: "detector_rms_mix",
        value: 1.0,
    },
    Constraint::Clamped {
        param: "drive",
        range: 0.0..=0.35,
    },
    Constraint::Fixed {
        param: "character_mode",
        value: 1.0,
    },
];

fn peak_reduction_threshold(x: f64) -> f64 {
    -6.0 - x.clamp(0.0, 1.0) * 42.0
}

fn peak_reduction_ratio(x: f64) -> f64 {
    2.0 + x.clamp(0.0, 1.0) * 4.0
}

fn peak_reduction_knee(x: f64) -> f64 {
    12.0 + x.clamp(0.0, 1.0) * 18.0
}

fn peak_reduction_range(x: f64) -> f64 {
    12.0 + x.clamp(0.0, 1.0) * 24.0
}

fn peak_reduction_drive(x: f64) -> f64 {
    x.clamp(0.0, 1.0) * 0.25
}

impl Profile for La2aProfile {
    fn id(&self) -> &'static str {
        "la2a"
    }

    fn name(&self) -> &'static str {
        "LA-2A"
    }

    fn controls(&self) -> &[ProfileControl] {
        CONTROLS
    }

    fn constraints(&self) -> &[Constraint] {
        CONSTRAINTS
    }
}
