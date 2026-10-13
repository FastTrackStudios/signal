//! API 550A EQ profile — 3 bands with proportional Q.

use crate::core::{Constraint, ParamMapping, Profile, ProfileControl};

pub struct Api550aProfile;

impl Profile for Api550aProfile {
    fn id(&self) -> &'static str {
        "eq_api_550a"
    }
    fn name(&self) -> &'static str {
        "API 550A"
    }
    fn controls(&self) -> &[ProfileControl] {
        &API_550A_CONTROLS
    }
    fn constraints(&self) -> &[Constraint] {
        &API_550A_CONSTRAINTS
    }
}

static API_550A_CONTROLS: [ProfileControl; 6] = [
    ProfileControl {
        id: "low_freq",
        label: "Low Frequency",
        mapping: ParamMapping::Stepped {
            param: "api_low_freq",
            values: &[50.0, 100.0, 200.0, 400.0],
            labels: &["50", "100", "200", "400"],
        },
    },
    ProfileControl {
        id: "low_gain",
        label: "Low Gain",
        mapping: ParamMapping::Direct {
            param: "api_low_gain",
            range: -12.0..=12.0,
        },
    },
    ProfileControl {
        id: "mid_freq",
        label: "Mid Frequency",
        mapping: ParamMapping::Stepped {
            param: "api_mid_freq",
            values: &[400.0, 800.0, 1500.0, 3000.0, 5000.0],
            labels: &["400", "800", "1.5k", "3k", "5k"],
        },
    },
    ProfileControl {
        id: "mid_gain",
        label: "Mid Gain",
        mapping: ParamMapping::Direct {
            param: "api_mid_gain",
            range: -12.0..=12.0,
        },
    },
    ProfileControl {
        id: "high_freq",
        label: "High Frequency",
        mapping: ParamMapping::Stepped {
            param: "api_high_freq",
            values: &[5000.0, 7000.0, 10000.0, 12500.0, 15000.0],
            labels: &["5k", "7k", "10k", "12.5k", "15k"],
        },
    },
    ProfileControl {
        id: "high_gain",
        label: "High Gain",
        mapping: ParamMapping::Direct {
            param: "api_high_gain",
            range: -12.0..=12.0,
        },
    },
];

static API_550A_CONSTRAINTS: [Constraint; 1] = [Constraint::Fixed {
    param: "model",
    value: 3.0,
}];
