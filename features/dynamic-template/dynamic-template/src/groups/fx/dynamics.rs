//! Dynamics effect group definition

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Dynamics effect group
pub struct Dynamics;

impl From<Dynamics> for Group<ItemMetadata> {
    fn from(_val: Dynamics) -> Self {
        Group::builder("Dynamics")
            .patterns(vec![
                "compressor",
                "comp",
                "limiter",
                "gate",
                "expander",
                "de-esser",
                "deesser",
                "transient",
                "maximizer",
                "leveler",
                "multiband",
                "opto",
                "vca",
                "fet",
                "tube",
            ])
            .build()
    }
}
