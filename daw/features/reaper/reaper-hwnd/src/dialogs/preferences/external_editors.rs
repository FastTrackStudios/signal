//! Preferences -> External Editors page child window IDs.

use crate::ChildId;

/// Preferences -> External Editors page child window IDs.
pub struct ExternalEditorsPrefs;

impl ExternalEditorsPrefs {
    /// External editor list - Class: SysListView32
    pub const EDITOR_LIST: ChildId = ChildId(1000);
    /// Add editor button - Class: Button
    pub const ADD: ChildId = ChildId(1001);
    /// Edit button - Class: Button
    pub const EDIT: ChildId = ChildId(1002);
    /// Remove button - Class: Button
    pub const REMOVE: ChildId = ChildId(1003);
    /// External editor label - Class: Static
    pub const EDITOR_LABEL: ChildId = ChildId(1004);
    /// Primary external editor path inputbox - Class: Edit
    pub const PRIMARY_EDITOR_PATH: ChildId = ChildId(1005);
    /// Browse primary editor - Class: Button
    pub const BROWSE_PRIMARY: ChildId = ChildId(1006);
}
