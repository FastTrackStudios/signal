//! Accordion group definition

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Accordion group
pub struct Accordion;

impl From<Accordion> for Group<ItemMetadata> {
    fn from(_val: Accordion) -> Self {
        Self::builder("Accordion")
            .patterns(vec!["accordion", "accordian", "squeeze box", "squeezebox"])
            .build()
    }
}
