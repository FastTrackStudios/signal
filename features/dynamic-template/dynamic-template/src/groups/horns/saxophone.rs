//! Saxophone group definition

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Saxophone group
pub struct Saxophone;

impl From<Saxophone> for Group<ItemMetadata> {
    fn from(_val: Saxophone) -> Self {
        Self::builder("Saxophone")
            .patterns(vec![
                // Generic patterns
                "sax",
                "saxophone",
                // Saxophone types
                "soprano sax",
                "alto sax",
                "tenor sax",
                "baritone sax",
                "bari sax",
                // Common abbreviations
                "ssax",
                "asax",
                "tsax",
                "bsax",
            ])
            .build()
    }
}
