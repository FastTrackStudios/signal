//! Contrabass group definition

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Contrabass group
pub struct Contrabass;

impl From<Contrabass> for Group<ItemMetadata> {
    fn from(_val: Contrabass) -> Self {
        Self::builder("Contrabass")
            .patterns(vec![
                "contrabass",
                "double_bass",
                "doublebass",
                "string_bass",
                "stringbass",
                "upright_bass",
                "uprightbass",
                "cb",
                "db",
            ])
            .build()
    }
}
