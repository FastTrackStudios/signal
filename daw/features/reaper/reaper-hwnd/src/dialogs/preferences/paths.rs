//! Preferences -> Paths page child window IDs.

use crate::ChildId;

/// Preferences -> Paths page child window IDs.
pub struct PathsPrefs;

impl PathsPrefs {
    /// Default path to save new projects inputbox - Class: Edit
    pub const DEFAULT_PROJECT_PATH: ChildId = ChildId(1000);
    /// Browse default project path - Class: Button
    pub const BROWSE_PROJECT_PATH: ChildId = ChildId(1001);
    /// Default render path inputbox - Class: Edit
    pub const DEFAULT_RENDER_PATH: ChildId = ChildId(1002);
    /// Browse default render path - Class: Button
    pub const BROWSE_RENDER_PATH: ChildId = ChildId(1003);
    /// Default recording path inputbox - Class: Edit
    pub const DEFAULT_RECORDING_PATH: ChildId = ChildId(1004);
    /// Browse default recording path - Class: Button
    pub const BROWSE_RECORDING_PATH: ChildId = ChildId(1005);
    /// Store all peak caches in alternate path - Class: Button
    pub const STORE_PEAK_CACHES_ALTERNATE: ChildId = ChildId(1006);
    /// Alternate peak cache path inputbox - Class: Edit
    pub const ALTERNATE_PEAK_CACHE_PATH: ChildId = ChildId(1007);
    /// Browse alternate peak cache path - Class: Button
    pub const BROWSE_PEAK_CACHE_PATH: ChildId = ChildId(1008);
    /// Default path label - Class: Static
    pub const DEFAULT_PATH_LABEL: ChildId = ChildId(1100);
    /// Render path label - Class: Static
    pub const RENDER_PATH_LABEL: ChildId = ChildId(1101);
    /// Recording path label - Class: Static
    pub const RECORDING_PATH_LABEL: ChildId = ChildId(1102);
}
