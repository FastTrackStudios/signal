//! Viola group definition

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Viola group
pub struct Viola;

impl From<Viola> for Group<ItemMetadata> {
    fn from(_val: Viola) -> Self {
        Self::builder("Viola")
            .patterns(vec!["viola", "violas", "vla"])
            .build()
    }
}
