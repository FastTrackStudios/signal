//! Brass section definitions

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

pub mod horns;
pub mod trombones;
pub mod trumpets;
pub mod tuba;

pub use horns::Horns;
pub use trombones::Trombones;
pub use trumpets::Trumpets;
pub use tuba::Tuba;

/// Brass section containing all brass instruments
pub struct Brass;

impl From<Brass> for Group<ItemMetadata> {
    fn from(_val: Brass) -> Self {
        Self::builder("Brass")
            .patterns(vec!["brass"])
            .group(Trumpets)
            .group(Horns)
            .group(Trombones)
            .group(Tuba)
            .build()
    }
}
