//! Preferences -> Control/OSC/Web page child window IDs.

use crate::ChildId;

/// Preferences -> Control/OSC/Web page child window IDs.
pub struct ControlOscPrefs;

impl ControlOscPrefs {
    /// Control surface list - Class: SysListView32
    pub const SURFACE_LIST: ChildId = ChildId(1000);
    /// Add surface button - Class: Button
    pub const ADD: ChildId = ChildId(1001);
    /// Edit surface button - Class: Button
    pub const EDIT: ChildId = ChildId(1002);
    /// Remove surface button - Class: Button
    pub const REMOVE: ChildId = ChildId(1003);
    /// Control surface label - Class: Static
    pub const SURFACE_LABEL: ChildId = ChildId(1004);
    /// OSC settings label - Class: Static
    pub const OSC_LABEL: ChildId = ChildId(1005);
}
