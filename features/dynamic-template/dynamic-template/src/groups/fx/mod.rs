//! Effects group definitions

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

pub mod delay;
pub mod distortion;
pub mod dynamics;
pub mod eq;
pub mod modulation;
pub mod pitch;
pub mod reverb;

pub use delay::Delay;
pub use distortion::Distortion;
pub use dynamics::Dynamics;
pub use eq::EQ;
pub use modulation::Modulation;
pub use pitch::Pitch;
pub use reverb::Reverb;

/// Top-level effects group containing all effect types
pub struct Effects;

impl From<Effects> for Group<ItemMetadata> {
    fn from(_val: Effects) -> Self {
        Group::builder("Effects")
            .prefix("FX")
            .patterns(vec!["effect", "effects", "fx"])
            .group(Reverb)
            .group(Delay)
            .group(EQ)
            .group(Dynamics)
            .group(Modulation)
            .group(Distortion)
            .group(Pitch)
            .build()
    }
}
