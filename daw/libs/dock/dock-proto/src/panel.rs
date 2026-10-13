//! Panel registry — all known panel types in the application.
//!
//! Each variant maps to a specific UI component that the dock renderer
//! will display. Adding a new panel = adding a variant here + adding a
//! match arm in the app's panel renderer callback.

use facet::Facet;

/// All known panel types in the application.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum PanelId {
    // Session panels
    Performance,
    ChartEditor,
    ChartPreview,
    Setlist,
    Settings,

    // Signal/Rig panels
    RigGrid,
    RigNodeGraph,
    PresetBrowser,
    ProfileBrowser,
    SongParts,
    SongSelector,
    SceneGrid,
    RigEditor,
    RigGridEditor,
    RigDetailEditor,
    FxBrowser,

    // DAW panels
    Transport,
    Mixer,
    FxChainTree,
    TrackControlPanel,
    ArrangementView,

    // Utility panels
    Navigator,
    Inspector,
    KeyboardVisualizer,

    // Sync panels
    SyncStatus,
    SyncSettings,

    // Test / development panels
    SnapshotTest,
}

impl std::fmt::Display for PanelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.display_name())
    }
}

impl From<PanelId> for String {
    fn from(panel: PanelId) -> Self {
        panel.as_str().to_owned()
    }
}

impl PanelId {
    /// Stable string identifier for this panel (used as registry key).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Performance => "performance",
            Self::ChartEditor => "chart-editor",
            Self::ChartPreview => "chart-preview",
            Self::Setlist => "setlist",
            Self::Settings => "settings",
            Self::RigGrid => "rig-grid",
            Self::RigNodeGraph => "rig-node-graph",
            Self::PresetBrowser => "preset-browser",
            Self::ProfileBrowser => "profile-browser",
            Self::SongParts => "song-parts",
            Self::SongSelector => "song-selector",
            Self::SceneGrid => "scene-grid",
            Self::RigEditor => "rig-editor",
            Self::RigGridEditor => "rig-grid-editor",
            Self::RigDetailEditor => "rig-detail-editor",
            Self::FxBrowser => "fx-browser",
            Self::Transport => "transport",
            Self::Mixer => "mixer",
            Self::FxChainTree => "fx-chain-tree",
            Self::TrackControlPanel => "track-control-panel",
            Self::ArrangementView => "arrangement-view",
            Self::Navigator => "navigator",
            Self::Inspector => "inspector",
            Self::KeyboardVisualizer => "keyboard-visualizer",
            Self::SyncStatus => "sync-status",
            Self::SyncSettings => "sync-settings",
            Self::SnapshotTest => "snapshot-test",
        }
    }

    /// Look up a PanelId from its string identifier. Returns `None` for
    /// unrecognized strings (which may be dynamic panels from the registry).
    pub fn from_str_id(s: &str) -> Option<Self> {
        match s {
            "performance" => Some(Self::Performance),
            "chart-editor" => Some(Self::ChartEditor),
            "chart-preview" => Some(Self::ChartPreview),
            "setlist" => Some(Self::Setlist),
            "settings" => Some(Self::Settings),
            "rig-grid" => Some(Self::RigGrid),
            "rig-node-graph" => Some(Self::RigNodeGraph),
            "preset-browser" => Some(Self::PresetBrowser),
            "profile-browser" => Some(Self::ProfileBrowser),
            "song-parts" => Some(Self::SongParts),
            "song-selector" => Some(Self::SongSelector),
            "scene-grid" => Some(Self::SceneGrid),
            "rig-editor" => Some(Self::RigEditor),
            "rig-grid-editor" => Some(Self::RigGridEditor),
            "rig-detail-editor" => Some(Self::RigDetailEditor),
            "fx-browser" => Some(Self::FxBrowser),
            "transport" => Some(Self::Transport),
            "mixer" => Some(Self::Mixer),
            "fx-chain-tree" => Some(Self::FxChainTree),
            "track-control-panel" => Some(Self::TrackControlPanel),
            "arrangement-view" => Some(Self::ArrangementView),
            "navigator" => Some(Self::Navigator),
            "inspector" => Some(Self::Inspector),
            "keyboard-visualizer" => Some(Self::KeyboardVisualizer),
            "sync-status" => Some(Self::SyncStatus),
            "sync-settings" => Some(Self::SyncSettings),
            "snapshot-test" => Some(Self::SnapshotTest),
            _ => None,
        }
    }

    /// Register all built-in panels into a [`PanelRegistry`](crate::registry::PanelRegistry).
    ///
    /// This seeds the registry with descriptors for all `PanelId` variants,
    /// preserving backward compatibility while enabling dynamic registration.
    pub fn register_all(registry: &mut crate::registry::PanelRegistry) {
        use crate::registry::{DockPosition, PanelDescriptor};

        for &panel in Self::all() {
            let category = match panel {
                Self::Performance
                | Self::ChartEditor
                | Self::ChartPreview
                | Self::Setlist
                | Self::Settings => "Session",
                Self::RigGrid
                | Self::RigNodeGraph
                | Self::PresetBrowser
                | Self::ProfileBrowser
                | Self::SongParts
                | Self::SongSelector
                | Self::SceneGrid
                | Self::RigEditor
                | Self::RigGridEditor
                | Self::RigDetailEditor
                | Self::FxBrowser => "Signal",
                Self::Transport
                | Self::Mixer
                | Self::FxChainTree
                | Self::TrackControlPanel
                | Self::ArrangementView => "DAW",
                Self::Navigator | Self::Inspector | Self::KeyboardVisualizer => "Utility",
                Self::SyncStatus | Self::SyncSettings => "Sync",
                Self::SnapshotTest => "Signal",
            };

            let default_position = match panel {
                Self::Navigator
                | Self::Setlist
                | Self::PresetBrowser
                | Self::ProfileBrowser
                | Self::TrackControlPanel => DockPosition::Left,
                Self::Inspector | Self::FxBrowser => DockPosition::Right,
                Self::Transport
                | Self::Mixer
                | Self::SceneGrid
                | Self::SnapshotTest
                | Self::RigDetailEditor => DockPosition::Bottom,
                Self::RigEditor | Self::RigGridEditor => DockPosition::Center,
                Self::FxChainTree => DockPosition::Left,
                Self::ArrangementView => DockPosition::Center,
                _ => DockPosition::Center,
            };

            let mut descriptor = PanelDescriptor::new(panel.as_str(), panel.display_name())
                .with_icon(panel.icon_name())
                .with_category(category)
                .with_default_position(default_position);

            if matches!(panel, Self::Transport) {
                descriptor = descriptor.with_visibility_when("tab:performance");
            }

            registry.register(descriptor);
        }
    }

    /// Human-readable display name for the panel.
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Performance => "Performance",
            Self::ChartEditor => "Chart Editor",
            Self::ChartPreview => "Chart Preview",
            Self::Setlist => "Setlist",
            Self::Settings => "Settings",
            Self::RigGrid => "Rig Grid",
            Self::RigNodeGraph => "Rig Node Graph",
            Self::PresetBrowser => "Preset Browser",
            Self::ProfileBrowser => "Profile Browser",
            Self::SongParts => "Song Parts",
            Self::SongSelector => "Song Selector",
            Self::SceneGrid => "Scene Grid",
            Self::RigEditor => "Rig Editor",
            Self::RigGridEditor => "Grid Editor",
            Self::RigDetailEditor => "Detail Editor",
            Self::FxBrowser => "FX Parameters",
            Self::Transport => "Transport",
            Self::Mixer => "Mixer",
            Self::FxChainTree => "FX Chain",
            Self::TrackControlPanel => "Track Control Panel",
            Self::ArrangementView => "Arrangement",
            Self::Navigator => "Navigator",
            Self::Inspector => "Inspector",
            Self::KeyboardVisualizer => "Keyboard Visualizer",
            Self::SyncStatus => "Sync Status",
            Self::SyncSettings => "Sync Settings",
            Self::SnapshotTest => "Snapshot Test",
        }
    }

    /// Icon identifier (for lucide-dioxus or similar).
    pub const fn icon_name(self) -> &'static str {
        match self {
            Self::Performance => "play",
            Self::ChartEditor => "music",
            Self::ChartPreview => "music-2",
            Self::Setlist => "list-music",
            Self::Settings => "settings",
            Self::RigGrid => "grid-3x3",
            Self::RigNodeGraph => "workflow",
            Self::PresetBrowser => "folder-open",
            Self::ProfileBrowser => "user",
            Self::SongParts => "layers",
            Self::SongSelector => "list-music",
            Self::SceneGrid => "layout-grid",
            Self::RigEditor => "guitar",
            Self::RigGridEditor => "grid-3x3",
            Self::RigDetailEditor => "sliders-horizontal",
            Self::FxBrowser => "plug",
            Self::Transport => "disc",
            Self::Mixer => "sliders-horizontal",
            Self::FxChainTree => "list-tree",
            Self::TrackControlPanel => "list",
            Self::ArrangementView => "layout-dashboard",
            Self::Navigator => "compass",
            Self::Inspector => "search",
            Self::KeyboardVisualizer => "keyboard",
            Self::SyncStatus => "radio",
            Self::SyncSettings => "settings-2",
            Self::SnapshotTest => "camera",
        }
    }

    /// Get all panels in display order.
    pub fn all() -> &'static [PanelId] {
        &[
            Self::Performance,
            Self::ChartEditor,
            Self::ChartPreview,
            Self::Setlist,
            Self::RigGrid,
            Self::RigNodeGraph,
            Self::PresetBrowser,
            Self::ProfileBrowser,
            Self::SongParts,
            Self::SongSelector,
            Self::SceneGrid,
            Self::RigEditor,
            Self::RigGridEditor,
            Self::RigDetailEditor,
            Self::FxBrowser,
            Self::Transport,
            Self::Mixer,
            Self::FxChainTree,
            Self::TrackControlPanel,
            Self::ArrangementView,
            Self::Navigator,
            Self::Inspector,
            Self::KeyboardVisualizer,
            Self::SyncStatus,
            Self::SyncSettings,
            Self::SnapshotTest,
            Self::Settings,
        ]
    }
}
