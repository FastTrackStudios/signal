//! Synth arpeggio group definition

use crate::item_metadata::prelude::ItemMetadataGroupExt;
use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Arpeggio group
pub struct Arp;

impl From<Arp> for Group<ItemMetadata> {
    fn from(_val: Arp) -> Self {
        Self::builder("Arp")
            .patterns(vec!["arp", "arpeggio"])
            .layers(super::layers_dimension())
            .channel(super::channel_dimension())
            .build()
    }
}
