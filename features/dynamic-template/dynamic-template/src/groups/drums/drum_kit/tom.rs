//! Tom drum group definition

use crate::item_metadata::prelude::*;

/// Tom drum group
pub struct Tom;

impl From<Tom> for ItemMetadataGroup {
    fn from(_val: Tom) -> Self {
        use crate::item_metadata::ItemMetadataField;
        use monarchy::FieldValueDescriptor;

        // Define tom types and sizes as field value descriptors
        // Rack toms with common sizes (8", 10", 12", 13", 14", 16")
        // Floor toms (14", 16", 18")
        // The value name becomes the display name
        let tom_type_descriptors = vec![
            // Floor toms - check first (more specific)
            FieldValueDescriptor::builder("Floor")
                .patterns(["floor", "ft", "floor tom"])
                .build(),
            // Rack toms with sizes (in inches) - common drum sizes
            FieldValueDescriptor::builder("Rack 8\"")
                .patterns(["rack 8", "rack8", "8 inch", "8\"", "8in"])
                .build(),
            FieldValueDescriptor::builder("Rack 10\"")
                .patterns([
                    "rack 10", "rack10", "10 inch", "10\"", "10in", "10-st", "10 st",
                ])
                .build(),
            FieldValueDescriptor::builder("Rack 12\"")
                .patterns(["rack 12", "rack12", "12 inch", "12\"", "12in"])
                .build(),
            FieldValueDescriptor::builder("Rack 13\"")
                .patterns(["rack 13", "rack13", "13 inch", "13\"", "13in"])
                .build(),
            FieldValueDescriptor::builder("Rack 14\"")
                .patterns(["rack 14", "rack14", "14 inch", "14\"", "14in"])
                .build(),
            FieldValueDescriptor::builder("Rack 16\"")
                .patterns(["rack 16", "rack16", "16 inch", "16\"", "16in"])
                .build(),
            // Generic numbered toms (T1, T2, etc.)
            FieldValueDescriptor::builder("T1")
                .patterns(["t1", "tom 1", "tom1"])
                .build(),
            FieldValueDescriptor::builder("T2")
                .patterns(["t2", "tom 2", "tom2"])
                .build(),
            FieldValueDescriptor::builder("T3")
                .patterns(["t3", "tom 3", "tom3"])
                .build(),
            FieldValueDescriptor::builder("T4")
                .patterns(["t4", "tom 4", "tom4"])
                .build(),
        ];

        // Use field_value_descriptors for MultiMic to get descriptive tom names
        Self::builder("Toms")
            .patterns(["tom", "toms", "rack", "floor", "ft", "t1", "t2", "t3", "t4"])
            .field_value_descriptors(ItemMetadataField::MultiMic, tom_type_descriptors)
            .build()
    }
}
