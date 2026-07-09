//! Neve 1073 EQ profile — 3 bands + HPF with fixed frequency selections.

use crate::core::{Constraint, ParamMapping, Profile, ProfileControl};

pub struct Neve1073Profile;

impl Profile for Neve1073Profile {
    fn id(&self) -> &'static str {
        "eq_neve_1073"
    }
    fn name(&self) -> &'static str {
        "Neve 1073"
    }
    fn controls(&self) -> &[ProfileControl] {
        &NEVE_1073_CONTROLS
    }
    fn constraints(&self) -> &[Constraint] {
        &NEVE_1073_CONSTRAINTS
    }
}

static NEVE_1073_CONTROLS: [ProfileControl; 10] = [
    ProfileControl {
        id: "trim",
        label: "Trim",
        mapping: ParamMapping::Direct {
            param: "neve_trim",
            range: -24.0..=24.0,
        },
    },
    ProfileControl {
        id: "drive",
        label: "Drive",
        mapping: ParamMapping::Direct {
            param: "neve_drive",
            range: 0.0..=100.0,
        },
    },
    ProfileControl {
        id: "hpf_freq",
        label: "HPF",
        mapping: ParamMapping::Stepped {
            param: "neve_hpf",
            values: &[0.0, 50.0, 80.0, 160.0, 300.0],
            labels: &["Off", "50", "80", "160", "300"],
        },
    },
    ProfileControl {
        id: "low_freq",
        label: "Low Frequency",
        mapping: ParamMapping::Stepped {
            param: "neve_low_freq",
            values: &[0.0, 35.0, 60.0, 110.0, 220.0],
            labels: &["Off", "35", "60", "110", "220"],
        },
    },
    ProfileControl {
        id: "low_gain",
        label: "Low Shelf",
        mapping: ParamMapping::Direct {
            param: "neve_low_gain",
            range: -16.0..=16.0,
        },
    },
    ProfileControl {
        id: "mid_freq",
        label: "Mid Frequency",
        mapping: ParamMapping::Stepped {
            param: "neve_mid_freq",
            values: &[0.0, 360.0, 700.0, 1600.0, 3200.0, 4800.0, 7200.0],
            labels: &["Off", "360", "700", "1.6k", "3.2k", "4.8k", "7.2k"],
        },
    },
    ProfileControl {
        id: "mid_gain",
        label: "Mid Bell",
        mapping: ParamMapping::Direct {
            param: "neve_mid_gain",
            range: -18.0..=18.0,
        },
    },
    ProfileControl {
        id: "high_gain",
        label: "High Shelf",
        mapping: ParamMapping::Direct {
            param: "neve_high_gain",
            range: -16.0..=16.0,
        },
    },
    ProfileControl {
        id: "eq_on",
        label: "EQ In",
        mapping: ParamMapping::Direct {
            param: "neve_eq_in",
            range: 0.0..=1.0,
        },
    },
    ProfileControl {
        id: "phase",
        label: "Phase",
        mapping: ParamMapping::Direct {
            param: "neve_phase",
            range: 0.0..=1.0,
        },
    },
];

static NEVE_1073_CONSTRAINTS: [Constraint; 1] = [Constraint::Fixed {
    param: "model",
    value: 1.0,
}];
