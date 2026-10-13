//! Preferences -> Audio -> Loop Recording page child window IDs.

use crate::ChildId;

/// Preferences -> Audio -> Loop Recording page child window IDs.
pub struct LoopRecordingPrefs;

impl LoopRecordingPrefs {
    /// Loop recording mode dropdown - Class: ComboBox
    pub const MODE: ChildId = ChildId(1000);
    /// Mode label - Class: Static
    pub const MODE_LABEL: ChildId = ChildId(1001);
    /// Discard incomplete first or last takes - Class: Button
    pub const DISCARD_INCOMPLETE: ChildId = ChildId(1002);
    /// When recording looped, add recorded media to project dropdown - Class: ComboBox
    pub const ADD_MEDIA_MODE: ChildId = ChildId(1003);
    /// Auto-crossfade loop takes - Class: Button
    pub const AUTO_CROSSFADE: ChildId = ChildId(1004);
}
