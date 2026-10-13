//! Tuba group definition

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Tuba group (includes euphonium)
pub struct Tuba;

impl From<Tuba> for Group<ItemMetadata> {
    fn from(_val: Tuba) -> Self {
        Self::builder("Tuba")
            .patterns(vec!["tuba", "euphonium", "euph"])
            .build()
    }
}
