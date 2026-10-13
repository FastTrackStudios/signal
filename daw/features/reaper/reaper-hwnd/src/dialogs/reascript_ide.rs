//! ReaScript IDE dialog child window IDs.

use crate::ChildId;

/// ReaScript IDE dialog child window IDs.
pub struct ReaScriptIde;

impl ReaScriptIde {
    /// Run/close button - Class: Button
    pub const RUN_CLOSE: ChildId = ChildId(1);
    /// Close button - Class: Button
    pub const CLOSE: ChildId = ChildId(2);
    /// Editor area - Class: WDLCursesWindow
    pub const EDITOR: ChildId = ChildId(1000);
    /// File path inputbox - Class: Edit
    pub const FILE_PATH: ChildId = ChildId(1001);
    /// Browse file button - Class: Button
    pub const BROWSE_FILE: ChildId = ChildId(1002);
    /// Run button - Class: Button
    pub const RUN: ChildId = ChildId(1003);
    /// Stop button - Class: Button
    pub const STOP: ChildId = ChildId(1004);
    /// Save button - Class: Button
    pub const SAVE: ChildId = ChildId(1005);
    /// Open button - Class: Button
    pub const OPEN: ChildId = ChildId(1006);
    /// New button - Class: Button
    pub const NEW: ChildId = ChildId(1007);
    /// Console output - Class: RichEditChild
    pub const CONSOLE_OUTPUT: ChildId = ChildId(1008);
    /// Watch list - Class: SysListView32
    pub const WATCH_LIST: ChildId = ChildId(1009);
    /// Add watch button - Class: Button
    pub const ADD_WATCH: ChildId = ChildId(1010);
    /// Remove watch button - Class: Button
    pub const REMOVE_WATCH: ChildId = ChildId(1011);
    /// Step into button - Class: Button
    pub const STEP_INTO: ChildId = ChildId(1012);
    /// Step over button - Class: Button
    pub const STEP_OVER: ChildId = ChildId(1013);
    /// Toggle breakpoint button - Class: Button
    pub const TOGGLE_BREAKPOINT: ChildId = ChildId(1014);
    /// IDE label - Class: Static
    pub const IDE_LABEL: ChildId = ChildId(1100);
}
