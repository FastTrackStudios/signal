//! Drum kit group definitions

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

mod cymbals;
mod kick;
mod room;
mod snare;
mod tom;

pub use cymbals::Cymbals;
pub use kick::Kick;
pub use room::Room;
pub use snare::Snare;
pub use tom::Tom;

/// Drum kit container group that includes all drum subgroups
pub struct DrumKit;

impl From<DrumKit> for Group<ItemMetadata> {
    fn from(_val: DrumKit) -> Self {
        Self::builder("Drum Kit")
            .patterns(vec!["d_", "drum", "kit", "drums", "brush", "brushes"])
            .exclude(vec![
                // A talkback mic is named for whoever it belongs to
                // ("TB Drums"), so without this it classifies as the kit it is
                // listening to. The exclusion has to sit here as well as on the
                // Drums container: the parser matches this group on its own
                // patterns, so blocking only the parent leaves it reachable.
                "tb",
                "vca",
                "hp",
                "headphone",
                "talkback",
                "electronic",
                "elec",
                "e-kit",
                "ekit",
                "808",
                "909",
                "drum machine",
                "machine",
                "midi drum",
                "vst drum",
            ])
            .block_prefix("D") // Avoid "D Drum Kit" redundancy
            .group(Kick)
            .group(Snare)
            .group(Tom)
            .group(Cymbals)
            .group(Room)
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
    fn full_drum_kit_integration_test() -> Result<()> {
        // -- Setup & Fixtures
        let items = vec![
            "Kick In",
            "Kick Out",
            "Snare Top",
            "Snare Bottom",
            "Tom 1",
            "Tom 2",
            "Tom 3",
            "Hi Hat",
            "Ride",
            "OH L",
            "OH R",
            "Rooms L",
            "Rooms R",
        ];
        let config = default_config();

        // -- Exec
        let tracks = items.organize_into_tracks(&config, None)?;

        // -- Check
        println!("\nTrack list:");
        daw_proto::display_tracklist(&tracks);

        let expected = TrackStructureBuilder::new()
            .folder("Drums")
            .folder("Kick")
            .track("In")
            .item("Kick In")
            .track("Out")
            .item("Kick Out")
            .end()
            .folder("Snare")
            .track("Top")
            .item("Snare Top")
            .track("Bottom")
            .item("Snare Bottom")
            .end()
            .folder("Toms")
            .track("T1")
            .item("Tom 1")
            .track("T2")
            .item("Tom 2")
            .track("T3")
            .item("Tom 3")
            .end()
            .folder("Cymbals")
            .track("Hi Hat")
            .item("Hi Hat")
            .track("Ride")
            .item("Ride")
            .folder("OH")
            .track("L")
            .item("OH L")
            .track("R")
            .item("OH R")
            .end()
            .end()
            .folder("Rooms")
            .track("L")
            .item("Rooms L")
            .track("R")
            .item("Rooms R")
            .end()
            .end()
            .build();

        assert_tracks_equal(&tracks, &expected)?;

        Ok(())
    }
}

// endregion: --- Tests
