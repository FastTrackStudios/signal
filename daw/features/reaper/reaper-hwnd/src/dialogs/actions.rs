//! Actions dialog child window IDs.

use crate::ChildId;

/// Actions dialog child window IDs.
pub struct ActionsDialog;

impl ActionsDialog {
    /// Run/close - Class: Button
    pub const RUN_CLOSE: ChildId = ChildId(1);
    /// Close - Class: Button
    pub const CLOSE: ChildId = ChildId(2);
    /// New - Class: Button
    pub const NEW: ChildId = ChildId(3);
    /// Delete - Class: Button
    pub const DELETE: ChildId = ChildId(4);
    /// Edit - Class: Button
    pub const EDIT: ChildId = ChildId(5);
    /// Import/export - Class: Button
    pub const IMPORT_EXPORT: ChildId = ChildId(6);
    /// Copy - Class: Button
    pub const COPY: ChildId = ChildId(7);
    /// Add - Class: Button
    pub const ADD: ChildId = ChildId(8);
    /// Find shortcut - Class: Button
    pub const FIND_SHORTCUT: ChildId = ChildId(9);
    /// Delete (secondary) - Class: Button
    pub const DELETE_2: ChildId = ChildId(10);
    /// Clear - Class: Button
    pub const CLEAR: ChildId = ChildId(11);
    /// Run - Class: Button
    pub const RUN: ChildId = ChildId(13);
    /// Load - Class: Button
    pub const LOAD: ChildId = ChildId(16);
    /// Edit (secondary) - Class: Button
    pub const EDIT_2: ChildId = ChildId(17);
    /// Delete (tertiary) - Class: Button
    pub const DELETE_3: ChildId = ChildId(18);
    /// New (secondary) - Class: Button
    pub const NEW_2: ChildId = ChildId(19);
    /// Filter label - Class: Static
    pub const FILTER_LABEL: ChildId = ChildId(1279);
    /// Section dropdown - Class: ComboBox
    pub const SECTION: ChildId = ChildId(1317);
    /// Actions list - Class: SysListView32
    pub const ACTIONS_LIST: ChildId = ChildId(1323);
    /// Filter inputbox - Class: Edit
    pub const FILTER: ChildId = ChildId(1324);
    /// Section label - Class: Static
    pub const SECTION_LABEL: ChildId = ChildId(1328);
    /// List - Class: ListBox
    pub const LIST: ChildId = ChildId(1329);
    /// Shortcuts for selected action - Class: Button
    pub const SHORTCUTS_FOR_SELECTED: ChildId = ChildId(1331);
    /// ReaScript label - Class: Static
    pub const REASCRIPT_LABEL: ChildId = ChildId(1526);
    /// Custom actions label - Class: Static
    pub const CUSTOM_ACTIONS_LABEL: ChildId = ChildId(1527);
    /// Menu editor button - Class: Button
    pub const MENU_EDITOR: ChildId = ChildId(1528);
}
