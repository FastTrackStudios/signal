//! Tube-Tech CL 1B — the other optical tube compressor.
//!
//! Where the LA-2A gives you two knobs and an opinion, the CL 1B gives you the
//! same optical/tube behaviour with the controls unlocked: continuously
//! variable ratio, threshold, attack and release, plus output gain. That is
//! the whole reason both units are in the Opto category — same physics,
//! opposite philosophy about how much of it you get to touch.
//!
//! Panel: five large black knobs on the Tube-Tech blue face, VU on the left,
//! and an attack/release mode switch (fixed / manual / a blend of the two).

use crate::{Constraint, ParamMapping, Profile, ProfileControl};

pub struct Cl1bProfile;

static CONTROLS: &[ProfileControl] = &[
    ProfileControl {
        id: "threshold",
        label: "Threshold",
        mapping: ParamMapping::Direct {
            param: "threshold_db",
            range: -40.0..=0.0,
        },
    },
    ProfileControl {
        id: "ratio",
        label: "Ratio",
        mapping: ParamMapping::Direct {
            param: "ratio",
            range: 2.0..=10.0,
        },
    },
    // The unit's own ranges: 0.5–300 ms attack, 0.05–10 s release.
    ProfileControl {
        id: "attack",
        label: "Attack",
        mapping: ParamMapping::Direct {
            param: "attack_ms",
            range: 0.5..=300.0,
        },
    },
    ProfileControl {
        id: "release",
        label: "Release",
        mapping: ParamMapping::Direct {
            param: "release_ms",
            range: 50.0..=3000.0,
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
];

static CONSTRAINTS: &[Constraint] = &[
    // Opto detector, RMS-ish, with the tube stage lightly in play.
    Constraint::Fixed {
        param: "style",
        value: 2.0,
    },
    Constraint::Fixed {
        param: "detector_rms_mix",
        value: 0.85,
    },
    Constraint::Fixed {
        param: "channel_link",
        value: 1.0,
    },
    Constraint::Clamped {
        param: "knee_db",
        range: 6.0..=20.0,
    },
    Constraint::Clamped {
        param: "drive",
        range: 0.0..=0.22,
    },
    Constraint::Fixed {
        param: "character_mode",
        value: 1.0,
    },
];

impl Profile for Cl1bProfile {
    fn id(&self) -> &'static str {
        "cl1b"
    }

    fn name(&self) -> &'static str {
        "CL 1B"
    }

    fn controls(&self) -> &[ProfileControl] {
        CONTROLS
    }

    fn constraints(&self) -> &[Constraint] {
        CONSTRAINTS
    }
}
