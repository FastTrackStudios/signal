//! Bassoons group definition

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Bassoons group
pub struct Bassoons;

impl From<Bassoons> for Group<ItemMetadata> {
    fn from(_val: Bassoons) -> Self {
        Self::builder("Bassoons")
            .patterns(vec!["bassoon", "bassoons", "bsn"])
            .build()
    }
}
