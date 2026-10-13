//! Trombone group definition

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Trombone group
pub struct Trombone;

impl From<Trombone> for Group<ItemMetadata> {
    fn from(_val: Trombone) -> Self {
        Self::builder("Trombone")
            .patterns(vec!["trombone", "trombones", "tbn"])
            .build()
    }
}
