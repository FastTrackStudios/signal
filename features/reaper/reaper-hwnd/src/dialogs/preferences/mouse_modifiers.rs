//! Preferences -> Mouse Modifiers page child window IDs.

use crate::ChildId;

/// Preferences -> Mouse Modifiers page child window IDs.
pub struct MouseModifiersPrefs;

impl MouseModifiersPrefs {
    /// Context dropdown - Class: ComboBox
    pub const CONTEXT: ChildId = ChildId(1000);
    /// Context label - Class: Static
    pub const CONTEXT_LABEL: ChildId = ChildId(1001);
    /// Modifier actions list - Class: SysListView32
    pub const MODIFIER_LIST: ChildId = ChildId(1002);
    /// Edit action button - Class: Button
    pub const EDIT_ACTION: ChildId = ChildId(1003);
    /// Reset to defaults - Class: Button
    pub const RESET_DEFAULTS: ChildId = ChildId(1004);
    /// Action label - Class: Static
    pub const ACTION_LABEL: ChildId = ChildId(1005);
    /// Modifier combination label - Class: Static
    pub const MODIFIER_LABEL: ChildId = ChildId(1006);
}
