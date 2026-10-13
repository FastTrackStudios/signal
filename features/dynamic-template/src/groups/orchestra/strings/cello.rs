//! Cello group definition

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Cello group
pub struct Cello;

impl From<Cello> for Group<ItemMetadata> {
    fn from(_val: Cello) -> Self {
        Self::builder("Cello")
            .patterns(vec!["cello", "cellos", "violoncello", "vc"])
            .build()
    }
}
