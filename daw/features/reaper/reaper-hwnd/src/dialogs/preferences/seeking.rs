//! Preferences -> Audio -> Seeking page child window IDs.

use crate::ChildId;

/// Preferences -> Audio -> Seeking page child window IDs.
pub struct SeekingPrefs;

impl SeekingPrefs {
    /// Playback position follows project timebase - Class: Button
    pub const PLAY_FOLLOWS_TIMEBASE: ChildId = ChildId(1000);
    /// Scrub/jog behavior dropdown - Class: ComboBox
    pub const SCRUB_BEHAVIOR: ChildId = ChildId(1001);
    /// Seek mode dropdown - Class: ComboBox
    pub const SEEK_MODE: ChildId = ChildId(1002);
    /// Seek behavior label - Class: Static
    pub const SEEK_LABEL: ChildId = ChildId(1003);
    /// Seek playback on click in ruler - Class: Button
    pub const SEEK_ON_RULER_CLICK: ChildId = ChildId(1004);
    /// Move edit cursor on click in arrange - Class: Button
    pub const MOVE_CURSOR_ON_ARRANGE_CLICK: ChildId = ChildId(1005);
}
