//! Synth lead group definition

use crate::item_metadata::prelude::ItemMetadataGroupExt;
use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Lead synth group
///
/// Note: Uses specific synth-lead patterns to avoid matching guitar/vocal leads.
/// Generic "lead" pattern would incorrectly match "Lead Voc" or "`JohnyLead`" guitar.
pub struct Lead;

impl From<Lead> for Group<ItemMetadata> {
    fn from(_val: Lead) -> Self {
        Self::builder("Lead")
            .patterns(vec![
                "synth lead",
                "lead synth",
                "lead line", // Common synth term
            ])
            // This is a child of Synths, so requires_parent_match ensures
            // items only match if they already matched Synths
            .requires_parent_match()
            .layers(super::layers_dimension())
            .channel(super::channel_dimension())
            .build()
    }
}
