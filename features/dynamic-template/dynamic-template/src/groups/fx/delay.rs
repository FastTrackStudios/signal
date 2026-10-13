//! Delay effect group definition

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Delay effect group
pub struct Delay;

impl From<Delay> for Group<ItemMetadata> {
    fn from(_val: Delay) -> Self {
        Group::builder("Delay")
            .patterns(vec![
                "delay",
                "echo",
                "tape_delay",
                "analog_delay",
                "digital_delay",
                "ping_pong",
                "slapback",
                "dub_delay",
                "multitap",
            ])
            .build()
    }
}
