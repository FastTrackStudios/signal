//! Trumpet group definition

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Trumpet group (includes flugelhorn)
pub struct Trumpet;

impl From<Trumpet> for Group<ItemMetadata> {
    fn from(_val: Trumpet) -> Self {
        Self::builder("Trumpet")
            .patterns(vec![
                "trumpet",
                "tpt",
                "flugelhorn",
                "fluegel",
                "flugel",
                "cornet",
            ])
            .build()
    }
}
