//! Preferences -> Video page child window IDs.

use crate::ChildId;

/// Preferences -> Video page child window IDs.
pub struct VideoPrefs;

impl VideoPrefs {
    /// Video display mode dropdown - Class: ComboBox
    pub const DISPLAY_MODE: ChildId = ChildId(1000);
    /// Display mode label - Class: Static
    pub const DISPLAY_MODE_LABEL: ChildId = ChildId(1001);
    /// Video decoder dropdown - Class: ComboBox
    pub const DECODER: ChildId = ChildId(1002);
    /// Decoder label - Class: Static
    pub const DECODER_LABEL: ChildId = ChildId(1003);
    /// Video framerate inputbox - Class: Edit
    pub const FRAMERATE: ChildId = ChildId(1004);
    /// Framerate label - Class: Static
    pub const FRAMERATE_LABEL: ChildId = ChildId(1005);
    /// Video colorspace dropdown - Class: ComboBox
    pub const COLORSPACE: ChildId = ChildId(1006);
    /// Colorspace label - Class: Static
    pub const COLORSPACE_LABEL: ChildId = ChildId(1007);
    /// Resize quality dropdown - Class: ComboBox
    pub const RESIZE_QUALITY: ChildId = ChildId(1008);
    /// Resize quality label - Class: Static
    pub const RESIZE_QUALITY_LABEL: ChildId = ChildId(1009);
    /// Video settings label - Class: Static
    pub const VIDEO_LABEL: ChildId = ChildId(1100);
}
