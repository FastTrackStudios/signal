//! Empirical Labs Distressor EL8 — the hybrid.
//!
//! Digitally-controlled analogue: the ratio switch is not just a number, it is
//! a mode. 1:1 through 10:1 behave like ordinary ratios; 20:1 is the opto-ish
//! curve; NUKE is the flattening, saturating extreme the unit is famous for.
//! That is why RATIO here also moves the knee, the drive and the detector
//! blend — on the real unit those change with the switch too.
//!
//! Panel: INPUT / OUTPUT / ATTACK / RELEASE across the bottom, the ratio and
//! mode buttons above, and an LED gain-reduction ladder rather than a VU.

use crate::{Constraint, ParamMapping, Profile, ProfileControl};

pub struct DistressorProfile;

/// The ratio switch's positions, in panel order.
pub const RATIOS: &[f64] = &[1.0, 2.0, 3.0, 4.0, 6.0, 10.0, 20.0, 32.0];
/// What is printed next to each.
pub const RATIO_LABELS: &[&str] = &["1", "2", "3", "4", "6", "10", "20", "Nuke"];

static CONTROLS: &[ProfileControl] = &[
    // INPUT drives into the detector, like the 1176's.
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
            range: 0.05..=50.0,
        },
    },
    ProfileControl {
        id: "release",
        label: "Release",
        mapping: ParamMapping::Direct {
            param: "release_ms",
            range: 50.0..=3500.0,
        },
    },
    ProfileControl {
        id: "ratio",
        label: "Ratio",
        mapping: ParamMapping::Stepped {
            param: "ratio",
            values: RATIOS,
            labels: RATIO_LABELS,
        },
    },
    // "Dist 2" and "Dist 3" — the unit's deliberate distortion modes, on top
    // of the clean setting.
    ProfileControl {
        id: "audio_mode",
        label: "Audio",
        mapping: ParamMapping::Stepped {
            param: "character_mode",
            values: &[0.0, 4.0, 6.0],
            labels: &["Clean", "Dist 2", "Dist 3"],
        },
    },
    // The unit's wet/dry blend — parallel compression without a bus.
    ProfileControl {
        id: "mix",
        label: "Mix",
        mapping: ParamMapping::Direct {
            param: "fold",
            range: 0.0..=1.0,
        },
    },
    // The detector high-pass ("British mode" territory).
    ProfileControl {
        id: "detector",
        label: "Detector",
        mapping: ParamMapping::Stepped {
            param: "sidechain_freq",
            values: &[20.0, 80.0, 200.0],
            labels: &["Off", "HP", "HP+"],
        },
    },
];

static CONSTRAINTS: &[Constraint] = &[
    // A hybrid: fast solid-state control with the saturation stage available.
    Constraint::Fixed {
        param: "style",
        value: 1.0,
    },
    Constraint::Clamped {
        param: "knee_db",
        range: 0.0..=12.0,
    },
    Constraint::Clamped {
        param: "detector_rms_mix",
        range: 0.0..=0.6,
    },
    Constraint::Clamped {
        param: "drive",
        range: 0.0..=1.0,
    },
];

fn input_gain(x: f64) -> f64 {
    -6.0 + x.clamp(0.0, 1.0) * 28.0
}

fn input_threshold(x: f64) -> f64 {
    -6.0 - x.clamp(0.0, 1.0) * 38.0
}

fn input_drive(x: f64) -> f64 {
    x.clamp(0.0, 1.0) * 0.7
}

impl Profile for DistressorProfile {
    fn id(&self) -> &'static str {
        "distressor"
    }

    fn name(&self) -> &'static str {
        "Distressor"
    }

    fn controls(&self) -> &[ProfileControl] {
        CONTROLS
    }

    fn constraints(&self) -> &[Constraint] {
        CONSTRAINTS
    }
}
