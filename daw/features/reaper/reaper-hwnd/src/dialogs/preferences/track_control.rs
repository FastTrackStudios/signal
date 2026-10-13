//! Preferences -> Appearance -> Track Control Panels page child window IDs.

use crate::ChildId;

/// Preferences -> Appearance -> Track Control Panels page child window IDs.
pub struct TrackControlPrefs;

impl TrackControlPrefs {
    /// TCP layout dropdown - Class: ComboBox
    pub const TCP_LAYOUT: ChildId = ChildId(1000);
    /// TCP layout label - Class: Static
    pub const TCP_LAYOUT_LABEL: ChildId = ChildId(1001);
    /// MCP layout dropdown - Class: ComboBox
    pub const MCP_LAYOUT: ChildId = ChildId(1002);
    /// MCP layout label - Class: Static
    pub const MCP_LAYOUT_LABEL: ChildId = ChildId(1003);
    /// Show track numbers - Class: Button
    pub const SHOW_TRACK_NUMBERS: ChildId = ChildId(1004);
    /// Show record arm button - Class: Button
    pub const SHOW_RECORD_ARM: ChildId = ChildId(1005);
    /// Show input monitoring button - Class: Button
    pub const SHOW_INPUT_MONITOR: ChildId = ChildId(1006);
    /// Show track icon - Class: Button
    pub const SHOW_TRACK_ICON: ChildId = ChildId(1007);
    /// TCP label layout dropdown - Class: ComboBox
    pub const TCP_LABEL_LAYOUT: ChildId = ChildId(1008);
    /// TCP label layout label - Class: Static
    pub const TCP_LABEL_LAYOUT_LABEL: ChildId = ChildId(1009);
}
