//! Harmonica group definition

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Harmonica group
pub struct Harmonica;

impl From<Harmonica> for Group<ItemMetadata> {
    fn from(_val: Harmonica) -> Self {
        Self::builder("Harmonica")
            .patterns(vec![
                "harmonica",
                "harp", // Common blues term for harmonica
                "mouth harp",
                "blues harp",
            ])
            // Exclude "harp" when it's part of harpsichord
            .exclude(vec!["harpsichord"])
            .build()
    }
}
