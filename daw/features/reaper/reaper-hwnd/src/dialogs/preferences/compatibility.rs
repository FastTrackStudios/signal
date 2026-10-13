//! Preferences -> Plug-ins -> Compatibility page child window IDs.

use crate::ChildId;

/// Preferences -> Plug-ins -> Compatibility page child window IDs.
pub struct CompatibilityPrefs;

impl CompatibilityPrefs {
    /// Plug-in compatibility list - Class: SysListView32
    pub const COMPATIBILITY_LIST: ChildId = ChildId(1000);
    /// Add button - Class: Button
    pub const ADD: ChildId = ChildId(1001);
    /// Remove button - Class: Button
    pub const REMOVE: ChildId = ChildId(1002);
    /// Edit button - Class: Button
    pub const EDIT: ChildId = ChildId(1003);
    /// Compatibility settings label - Class: Static
    pub const COMPATIBILITY_LABEL: ChildId = ChildId(1004);
    /// Plug-in name filter inputbox - Class: Edit
    pub const NAME_FILTER: ChildId = ChildId(1005);
    /// Filter label - Class: Static
    pub const FILTER_LABEL: ChildId = ChildId(1006);
}
