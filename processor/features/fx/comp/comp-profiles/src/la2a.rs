//! Native panel controls for the measured LA-2A Gray model, in Compress mode.
//! The gain law and optical cell belong to comp-dsp, not to UI parameter macros.
use crate::{Constraint, ParamMapping, Profile, ProfileControl};
pub struct La2aProfile;
static CONTROLS: &[ProfileControl] = &[
    ProfileControl {
        id: "peak_reduction",
        label: "Peak Reduction",
        mapping: ParamMapping::Direct {
            param: "la2a_peak_reduction",
            range: 0.0..=1.0,
        },
    },
    ProfileControl {
        id: "gain",
        label: "Gain",
        mapping: ParamMapping::Direct {
            param: "la2a_gain",
            range: 0.0..=1.0,
        },
    },
];
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
        &[]
    }
}
