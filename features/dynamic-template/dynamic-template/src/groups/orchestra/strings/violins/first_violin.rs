//! First Violin group definition

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// First Violin group
pub struct FirstViolin;

impl From<FirstViolin> for Group<ItemMetadata> {
    fn from(_val: FirstViolin) -> Self {
        Self::builder("First Violin")
            .patterns(vec![
                "first_violin",
                "firstviolin",
                "violin_1",
                "violin1",
                "vln1",
                "vln_1",
            ])
            .build()
    }
}
