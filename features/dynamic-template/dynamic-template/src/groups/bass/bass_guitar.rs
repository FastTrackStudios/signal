//! Bass guitar group definition

use crate::item_metadata::prelude::*;
use crate::item_metadata::ItemMetadataField;
use monarchy::FieldValueDescriptor;

/// Bass guitar group
pub struct BassGuitar;

impl From<BassGuitar> for ItemMetadataGroup {
    fn from(_val: BassGuitar) -> Self {
        // Define bass model/variant descriptors
        let variant_descriptors = vec![
            FieldValueDescriptor::builder("Hofner")
                .patterns(["hofner", "höfner"])
                .build(),
            FieldValueDescriptor::builder("Precision")
                .patterns(["precision", "p-bass", "pbass", "p bass"])
                .build(),
            FieldValueDescriptor::builder("Jazz")
                .patterns(["jazz bass", "j-bass", "jbass", "j bass"])
                .build(),
            FieldValueDescriptor::builder("Rickenbacker")
                .patterns(["rickenbacker", "ric", "ricky"])
                .build(),
            FieldValueDescriptor::builder("Stingray")
                .patterns(["stingray", "musicman", "music man"])
                .build(),
            FieldValueDescriptor::builder("Thunderbird")
                .patterns(["thunderbird", "t-bird", "tbird"])
                .build(),
            FieldValueDescriptor::builder("Mustang")
                .patterns(["mustang bass", "mustang"])
                .build(),
        ];

        Self::builder("Guitar")
            .patterns([
                "bass_guitar",
                "bassguitar",
                "bass guitar",
                "electric_bass",
                "electric bass",
                "elec_bass",
                "elecbass",
                "elec bass",
                // Common bass guitar models (also used for matching)
                "hofner",
                "precision",
                "p-bass",
                "pbass",
                "jazz bass",
                "j-bass",
                "jbass",
                "rickenbacker",
                "stingray",
                "thunderbird",
                "mustang bass",
            ])
            .field_value_descriptors(ItemMetadataField::Variant, variant_descriptors)
            // Bass guitar capture sources: DI box plus a mic'd amp.
            .field_value_descriptors(
                ItemMetadataField::MultiMic,
                vec![
                    FieldValueDescriptor::builder("DI").patterns(["di"]).build(),
                    FieldValueDescriptor::builder("Amp")
                        .patterns(["amp", "amplifier"])
                        .build(),
                ],
            )
            .build()
    }
}
