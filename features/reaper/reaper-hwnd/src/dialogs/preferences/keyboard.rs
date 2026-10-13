//! Preferences -> Keyboard/Multitouch page child window IDs.

use crate::ChildId;

/// Preferences -> Keyboard/Multitouch page child window IDs.
pub struct KeyboardPrefs;

impl KeyboardPrefs {
    /// Allow keyboard commands when editing text - Class: Button
    pub const ALLOW_KB_COMMANDS_IN_TEXT: ChildId = ChildId(1000);
    /// Enable multitouch support - Class: Button
    pub const ENABLE_MULTITOUCH: ChildId = ChildId(1001);
    /// Touchscreen emulates mouse - Class: Button
    pub const TOUCHSCREEN_EMULATES_MOUSE: ChildId = ChildId(1002);
    /// Language-specific keyboard shortcuts - Class: Button
    pub const LANGUAGE_SPECIFIC_SHORTCUTS: ChildId = ChildId(1003);
    /// Allow space in record mode - Class: Button
    pub const ALLOW_SPACE_IN_RECORD: ChildId = ChildId(1004);
}
