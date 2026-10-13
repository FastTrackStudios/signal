//! Snare drum group definition

use crate::item_metadata::prelude::*;

/// Snare drum group
pub struct Snare;

impl From<Snare> for ItemMetadataGroup {
    fn from(_val: Snare) -> Self {
        use crate::item_metadata::ItemMetadataField;
        use monarchy::FieldValueDescriptor;

        // Define multi-mic positions using field_value_descriptors to ensure Top comes before Bottom
        // Order matters - items will be sorted by the order of descriptors
        let multi_mic_descriptors = vec![
            FieldValueDescriptor::builder("Top")
                .patterns(["Top", "top"])
                .build(),
            FieldValueDescriptor::builder("Bottom")
                .patterns(["Bottom", "bottom", "Bot", "bot", "Btm", "btm"])
                .build(),
            FieldValueDescriptor::builder("Side")
                .patterns(["Side", "side"])
                .build(),
            FieldValueDescriptor::builder("OH")
                .patterns(["OH", "oh", "overhead"])
                .build(),
            FieldValueDescriptor::builder("Trig")
                .patterns(["Trig", "trig", "trigger", "Sample", "sample", "samp"])
                .build(),
        ];

        // Define SUM tagged collection - all items go into SUM EXCEPT effect prints
        // Effect prints (VERB, DLY, etc.) will NOT match and stay as siblings to SUM
        // Empty patterns = matches everything, exclude() adds negative patterns for effects
        let sum_collection = Self::builder("SUM")
            .exclude(["verb", "vrb", "rev", "dly", "delay", "crush", "dist", "fx"])
            .build();

        // Use the convenience method - extension trait is in scope via prelude
        Self::builder("Snare")
            .patterns([
                "snare",
                "snr",
                "sn",
                "snareloop",
                "snare_loop",
                "snare loop",
            ])
            .field_value_descriptors(ItemMetadataField::MultiMic, multi_mic_descriptors)
            .tagged_collection(sum_collection)
            .build()
    }
}
