//! Preferences -> Plug-ins -> ReaScript page child window IDs.

use crate::ChildId;

/// Preferences -> Plug-ins -> ReaScript page child window IDs.
pub struct ReaScriptPrefs;

impl ReaScriptPrefs {
    /// ReaScript documentation path inputbox - Class: Edit
    pub const DOC_PATH: ChildId = ChildId(1000);
    /// Browse documentation path - Class: Button
    pub const BROWSE_DOC_PATH: ChildId = ChildId(1001);
    /// External editor path inputbox - Class: Edit
    pub const EXTERNAL_EDITOR: ChildId = ChildId(1002);
    /// Browse external editor - Class: Button
    pub const BROWSE_EXTERNAL_EDITOR: ChildId = ChildId(1003);
    /// ReaScript settings label - Class: Static
    pub const REASCRIPT_LABEL: ChildId = ChildId(1004);
    /// Custom colors path inputbox - Class: Edit
    pub const CUSTOM_COLORS_PATH: ChildId = ChildId(1005);
    /// Browse custom colors - Class: Button
    pub const BROWSE_CUSTOM_COLORS: ChildId = ChildId(1006);
}
