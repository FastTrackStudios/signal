//! Types for dynamic template organization
//!
//! These types define the request/response structures for organizing
//! audio items into track hierarchies.

use daw_proto::TrackHierarchy;
use facet::Facet;

/// Request to organize items into a track hierarchy
#[derive(Clone, Debug, Facet)]
pub struct OrganizeRequest {
    /// Item names to organize (e.g., audio file names)
    pub items: Vec<String>,
    /// Optional: existing track names to match against
    pub existing_tracks: Option<Vec<String>>,
    /// Organization options
    pub options: OrganizeOptions,
}

impl OrganizeRequest {
    /// Create a new organize request with default options
    #[must_use]
    pub fn new(items: Vec<String>) -> Self {
        Self {
            items,
            existing_tracks: None,
            options: OrganizeOptions::default(),
        }
    }

    /// Create a request with existing tracks to match against
    #[must_use]
    pub fn with_existing_tracks(mut self, tracks: Vec<String>) -> Self {
        self.existing_tracks = Some(tracks);
        self
    }

    /// Set organization options
    #[must_use]
    pub const fn with_options(mut self, options: OrganizeOptions) -> Self {
        self.options = options;
        self
    }
}

/// Options for organizing items into tracks
#[derive(Clone, Debug, Facet)]
pub struct OrganizeOptions {
    /// Track/folder shape toggles
    pub structure: StructureOptions,
    /// Display-name cleanup toggles
    pub naming: NamingOptions,
    /// Marker- and split-detection toggles
    pub detection: DetectionOptions,
}

/// Toggles that affect the shape of the generated track hierarchy.
#[derive(Clone, Debug, Facet)]
pub struct StructureOptions {
    /// Expand multiple items on a node into individual child tracks
    pub expand_items: bool,
    /// Collapse single-child intermediate folders (e.g., Cymbals/OH -> OH)
    pub collapse_single_child: bool,
}

/// Toggles that clean up display names by stripping redundant context.
#[derive(Clone, Debug, Facet)]
pub struct NamingOptions {
    /// Clean up display names by stripping redundant context
    pub cleanup_names: bool,
    /// Strip detected song/project names from display names
    pub strip_song_names: bool,
    /// Strip tempo markers (e.g., "126bpm") from display names
    pub strip_tempo: bool,
}

/// Toggles that detect and strip equipment/session markers.
#[derive(Clone, Debug, Facet)]
pub struct DetectionOptions {
    /// Strip Pro Tools markers (e.g., "_01", ".01-02") from display names
    pub strip_protools: bool,
    /// Strip equipment names (mics, preamps) from display names
    pub strip_equipment: bool,
    /// Detect and group stem-split outputs (Demucs, LALAL.ai, etc.)
    pub detect_stem_splits: bool,
}

impl Default for OrganizeOptions {
    /// Default options enable all cleanup transformations
    fn default() -> Self {
        Self {
            structure: StructureOptions {
                expand_items: true,
                collapse_single_child: true,
            },
            naming: NamingOptions {
                cleanup_names: true,
                strip_song_names: true,
                strip_tempo: true,
            },
            detection: DetectionOptions {
                strip_protools: true,
                strip_equipment: true,
                detect_stem_splits: true,
            },
        }
    }
}

impl OrganizeOptions {
    /// Create options with no transformations (raw monarchy output)
    #[must_use]
    pub const fn none() -> Self {
        Self {
            structure: StructureOptions {
                expand_items: false,
                collapse_single_child: false,
            },
            naming: NamingOptions {
                cleanup_names: false,
                strip_song_names: false,
                strip_tempo: false,
            },
            detection: DetectionOptions {
                strip_protools: false,
                strip_equipment: false,
                detect_stem_splits: false,
            },
        }
    }

    /// Create options with only basic cleanup (no stripping)
    #[must_use]
    pub const fn basic() -> Self {
        Self {
            structure: StructureOptions {
                expand_items: true,
                collapse_single_child: true,
            },
            naming: NamingOptions {
                cleanup_names: true,
                strip_song_names: false,
                strip_tempo: false,
            },
            detection: DetectionOptions {
                strip_protools: false,
                strip_equipment: false,
                detect_stem_splits: true,
            },
        }
    }
}

/// Result of organizing items
#[derive(Clone, Debug, Facet)]
pub struct OrganizeResponse {
    /// The organized track hierarchy
    pub hierarchy: TrackHierarchy,
    /// Detected song names (if any were found)
    pub detected_song_names: Vec<String>,
    /// Detected tempo in BPM (if any was found)
    pub detected_tempo: Option<f64>,
    /// Number of items that were organized
    pub items_organized: u32,
    /// Number of tracks created
    pub tracks_created: u32,
}

impl OrganizeResponse {
    /// Create an empty response
    #[must_use]
    pub fn empty() -> Self {
        Self {
            hierarchy: TrackHierarchy::default(),
            detected_song_names: Vec::new(),
            detected_tempo: None,
            items_organized: 0,
            tracks_created: 0,
        }
    }
}

/// Information about an instrument group
#[derive(Clone, Debug, Facet)]
pub struct GroupInfo {
    /// Group name (e.g., "Drums", "Bass", "Guitars")
    pub name: String,
    /// Human-readable display name
    pub display_name: String,
    /// Optional description
    pub description: Option<String>,
    /// Patterns this group matches (for debugging/display)
    pub patterns: Vec<String>,
    /// Sub-groups (e.g., Drums has Kick, Snare, etc.)
    pub subgroups: Vec<String>,
}

/// Metadata extracted from an item name
#[derive(Clone, Debug, Default, Facet)]
pub struct ItemMetadata {
    /// Detected tempo in BPM
    pub tempo: Option<f64>,
    /// Detected song/project name
    pub song_name: Option<String>,
    /// Detected equipment names (mics, preamps, etc.)
    pub equipment: Vec<String>,
    /// Pro Tools playlist/take information
    pub protools_playlist: Option<String>,
    /// Pro Tools take number
    pub protools_take: Option<u32>,
    /// Matched group names (e.g., `["Drums", "Kick"]`)
    pub matched_groups: Vec<String>,
}

impl ItemMetadata {
    /// Check if any metadata was extracted
    #[must_use]
    pub const fn has_metadata(&self) -> bool {
        self.tempo.is_some()
            || self.song_name.is_some()
            || !self.equipment.is_empty()
            || self.protools_playlist.is_some()
            || !self.matched_groups.is_empty()
    }
}

/// Request to extract metadata from an item name
#[derive(Clone, Debug, Facet)]
pub struct ExtractMetadataRequest {
    /// The item name to extract metadata from
    pub item_name: String,
}

/// Response containing extracted metadata
#[derive(Clone, Debug, Facet)]
pub struct ExtractMetadataResponse {
    /// The extracted metadata
    pub metadata: ItemMetadata,
    /// The cleaned item name (with metadata stripped)
    pub cleaned_name: String,
}
