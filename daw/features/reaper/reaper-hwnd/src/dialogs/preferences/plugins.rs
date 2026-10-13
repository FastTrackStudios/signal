//! Preferences -> Plug-ins page child window IDs.

use crate::ChildId;

/// Preferences -> Plug-ins page child window IDs.
pub struct PluginsPrefs;

impl PluginsPrefs {
    /// VST plug-in paths inputbox - Class: Edit
    pub const VST_PATHS: ChildId = ChildId(1000);
    /// VST paths label - Class: Static
    pub const VST_PATHS_LABEL: ChildId = ChildId(1001);
    /// Rescan button - Class: Button
    pub const RESCAN: ChildId = ChildId(1002);
    /// Clear cache and rescan - Class: Button
    pub const CLEAR_CACHE_RESCAN: ChildId = ChildId(1003);
    /// Auto-detect plug-in paths - Class: Button
    pub const AUTO_DETECT_PATHS: ChildId = ChildId(1004);
    /// Plug-in scan on startup dropdown - Class: ComboBox
    pub const SCAN_ON_STARTUP: ChildId = ChildId(1005);
    /// Scan on startup label - Class: Static
    pub const SCAN_LABEL: ChildId = ChildId(1006);
    /// Scan for new plug-ins on focus - Class: Button
    pub const SCAN_ON_FOCUS: ChildId = ChildId(1007);
    /// Plug-in bridge mode dropdown - Class: ComboBox
    pub const BRIDGE_MODE: ChildId = ChildId(1008);
    /// Bridge label - Class: Static
    pub const BRIDGE_LABEL: ChildId = ChildId(1009);
    /// Plug-in settings label - Class: Static
    pub const PLUGINS_LABEL: ChildId = ChildId(1100);
}
