//! `song` — the durable **Song + Arrangement** schema.
//!
//! A [`Song`] is a portable, self-contained vault folder (plaintext +
//! attachment references) — the *stored* form of a song. It is NOT the
//! runtime `session_proto::Song` (that is hydrated from this later, out of
//! scope here).
//!
//! A song has an identity plus one or more [`Arrangement`]s: there is a
//! **default arrangement**, and more can be added over time. Each
//! arrangement carries its [`Key`], a chart **reference** ([`ChartRef`]),
//! its parts ([`PartsManifest`]), and attachment references
//! ([`AttachmentRef`]).
//!
//! ## Keyflow-agnostic by design
//!
//! This crate has **no dependency on `keyflow`**. Charts are stored as
//! references — a relative path within the song folder and/or an
//! attachment id — never as an embedded `keyflow::Chart`. Parsing and
//! rendering charts is a separate, later workstream.
//!
//! ## On-disk form
//!
//! [`to_folder`] / [`from_folder`] round-trip a [`Song`] to a folder. See
//! `docs/song-folder-format.md` for the layout.

#[cfg(feature = "attachments")]
pub mod attachments;
pub mod folder;
pub mod key;
pub mod model;

pub use folder::{ReadError, WriteError, from_folder, to_folder};
pub use key::{Accidental, Key, Letter, Mode, ParseKeyError};
pub use model::{
    Arrangement, ArrangementId, AttachmentRef, ChartRef, Part, PartsManifest, ResourceRef, Song,
    SongId, TimeSignature,
};

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn sample_song() -> Song {
        let default_id = Uuid::new_v4();
        let alt_id = Uuid::new_v4();

        let default = Arrangement {
            id: default_id,
            name: "Default".to_string(),
            key: "Bb Major".parse().unwrap(),
            tempo_bpm: Some(72.0),
            time_signature: Some(TimeSignature::COMMON),
            chart_ref: Some(ChartRef::from_path("arrangements/default/chart.kf")),
            parts: PartsManifest {
                parts: vec![
                    Part {
                        name: "Verse 1".to_string(),
                        bars: Some(8),
                        patch: Some("Dry Piano".to_string()),
                        resource_refs: vec![ResourceRef {
                            path: Some("arrangements/default/verse-1.md".to_string()),
                            attachment_id: None,
                            kind: Some("chart".to_string()),
                        }],
                        ..Part::default()
                    },
                    // The overriding part: half-time and in 6/8, so it exercises
                    // both inheritance escapes at once.
                    Part {
                        name: "Bridge".to_string(),
                        bars: Some(16),
                        tempo_bpm: Some(36.0),
                        time_signature: Some(TimeSignature::new(6, 8)),
                        notes: Some("drop to pad under the leader".to_string()),
                        ..Part::default()
                    },
                    Part::new("Chorus"),
                ],
            },
            attachment_refs: vec![AttachmentRef {
                id: "att-001".to_string(),
                path: Some("attachments/reference.mp3".to_string()),
                sha256: Some("deadbeef".to_string()),
                kind: Some("audio".to_string()),
            }],
        };

        let acoustic = Arrangement {
            id: alt_id,
            name: "Acoustic".to_string(),
            key: "G Minor".parse().unwrap(),
            // An arrangement that declares neither: both stay None and
            // round-trip as absent, which is the backward-compatible shape a
            // song written before these fields existed has on disk.
            tempo_bpm: None,
            time_signature: None,
            chart_ref: None,
            parts: PartsManifest::default(),
            attachment_refs: vec![],
        };

        Song {
            id: Uuid::new_v4(),
            title: "Great Are You Lord".to_string(),
            tags: vec!["worship".to_string(), "set-a".to_string()],
            default_arrangement: default_id,
            arrangements: vec![default, acoustic],
        }
    }

    #[test]
    fn round_trips_through_a_folder() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("great-are-you-lord");

        let song = sample_song();
        to_folder(&song, &root).unwrap();

        // Expected on-disk layout exists.
        assert!(root.join("song.md").is_file());
        assert!(
            root.join("arrangements/default/arrangement.md").is_file(),
            "default arrangement record written"
        );
        assert!(
            root.join("arrangements/acoustic/arrangement.md").is_file(),
            "alternate arrangement record written"
        );

        let back = from_folder(&root).unwrap();
        assert_eq!(back, song, "song round-trips byte-for-byte-equal");

        // Spot-check the semantics survived.
        assert_eq!(back.arrangements.len(), 2);
        assert_eq!(back.default().unwrap().name, "Default");
        assert_eq!(back.default().unwrap().key.to_string(), "Bb Major");
        assert_eq!(
            back.arrangement(song.arrangements[1].id)
                .unwrap()
                .key
                .to_string(),
            "G Minor"
        );
    }

    #[test]
    fn tempo_meter_and_parts_survive_a_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("song");
        to_folder(&sample_song(), &root).unwrap();
        let back = from_folder(&root).unwrap();
        let arr = back.default().unwrap();

        assert_eq!(arr.tempo_bpm, Some(72.0));
        assert_eq!(arr.time_signature, Some(TimeSignature::COMMON));
        assert_eq!(arr.parts.parts.len(), 3, "parts keep their running order");
        assert_eq!(arr.parts.parts[0].name, "Verse 1");
        assert_eq!(arr.parts.parts[2].name, "Chorus");
        assert_eq!(arr.parts.total_bars(), None, "Chorus declares no bars");

        // A part that overrides both, and one that inherits both.
        let bridge = &arr.parts.parts[1];
        assert_eq!(bridge.tempo_bpm, Some(36.0));
        assert_eq!(bridge.time_signature, Some(TimeSignature::new(6, 8)));
        assert_eq!(bridge.time_signature.unwrap().to_string(), "6/8");
        assert_eq!(
            arr.parts.parts[2].tempo_bpm, None,
            "inherits the arrangement"
        );
        assert_eq!(arr.parts.parts[0].patch.as_deref(), Some("Dry Piano"));
    }

    #[test]
    fn colliding_arrangement_names_get_distinct_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("song");
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let song = Song {
            id: Uuid::new_v4(),
            title: "Dup".to_string(),
            tags: vec![],
            default_arrangement: a,
            arrangements: vec![
                Arrangement {
                    id: a,
                    name: "Default".to_string(),
                    key: Key::c_major(),
                    tempo_bpm: None,
                    time_signature: None,
                    chart_ref: None,
                    parts: PartsManifest::default(),
                    attachment_refs: vec![],
                },
                Arrangement {
                    id: b,
                    name: "Default".to_string(),
                    key: Key::c_major(),
                    tempo_bpm: None,
                    time_signature: None,
                    chart_ref: None,
                    parts: PartsManifest::default(),
                    attachment_refs: vec![],
                },
            ],
        };
        to_folder(&song, &root).unwrap();
        let back = from_folder(&root).unwrap();
        assert_eq!(back, song);
    }
}
