//! Lead vocals group definition

use crate::item_metadata::prelude::*;
use crate::item_metadata::ItemMetadataField;

/// Lead vocals group
///
/// Sorting priority: Performer → Section → Layers → Channels
pub struct LeadVocals;

impl From<LeadVocals> for ItemMetadataGroup {
    fn from(_val: LeadVocals) -> Self {
        // Configure lead vocals with field priority: Performer → Section → Layers → Channels
        // The order of these calls determines the priority order
        // Layers uses "Main" as default value so items without a layer are grouped alongside items with layers
        // Note: No prefix for Lead Vocals - parent Vocals has "V" prefix which is sufficient
        Self::builder("Lead")
            // Patterns for lead vocals - includes generic vocal patterns since Lead is the
            // default destination for vocal tracks that don't match BGVs patterns
            // NOTE: "lead" and "solo" are intentionally excluded - they're ambiguous without
            // vocal context (could be guitar lead/solo). Use requires_parent_match() to ensure
            // parent Vocals group matches first, providing the vocal context.
            .patterns([
                "main", "ld", "ldv", "lv", "vld", "voc", "vox", "vocal", "voca", "voice", "scream",
                "screams", "screamer",
            ])
            // Only match if parent (Vocals) also matches - prevents "JohnyLead" from matching
            // just because it contains "Lead" without any vocal-related patterns
            .requires_parent_match()
            .performer(Self::builder("Performer").build()) // Priority 1: Performer (uses global patterns)
            .section(Self::builder("Section").build()) // Priority 2: Section (uses global patterns)
            // Layers includes quad (quadruple tracking), stereo, mono, etc.
            .layers(
                Self::builder("Layers")
                    .patterns(["quad", "stereo", "mono", "double", "triple"])
                    .build(),
            ) // Priority 3: Layers
            .field_default_value(ItemMetadataField::Layers, "Main") // Default layer name for items without a layer
            .channel(
                Self::builder("Channel")
                    .patterns(["L", "C", "R", "Left", "Center", "Right"])
                    .build(),
            ) // Priority 4: Channel (order: L, C, R)
            .build()
    }
}

// region: --- Tests

#[cfg(test)]
mod tests {
    use crate::{default_config, OrganizeIntoTracks};
    use daw_proto::{assert_tracks_equal, TrackStructureBuilder};

    type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

    #[test]
    fn single_track_no_grouping_needed() -> Result<()> {
        // -- Setup & Fixtures
        let items = vec!["Vocal Chorus Cody DBL L"];
        let config = default_config();

        // -- Exec
        let tracks = items.organize_into_tracks(&config, None)?;

        // -- Check
        println!("\nTrack list:");
        daw_proto::display_tracklist(&tracks);

        let expected = TrackStructureBuilder::new()
            .track("Vocals")
            .item("Vocal Chorus Cody DBL L")
            .build();

        assert_tracks_equal(&tracks, &expected)?;

        Ok(())
    }

    #[test]
    fn multiple_sections_grouped() -> Result<()> {
        // -- Setup & Fixtures
        let items = vec!["Vocal Verse Cody", "Vocal Chorus Cody"];
        let config = default_config();

        // -- Exec
        let tracks = items.organize_into_tracks(&config, None)?;

        // -- Check
        println!("\nTrack list:");
        daw_proto::display_tracklist(&tracks);

        let expected = TrackStructureBuilder::new()
            .folder("Vocals")
            .track("Chorus")
            .item("Vocal Chorus Cody")
            .track("Verse")
            .item("Vocal Verse Cody")
            .end()
            .build();

        assert_tracks_equal(&tracks, &expected)?;

        Ok(())
    }

    #[test]
    fn multiple_performers_grouped() -> Result<()> {
        // -- Setup & Fixtures
        let items = vec!["Vocal Chorus Cody", "Vocal Chorus John"];
        let config = default_config();

        // -- Exec
        let tracks = items.organize_into_tracks(&config, None)?;

        // -- Check
        println!("\nTrack list:");
        daw_proto::display_tracklist(&tracks);

        let expected = TrackStructureBuilder::new()
            .folder("Vocals")
            .track("Cody")
            .item("Vocal Chorus Cody")
            .track("John")
            .item("Vocal Chorus John")
            .end()
            .build();

        assert_tracks_equal(&tracks, &expected)?;

        Ok(())
    }

    #[test]
    fn adding_layers() -> Result<()> {
        // -- Setup & Fixtures
        let items = vec!["Vocal Chorus Cody", "Vocal Chorus Cody DBL"];
        let config = default_config();

        // -- Exec
        let tracks = items.organize_into_tracks(&config, None)?;

        // -- Check
        println!("\nTrack list:");
        daw_proto::display_tracklist(&tracks);

        let expected = TrackStructureBuilder::new()
            .folder("Vocals")
            .track("Main")
            .item("Vocal Chorus Cody")
            .track("DBL")
            .item("Vocal Chorus Cody DBL")
            .end()
            .build();

        assert_tracks_equal(&tracks, &expected)?;

        Ok(())
    }

    #[test]
    fn adding_channels() -> Result<()> {
        // -- Setup & Fixtures
        let items = vec![
            "Vocal Chorus Cody L",
            "Vocal Chorus Cody C",
            "Vocal Chorus Cody R",
        ];
        let config = default_config();

        // -- Exec
        let tracks = items.organize_into_tracks(&config, None)?;

        // -- Check
        println!("\nTrack list:");
        daw_proto::display_tracklist(&tracks);

        let expected = TrackStructureBuilder::new()
            .folder("Vocals")
            .track("L")
            .item("Vocal Chorus Cody L")
            .track("C")
            .item("Vocal Chorus Cody C")
            .track("R")
            .item("Vocal Chorus Cody R")
            .end()
            .build();

        assert_tracks_equal(&tracks, &expected)?;

        Ok(())
    }

    #[test]
    fn layers_and_channels_together() -> Result<()> {
        // -- Setup & Fixtures
        let items = vec![
            "Vocal Chorus Cody Main L",
            "Vocal Chorus Cody Main C",
            "Vocal Chorus Cody Main R",
            "Vocal Chorus Cody DBL L",
            "Vocal Chorus Cody DBL C",
            "Vocal Chorus Cody DBL R",
        ];
        let config = default_config();

        // -- Exec
        let tracks = items.organize_into_tracks(&config, None)?;

        // -- Check
        println!("\nTrack list:");
        daw_proto::display_tracklist(&tracks);

        let expected = TrackStructureBuilder::new()
            .folder("Vocals")
            .folder("Main")
            .track("L")
            .item("Vocal Chorus Cody Main L")
            .track("C")
            .item("Vocal Chorus Cody Main C")
            .track("R")
            .item("Vocal Chorus Cody Main R")
            .end()
            .folder("DBL")
            .track("L")
            .item("Vocal Chorus Cody DBL L")
            .track("C")
            .item("Vocal Chorus Cody DBL C")
            .track("R")
            .item("Vocal Chorus Cody DBL R")
            .end()
            .end()
            .build();

        assert_tracks_equal(&tracks, &expected)?;

        Ok(())
    }
}

// endregion: --- Tests
