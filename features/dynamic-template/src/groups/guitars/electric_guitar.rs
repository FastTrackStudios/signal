//! Electric guitar group definition

use crate::item_metadata::prelude::*;
use monarchy::{FieldGroupingStrategy, FieldValueDescriptor};

/// Electric guitar group
pub struct ElectricGuitar;

fn build_guitar_arrangement() -> ItemMetadataGroup {
    ItemMetadataGroup::builder("Arrangement")
        .patterns([
            "Clean",
            "Crunch",
            "Drive",
            "Lead",
            "Pick",
            "Chug",
            "Rhythm",
            "Slide",
            "Solo",
            "Outro",
            "Phaser",
            "Pitch",
            "Wah",
            "Distortion",
            "Overdrive",
        ])
        .build()
}

fn build_variant_descriptors() -> Vec<FieldValueDescriptor> {
    vec![
        FieldValueDescriptor::builder("Strat")
            .patterns(["strat", "stratocaster"])
            .build(),
        FieldValueDescriptor::builder("Tele")
            .patterns(["tele", "telecaster"])
            .build(),
        FieldValueDescriptor::builder("Les Paul")
            .patterns(["les paul", "lespaul", "lp"])
            .build(),
        FieldValueDescriptor::builder("SG").patterns(["sg"]).build(),
        FieldValueDescriptor::builder("Gretsch")
            .patterns(["gretsch"])
            .build(),
        FieldValueDescriptor::builder("Jazzmaster")
            .patterns(["jazzmaster", "jm"])
            .build(),
        FieldValueDescriptor::builder("Jaguar")
            .patterns(["jaguar", "jag"])
            .build(),
        FieldValueDescriptor::builder("335")
            .patterns(["335", "es-335", "es335"])
            .build(),
        FieldValueDescriptor::builder("Rickenbacker")
            .patterns(["rickenbacker", "ric", "ricky"])
            .build(),
        FieldValueDescriptor::builder("PRS")
            .patterns(["prs", "paul reed smith"])
            .build(),
    ]
}

/// The sources of one electric channel (`flow.guitars.dimensions`): up to
/// seven for a single take of a single channel — the DI, the pedalboard,
/// two amps, and the 57 and 121 on each amp.
///
/// This list is *mixer* order, which is what organizing a folder sorts
/// by, so Amp stays at the head where it has always been. The order a
/// part **grows** through these is a different question, and
/// `track_schema::growth_values_for_dimension` answers it.
fn build_multi_mic_descriptors() -> Vec<FieldValueDescriptor> {
    vec![
        FieldValueDescriptor::builder("Amp")
            .patterns(["amp", "amplitube"])
            .build(),
        FieldValueDescriptor::builder("DI").patterns(["di"]).build(),
        FieldValueDescriptor::builder("Pedalboard")
            .patterns(["pedalboard", "pedal board", "pedals", "pedalbrd"])
            .build(),
        FieldValueDescriptor::builder("Amp 1")
            .patterns(["amp 1", "amp1", "amp-1"])
            .build(),
        FieldValueDescriptor::builder("Amp 2")
            .patterns(["amp 2", "amp2", "amp-2"])
            .build(),
        // The two mics on a cab, by model. Not "57" and "R121", which is
        // what an engineer says out loud: a bare "57" also matches the
        // mic named in a capture's file name ("EG2 (57)"), where it is
        // equipment metadata rather than a source track of its own, and
        // matching it reshaped a real multitrack fixture; and "R121"
        // reads as the R channel, because "R" is a channel and the
        // classifier checks channels first.
        FieldValueDescriptor::builder("SM57")
            .patterns(["sm57", "sm-57"])
            .build(),
        FieldValueDescriptor::builder("Royer")
            .patterns(["royer", "r121", "r-121", "121"])
            .build(),
    ]
}

impl From<ElectricGuitar> for ItemMetadataGroup {
    fn from(_val: ElectricGuitar) -> Self {
        use crate::item_metadata::ItemMetadataField;

        let guitar_arrangement = build_guitar_arrangement();
        let variant_descriptors = build_variant_descriptors();
        let multi_mic_descriptors = build_multi_mic_descriptors();

        // Configure electric guitar with field priority: Performer → Arrangement → Layers → Channel → MultiMic
        // The order of these calls determines the priority order
        // MultiMic uses MainOnContainer strategy so base tracks go on folder, multi-mic versions become children
        // Layers uses "Main" as default value so items without a layer are grouped alongside items with layers
        Self::builder("Electric")
            .prefix("E")
            .patterns([
                "electric",
                "elec", // Common abbreviation for Electric (as in "Elec Gui")
                "guitar",
                "gtr", // Very common 3-letter abbreviation used by engineers
                "eg",  // Common abbreviation for Electric Guitar
                "lead guitar",
                "lead_guitar",
                "leadguitar",
                // Common electric guitar models (also matched as variants)
                "strat",
                "stratocaster",
                "tele",
                "telecaster",
                "les paul",
                "lespaul",
                "sg",
                "gretsch",
                "jazzmaster",
                "jaguar",
                "firebird",
                "explorer",
                "flying v",
            ])
            .exclude([
                // "GTR A" is the acoustic in the house convention; the bare
                // "gtr" pattern below would otherwise claim it for electric.
                "gtr a",
                "gtr-a",
                "gtr_a",
                "guitar a",
                // A talkback mic is named for whoever it belongs to
                // ("TB Guitar"), so without this it classifies as the
                // instrument it is listening to. Excluding on the Guitars
                // container is not enough — the parser matches this group on
                // its own patterns, independently of its parent.
                "tb",
                "vca",
                "hp",
                "headphone",
                "talkback",
            ])
            // Prevent matching bass or piano items that start with "Elec"
            .exclude(["bass", "piano", "pno", "keys", "keyboard"])
            .performer(Self::builder("Performer").build()) // Priority 1: Performer (uses global patterns)
            .field_value_descriptors(ItemMetadataField::Variant, variant_descriptors) // Priority 2: Variant/model
            .arrangement(guitar_arrangement) // Priority 3: Arrangement
            .layers(Self::builder("Layers").build()) // Priority 4: Layers (global patterns)
            .field_default_value(ItemMetadataField::Layers, "Main") // Default layer name for items without a layer
            .channel(
                Self::builder("Channel")
                    .patterns(["L", "C", "R", "Left", "Center", "Right"])
                    .build(),
            ) // Priority 5: Channel (order: L, C, R)
            // L/R is the Channel dimension for guitars, not a Layer: a
            // Channel shares a chain, a Layer has its own
            // (`flow.guitars.dimensions`).
            // Note: We use field_value_descriptors for MultiMic, so we don't need the nested .multi_mic() group
            // The field_value_descriptors handle the MultiMic value extraction and matching
            .field_value_descriptors(ItemMetadataField::MultiMic, multi_mic_descriptors)
            .field_strategy(
                ItemMetadataField::MultiMic,
                FieldGroupingStrategy::MainOnContainer,
            )
            // The DI feed nests under its sibling amp track (muted,
            // folder-collapsed) instead of sitting beside it as an equal.
            .nest_secondary_mics(true)
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
        let items = vec!["Guitar Clean DBL L"];
        let config = default_config();

        // -- Exec
        let tracks = items.organize_into_tracks(&config, None)?;

        // -- Check
        println!("\nTrack list:");
        daw_proto::display_tracklist(&tracks);

        let expected = TrackStructureBuilder::new()
            .track("Guitars")
            .item("Guitar Clean DBL L")
            .build();

        assert_tracks_equal(&tracks, &expected)?;

        Ok(())
    }

    #[test]
    fn multiple_arrangements_grouped() -> Result<()> {
        // -- Setup & Fixtures
        let items = vec!["Guitar Clean", "Guitar Drive"];
        let config = default_config();

        // -- Exec
        let tracks = items.organize_into_tracks(&config, None)?;

        // -- Check
        println!("\nTrack list:");
        daw_proto::display_tracklist(&tracks);

        let expected = TrackStructureBuilder::new()
            .folder("Guitars")
            .track("Clean")
            .item("Guitar Clean")
            .track("Drive")
            .item("Guitar Drive")
            .end()
            .build();

        assert_tracks_equal(&tracks, &expected)?;

        Ok(())
    }

    #[test]
    fn guitars_with_multi_mics() -> Result<()> {
        // -- Setup & Fixtures
        let items = vec!["Guitar Clean", "Guitar Clean Amp", "Guitar Clean DI"];
        let config = default_config();

        // -- Exec
        let tracks = items.organize_into_tracks(&config, None)?;

        // -- Check
        println!("\nTrack list:");
        daw_proto::display_tracklist(&tracks);

        let expected = TrackStructureBuilder::new()
            .folder("Guitars")
            .track("Amp")
            .item("Guitar Clean Amp")
            .track("DI")
            .item("Guitar Clean DI")
            .track("Electric Clean")
            .item("Guitar Clean")
            .end()
            .build();

        assert_tracks_equal(&tracks, &expected)?;

        Ok(())
    }

    #[test]
    fn multiple_arrangements_with_multi_mics() -> Result<()> {
        // -- Setup & Fixtures
        let items = vec![
            "Guitar Clean",
            "Guitar Clean Amp",
            "Guitar Clean DI",
            "Guitar Drive",
            "Guitar Drive Amp",
            "Guitar Drive DI",
        ];
        let config = default_config();

        // -- Exec
        let tracks = items.organize_into_tracks(&config, None)?;

        // -- Check
        println!("\nTrack list:");
        daw_proto::display_tracklist(&tracks);

        let expected = TrackStructureBuilder::new()
            .folder("Guitars")
            .folder("Clean")
            .track("Amp")
            .item("Guitar Clean Amp")
            .track("DI")
            .item("Guitar Clean DI")
            .track("Electric")
            .item("Guitar Clean")
            .end()
            .folder("Drive")
            .track("Amp")
            .item("Guitar Drive Amp")
            .track("DI")
            .item("Guitar Drive DI")
            .track("Electric")
            .item("Guitar Drive")
            .end()
            .end()
            .build();

        assert_tracks_equal(&tracks, &expected)?;

        Ok(())
    }

    #[test]
    fn adding_layers() -> Result<()> {
        // -- Setup & Fixtures
        let items = vec![
            "Guitar Clean",
            "Guitar Clean Amp",
            "Guitar Clean DI",
            "Guitar Clean DBL",
            "Guitar Clean Amp DBL",
            "Guitar Clean DI DBL",
        ];
        let config = default_config();

        // -- Exec
        let tracks = items.organize_into_tracks(&config, None)?;

        // -- Check
        println!("\nTrack list:");
        daw_proto::display_tracklist(&tracks);

        let expected = TrackStructureBuilder::new()
            .folder("Guitars")
            .folder("Main")
            .track("Amp")
            .item("Guitar Clean Amp")
            .track("DI")
            .item("Guitar Clean DI")
            .track("Electric Clean")
            .item("Guitar Clean")
            .end()
            .folder("DBL")
            .track("Amp")
            .item("Guitar Clean Amp DBL")
            .track("DI")
            .item("Guitar Clean DI DBL")
            .track("Electric Clean DBL")
            .item("Guitar Clean DBL")
            .end()
            .end()
            .build();

        assert_tracks_equal(&tracks, &expected)?;

        Ok(())
    }

    #[test]
    fn adding_channels() -> Result<()> {
        // -- Setup & Fixtures
        let items = vec![
            "Guitar Clean L",
            "Guitar Clean Amp L",
            "Guitar Clean DI L",
            "Guitar Clean C",
            "Guitar Clean Amp C",
            "Guitar Clean DI C",
            "Guitar Clean R",
            "Guitar Clean Amp R",
            "Guitar Clean DI R",
        ];
        let config = default_config();

        // -- Exec
        let tracks = items.organize_into_tracks(&config, None)?;

        // -- Check
        println!("\nTrack list:");
        daw_proto::display_tracklist(&tracks);

        let expected = TrackStructureBuilder::new()
            .folder("Guitars")
            .folder("L")
            .track("Amp")
            .item("Guitar Clean Amp L")
            .track("DI")
            .item("Guitar Clean DI L")
            .track("Electric Clean L")
            .item("Guitar Clean L")
            .end()
            .folder("C")
            .track("Amp")
            .item("Guitar Clean Amp C")
            .track("DI")
            .item("Guitar Clean DI C")
            .track("Electric Clean C")
            .item("Guitar Clean C")
            .end()
            .folder("R")
            .track("Amp")
            .item("Guitar Clean Amp R")
            .track("DI")
            .item("Guitar Clean DI R")
            .track("Electric Clean R")
            .item("Guitar Clean R")
            .end()
            .end()
            .build();

        assert_tracks_equal(&tracks, &expected)?;

        Ok(())
    }

    #[test]
    fn layers_and_channels_together() -> Result<()> {
        // -- Setup & Fixtures
        let items = vec![
            "Guitar Clean Main L",
            "Guitar Clean Amp Main L",
            "Guitar Clean DI Main L",
            "Guitar Clean Main C",
            "Guitar Clean Amp Main C",
            "Guitar Clean DI Main C",
            "Guitar Clean Main R",
            "Guitar Clean Amp Main R",
            "Guitar Clean DI Main R",
            "Guitar Clean DBL L",
            "Guitar Clean Amp DBL L",
            "Guitar Clean DI DBL L",
            "Guitar Clean DBL C",
            "Guitar Clean Amp DBL C",
            "Guitar Clean DI DBL C",
            "Guitar Clean DBL R",
            "Guitar Clean Amp DBL R",
            "Guitar Clean DI DBL R",
        ];
        let config = default_config();

        // -- Exec
        let tracks = items.organize_into_tracks(&config, None)?;

        // -- Check
        println!("\nTrack list:");
        daw_proto::display_tracklist(&tracks);

        let expected = TrackStructureBuilder::new()
            .folder("Guitars")
            .folder("Main")
            .folder("L")
            .track("Amp")
            .item("Guitar Clean Amp Main L")
            .track("DI")
            .item("Guitar Clean DI Main L")
            .track("Electric Clean Main L")
            .item("Guitar Clean Main L")
            .end()
            .folder("C")
            .track("Amp")
            .item("Guitar Clean Amp Main C")
            .track("DI")
            .item("Guitar Clean DI Main C")
            .track("Electric Clean Main C")
            .item("Guitar Clean Main C")
            .end()
            .folder("R")
            .track("Amp")
            .item("Guitar Clean Amp Main R")
            .track("DI")
            .item("Guitar Clean DI Main R")
            .track("Electric Clean Main R")
            .item("Guitar Clean Main R")
            .end()
            .end()
            .folder("DBL")
            .folder("L")
            .track("Amp")
            .item("Guitar Clean Amp DBL L")
            .track("DI")
            .item("Guitar Clean DI DBL L")
            .track("Electric Clean DBL L")
            .item("Guitar Clean DBL L")
            .end()
            .folder("C")
            .track("Amp")
            .item("Guitar Clean Amp DBL C")
            .track("DI")
            .item("Guitar Clean DI DBL C")
            .track("Electric Clean DBL C")
            .item("Guitar Clean DBL C")
            .end()
            .folder("R")
            .track("Amp")
            .item("Guitar Clean Amp DBL R")
            .track("DI")
            .item("Guitar Clean DI DBL R")
            .track("Electric Clean DBL R")
            .item("Guitar Clean DBL R")
            .end()
            .end()
            .end()
            .build();

        assert_tracks_equal(&tracks, &expected)?;

        Ok(())
    }

    #[test]
    fn arrangements_and_layers_without_multi_mics() -> Result<()> {
        // -- Setup & Fixtures
        let items = vec![
            "Guitar Clean",
            "Guitar Clean DBL",
            "Guitar Drive",
            "Guitar Drive DBL",
        ];
        let config = default_config();

        // -- Exec
        let tracks = items.organize_into_tracks(&config, None)?;

        // -- Check
        println!("\nTrack list:");
        daw_proto::display_tracklist(&tracks);

        let expected = TrackStructureBuilder::new()
            .folder("Guitars")
            .folder("Clean")
            .track("Main")
            .item("Guitar Clean")
            .track("DBL")
            .item("Guitar Clean DBL")
            .end()
            .folder("Drive")
            .track("Main")
            .item("Guitar Drive")
            .track("DBL")
            .item("Guitar Drive DBL")
            .end()
            .end()
            .build();

        assert_tracks_equal(&tracks, &expected)?;

        Ok(())
    }

    #[test]
    fn arrangements_and_channels_without_multi_mics() -> Result<()> {
        // -- Setup & Fixtures
        let items = vec![
            "Guitar Clean L",
            "Guitar Clean C",
            "Guitar Clean R",
            "Guitar Drive L",
            "Guitar Drive C",
            "Guitar Drive R",
        ];
        let config = default_config();

        // -- Exec
        let tracks = items.organize_into_tracks(&config, None)?;

        // -- Check
        println!("\nTrack list:");
        daw_proto::display_tracklist(&tracks);

        let expected = TrackStructureBuilder::new()
            .folder("Guitars")
            .folder("Clean")
            .track("L")
            .item("Guitar Clean L")
            .track("C")
            .item("Guitar Clean C")
            .track("R")
            .item("Guitar Clean R")
            .end()
            .folder("Drive")
            .track("L")
            .item("Guitar Drive L")
            .track("C")
            .item("Guitar Drive C")
            .track("R")
            .item("Guitar Drive R")
            .end()
            .end()
            .build();

        assert_tracks_equal(&tracks, &expected)?;

        Ok(())
    }

    #[test]
    fn just_layers_without_multi_mics_or_channels() -> Result<()> {
        // -- Setup & Fixtures
        let items = vec!["Guitar Clean", "Guitar Clean DBL"];
        let config = default_config();

        // -- Exec
        let tracks = items.organize_into_tracks(&config, None)?;

        // -- Check
        println!("\nTrack list:");
        daw_proto::display_tracklist(&tracks);

        let expected = TrackStructureBuilder::new()
            .folder("Guitars")
            .track("Main")
            .item("Guitar Clean")
            .track("DBL")
            .item("Guitar Clean DBL")
            .end()
            .build();

        assert_tracks_equal(&tracks, &expected)?;

        Ok(())
    }

    #[test]
    fn just_channels_without_multi_mics_or_layers() -> Result<()> {
        // -- Setup & Fixtures
        let items = vec!["Guitar Clean L", "Guitar Clean C", "Guitar Clean R"];
        let config = default_config();

        // -- Exec
        let tracks = items.organize_into_tracks(&config, None)?;

        // -- Check
        println!("\nTrack list:");
        daw_proto::display_tracklist(&tracks);

        let expected = TrackStructureBuilder::new()
            .folder("Guitars")
            .track("L")
            .item("Guitar Clean L")
            .track("C")
            .item("Guitar Clean C")
            .track("R")
            .item("Guitar Clean R")
            .end()
            .build();

        assert_tracks_equal(&tracks, &expected)?;

        Ok(())
    }
}

// endregion: --- Tests
