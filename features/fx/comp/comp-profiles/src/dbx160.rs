//! dbx 160 — three knobs and a VU.
//!
//! THRESHOLD, COMPRESSION (ratio, 1:1 to ∞:1) and OUTPUT GAIN, and that is the
//! entire front panel. The character is in what it does rather than in what it
//! offers: a fast, hard VCA that flattens a drum without asking, which is why
//! the ratio range runs all the way to limiting.

use crate::{Constraint, ParamMapping, Profile, ProfileControl};

pub struct Dbx160Profile;

static CONTROLS: &[ProfileControl] = &[
    ProfileControl {
        id: "threshold",
        label: "Threshold",
        mapping: ParamMapping::Direct {
            param: "threshold_db",
            range: -40.0..=0.0,
        },
    },
    // The panel says 1:1 to ∞:1; the engine's ceiling stands in for infinity.
    ProfileControl {
        id: "compression",
        label: "Compression",
        mapping: ParamMapping::Direct {
            param: "ratio",
            range: 1.0..=20.0,
        },
    },
    ProfileControl {
        id: "output",
        label: "Output Gain",
        mapping: ParamMapping::Direct {
            param: "output_gain_db",
            range: -20.0..=20.0,
        },
    },
];

static CONSTRAINTS: &[Constraint] = &[
    // Solid-state VCA: fast, peak-reading, and hard-kneed.
    Constraint::Fixed {
        param: "style",
        value: 1.0,
    },
    Constraint::Fixed {
        param: "attack_ms",
        value: 1.2,
    },
    Constraint::Clamped {
        param: "release_ms",
        range: 40.0..=600.0,
    },
    Constraint::Clamped {
        param: "knee_db",
        range: 0.0..=4.0,
    },
    Constraint::Fixed {
        param: "detector_rms_mix",
        value: 0.15,
    },
    Constraint::Clamped {
        param: "drive",
        range: 0.0..=0.4,
    },
    Constraint::Fixed {
        param: "character_mode",
        value: 2.0,
    },
];

impl Profile for Dbx160Profile {
    fn id(&self) -> &'static str {
        "dbx160"
    }

    fn name(&self) -> &'static str {
        "dbx 160"
    }

    fn controls(&self) -> &[ProfileControl] {
        CONTROLS
    }

    fn constraints(&self) -> &[Constraint] {
        CONSTRAINTS
    }
}
