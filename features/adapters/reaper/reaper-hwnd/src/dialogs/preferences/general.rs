//! Preferences -> General page child window IDs.

use crate::ChildId;

/// Preferences -> General page child window IDs.
pub struct GeneralPrefs;

impl GeneralPrefs {
    /// Open project(s) on startup dropdown - Class: ComboBox
    pub const OPEN_PROJECT_ON_STARTUP: ChildId = ChildId(1000);
    /// Default project template inputbox - Class: Edit
    pub const DEFAULT_PROJECT_TEMPLATE: ChildId = ChildId(1001);
    /// Browse for default project template - Class: Button
    pub const BROWSE_PROJECT_TEMPLATE: ChildId = ChildId(1002);
    /// Undo memory (MB) inputbox - Class: Edit
    pub const UNDO_MEMORY_MB: ChildId = ChildId(1003);
    /// Maximum undo memory label - Class: Static
    pub const UNDO_MEMORY_LABEL: ChildId = ChildId(1004);
    /// Max recent projects inputbox - Class: Edit
    pub const MAX_RECENT_PROJECTS: ChildId = ChildId(1005);
    /// Recent projects label - Class: Static
    pub const RECENT_PROJECTS_LABEL: ChildId = ChildId(1006);
    /// Warn when RAM usage exceeds inputbox - Class: Edit
    pub const WARN_RAM: ChildId = ChildId(1007);
    /// RAM warning label - Class: Static
    pub const WARN_RAM_LABEL: ChildId = ChildId(1008);
    /// Show splash screen on startup - Class: Button
    pub const SHOW_SPLASH_SCREEN: ChildId = ChildId(1009);
    /// Undo settings: track selection - Class: Button
    pub const UNDO_TRACK_SELECTION: ChildId = ChildId(1042);
    /// Undo settings: loop/time selection - Class: Button
    pub const UNDO_LOOP_TIME_SELECTION: ChildId = ChildId(1043);
    /// Undo settings: cursor position - Class: Button
    pub const UNDO_CURSOR_POSITION: ChildId = ChildId(1044);
    /// Undo settings: track/item visibility - Class: Button
    pub const UNDO_TRACK_ITEM_VISIBILITY: ChildId = ChildId(1045);
    /// Language dropdown - Class: ComboBox
    pub const LANGUAGE: ChildId = ChildId(1046);
    /// Open properties on new track - Class: Button
    pub const OPEN_PROPERTIES_ON_NEW_TRACK: ChildId = ChildId(1047);
    /// Create undo points for item/track param changes - Class: Button
    pub const UNDO_ITEM_TRACK_PARAM_CHANGES: ChildId = ChildId(1048);
    /// Store multiple redo paths - Class: Button
    pub const STORE_MULTIPLE_REDO_PATHS: ChildId = ChildId(1049);
    /// Allow undo after save - Class: Button
    pub const ALLOW_UNDO_AFTER_SAVE: ChildId = ChildId(1050);
    /// Show last undo point in menu bar - Class: Button
    pub const SHOW_LAST_UNDO_IN_MENU: ChildId = ChildId(1051);
    /// Automatically save to undo history - Class: Button
    pub const AUTO_SAVE_UNDO_HISTORY: ChildId = ChildId(1052);
    /// Timestamps in undo history - Class: Button
    pub const TIMESTAMPS_IN_UNDO: ChildId = ChildId(1053);
    /// Max undo/redo items inputbox - Class: Edit
    pub const MAX_UNDO_REDO_ITEMS: ChildId = ChildId(1054);
    /// Max undo/redo label - Class: Static
    pub const MAX_UNDO_REDO_LABEL: ChildId = ChildId(1055);
    /// Startup settings label - Class: Static
    pub const STARTUP_SETTINGS_LABEL: ChildId = ChildId(1100);
    /// Undo settings label - Class: Static
    pub const UNDO_SETTINGS_LABEL: ChildId = ChildId(1101);
    /// Language label - Class: Static
    pub const LANGUAGE_LABEL: ChildId = ChildId(1102);
    /// Advanced UI/system tweaks button - Class: Button
    pub const ADVANCED_UI_TWEAKS: ChildId = ChildId(1725);
}
