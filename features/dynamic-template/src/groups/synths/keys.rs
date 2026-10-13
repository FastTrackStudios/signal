//! Synth keys group definition

use crate::item_metadata::prelude::ItemMetadataGroupExt;
use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Keys group (for synthesizer keys sounds)
pub struct Keys;

impl From<Keys> for Group<ItemMetadata> {
    fn from(_val: Keys) -> Self {
        Self::builder("Keys")
            .patterns(vec!["polysynth", "poly"])
            .layers(super::layers_dimension())
            .channel(super::channel_dimension())
            .build()
    }
}
