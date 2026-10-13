//! Modulation effect group definition

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Modulation effect group
pub struct Modulation;

impl From<Modulation> for Group<ItemMetadata> {
    fn from(_val: Modulation) -> Self {
        Group::builder("Modulation")
            .patterns(vec![
                "modulation",
                "mod",
                "chorus",
                "flanger",
                "phaser",
                "tremolo",
                "vibrato",
                "rotary",
                "leslie",
                "ring_mod",
                "ringmod",
                "frequency_shifter",
                "ensemble",
            ])
            .build()
    }
}
