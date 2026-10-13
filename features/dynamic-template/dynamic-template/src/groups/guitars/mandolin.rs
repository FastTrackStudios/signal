//! Mandolin group definition
//!
//! Mandolin is a plucked stringed instrument closely related to the acoustic guitar
//! family, common in folk, country, bluegrass, and rock sessions.

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Mandolin subgroup (nested under Acoustic Guitar)
pub struct Mandolin;

impl From<Mandolin> for Group<ItemMetadata> {
    fn from(_val: Mandolin) -> Self {
        Self::builder("Mando")
            .patterns(vec!["mando", "mandolin"])
            .build()
    }
}
