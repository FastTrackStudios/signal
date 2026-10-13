//! Track Manager dialog child window IDs.

use crate::ChildId;

/// Track Manager dialog child window IDs.
pub struct TrackManager;

impl TrackManager {
    /// Close button - Class: Button
    pub const CLOSE: ChildId = ChildId(2);
    /// Track list - Class: SysListView32
    pub const TRACK_LIST: ChildId = ChildId(1000);
    /// Show all tracks button - Class: Button
    pub const SHOW_ALL: ChildId = ChildId(1001);
    /// Hide all tracks button - Class: Button
    pub const HIDE_ALL: ChildId = ChildId(1002);
    /// Show selected tracks button - Class: Button
    pub const SHOW_SELECTED: ChildId = ChildId(1003);
    /// Invert visibility button - Class: Button
    pub const INVERT_VISIBILITY: ChildId = ChildId(1004);
    /// Filter inputbox - Class: Edit
    pub const FILTER: ChildId = ChildId(1005);
    /// Filter label - Class: Static
    pub const FILTER_LABEL: ChildId = ChildId(1006);
    /// Show TCP column - Class: Button
    pub const SHOW_TCP: ChildId = ChildId(1007);
    /// Show MCP column - Class: Button
    pub const SHOW_MCP: ChildId = ChildId(1008);
    /// Show in arrange - Class: Button
    pub const SHOW_IN_ARRANGE: ChildId = ChildId(1009);
    /// Mirror TCP/MCP visibility - Class: Button
    pub const MIRROR_VISIBILITY: ChildId = ChildId(1010);
    /// Dock in docker - Class: Button
    pub const DOCK: ChildId = ChildId(1100);
    /// Track manager label - Class: Static
    pub const LABEL: ChildId = ChildId(1200);
}
