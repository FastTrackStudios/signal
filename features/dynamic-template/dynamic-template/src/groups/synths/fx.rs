//! Synth fx group definition
//!
//! This is for synthesizer-based sound design that's clearly part of the synth section,
//! NOT for generic "FX" tracks which should go to the top-level SFX group.

use crate::item_metadata::prelude::ItemMetadataGroupExt;
use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// FX group (for synthesizer effects/sound design)
///
/// Note: Generic "fx" pattern is handled by the top-level SFX group.
/// This group only matches when combined with synth context.
pub struct Fx;

impl From<Fx> for Group<ItemMetadata> {
    fn from(_val: Fx) -> Self {
        Self::builder("FX")
            // Removed generic "fx" and "effect" - those go to SFX group
            // These patterns are specific to synth-based sound design
            .patterns(vec![
                "synth fx",
                "synth effect",
                "synth sweep",
                "synth riser",
            ])
            .layers(super::layers_dimension())
            .channel(super::channel_dimension())
            .build()
    }
}
