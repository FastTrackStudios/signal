//! Track structure types for song projects and combined setlist projects.
//!
//! Defines the canonical folder hierarchy and track identity for mapping
//! between individual song projects and the merged setlist project.
//! All types are pure data with no DAW dependency.

// The `Facet` derive below expands to offset arithmetic internally (the
// lint fires at the derive macro's own generated code, per clippy's "this
// error originates in the derive macro `Facet`" note) — there is no
// hand-written code here to rewrite without forking the `facet` crate.
#![allow(clippy::arithmetic_side_effects)]

use facet::Facet;

// ─── Track Identity ──────────────────────────────────────────

/// How a track is identified for matching during incremental sync.
///
/// Prefer GUID when available (stable across renames). Fall back to
/// name-path matching (folder hierarchy) when GUIDs are unavailable
/// (e.g., newly created tracks that haven't been saved yet).
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
#[repr(C)]
pub enum TrackIdentity {
    /// REAPER-assigned track GUID — stable across renames.
    Guid(String),
    /// Folder-relative path, e.g. `["TRACKS", "Song A", "Guitar"]`.
    NamePath(Vec<String>),
    /// GUID preferred, name-path as fallback.
    GuidWithFallback {
        guid: String,
        name_path: Vec<String>,
    },
}

impl TrackIdentity {
    /// Create a GUID-preferred identity with name-path fallback.
    #[must_use]
    pub fn new(guid: Option<String>, name_path: Vec<String>) -> Self {
        match guid {
            Some(guid) => Self::GuidWithFallback { guid, name_path },
            None => Self::NamePath(name_path),
        }
    }

    /// Returns the GUID if available.
    #[must_use]
    pub fn guid(&self) -> Option<&str> {
        match self {
            Self::Guid(g) | Self::GuidWithFallback { guid: g, .. } => Some(g),
            Self::NamePath(_) => None,
        }
    }

    /// Returns the name path if available.
    #[must_use]
    pub fn name_path(&self) -> Option<&[String]> {
        match self {
            Self::NamePath(p) | Self::GuidWithFallback { name_path: p, .. } => Some(p),
            Self::Guid(_) => None,
        }
    }

    /// Check if two identities match (GUID takes priority).
    #[must_use]
    pub fn matches(&self, other: &Self) -> bool {
        // Try GUID match first
        if let (Some(a), Some(b)) = (self.guid(), other.guid()) {
            return a == b;
        }
        // Fall back to name-path match
        if let (Some(a), Some(b)) = (self.name_path(), other.name_path()) {
            return a == b;
        }
        false
    }
}

// ─── Well-Known Track Roles ──────────────────────────────────

/// Well-known track roles in the Click/Guide folder.
///
/// These tracks are shared across songs in the setlist project —
/// items from each song get merged onto the same track at the correct offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum GuideTrackRole {
    Click,
    Loop,
    Count,
    Guide,
}

impl GuideTrackRole {
    /// Canonical track name in the DAW.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Click => "Click",
            Self::Loop => "Loop",
            Self::Count => "Count",
            Self::Guide => "Guide",
        }
    }

    /// All guide track roles in display order.
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[Self::Click, Self::Loop, Self::Count, Self::Guide]
    }
}

// ─── Song Track Mapping ──────────────────────────────────────

/// Maps a song's local tracks to their counterparts in the setlist project.
///
/// For each song, this captures:
/// - Which guide tracks (click/loop/count/guide) exist and where their items go
/// - Which content tracks exist and their identities for sync matching
/// - Reference track structure
#[derive(Clone, Debug, PartialEq, Facet)]
pub struct SongTrackMapping {
    /// Song name — used as the folder name in the setlist project.
    pub song_name: String,
    /// Song index in the setlist.
    pub song_index: usize,
    /// Guide tracks found in this song (mapped to shared tracks in setlist).
    pub guide_tracks: Vec<GuideTrackEntry>,
    /// Content tracks (everything under TRACKS/ that isn't Click/Guide or Reference).
    pub content_tracks: Vec<TrackEntry>,
    /// Reference tracks (Mix + Stem Split children).
    pub reference_tracks: ReferenceStructure,
}

/// A guide track entry — identifies a track in the song project that maps
/// to a shared guide track in the setlist project.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct GuideTrackEntry {
    pub role: GuideTrackRole,
    pub identity: TrackIdentity,
}

/// A generic track entry with identity and optional children (for folders).
#[derive(Clone, Debug, PartialEq, Facet)]
pub struct TrackEntry {
    /// Track name as it appears in the DAW.
    pub name: String,
    /// Track identity for sync matching.
    pub identity: TrackIdentity,
    /// Whether this is a folder track.
    pub is_folder: bool,
    /// Child tracks (if this is a folder).
    pub children: Vec<Self>,
}

/// Reference track structure within a song.
#[derive(Clone, Debug, Default, PartialEq, Facet)]
pub struct ReferenceStructure {
    /// Identity of the Reference folder itself.
    pub folder_identity: Option<TrackIdentity>,
    /// Mix track identity (if present).
    pub mix_identity: Option<TrackIdentity>,
    /// Stem Split folder identity (if present).
    pub stem_split_folder_identity: Option<TrackIdentity>,
    /// Individual stem tracks.
    pub stems: Vec<TrackEntry>,
}

// ─── Setlist Track Structure ─────────────────────────────────

/// Complete track structure for a setlist project.
///
/// ```text
/// Click/Guide/
/// ├── Click         (shared — items from all songs)
/// ├── Loop          (shared)
/// ├── Count         (shared)
/// └── Guide         (shared)
/// TRACKS/
/// ├── {Song 1}/
/// │   ├── (content tracks)
/// │   └── Reference/
/// │       ├── Mix
/// │       └── Stem Split/
/// │           └── (stems)
/// ├── {Song 2}/
/// │   └── ...
/// └── ...
/// ```
#[derive(Clone, Debug, PartialEq, Facet)]
pub struct SetlistTrackStructure {
    /// Per-song track mappings, in setlist order.
    pub song_mappings: Vec<SongTrackMapping>,
}

impl SetlistTrackStructure {
    /// Create a new setlist track structure from song mappings.
    #[must_use]
    pub const fn new(song_mappings: Vec<SongTrackMapping>) -> Self {
        Self { song_mappings }
    }

    /// Get the mapping for a specific song by index.
    #[must_use]
    pub fn song_mapping(&self, song_index: usize) -> Option<&SongTrackMapping> {
        self.song_mappings.get(song_index)
    }

    /// Find a song mapping by name.
    #[must_use]
    pub fn song_mapping_by_name(&self, name: &str) -> Option<&SongTrackMapping> {
        self.song_mappings.iter().find(|m| m.song_name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_identity_guid_match() {
        let a = TrackIdentity::Guid("abc-123".to_string());
        let b = TrackIdentity::GuidWithFallback {
            guid: "abc-123".to_string(),
            name_path: vec!["TRACKS".into(), "Guitar".into()],
        };
        assert!(a.matches(&b));
    }

    #[test]
    fn track_identity_name_path_fallback() {
        let a = TrackIdentity::NamePath(vec!["TRACKS".into(), "Guitar".into()]);
        let b = TrackIdentity::NamePath(vec!["TRACKS".into(), "Guitar".into()]);
        assert!(a.matches(&b));

        let c = TrackIdentity::NamePath(vec!["TRACKS".into(), "Bass".into()]);
        assert!(!a.matches(&c));
    }

    #[test]
    fn track_identity_no_match_different_types() {
        let a = TrackIdentity::Guid("abc".to_string());
        let b = TrackIdentity::NamePath(vec!["TRACKS".into()]);
        // No common basis for comparison
        assert!(!a.matches(&b));
    }

    #[test]
    fn guide_track_role_names() {
        assert_eq!(GuideTrackRole::Click.name(), "Click");
        assert_eq!(GuideTrackRole::Loop.name(), "Loop");
        assert_eq!(GuideTrackRole::Count.name(), "Count");
        assert_eq!(GuideTrackRole::Guide.name(), "Guide");
        assert_eq!(GuideTrackRole::all().len(), 4);
    }

    #[test]
    fn empty_song_produces_skeleton() {
        let mapping = SongTrackMapping {
            song_name: "Empty Song".to_string(),
            song_index: 0,
            guide_tracks: vec![],
            content_tracks: vec![],
            reference_tracks: ReferenceStructure::default(),
        };

        // Even with no content, the mapping exists and can be used to
        // generate the {SONG NAME}/Reference/Stem Split/ skeleton
        assert_eq!(mapping.song_name, "Empty Song");
        assert!(mapping.content_tracks.is_empty());
        assert!(mapping.reference_tracks.stems.is_empty());
    }

    #[test]
    fn setlist_structure_lookup() {
        let structure = SetlistTrackStructure::new(vec![
            SongTrackMapping {
                song_name: "Song A".to_string(),
                song_index: 0,
                guide_tracks: vec![],
                content_tracks: vec![],
                reference_tracks: ReferenceStructure::default(),
            },
            SongTrackMapping {
                song_name: "Song B".to_string(),
                song_index: 1,
                guide_tracks: vec![],
                content_tracks: vec![],
                reference_tracks: ReferenceStructure::default(),
            },
        ]);

        assert!(structure.song_mapping(0).is_some());
        assert_eq!(structure.song_mapping(0).unwrap().song_name, "Song A");
        assert!(structure.song_mapping_by_name("Song B").is_some());
        assert!(structure.song_mapping(2).is_none());
    }
}
