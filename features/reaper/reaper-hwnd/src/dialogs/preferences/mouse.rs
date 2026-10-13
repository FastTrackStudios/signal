//! Preferences -> Mouse page child window IDs.

use crate::ChildId;

/// Preferences -> Mouse page child window IDs.
pub struct MousePrefs;

impl MousePrefs {
    /// Mouse scroll speed inputbox - Class: Edit
    pub const SCROLL_SPEED: ChildId = ChildId(1000);
    /// Scroll speed label - Class: Static
    pub const SCROLL_SPEED_LABEL: ChildId = ChildId(1001);
    /// Double-click behavior dropdown - Class: ComboBox
    pub const DOUBLE_CLICK_BEHAVIOR: ChildId = ChildId(1002);
    /// Double-click label - Class: Static
    pub const DOUBLE_CLICK_LABEL: ChildId = ChildId(1003);
    /// Mouse click behavior dropdown - Class: ComboBox
    pub const CLICK_BEHAVIOR: ChildId = ChildId(1004);
    /// Click label - Class: Static
    pub const CLICK_LABEL: ChildId = ChildId(1005);
    /// Use mousewheel to zoom - Class: Button
    pub const MOUSEWHEEL_ZOOM: ChildId = ChildId(1006);
    /// Horizontal zoom with Ctrl+mousewheel - Class: Button
    pub const CTRL_MOUSEWHEEL_HZOOM: ChildId = ChildId(1007);
    /// Allow drag to reorder tracks - Class: Button
    pub const DRAG_REORDER_TRACKS: ChildId = ChildId(1008);
    /// Drag-drop import copies files - Class: Button
    pub const DRAG_DROP_COPIES: ChildId = ChildId(1009);
    /// Mouse settings label - Class: Static
    pub const MOUSE_LABEL: ChildId = ChildId(1100);
}
