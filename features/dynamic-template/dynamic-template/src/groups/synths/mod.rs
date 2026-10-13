//! Synthesizer group definitions

use crate::item_metadata::{ItemMetadata, ItemMetadataGroup};
use monarchy::Group;

/// Per-project layer stacking shared by all synth types (named per take).
/// Synths are programmed, so there's no Performer level and the synth type
/// itself is the arrangement — under it we deepen by layer then channel.
pub(super) fn layers_dimension() -> ItemMetadataGroup {
    ItemMetadataGroup::builder("Layers").build()
}

/// Channel dimension (L/C/R) shared by all synth types.
pub(super) fn channel_dimension() -> ItemMetadataGroup {
    ItemMetadataGroup::builder("Channel")
        .patterns(["L", "C", "R", "Left", "Center", "Right"])
        .build()
}

pub mod arp;
pub mod chord;
pub mod fx;
pub mod keys;
pub mod lead;
pub mod pad;

pub use arp::Arp;
pub use chord::Chord;
pub use fx::Fx;
pub use keys::Keys;
pub use lead::Lead;
pub use pad::Pad;

/// Prophet subgroup for Sequential Prophet synthesizers
pub struct Prophet;

impl From<Prophet> for Group<ItemMetadata> {
    fn from(_val: Prophet) -> Self {
        Self::builder("Prophet").patterns(vec!["prophet"]).build()
    }
}

/// Top-level synths group containing all synthesizer types
pub struct Synths;

impl From<Synths> for Group<ItemMetadata> {
    fn from(_val: Synths) -> Self {
        Self::builder("Synths")
            .prefix("SY")
            .patterns(vec!["synth", "synths", "synthesizer", "bells", "sampler"])
            // A talkback mic is named for whoever it belongs to ("TB Synth"),
            // so without this it classifies as the thing it is listening to.
            .exclude(["tb", "talkback", "vca", "hp", "headphone"])
            .group(Prophet)
            .group(Lead)
            .group(Pad)
            .group(Arp)
            .group(Chord)
            .group(Keys)
            .group(Fx)
            .build()
    }
}
