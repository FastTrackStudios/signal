//! Distortion effect group definition

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Distortion effect group
pub struct Distortion;

impl From<Distortion> for Group<ItemMetadata> {
    fn from(_val: Distortion) -> Self {
        Group::builder("Distortion")
            .patterns(vec![
                "distortion",
                "dist",
                "overdrive",
                "drive",
                "fuzz",
                "saturation",
                "saturator",
                "bitcrusher",
                "bitcrush",
                "waveshaper",
                "tube",
                "tape",
                "analog",
                "crunch",
                "grit",
                "dirt",
            ])
            .build()
    }
}
