//! Room/ambient drums group definition

use crate::item_metadata::prelude::*;

/// Room/ambient drums group
pub struct Room;

impl From<Room> for ItemMetadataGroup {
    fn from(_val: Room) -> Self {
        use crate::item_metadata::ItemMetadataField;
        use monarchy::FieldValueDescriptor;

        // Define room positions using field_value_descriptors to ensure L comes before R
        // Order matters - items will be sorted by the order of descriptors
        let room_position_descriptors = vec![
            FieldValueDescriptor::builder("L")
                .patterns(["L", "Left", "left"])
                .build(),
            FieldValueDescriptor::builder("R")
                .patterns(["R", "Right", "right"])
                .build(),
            FieldValueDescriptor::builder("Mono")
                .patterns(["Mono", "mono"])
                .build(),
            FieldValueDescriptor::builder("Stereo")
                .patterns(["Stereo", "stereo"])
                .build(),
        ];

        // Use the convenience method - extension trait is in scope via prelude
        // Note: "verb" and "reverb" are now global effect patterns, not room-specific
        // Room mics are physical mics in the room, not reverb sends
        Self::builder("Rooms")
            .patterns([
                "room",
                "rooms",
                "amb",
                "ambient",
                "ambience",
                "knee",
                "floor mic",
                "corner",
                // Mic placement terms
                "crotch",   // Between kick and floor tom
                "midside",  // Mid-side stereo technique
                "mid side", // Alternative spelling
                "mid-side", // Hyphenated
                "ms mic",   // Mid-side abbreviation
            ])
            // Exclude cymbal-related patterns so they go to Cymbals group instead.
            // Also exclude snare patterns so "SNR VERB" goes to Snare, not Rooms.
            // Exclude non-drum instrument keywords so "Piano Room Mono", "Guitar Room",
            // "Vocal Room" etc. route to their own instrument groups instead.
            .exclude([
                // Cymbals / hi-hats
                "oh",
                "overhead",
                "overheads",
                "ovh",
                "hihat",
                "hi-hat",
                "hi hat",
                "hh",
                "hat",
                // Snare
                "snr",
                "snare",
                "sn",
                // Non-drum instruments — their room mics belong with the instrument, not Drums
                "piano",
                "guitar",
                "gtr",
                "violin",
                "fiddle",
                "cello",
                "vocal",
                "voice",
                "vox",
            ])
            .field_value_descriptors(ItemMetadataField::MultiMic, room_position_descriptors)
            .build()
    }
}
