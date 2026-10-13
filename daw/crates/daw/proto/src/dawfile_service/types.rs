//! DAW file service data types.

use facet::Facet;

/// Stable summary row for one track inside a parsed project file.
#[derive(Debug, Clone, Default, Facet)]
pub struct ProjectTrackSummary {
    pub name: String,
    pub item_count: u32,
    pub fx_count: u32,
}

/// Result of summarizing a project file from disk.
#[derive(Debug, Clone, Default, Facet)]
pub struct ProjectSummary {
    pub path: String,
    pub version: f64,
    pub version_string: String,
    pub track_count: u32,
    pub marker_count: u32,
    pub region_count: u32,
    pub tracks: Vec<ProjectTrackSummary>,
    /// Empty on success; populated when the path could not be read or parsed.
    pub error: String,
}

/// Per-song info returned alongside a combined setlist.
#[derive(Debug, Clone, Default, Facet)]
pub struct SetlistSong {
    pub index: u32,
    pub name: String,
    pub global_start_seconds: f64,
    pub duration_seconds: f64,
}

/// Options controlling `DawFileService::combine_setlist`.
#[derive(Debug, Clone, Default, Facet)]
pub struct CombineSetlistOptions {
    pub gap_measures: u32,
}

/// Result of combining an `.RPL` setlist into a single `.RPP`.
#[derive(Debug, Clone, Default, Facet)]
pub struct CombineSetlistResult {
    pub input: String,
    pub output: String,
    pub song_count: u32,
    pub gap_measures: u32,
    pub songs: Vec<SetlistSong>,
    pub total_seconds: f64,
    /// Empty on success.
    pub error: String,
}
