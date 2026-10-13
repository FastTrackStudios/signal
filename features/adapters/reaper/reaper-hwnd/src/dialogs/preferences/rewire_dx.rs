//! Preferences -> Plug-ins -> ReWire/DX page child window IDs.

use crate::ChildId;

/// Preferences -> Plug-ins -> ReWire/DX page child window IDs.
pub struct ReWireDxPrefs;

impl ReWireDxPrefs {
    /// ReWire device list - Class: SysListView32
    pub const REWIRE_LIST: ChildId = ChildId(1000);
    /// Enable ReWire - Class: Button
    pub const ENABLE_REWIRE: ChildId = ChildId(1001);
    /// DirectX plug-in list - Class: SysListView32
    pub const DX_LIST: ChildId = ChildId(1002);
    /// Enable DirectX - Class: Button
    pub const ENABLE_DX: ChildId = ChildId(1003);
    /// ReWire label - Class: Static
    pub const REWIRE_LABEL: ChildId = ChildId(1004);
    /// DirectX label - Class: Static
    pub const DX_LABEL: ChildId = ChildId(1005);
}
