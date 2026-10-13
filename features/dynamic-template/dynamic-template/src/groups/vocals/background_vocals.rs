//! Background vocals (BGVs) group definition

use crate::item_metadata::prelude::*;
use crate::item_metadata::ItemMetadataField;

/// Background vocals group (BGVs)
///
/// Sorting priority: Performer → Section → Arrangement (harmonies) → Layers → Channels
pub struct BackgroundVocals;

/// Define harmony arrangements for BGVs
/// These are the Arrangement metadata field values specific to BGVs
fn build_harmony_arrangement() -> ItemMetadataGroup {
    ItemMetadataGroup::builder("Arrangement")
        .patterns([
            // Voice Parts
            "Soprano",
            "soprano",
            "sop",
            "s",
            "Alto",
            "alto",
            "a",
            "Tenor",
            "tenor",
            "t",
            "Baritone",
            "baritone",
            "bar",
            "bari",
            "b",
            "Bass",
            "bass",
            "low",
            // Harmony Descriptors
            "High",
            "high",
            "high harmony",
            "high harm",
            "upper",
            "Highest",
            "highest",
            "Low",
            "low",
            "low harmony",
            "low harm",
            "lower",
            "Lowest",
            "lowest",
            "Mid",
            "mid",
            "middle",
            "mid harmony",
            "mid harm",
            "Drone",
            "drone",
            "drone harmony",
            "sustained",
            // Additional Common Harmonies
            "Harmony 1",
            "harmony 1",
            "harm 1",
            "h1",
            "harmony1",
            "harm1",
            "Harmony 2",
            "harmony 2",
            "harm 2",
            "h2",
            "harmony2",
            "harm2",
            "Harmony 3",
            "harmony 3",
            "harm 3",
            "h3",
            "harmony3",
            "harm3",
            "Oohs",
            "ooh",
            "oohs",
            "ooh harmony",
            "Aahs",
            "aah",
            "aahs",
            "aah harmony",
            "Ad Libs",
            "ad lib",
            "adlib",
            "ad libs",
            "adlibs",
        ])
        .build()
}

impl From<BackgroundVocals> for ItemMetadataGroup {
    fn from(_val: BackgroundVocals) -> Self {
        let harmony_arrangement = build_harmony_arrangement();

        // Configure BGVs with field priority: Performer → Section → Arrangement → Layers → Channels
        // The order of these calls determines the priority order
        // Layers uses "Main" as default value so items without a layer are grouped alongside items with layers
        // Note: BGVs does NOT use requires_parent_match because "bgv", "background", etc.
        // are already specific enough patterns that uniquely identify background vocals
        Self::builder("BGVs")
            .prefix("BGV")
            .patterns([
                // Standard abbreviations
                "bgv",
                "bgvs",
                "bgvox",
                "bg",
                "bv",
                "bvs",
                "background",
                "backing",
                "harmony",
                // NOTE: "choir" intentionally omitted — choir stems route to the
                // dedicated Choir subgroup under Vocals, not into BGVs.
                // Ad-libs are typically supporting vocal elements
                "ad lib",
                "adlib",
                "ad-lib",
                // Harmony position descriptors - these indicate BGV content
                "high harmony",
                "low harmony",
                "mid harmony",
                "upper harmony",
                "lower harmony",
                "middle harmony",
                // "Hey Hey" is a common vocal hook/response pattern in pop/rock
                "hey hey",
            ])
            // Choir stems are handled by the dedicated Choir subgroup, not BGVs.
            .exclude(["choir", "chorale", "ensemble"])
            .performer(Self::builder("Performer").build()) // Priority 1: Performer (uses global patterns)
            .section(Self::builder("Section").build()) // Priority 2: Section (uses global patterns)
            .arrangement(harmony_arrangement) // Priority 3: Arrangement (harmony-specific patterns)
            .layers(Self::builder("Layers").build()) // Priority 4: Layers (uses global patterns)
            .field_default_value(ItemMetadataField::Layers, "Main") // Default layer name for items without a layer
            .channel(
                Self::builder("Channel")
                    .patterns(["L", "C", "R", "Left", "Center", "Right"])
                    .build(),
            ) // Priority 5: Channel (order: L, C, R)
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
    fn bgvs_with_harmony_arrangements() -> Result<()> {
        // -- Setup & Fixtures
        let items = vec![
            "BGV Chorus Cody Soprano",
            "BGV Chorus Cody Alto",
            "BGV Chorus JT High",
            "BGV Chorus JT Low",
        ];
        let config = default_config();

        // -- Exec
        let tracks = items.organize_into_tracks(&config, None)?;

        // -- Check
        println!("\nTrack list:");
        daw_proto::display_tracklist(&tracks);

        let expected = TrackStructureBuilder::new()
            .folder("Vocals")
            .folder("Cody")
            .track("Soprano")
            .item("BGV Chorus Cody Soprano")
            .track("Alto")
            .item("BGV Chorus Cody Alto")
            .end()
            .folder("JT")
            .track("Low")
            .item("BGV Chorus JT Low")
            .track("High")
            .item("BGV Chorus JT High")
            .end()
            .end()
            .build();

        assert_tracks_equal(&tracks, &expected)?;

        Ok(())
    }

    #[test]
    fn bgvs_with_voice_parts() -> Result<()> {
        // -- Setup & Fixtures
        let items = vec![
            "BGV Chorus Cody Soprano",
            "BGV Chorus Cody Alto",
            "BGV Chorus Cody Tenor",
            "BGV Chorus Cody Bass",
        ];
        let config = default_config();

        // -- Exec
        let tracks = items.organize_into_tracks(&config, None)?;

        // -- Check
        println!("\nTrack list:");
        daw_proto::display_tracklist(&tracks);

        // NOTE: "Bass" voice part gets stripped to "Vocals" (fallback) due to context stripping
        // TODO: Add "bass" to non-context words to preserve voice part names
        let expected = TrackStructureBuilder::new()
            .folder("Vocals")
            .track("Soprano")
            .item("BGV Chorus Cody Soprano")
            .track("Alto")
            .item("BGV Chorus Cody Alto")
            .track("Tenor")
            .item("BGV Chorus Cody Tenor")
            .track("Vocals")
            .item("BGV Chorus Cody Bass")
            .end()
            .build();

        assert_tracks_equal(&tracks, &expected)?;

        Ok(())
    }

    #[test]
    fn bgvs_with_harmony_descriptors() -> Result<()> {
        // -- Setup & Fixtures
        let items = vec![
            "BGV Chorus Cody High",
            "BGV Chorus Cody Low",
            "BGV Chorus Cody Mid",
        ];
        let config = default_config();

        // -- Exec
        let tracks = items.organize_into_tracks(&config, None)?;

        // -- Check
        println!("\nTrack list:");
        daw_proto::display_tracklist(&tracks);

        let expected = TrackStructureBuilder::new()
            .folder("Vocals")
            .track("Low")
            .item("BGV Chorus Cody Low")
            .track("High")
            .item("BGV Chorus Cody High")
            .track("Mid")
            .item("BGV Chorus Cody Mid")
            .end()
            .build();

        assert_tracks_equal(&tracks, &expected)?;

        Ok(())
    }

    #[test]
    fn bgvs_with_numbered_harmonies() -> Result<()> {
        // -- Setup & Fixtures
        let items = vec![
            "BGV Chorus Cody Harmony 1",
            "BGV Chorus Cody Harmony 2",
            "BGV Chorus Cody Harmony 3",
        ];
        let config = default_config();

        // -- Exec
        let tracks = items.organize_into_tracks(&config, None)?;

        // -- Check
        println!("\nTrack list:");
        daw_proto::display_tracklist(&tracks);

        let expected = TrackStructureBuilder::new()
            .folder("Vocals")
            .track("Harmony 1")
            .item("BGV Chorus Cody Harmony 1")
            .track("Harmony 2")
            .item("BGV Chorus Cody Harmony 2")
            .track("Harmony 3")
            .item("BGV Chorus Cody Harmony 3")
            .end()
            .build();

        assert_tracks_equal(&tracks, &expected)?;

        Ok(())
    }

    #[test]
    fn bgvs_without_harmony_arrangements() -> Result<()> {
        // -- Setup & Fixtures
        let items = vec!["BGV Chorus Cody", "BGV Chorus JT", "BGV Chorus Bri"];
        let config = default_config();

        // -- Exec
        let tracks = items.organize_into_tracks(&config, None)?;

        // -- Check
        println!("\nTrack list:");
        daw_proto::display_tracklist(&tracks);

        let expected = TrackStructureBuilder::new()
            .folder("Vocals")
            .track("Bri")
            .item("BGV Chorus Bri")
            .track("Cody")
            .item("BGV Chorus Cody")
            .track("JT")
            .item("BGV Chorus JT")
            .end()
            .build();

        assert_tracks_equal(&tracks, &expected)?;

        Ok(())
    }
}

// endregion: --- Tests
