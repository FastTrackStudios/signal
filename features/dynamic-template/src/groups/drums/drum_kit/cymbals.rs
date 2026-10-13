//! Cymbals group definition

use crate::item_metadata::prelude::*;
use crate::item_metadata::ItemMetadataField;
use monarchy::FieldValueDescriptor;

/// Cymbals group
pub struct Cymbals;

impl From<Cymbals> for ItemMetadataGroup {
    fn from(_val: Cymbals) -> Self {
        // Define OH (overhead) positions using field_value_descriptors to ensure L comes before R
        // Order matters - items will be sorted by the order of descriptors
        let oh_position_descriptors = vec![
            FieldValueDescriptor::builder("L")
                .patterns(["L", "Left", "left"])
                .build(),
            FieldValueDescriptor::builder("R")
                .patterns(["R", "Right", "right"])
                .build(),
        ];

        let oh_group = Self::builder("OH")
            .patterns([
                "oh",
                "ohd",
                "ohl",
                "ohr",
                "ohs",
                "ovh",
                "overhead",
                "overheads",
            ])
            .field_value_descriptors(ItemMetadataField::MultiMic, oh_position_descriptors)
            .build();

        // Define cymbal types as field value descriptors
        // Each cymbal type (Hi Hat, Ride, etc.) can have its own patterns
        // Order matters - items will be sorted by the order of descriptors
        let cymbal_type_descriptors = vec![
            FieldValueDescriptor::builder("Hi Hat")
                .patterns(["hat", "hh", "hihat", "hi-hat", "hi hat"])
                .build(),
            FieldValueDescriptor::builder("Ride")
                .patterns(["ride"])
                .build(),
            FieldValueDescriptor::builder("Crash")
                .patterns(["crash"])
                .build(),
            FieldValueDescriptor::builder("China")
                .patterns(["china"])
                .build(),
            FieldValueDescriptor::builder("Splash")
                .patterns(["splash"])
                .build(),
            FieldValueDescriptor::builder("Bell")
                .patterns(["bell"])
                .build(),
        ];

        // Use field_value_descriptors for MultiMic to distinguish cymbal types
        // This allows "Hi Hat" and "Ride" to be separate entries
        Self::builder("Cymbals")
            .patterns([
                "cymbal",
                "cymbals",
                "oh",
                "ohd",
                "ohl",
                "ohr",
                "ohs",
                "ovh",
                "overhead",
                "overheads",
                "hat",
                "hh",
                "hihat",
                "hi-hat",
                "ride",
                "crash",
                "china",
                "splash",
                "bell",
            ])
            .field_value_descriptors(ItemMetadataField::MultiMic, cymbal_type_descriptors)
            .group(oh_group)
            .build()
    }
}
