//! Strings section definitions

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

pub mod cello;
pub mod contrabass;
pub mod viola;
pub mod violins;

pub use cello::Cello;
pub use contrabass::Contrabass;
pub use viola::Viola;
pub use violins::Violins;

/// Strings section containing all string instruments
pub struct Strings;

impl From<Strings> for Group<ItemMetadata> {
    fn from(_val: Strings) -> Self {
        Self::builder("Strings")
            .patterns(vec!["strings", "string_section"])
            .group(Violins)
            .group(Viola)
            .group(Cello)
            .group(Contrabass)
            .build()
    }
}
