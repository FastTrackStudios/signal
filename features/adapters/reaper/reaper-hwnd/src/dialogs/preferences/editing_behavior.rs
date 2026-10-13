//! Preferences -> Editing Behavior page child window IDs.

use crate::ChildId;

/// Preferences -> Editing Behavior page child window IDs.
pub struct EditingBehaviorPrefs;

impl EditingBehaviorPrefs {
    /// Move edit cursor when clicking in empty arrange area - Class: Button
    pub const MOVE_CURSOR_EMPTY_CLICK: ChildId = ChildId(1000);
    /// Link loop points to time selection - Class: Button
    pub const LINK_LOOP_TIME_SELECTION: ChildId = ChildId(1001);
    /// Move cursor to start of time selection on change - Class: Button
    pub const MOVE_CURSOR_TO_SELECTION: ChildId = ChildId(1002);
    /// Select all items in time selection when no items are selected - Class: Button
    pub const SELECT_ALL_IN_TIME_SELECTION: ChildId = ChildId(1003);
    /// Snap settings label - Class: Static
    pub const SNAP_LABEL: ChildId = ChildId(1004);
    /// Default snap mode dropdown - Class: ComboBox
    pub const DEFAULT_SNAP_MODE: ChildId = ChildId(1005);
    /// Snap to grid - Class: Button
    pub const SNAP_TO_GRID: ChildId = ChildId(1006);
    /// Relative snap - Class: Button
    pub const RELATIVE_SNAP: ChildId = ChildId(1007);
    /// Razor edit selection follows time selection - Class: Button
    pub const RAZOR_FOLLOWS_TIME: ChildId = ChildId(1008);
    /// Auto-crossfade on split - Class: Button
    pub const AUTO_CROSSFADE_SPLIT: ChildId = ChildId(1009);
    /// Trim behind items when splitting - Class: Button
    pub const TRIM_BEHIND_SPLIT: ChildId = ChildId(1010);
    /// Selection and cursor label - Class: Static
    pub const SELECTION_CURSOR_LABEL: ChildId = ChildId(1100);
    /// Editing behavior label - Class: Static
    pub const EDITING_LABEL: ChildId = ChildId(1101);
}
