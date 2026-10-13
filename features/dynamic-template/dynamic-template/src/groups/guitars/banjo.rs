//! Banjo group definition

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Banjo group
pub struct Banjo;

impl From<Banjo> for Group<ItemMetadata> {
    fn from(_val: Banjo) -> Self {
        Self::builder("Banjo")
            .patterns(vec!["banjo", "bjo"])
            .build()
    }
}
