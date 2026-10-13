//! Violins group definitions

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

pub mod first_violin;
pub mod second_violin;

pub use first_violin::FirstViolin;
pub use second_violin::SecondViolin;

/// Transparent violins group containing first and second violins
/// This is transparent so First Violin and Second Violin appear at the Strings level
pub struct Violins;

impl From<Violins> for Group<ItemMetadata> {
    fn from(_val: Violins) -> Self {
        Self::builder("Violins")
            .patterns(vec!["violin", "violins", "vln"])
            // Make transparent so First Violin and Second Violin appear at Strings level
            .transparent()
            .group(FirstViolin)
            .group(SecondViolin)
            .build()
    }
}
