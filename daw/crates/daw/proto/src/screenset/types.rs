//! Screenset data types.

use facet::Facet;

/// Where a screenset should be stored.
#[repr(u8)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Facet)]
#[facet(rename_all = "snake_case")]
pub enum ScreensetScope {
    /// Persist in the DAW profile/global extension state.
    #[default]
    Global,
    /// Persist with the current project when the backend supports it.
    Project,
}

/// Discriminator selecting which kind of workspace state the screenset
/// captures and applies.
#[repr(u8)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Facet)]
#[facet(rename_all = "snake_case")]
pub enum ScreensetKind {
    #[default]
    Window,
    /// Per-track TCP/MCP visibility map.
    TrackSet,
    /// Track selection + time selection range.
    SelectionSet,
}

/// Capture/apply options for a screenset operation.
#[derive(Debug, Clone, Default, Facet)]
pub struct ScreensetOptions {
    pub scope: ScreensetScope,
    /// Persist across host restarts when using global storage.
    pub persist: bool,
}

/// A known display in a screenset.
#[derive(Debug, Clone, Default, Facet)]
pub struct ScreensetMonitor {
    pub id: String,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    pub primary: bool,
}

/// Rectangle in screen coordinates.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Facet)]
pub struct ScreensetRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// State for one managed panel/window.
#[derive(Debug, Clone, Default, Facet)]
pub struct ScreensetWindow {
    pub id: String,
    pub title: String,
    pub monitor_id: Option<String>,
    pub bounds: Option<ScreensetRect>,
    pub visible: bool,
    /// REAPER docker id when the window is docked.
    pub dock_id: Option<i32>,
    pub floating: bool,
}

/// Per-track TCP/MCP visibility entry (TrackSet kind). Tracks are
/// addressed by GUID so layouts survive reorderings.
#[derive(Debug, Clone, Default, Facet)]
pub struct ScreensetTrackVisibility {
    pub guid: String,
    /// Recorded for human inspection / migration; not used for matching.
    pub name: String,
    pub visible_in_tcp: bool,
    pub visible_in_mixer: bool,
}

/// Selection-state snapshot (SelectionSet kind).
#[derive(Debug, Clone, Default, Facet)]
pub struct ScreensetSelection {
    pub selected_track_guids: Vec<String>,
    /// Loop / time-selection start (seconds). Equal to end when no
    /// time selection is active.
    pub time_start_seconds: f64,
    pub time_end_seconds: f64,
}

/// Full screenset snapshot. Populated fields depend on `kind`.
#[derive(Debug, Clone, Default, Facet)]
pub struct Screenset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub kind: ScreensetKind,
    pub schema_version: u32,
    pub updated_at_unix: u64,
    /// Workflow tags such as `laptop`, `mixing`, `tracking`.
    pub tags: Vec<String>,
    pub monitors: Vec<ScreensetMonitor>,
    pub windows: Vec<ScreensetWindow>,
    /// Opaque dock-host layout blob for host-specific round trips.
    pub dock_layout_blob: Vec<u8>,
    pub track_visibility: Vec<ScreensetTrackVisibility>,
    pub selection: Option<ScreensetSelection>,
    /// Optional REAPER/extension actions to run after applying.
    pub actions_on_apply: Vec<String>,
}

/// Compact row returned by list operations.
#[derive(Debug, Clone, Default, Facet)]
pub struct ScreensetSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub kind: ScreensetKind,
    pub updated_at_unix: u64,
    pub tags: Vec<String>,
    pub window_count: u32,
    pub monitor_count: u32,
    pub track_visibility_count: u32,
    pub selected_track_count: u32,
    pub action_count: u32,
}

/// Result for mutation operations.
#[derive(Debug, Clone, Default, Facet)]
pub struct ScreensetResult {
    pub ok: bool,
    pub id: Option<String>,
    pub error: Option<String>,
}

impl ScreensetResult {
    pub fn ok(id: impl Into<String>) -> Self {
        Self {
            ok: true,
            id: Some(id.into()),
            error: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            id: None,
            error: Some(message.into()),
        }
    }
}

/// Request for capturing a screenset.
#[derive(Debug, Clone, Default, Facet)]
pub struct CaptureScreensetRequest {
    pub id: String,
    pub name: String,
    pub description: String,
    pub kind: ScreensetKind,
    pub tags: Vec<String>,
    pub actions_on_apply: Vec<String>,
    pub options: ScreensetOptions,
}
