//! Harp group definition

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Harp group
pub struct Harp;

impl From<Harp> for Group<ItemMetadata> {
    fn from(_val: Harp) -> Self {
        Self::builder("Harp")
            .patterns(vec!["harp", "harps"])
            .build()
    }
}
