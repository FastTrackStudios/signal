//! Clavichord group definition

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Clavichord group
pub struct Clavichord;

impl From<Clavichord> for Group<ItemMetadata> {
    fn from(_val: Clavichord) -> Self {
        Self::builder("Clavichord")
            .patterns(vec!["clavichord", "clavi"])
            .build()
    }
}
