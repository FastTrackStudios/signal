//! Backing / playback "Tracks" group definitions.
//!
//! Captures pre-produced playback elements that run alongside the live band —
//! loops, sequences, and backing tracks (the "tracks" a worship/live rig plays
//! to). These are distinct from live percussion (they're programmed) and from
//! the reference mix (that's the finished master, not a stem to play).

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Top-level Tracks group for loops and backing/playback elements.
pub struct Tracks;

impl From<Tracks> for Group<ItemMetadata> {
    fn from(_val: Tracks) -> Self {
        Self::builder("Tracks")
            .prefix("TRK")
            .patterns(vec![
                // The bare plural is the folder name for this group. It is a
                // generic word, but every content group is registered ahead of
                // Tracks, so "Vocal Tracks" still reaches Vocals first.
                "tracks",
                "loop",
                "loops",
                "playback",
                "playback track",
                "backing track",
                "backing tracks",
                "sequence",
                "sequenced",
                "multitrack",
                "programming",
                "arp",
            ])
            // Don't swallow the reference master or live instruments that happen
            // to share a word (e.g. "Drum Loop" should stay with drums).
            .exclude(vec![
                "original",
                "reference",
                "master",
                "drum",
                "snare",
                "kick",
                "guitar",
                "bass",
                "vocal",
            ])
            .build()
    }
}
