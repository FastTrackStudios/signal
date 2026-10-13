//! Preferences -> Plug-ins -> VST page child window IDs.

use crate::ChildId;

/// Preferences -> Plug-ins -> VST page child window IDs.
pub struct VstPrefs;

impl VstPrefs {
    /// VST plug-in path list - Class: Edit
    pub const PATH_LIST: ChildId = ChildId(1000);
    /// Edit paths button - Class: Button
    pub const EDIT_PATHS: ChildId = ChildId(1001);
    /// Rescan - Class: Button
    pub const RESCAN: ChildId = ChildId(1002);
    /// Clear cache/rescan - Class: Button
    pub const CLEAR_CACHE_RESCAN: ChildId = ChildId(1003);
    /// VST folder label - Class: Static
    pub const FOLDER_LABEL: ChildId = ChildId(1004);
    /// Notify on plug-in scan errors - Class: Button
    pub const NOTIFY_SCAN_ERRORS: ChildId = ChildId(1005);
    /// Don't prompt for save on unloaded VST changes - Class: Button
    pub const NO_PROMPT_UNLOADED: ChildId = ChildId(1006);
    /// VST bridging settings dropdown - Class: ComboBox
    pub const BRIDGING: ChildId = ChildId(1007);
    /// Bridging label - Class: Static
    pub const BRIDGING_LABEL: ChildId = ChildId(1008);
    /// Instrument settings label - Class: Static
    pub const INSTRUMENT_LABEL: ChildId = ChildId(1009);
    /// VST settings label - Class: Static
    pub const VST_LABEL: ChildId = ChildId(1100);
}
