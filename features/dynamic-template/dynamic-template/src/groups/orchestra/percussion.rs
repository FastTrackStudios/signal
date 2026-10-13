//! Orchestral percussion group definition

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Orchestral percussion group
pub struct Percussion;

impl From<Percussion> for Group<ItemMetadata> {
    fn from(_val: Percussion) -> Self {
        Self::builder("Percussion")
            .patterns(vec![
                "timpani",
                "timps",
                "timp",
                "snare",
                "snare_drum",
                "bass_drum",
                "gran_cassa",
                "cymbals",
                "crash",
                "suspended_cymbal",
                "triangle",
                "tambourine",
                "gong",
                "tam-tam",
                "tamtam",
                "glockenspiel",
                "glock",
                "xylophone",
                "xylo",
                "marimba",
                "vibraphone",
                "vibes",
                "tubular_bells",
                "chimes",
                "crotales",
                "wood_block",
                "woodblock",
                "temple_blocks",
                "castanets",
                "ratchet",
                "slapstick",
                "whip",
            ])
            .build()
    }
}
