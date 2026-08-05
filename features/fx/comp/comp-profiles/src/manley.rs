//! Manley Variable Mu — the modern variable-mu bus compressor.
//!
//! The mix-bus staple: a detented input gain, a threshold that gets more
//! extreme counter-clockwise, continuous attack and a stepped recovery, a
//! COMP/LIMIT switch that is really a ratio switch (1.5:1 soft-knee vs 4:1
//! sharper), and the high-pass sidechain that stops the bass from working the
//! whole mix.

use crate::{Constraint, ParamMapping, Profile, ProfileControl};

pub struct ManleyVariMuProfile;

static CONTROLS: &[ProfileControl] = &[
    // Detented ±8 dB over eleven positions on the unit.
    ProfileControl {
        id: "input",
        label: "Input",
        mapping: ParamMapping::Direct {
            param: "input_gain_db",
            range: -8.0..=8.0,
        },
    },
    ProfileControl {
        id: "threshold",
        label: "Threshold",
        mapping: ParamMapping::Direct {
            param: "threshold_db",
            range: -40.0..=0.0,
        },
    },
    ProfileControl {
        id: "attack",
        label: "Attack",
        mapping: ParamMapping::Direct {
            param: "attack_ms",
            range: 25.0..=70.0,
        },
    },
    // RECOVERY is a five-position switch, fastest first.
    ProfileControl {
        id: "recovery",
        label: "Recovery",
        mapping: ParamMapping::Stepped {
            param: "release_ms",
            values: &[200.0, 400.0, 600.0, 1200.0, 2500.0],
            labels: &["1", "2", "3", "4", "5"],
        },
    },
    // COMP is 1.5:1 and soft; LIMIT is 4:1 with a sharper knee.
    ProfileControl {
        id: "mode",
        label: "Mode",
        mapping: ParamMapping::Stepped {
            param: "ratio",
            values: &[1.5, 4.0],
            labels: &["Comp", "Limit"],
        },
    },
    // The sidechain high-pass: out, or ignoring everything under 100 Hz.
    ProfileControl {
        id: "hp_sidechain",
        label: "HP SC",
        mapping: ParamMapping::Stepped {
            param: "sidechain_freq",
            values: &[20.0, 100.0],
            labels: &["Out", "100"],
        },
    },
    ProfileControl {
        id: "output",
        label: "Output",
        mapping: ParamMapping::Direct {
            param: "output_gain_db",
            range: -6.0..=18.0,
        },
    },
];

static CONSTRAINTS: &[Constraint] = &[
    Constraint::Fixed {
        param: "style",
        value: 2.0,
    },
    Constraint::Fixed {
        param: "channel_link",
        value: 1.0,
    },
    Constraint::Clamped {
        param: "knee_db",
        range: 8.0..=22.0,
    },
    Constraint::Fixed {
        param: "detector_rms_mix",
        value: 0.8,
    },
    Constraint::Clamped {
        param: "drive",
        range: 0.0..=0.25,
    },
    Constraint::Fixed {
        param: "character_mode",
        value: 1.0,
    },
];

impl Profile for ManleyVariMuProfile {
    fn id(&self) -> &'static str {
        "manley_vari_mu"
    }

    fn name(&self) -> &'static str {
        "Manley Vari-Mu"
    }

    fn controls(&self) -> &[ProfileControl] {
        CONTROLS
    }

    fn constraints(&self) -> &[Constraint] {
        CONSTRAINTS
    }
}
