//! SSL bus compressor profile.
//!
//! Maps the SSL G-series bus compressor controls:
//! - Threshold
//! - Ratio (stepped: 2:1, 4:1, 10:1)
//! - Attack (stepped: 0.1, 0.3, 1, 3, 10, 30 ms)
//! - Release (stepped: 0.1, 0.3, 0.6, 1.2 s + Auto)
//! - Makeup gain

use crate::{Constraint, ParamMapping, Profile, ProfileControl};

pub struct SslBusProfile;

static CONTROLS: &[ProfileControl] = &[
    direct("threshold", "Threshold", "threshold_db", -30.0..=0.0),
    ProfileControl {
        id: "ratio",
        label: "Ratio",
        mapping: ParamMapping::Stepped {
            param: "ratio",
            values: &[2.0, 4.0, 10.0],
            labels: &["2:1", "4:1", "10:1"],
        },
    },
    ProfileControl {
        id: "attack",
        label: "Attack",
        mapping: ParamMapping::Stepped {
            param: "attack_ms",
            values: &[0.1, 0.3, 1.0, 3.0, 10.0, 30.0],
            labels: &["0.1", "0.3", "1", "3", "10", "30"],
        },
    },
    ProfileControl {
        id: "release",
        label: "Release",
        mapping: ParamMapping::Stepped {
            param: "release_ms",
            values: &[100.0, 300.0, 600.0, 1200.0, 2500.0],
            labels: &["0.1", "0.3", "0.6", "1.2", "Auto"],
        },
    },
    direct("makeup", "Makeup", "output_gain_db", 0.0..=18.0),
    direct("mix", "Mix", "fold", 0.0..=1.0),
];

static CONSTRAINTS: &[Constraint] = &[
    Constraint::Fixed {
        param: "style",
        value: 1.0,
    },
    Constraint::Fixed {
        param: "knee_db",
        value: 3.0,
    },
    Constraint::Fixed {
        param: "channel_link",
        value: 1.0,
    },
    Constraint::Fixed {
        param: "detector_rms_mix",
        value: 0.35,
    },
    Constraint::Clamped {
        param: "drive",
        range: 0.0..=0.2,
    },
    Constraint::Fixed {
        param: "character_mode",
        value: 0.0,
    },
    Constraint::Clamped {
        param: "range_db",
        range: 0.0..=18.0,
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

impl Profile for SslBusProfile {
    fn id(&self) -> &'static str {
        "ssl_bus"
    }

    fn name(&self) -> &'static str {
        "SSL Bus"
    }

    fn controls(&self) -> &[ProfileControl] {
        CONTROLS
    }

    fn constraints(&self) -> &[Constraint] {
        CONSTRAINTS
    }
}
