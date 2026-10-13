//! Electronic drum kit group definitions

use crate::item_metadata::{ItemMetadata, ItemMetadataField};
use monarchy::{FieldValueDescriptor, Group};

mod kick;
mod snare;
// mod hat;
// mod pad;
// mod trigger;

pub use kick::Kick;
pub use snare::Snare;
// pub use hat::Hat;
// pub use pad::Pad;
// pub use trigger::Trigger;

/// Electronic drum kit container group
pub struct ElectronicKit;

impl From<ElectronicKit> for Group<ItemMetadata> {
    fn from(_val: ElectronicKit) -> Self {
        // Define variant descriptors for different drum machine types
        let variant_descriptors = vec![
            FieldValueDescriptor::builder("808")
                .patterns(["808", "tr-808", "tr808"])
                .build(),
            FieldValueDescriptor::builder("909")
                .patterns(["909", "tr-909", "tr909"])
                .build(),
            FieldValueDescriptor::builder("707")
                .patterns(["707", "tr-707", "tr707"])
                .build(),
            FieldValueDescriptor::builder("LinnDrum")
                .patterns(["linn", "linndrum", "linn drum"])
                .build(),
            FieldValueDescriptor::builder("SP-1200")
                .patterns(["sp1200", "sp-1200", "1200"])
                .build(),
            FieldValueDescriptor::builder("MPC")
                .patterns(["mpc", "mpc60", "mpc3000"])
                .build(),
        ];

        Self::builder("Electronic Kit")
            .patterns(vec![
                "electronic",
                "e-kit",
                "ekit",
                "808",
                "909",
                "707",
                "drum machine",
                "machine",
                "midi drum",
                "vst drum",
            ])
            .field_value_descriptors(ItemMetadataField::Variant, variant_descriptors)
            .block_prefix("D") // Avoid "D Electronic Drums" redundancy
            .group(Kick)
            .group(Snare)
            // .group(Hat)
            // .group(Pad)
            // .group(Trigger)
            .build()
    }
}
