//! Toolbar data types — targets, placements, buttons, snapshots.

use facet::Facet;

/// Target toolbar for button placement.
#[repr(u8)]
#[derive(Debug, Clone, Default, PartialEq, Eq, Facet)]
pub enum ToolbarTarget {
    #[default]
    Main,
    /// Floating toolbar (1–32).
    Floating(u8),
    /// Floating MIDI toolbar (1–8).
    Midi(u8),
}

/// Where a toolbar item should be inserted.
#[repr(u8)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Facet)]
#[facet(rename_all = "snake_case")]
pub enum ToolbarPlacement {
    #[default]
    Append,
    /// Zero-based toolbar position.
    Position(u32),
}

/// How REAPER should resolve a toolbar icon value.
#[repr(u8)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Facet)]
#[facet(rename_all = "snake_case")]
pub enum ToolbarIconKind {
    /// REAPER toolbar icon file name (e.g. `toolbar_custom.png`).
    #[default]
    FileName,
    /// Filesystem path to an icon file.
    Path,
}

/// Icon assigned to a toolbar button.
#[derive(Debug, Clone, Default, PartialEq, Eq, Facet)]
pub struct ToolbarIcon {
    pub kind: ToolbarIconKind,
    pub value: String,
}

/// A toolbar button to add or update.
#[derive(Debug, Clone, Default, Facet)]
pub struct ToolbarButton {
    /// REAPER command name (e.g., `_FTS_SIGNAL_OPEN_BROWSER`).
    pub command_name: String,
    pub label: String,
    pub icon: Option<ToolbarIcon>,
    pub target: ToolbarTarget,
    pub placement: ToolbarPlacement,
    /// Toolbar button flags (bitmask).
    pub flags: u32,
}

/// Result of a toolbar operation.
#[derive(Debug, Clone, Default, Facet)]
pub struct ToolbarResult {
    pub ok: bool,
    pub command_id: Option<u32>,
    pub error: Option<String>,
}

impl ToolbarResult {
    pub fn ok(command_id: u32) -> Self {
        Self {
            ok: true,
            command_id: Some(command_id),
            error: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            command_id: None,
            error: Some(message.into()),
        }
    }
}

/// Source used to build a toolbar snapshot.
#[repr(u8)]
#[derive(Debug, Clone, Default, PartialEq, Eq, Facet)]
#[facet(rename_all = "snake_case")]
pub enum ToolbarSnapshotSource {
    /// Live REAPER API state.
    #[default]
    Live,
    /// Parsed `reaper-menu.ini` state.
    Config,
}

/// A toolbar snapshot.
#[derive(Debug, Clone, Default, Facet)]
pub struct ToolbarSnapshot {
    pub toolbar_name: String,
    pub source: ToolbarSnapshotSource,
    pub items: Vec<ToolbarItemInfo>,
}

/// A single toolbar item.
#[derive(Debug, Clone, Default, Facet)]
pub struct ToolbarItemInfo {
    pub position: u32,
    /// `command`, `separator`, `submenu-start`, `submenu-end`, `unknown`.
    pub kind: String,
    pub command_id: Option<u32>,
    pub command_name: Option<String>,
    pub label: String,
    pub flags: u32,
    pub icon: Option<String>,
    /// Raw config line value when parsed from `reaper-menu.ini`.
    pub raw: Option<String>,
}

/// A tracked toolbar button entry.
#[derive(Debug, Clone, Facet)]
pub struct TrackedButton {
    pub toolbar_name: String,
    pub command_name: String,
    pub workflow_id: String,
}
