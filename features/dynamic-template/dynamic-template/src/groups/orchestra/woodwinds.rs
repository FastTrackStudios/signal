//! Woodwinds section definitions

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

pub mod bassoons;
pub mod clarinets;
pub mod flutes;
pub mod oboes;
pub mod piccolo;

pub use bassoons::Bassoons;
pub use clarinets::Clarinets;
pub use flutes::Flutes;
pub use oboes::Oboes;
pub use piccolo::Piccolo;

/// Woodwinds section containing all woodwind instruments
pub struct Woodwinds;

impl From<Woodwinds> for Group<ItemMetadata> {
    fn from(_val: Woodwinds) -> Self {
        Self::builder("Woodwinds")
            .patterns(vec!["woodwind", "woodwinds", "winds"])
            .group(Piccolo)
            .group(Flutes)
            .group(Oboes)
            .group(Clarinets)
            .group(Bassoons)
            .build()
    }
}
