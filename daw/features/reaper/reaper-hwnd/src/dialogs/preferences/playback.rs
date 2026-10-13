//! Preferences -> Audio -> Playback page child window IDs.

use crate::ChildId;

/// Preferences -> Audio -> Playback page child window IDs.
pub struct PlaybackPrefs;

impl PlaybackPrefs {
    /// Scroll view to edit cursor on play - Class: Button
    pub const SCROLL_VIEW_ON_PLAY: ChildId = ChildId(1000);
    /// Smooth seeking checkbox - Class: Button
    pub const SMOOTH_SEEKING: ChildId = ChildId(1001);
    /// Reset CC on play/seek - Class: Button
    pub const RESET_CC_ON_PLAY: ChildId = ChildId(1002);
    /// Run FX on stop - Class: Button
    pub const FX_ON_STOP: ChildId = ChildId(1003);
    /// FX tail length inputbox - Class: Edit
    pub const FX_TAIL_LENGTH: ChildId = ChildId(1004);
    /// FX tail label - Class: Static
    pub const FX_TAIL_LABEL: ChildId = ChildId(1005);
    /// Scrub/jog behavior dropdown - Class: ComboBox
    pub const SCRUB_BEHAVIOR: ChildId = ChildId(1006);
    /// Autoscroll during playback dropdown - Class: ComboBox
    pub const AUTOSCROLL: ChildId = ChildId(1007);
    /// Cursor behavior label - Class: Static
    pub const CURSOR_BEHAVIOR_LABEL: ChildId = ChildId(1100);
    /// Playback settings label - Class: Static
    pub const PLAYBACK_SETTINGS_LABEL: ChildId = ChildId(1101);
    /// Stop playback at end of project - Class: Button
    pub const STOP_AT_END: ChildId = ChildId(1200);
    /// Seek playback when editing - Class: Button
    pub const SEEK_ON_EDIT: ChildId = ChildId(1201);
    /// Link edit cursor and play position - Class: Button
    pub const LINK_EDIT_PLAY_CURSOR: ChildId = ChildId(1202);
    /// Play position follows project timebase - Class: Button
    pub const PLAY_FOLLOWS_TIMEBASE: ChildId = ChildId(1300);
    /// Use all CPU cores for mixing - Class: Button
    pub const USE_ALL_CORES_MIXING: ChildId = ChildId(1661);
}
