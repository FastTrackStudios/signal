//! Trumpets group definition

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Trumpets group
pub struct Trumpets;

impl From<Trumpets> for Group<ItemMetadata> {
    fn from(_val: Trumpets) -> Self {
        Self::builder("Trumpets")
            .patterns(vec!["trumpet", "trumpets", "tpt"])
            .build()
    }
}
