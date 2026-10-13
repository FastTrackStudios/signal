//! Bass-related group definitions

use crate::item_metadata::prelude::*;

pub mod bass_guitar;
pub mod synth_bass;
pub mod upright_bass;

pub use bass_guitar::BassGuitar;
pub use synth_bass::SynthBass;
pub use upright_bass::UprightBass;

/// Top-level bass group containing all bass types
pub struct Bass;

impl From<Bass> for ItemMetadataGroup {
    fn from(_val: Bass) -> Self {
        Self::builder("Bass")
            .prefix("Bass")
            .patterns(["bass"])
            // Negative patterns to avoid matching bass drums
            .exclude([
                "bassdrum",
                "bass_drum",
                "bd",
                "kick",
                "tb",
                "talkback",
                "vca",
                "hp",
                "headphone",
            ])
            .group(BassGuitar)
            .group(SynthBass)
            .group(UprightBass)
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
    fn full_bass_integration_test() -> Result<()> {
        // -- Setup & Fixtures
        let items = vec!["Bass Guitar", "Bass Synth", "Upright Bass"];
        let config = default_config();

        // -- Exec
        let tracks = items.organize_into_tracks(&config, None)?;

        // -- Check
        println!("\nTrack list:");
        daw_proto::display_tracklist(&tracks);

        let expected = TrackStructureBuilder::new()
            .folder("Bass")
            .track("Guitar")
            .item("Bass Guitar")
            .track("Synth")
            .item("Bass Synth")
            .track("Upright")
            .item("Upright Bass")
            .end()
            .build();

        assert_tracks_equal(&tracks, &expected)?;

        Ok(())
    }
}

// endregion: --- Tests
