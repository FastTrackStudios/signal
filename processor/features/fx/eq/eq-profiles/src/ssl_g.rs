//! SSL G-Series EQ profile — wider, smoother 4-band console EQ target.

use crate::core::{Constraint, ParamMapping, Profile, ProfileControl};

pub struct SslGProfile;

impl Profile for SslGProfile {
    fn id(&self) -> &'static str {
        "eq_ssl_g"
    }
    fn name(&self) -> &'static str {
        "SSL G-Series"
    }
    fn controls(&self) -> &[ProfileControl] {
        &SSL_G_CONTROLS
    }
    fn constraints(&self) -> &[Constraint] {
        &SSL_G_CONSTRAINTS
    }
}

static SSL_G_CONTROLS: [ProfileControl; 8] = [
    ProfileControl {
        id: "lf_freq",
        label: "LF Frequency",
        mapping: ParamMapping::Direct {
            param: "ssl_g_lf_freq",
            range: 30.0..=450.0,
        },
    },
    ProfileControl {
        id: "lf_gain",
        label: "LF Gain",
        mapping: ParamMapping::Direct {
            param: "ssl_g_lf_gain",
            range: -15.0..=15.0,
        },
    },
    ProfileControl {
        id: "lmf_freq",
        label: "LMF Frequency",
        mapping: ParamMapping::Direct {
            param: "ssl_g_lmf_freq",
            range: 200.0..=2500.0,
        },
    },
    ProfileControl {
        id: "lmf_gain",
        label: "LMF Gain",
        mapping: ParamMapping::Direct {
            param: "ssl_g_lmf_gain",
            range: -15.0..=15.0,
        },
    },
    ProfileControl {
        id: "hmf_freq",
        label: "HMF Frequency",
        mapping: ParamMapping::Direct {
            param: "ssl_g_hmf_freq",
            range: 600.0..=7000.0,
        },
    },
    ProfileControl {
        id: "hmf_gain",
        label: "HMF Gain",
        mapping: ParamMapping::Direct {
            param: "ssl_g_hmf_gain",
            range: -15.0..=15.0,
        },
    },
    ProfileControl {
        id: "hf_freq",
        label: "HF Frequency",
        mapping: ParamMapping::Direct {
            param: "ssl_g_hf_freq",
            range: 1500.0..=16000.0,
        },
    },
    ProfileControl {
        id: "hf_gain",
        label: "HF Gain",
        mapping: ParamMapping::Direct {
            param: "ssl_g_hf_gain",
            range: -15.0..=15.0,
        },
    },
];

static SSL_G_CONSTRAINTS: [Constraint; 1] = [Constraint::Fixed {
    param: "model",
    value: 5.0,
}];
