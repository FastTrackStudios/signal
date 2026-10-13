//! ReaScript console dialog child window IDs.

use crate::ChildId;

/// ReaScript console dialog child window IDs.
pub struct ReaScriptConsole;

impl ReaScriptConsole {
    /// Run button - Class: Button
    pub const RUN: ChildId = ChildId(1);
    /// Close button - Class: Button
    pub const CLOSE: ChildId = ChildId(2);
    /// Console output area - Class: RichEditChild
    pub const OUTPUT: ChildId = ChildId(1000);
    /// Command inputbox - Class: Edit
    pub const COMMAND_INPUT: ChildId = ChildId(1001);
    /// Clear output button - Class: Button
    pub const CLEAR: ChildId = ChildId(1002);
    /// Execute button - Class: Button
    pub const EXECUTE: ChildId = ChildId(1003);
    /// Console label - Class: Static
    pub const CONSOLE_LABEL: ChildId = ChildId(1004);
}
