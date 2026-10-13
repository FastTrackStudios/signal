//! Clarinets group definition

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Clarinets group
pub struct Clarinets;

impl From<Clarinets> for Group<ItemMetadata> {
    fn from(_val: Clarinets) -> Self {
        Self::builder("Clarinets")
            .patterns(vec!["clarinet", "clarinets", "cl"])
            .build()
    }
}
