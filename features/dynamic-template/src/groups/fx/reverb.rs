//! Reverb effect group definition

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Reverb effect group
pub struct Reverb;

impl From<Reverb> for Group<ItemMetadata> {
    fn from(_val: Reverb) -> Self {
        Group::builder("Reverb")
            .patterns(vec![
                "reverb",
                "verb",
                "room",
                "hall",
                "plate",
                "spring",
                "chamber",
                "cathedral",
                "ambience",
                "space",
            ])
            .build()
    }
}
