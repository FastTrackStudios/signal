//! Harpsichord group definition

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Harpsichord group
pub struct Harpsichord;

impl From<Harpsichord> for Group<ItemMetadata> {
    fn from(_val: Harpsichord) -> Self {
        Self::builder("Harpsichord")
            .patterns(vec!["harpsichord", "harpsi"])
            .build()
    }
}
