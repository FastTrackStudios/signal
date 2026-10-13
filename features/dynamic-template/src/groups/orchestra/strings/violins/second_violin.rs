//! Second Violin group definition

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Second Violin group
pub struct SecondViolin;

impl From<SecondViolin> for Group<ItemMetadata> {
    fn from(_val: SecondViolin) -> Self {
        Self::builder("Second Violin")
            .patterns(vec![
                "second_violin",
                "secondviolin",
                "violin_2",
                "violin2",
                "vln2",
                "vln_2",
            ])
            .build()
    }
}
