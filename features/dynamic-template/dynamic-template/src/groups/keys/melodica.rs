//! Melodica group definition

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Melodica group
pub struct Melodica;

impl From<Melodica> for Group<ItemMetadata> {
    fn from(_val: Melodica) -> Self {
        Self::builder("Melodica")
            .patterns(vec!["melodica", "melodion", "pianica"])
            .build()
    }
}
