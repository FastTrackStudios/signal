//! Organ group definition

use crate::item_metadata::{ItemMetadata, ItemMetadataGroupExt};
use monarchy::Group;

/// Organ group
pub struct Organ;

impl From<Organ> for Group<ItemMetadata> {
    fn from(_val: Organ) -> Self {
        // Organ playing styles and parts
        // "Organ Chords" = chord/pad part, "Organ Notes" = melodic line, "Organ Slide" = glissando/slide
        let arrangement = Self::builder("Arrangement")
            .patterns(vec![
                "chords", "chord", "pad", "pads", "notes", "note", "melody", "melodic", "slide",
                "gliss", "stab", "stabs", "bass", "lead", "solo", "riff", "lick",
            ])
            .build();

        Self::builder("Organ")
            .patterns(vec![
                "organ",
                "hammond",
                "b3",
                "leslie",
                "church_organ",
                "pipe_organ",
            ])
            .arrangement(arrangement)
            .build()
    }
}
