//! WindowManager data types — named workspace layouts that bundle
//! REAPER toolbar visibility and dock placement.
//!
//! Layouts are stored on disk as JSON (see `daw_reaper::window_manager`)
//! and applied by walking each toolbar's recorded state and reproducing
//! it via REAPER's public dock / show-action / position APIs. We
//! deliberately do **not** reach into REAPER's `reaper-screensets.ini` —
//! that file's per-window blobs (`poslist*_data`) are owned by
//! plugin-registered screenset callbacks and aren't safe to round-trip
//! from outside REAPER itself.

use facet::Facet;

/// Monitor-relative rectangle. Stored as fractions of monitor
/// width/height so the same layout reproduces sensibly across
/// different screens / DPI settings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Facet)]
pub struct MonitorRect {
    /// Left edge as a fraction of monitor width (`0.0..=1.0`).
    pub x_frac: f32,
    /// Top edge as a fraction of monitor height (`0.0..=1.0`).
    pub y_frac: f32,
    /// Width as a fraction of monitor width.
    pub w_frac: f32,
    /// Height as a fraction of monitor height.
    pub h_frac: f32,
}

/// Where a toolbar should land when its layout is applied.
///
/// `docker_id` values come straight from REAPER's `DockIsChildOfDock` —
/// they're stable per-machine but user-configurable, so docker_id=0
/// means "the docker REAPER currently has assigned as docker 0", not
/// "the top docker". Cross-machine portability requires the user to
/// keep docker positions consistent or re-save layouts per machine.
#[derive(Debug, Clone, Default, PartialEq, Facet)]
#[facet(rename_all = "snake_case")]
#[repr(u8)]
pub enum LayoutPlacement {
    /// Toolbar is hidden in this layout.
    #[default]
    Hidden,
    /// Toolbar floats as a separate window at the given monitor-relative
    /// rectangle.
    Floating { rect: MonitorRect },
    /// Toolbar is docked into one of REAPER's fixed dockers.
    Docked { docker_id: i32 },
    /// Toolbar is docked into one of REAPER's floating docker frames
    /// (free-positioned containers that can hold multiple panels).
    FloatingDocker { docker_id: i32 },
}

/// One toolbar's contribution to a layout.
#[derive(Debug, Clone, Default, Facet)]
pub struct LayoutToolbar {
    /// Toolbar name as it appears in `reaper-menu.ini` (the `title=`
    /// line of a `[Floating toolbar N]` section), e.g. `Organize 1`.
    pub toolbar_name: String,
    pub placement: LayoutPlacement,
}

/// Apply-time options.
#[derive(Debug, Clone, Copy, Default, Facet)]
pub struct WindowLayoutOptions {
    /// Whether to fire any layout-level post-apply REAPER actions.
    /// Defaults to `true`; set `false` for headless / preview use.
    pub run_actions: bool,
}

/// Full layout definition. `name` is the user-facing identifier and
/// the basename of the layout's on-disk JSON file.
#[derive(Debug, Clone, Default, Facet)]
pub struct WindowLayout {
    pub name: String,
    pub description: String,
    /// Per-toolbar placement records. Toolbars not in this list are
    /// untouched on apply (a layout can declare a partial state).
    pub toolbars: Vec<LayoutToolbar>,
    /// REAPER action command IDs to fire after applying the layout
    /// (e.g. `_FTS_…` named commands or numeric IDs as strings).
    pub actions_on_apply: Vec<String>,
}

/// Compact list row.
#[derive(Debug, Clone, Default, Facet)]
pub struct WindowLayoutSummary {
    pub name: String,
    pub description: String,
    pub toolbar_count: u32,
    pub action_count: u32,
}

/// Maps the three mode-toolbar slot positions to REAPER docker IDs.
/// Persisted at `<resource_path>/fasttrackstudio/mode_docker_layout.json`.
///
/// REAPER's 16 dockers are user-configured to physical screen positions
/// (top/left/right/bottom), so we can't infer the mapping — the user
/// tells us. Slot 1 of every mode goes to `top`, slot 2 to `left`,
/// slot 3 to `right`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub struct ModeDockerLayout {
    pub top: i32,
    pub left: i32,
    pub right: i32,
}

impl Default for ModeDockerLayout {
    fn default() -> Self {
        Self {
            top: 0,
            left: 1,
            right: 2,
        }
    }
}

/// Result of a mutating operation.
#[derive(Debug, Clone, Default, Facet)]
pub struct WindowLayoutResult {
    pub ok: bool,
    pub name: Option<String>,
    pub error: Option<String>,
}

impl WindowLayoutResult {
    pub fn ok(name: impl Into<String>) -> Self {
        Self {
            ok: true,
            name: Some(name.into()),
            error: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            name: None,
            error: Some(message.into()),
        }
    }
}
