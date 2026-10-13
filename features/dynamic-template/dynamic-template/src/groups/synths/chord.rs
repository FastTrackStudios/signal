//! Synth chord group definition

use crate::item_metadata::prelude::ItemMetadataGroupExt;
use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Chord group
pub struct Chord;

impl From<Chord> for Group<ItemMetadata> {
    fn from(_val: Chord) -> Self {
        Self::builder("Chord")
            .patterns(vec!["chord", "stab"])
            .layers(super::layers_dimension())
            .channel(super::channel_dimension())
            .build()
    }
}
