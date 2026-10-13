//! Synth bass group definition

use crate::item_metadata::prelude::*;

/// Synth bass group
pub struct SynthBass;

impl From<SynthBass> for ItemMetadataGroup {
    fn from(_val: SynthBass) -> Self {
        Self::builder("Synth")
            .patterns([
                "synth_bass",
                "synthbass",
                "bass synth",
                "bass_synth",
                "sub_bass",
                "sub bass",
            ])
            .exclude(["808"]) // Exclude 808 to avoid matching electronic kick drums
            // Synth bass stacks into per-project layers (named per take).
            .layers(Self::builder("Layers").build())
            .build()
    }
}
