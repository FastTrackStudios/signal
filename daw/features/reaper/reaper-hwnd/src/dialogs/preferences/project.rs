//! Preferences -> Project page child window IDs.

use crate::ChildId;

/// Preferences -> Project page child window IDs.
pub struct ProjectPrefs;

impl ProjectPrefs {
    /// When opening projects dropdown - Class: ComboBox
    pub const WHEN_OPENING_PROJECTS: ChildId = ChildId(1000);
    /// Project loading settings label - Class: Static
    pub const PROJECT_LOADING_LABEL: ChildId = ChildId(1001);
    /// Prompt to save on new project - Class: Button
    pub const PROMPT_SAVE_ON_NEW: ChildId = ChildId(1002);
    /// Auto-save interval (minutes) inputbox - Class: Edit
    pub const AUTO_SAVE_INTERVAL: ChildId = ChildId(1003);
    /// Auto-save label - Class: Static
    pub const AUTO_SAVE_LABEL: ChildId = ChildId(1004);
    /// Save undo history with project - Class: Button
    pub const SAVE_UNDO_HISTORY: ChildId = ChildId(1005);
    /// Auto-save to timestamped file - Class: Button
    pub const AUTO_SAVE_TIMESTAMPED: ChildId = ChildId(1006);
    /// Compact project on save - Class: Button
    pub const COMPACT_ON_SAVE: ChildId = ChildId(1007);
    /// Save to project directory - Class: Button
    pub const SAVE_TO_PROJECT_DIR: ChildId = ChildId(1008);
    /// Max auto-save files inputbox - Class: Edit
    pub const MAX_AUTO_SAVE_FILES: ChildId = ChildId(1009);
    /// When loading projects with missing files dropdown - Class: ComboBox
    pub const MISSING_FILES_BEHAVIOR: ChildId = ChildId(1010);
}
