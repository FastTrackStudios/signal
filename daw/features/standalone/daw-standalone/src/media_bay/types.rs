//! Media/FX Bay data types — internal to daw-standalone.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BayView {
    /// Unique source files referenced by any take.
    SourceMedia,
    /// Every individual `Item` in the project.
    MediaItems,
    /// Every FX in every chain.
    Effects,
}

/// One row in a bay view. Fields are populated for the kinds that
/// apply — others stay `None`.
#[derive(Clone, Debug)]
pub struct MediaBayEntry {
    /// Stable id within the bay. SourceMedia = file path,
    /// MediaItems = item GUID, Effects = FX GUID.
    pub id: String,
    /// Display name. SourceMedia = basename, MediaItems = active
    /// take name, Effects = plugin_name.
    pub name: String,
    /// Source file path (SourceMedia + MediaItems if known).
    pub path: Option<String>,
    /// Number of project items referencing this entry. For
    /// SourceMedia this is the dedupe usage count.
    pub usage_count: u32,
    /// Whether the entry is "retained" — kept in the bay even
    /// when no items reference it (`Available` status in REAPER).
    pub retained: bool,
    /// Whether every reference (or the entry itself, for
    /// MediaItems) is currently muted. `None` if no references.
    pub all_muted: Option<bool>,
    /// Which bay folder (if any) the entry was filed under.
    pub bay_folder: Option<String>,
}

/// One usage of a source file within the project.
#[derive(Clone, Debug)]
pub struct SourceUsage {
    pub item_guid: String,
    pub take_guid: String,
    /// 0-based ordinal across the project, used to pick "the Nth
    /// instance" in `replace_in_project`.
    pub ordinal: u32,
}

/// Scope for `replace_in_project`.
#[derive(Clone, Debug)]
pub enum ReplaceScope {
    AllInstances,
    SingleInstance { ordinal: u32 },
}

/// Bay-side folder (organization-only; not the project's track
/// folder tree).
#[derive(Clone, Debug)]
pub struct BayFolder {
    pub name: String,
    pub view: BayView,
    pub entries: Vec<String>,
}
