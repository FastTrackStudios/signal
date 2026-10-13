//! Horns group definition

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Horns group
pub struct Horns;

impl From<Horns> for Group<ItemMetadata> {
    fn from(_val: Horns) -> Self {
        Self::builder("Horns")
            .patterns(vec!["horn", "horns", "french_horn", "frenchhorn"])
            // Exclude "Horn Vocal" tracks — those are session horn players singing,
            // handled by the top-level Horns group instead.
            .exclude(vec!["vocal"])
            .build()
    }
}
