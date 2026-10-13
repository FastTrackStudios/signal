//! Flutes group definition

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Flutes group
pub struct Flutes;

impl From<Flutes> for Group<ItemMetadata> {
    fn from(_val: Flutes) -> Self {
        Self::builder("Flutes")
            .patterns(vec!["flute", "flutes", "fl"])
            .build()
    }
}
