//! Preferences -> Media Item Defaults page child window IDs.

use crate::ChildId;

/// Preferences -> Media Item Defaults page child window IDs.
pub struct MediaItemDefaultsPrefs;

impl MediaItemDefaultsPrefs {
    /// Create automatic fade-in length inputbox - Class: Edit
    pub const AUTO_FADE_IN_LENGTH: ChildId = ChildId(1000);
    /// Create automatic fade-out length inputbox - Class: Edit
    pub const AUTO_FADE_OUT_LENGTH: ChildId = ChildId(1001);
    /// Overlap/crossfade behavior dropdown - Class: ComboBox
    pub const CROSSFADE_BEHAVIOR: ChildId = ChildId(1002);
    /// Default fade-in shape dropdown - Class: ComboBox
    pub const DEFAULT_FADE_IN_SHAPE: ChildId = ChildId(1003);
    /// Default fade-out shape dropdown - Class: ComboBox
    pub const DEFAULT_FADE_OUT_SHAPE: ChildId = ChildId(1004);
    /// Default crossfade shape dropdown - Class: ComboBox
    pub const DEFAULT_CROSSFADE_SHAPE: ChildId = ChildId(1005);
    /// Fade-in label - Class: Static
    pub const FADE_IN_LABEL: ChildId = ChildId(1006);
    /// Fade-out label - Class: Static
    pub const FADE_OUT_LABEL: ChildId = ChildId(1007);
    /// Loop source by default - Class: Button
    pub const LOOP_SOURCE_BY_DEFAULT: ChildId = ChildId(1008);
    /// Enable looping on split - Class: Button
    pub const LOOP_ON_SPLIT: ChildId = ChildId(1009);
}
