//! Upright bass group definition

use crate::item_metadata::prelude::*;

/// Upright bass group
pub struct UprightBass;

impl From<UprightBass> for ItemMetadataGroup {
    fn from(_val: UprightBass) -> Self {
        Self::builder("Upright Bass")
            .patterns(["upright", "double_bass", "acoustic_bass"])
            .build()
    }
}
