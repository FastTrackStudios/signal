//! Open File dialog child window IDs.

use crate::ChildId;

/// Open File dialog child window IDs.
///
/// These are the standard Windows file dialog child IDs as used by REAPER.
pub struct OpenFileDialog;

impl OpenFileDialog {
    /// OK / Open button - Class: Button
    pub const OK: ChildId = ChildId(1);
    /// Cancel button - Class: Button
    pub const CANCEL: ChildId = ChildId(2);
    /// File name inputbox - Class: Edit
    pub const FILE_NAME: ChildId = ChildId(1148);
    /// File type dropdown - Class: ComboBox
    pub const FILE_TYPE: ChildId = ChildId(1136);
    /// Folder tree / places bar - Class: SysTreeView32
    pub const FOLDER_TREE: ChildId = ChildId(1120);
    /// File list - Class: SysListView32
    pub const FILE_LIST: ChildId = ChildId(1121);
    /// Look in dropdown (path bar) - Class: ComboBox
    pub const LOOK_IN: ChildId = ChildId(1137);
    /// File name label - Class: Static
    pub const FILE_NAME_LABEL: ChildId = ChildId(1090);
    /// File type label - Class: Static
    pub const FILE_TYPE_LABEL: ChildId = ChildId(1089);
    /// Up one level button - Class: Button
    pub const UP_ONE_LEVEL: ChildId = ChildId(40961);
    /// Create new folder button - Class: Button
    pub const NEW_FOLDER: ChildId = ChildId(40962);
    /// View menu button - Class: Button
    pub const VIEW_MENU: ChildId = ChildId(40963);
}
