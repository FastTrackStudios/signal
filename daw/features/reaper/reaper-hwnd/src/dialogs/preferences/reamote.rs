//! Preferences -> ReaMote page child window IDs.

use crate::ChildId;

/// Preferences -> ReaMote page child window IDs.
pub struct ReaMotePrefs;

impl ReaMotePrefs {
    /// ReaMote server address inputbox - Class: Edit
    pub const SERVER_ADDRESS: ChildId = ChildId(1000);
    /// Server address label - Class: Static
    pub const SERVER_LABEL: ChildId = ChildId(1001);
    /// Port inputbox - Class: Edit
    pub const PORT: ChildId = ChildId(1002);
    /// Port label - Class: Static
    pub const PORT_LABEL: ChildId = ChildId(1003);
    /// Connect button - Class: Button
    pub const CONNECT: ChildId = ChildId(1004);
    /// Disconnect button - Class: Button
    pub const DISCONNECT: ChildId = ChildId(1005);
    /// ReaMote status label - Class: Static
    pub const STATUS_LABEL: ChildId = ChildId(1006);
    /// Slave list - Class: SysListView32
    pub const SLAVE_LIST: ChildId = ChildId(1007);
    /// ReaMote settings label - Class: Static
    pub const REAMOTE_LABEL: ChildId = ChildId(1100);
}
