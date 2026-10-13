//! SSL E-Series EQ profile — 4 bands (LF/LMF/HMF/HF) + HPF/LPF.

use crate::core::{Constraint, ParamMapping, Profile, ProfileControl};

pub struct SslEProfile;

impl Profile for SslEProfile {
    fn id(&self) -> &'static str {
        "eq_ssl_e"
    }
    fn name(&self) -> &'static str {
        "SSL E-Series"
    }
    fn controls(&self) -> &[ProfileControl] {
        &SSL_E_CONTROLS
    }
    fn constraints(&self) -> &[Constraint] {
        &SSL_E_CONSTRAINTS
    }
}

static SSL_E_CONTROLS: [ProfileControl; 10] = [
    ProfileControl {
        id: "hpf",
        label: "HPF",
        mapping: ParamMapping::Direct {
            param: "ssl_e_hpf",
            range: 16.0..=350.0,
        },
    },
    ProfileControl {
        id: "lpf",
        label: "LPF",
        mapping: ParamMapping::Direct {
            param: "ssl_e_lpf",
            range: 3000.0..=22000.0,
        },
    },
    ProfileControl {
        id: "lf_freq",
        label: "LF Frequency",
        mapping: ParamMapping::Direct {
            param: "ssl_e_lf_freq",
            range: 30.0..=450.0,
        },
    },
    ProfileControl {
        id: "lf_gain",
        label: "LF Gain",
        mapping: ParamMapping::Direct {
            param: "ssl_e_lf_gain",
            range: -15.0..=15.0,
        },
    },
    ProfileControl {
        id: "lmf_freq",
        label: "LMF Frequency",
        mapping: ParamMapping::Direct {
            param: "ssl_e_lmf_freq",
            range: 200.0..=2500.0,
        },
    },
    ProfileControl {
        id: "lmf_gain",
        label: "LMF Gain",
        mapping: ParamMapping::Direct {
            param: "ssl_e_lmf_gain",
            range: -15.0..=15.0,
        },
    },
    ProfileControl {
        id: "hmf_freq",
        label: "HMF Frequency",
        mapping: ParamMapping::Direct {
            param: "ssl_e_hmf_freq",
            range: 600.0..=7000.0,
        },
    },
    ProfileControl {
        id: "hmf_gain",
        label: "HMF Gain",
        mapping: ParamMapping::Direct {
            param: "ssl_e_hmf_gain",
            range: -15.0..=15.0,
        },
    },
    ProfileControl {
        id: "hf_freq",
        label: "HF Frequency",
        mapping: ParamMapping::Direct {
            param: "ssl_e_hf_freq",
            range: 1500.0..=16000.0,
        },
    },
    ProfileControl {
        id: "hf_gain",
        label: "HF Gain",
        mapping: ParamMapping::Direct {
            param: "ssl_e_hf_gain",
            range: -15.0..=15.0,
        },
    },
];

static SSL_E_CONSTRAINTS: [Constraint; 1] = [Constraint::Fixed {
    param: "model",
    value: 4.0,
}];
