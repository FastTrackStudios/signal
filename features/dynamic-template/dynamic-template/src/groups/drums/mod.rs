//! Drum-related group definitions

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

pub mod drum_kit;
pub mod electronic_kit;

use drum_kit::DrumKit;
use electronic_kit::ElectronicKit;

// Only export the kit modules, not the individual drum types
// This avoids naming conflicts between drum_kit::Kick and electronic_kit::Kick
// Users should access specific drums as drum_kit::Kick, electronic_kit::Kick, etc.

/// Top-level drums group containing all drum kit types
pub struct Drums;

impl From<Drums> for Group<ItemMetadata> {
    fn from(_val: Drums) -> Self {
        Self::builder("Drums")
            .prefix("D")
            // No patterns - this is just a container group
            // The nested groups will handle pattern matching
            // A talkback mic is named for whoever it belongs to ("TB Drums"),
            // so without this it classifies as the thing it is listening to.
            // The exclusion sits on the container because a pattern-less group
            // matches everything, so blocking it here blocks the whole subtree.
            .exclude(["tb", "talkback", "vca", "hp", "headphone"])
            .group(DrumKit)
            .group(ElectronicKit)
            .build()
    }
}
