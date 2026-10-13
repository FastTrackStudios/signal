//! Piano group definition

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Piano group
pub struct Piano;

impl From<Piano> for Group<ItemMetadata> {
    fn from(_val: Piano) -> Self {
        Self::builder("Piano")
            .patterns(vec!["piano", "grand", "upright_piano", "steinway"])
            .build()
    }
}
