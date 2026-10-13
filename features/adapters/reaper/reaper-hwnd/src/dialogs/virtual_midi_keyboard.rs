//! Virtual MIDI Keyboard dialog child window IDs.

use crate::ChildId;

/// Virtual MIDI Keyboard dialog child window IDs.
pub struct VirtualMidiKeyboard;

impl VirtualMidiKeyboard {
    /// Keyboard area - Class: REAPERvkbctl
    pub const KEYBOARD: ChildId = ChildId(1000);
    /// Octave inputbox - Class: Edit
    pub const OCTAVE: ChildId = ChildId(1001);
    /// Octave label - Class: Static
    pub const OCTAVE_LABEL: ChildId = ChildId(1002);
    /// Velocity inputbox - Class: Edit
    pub const VELOCITY: ChildId = ChildId(1003);
    /// Velocity label - Class: Static
    pub const VELOCITY_LABEL: ChildId = ChildId(1004);
    /// Channel dropdown - Class: ComboBox
    pub const CHANNEL: ChildId = ChildId(1005);
    /// Channel label - Class: Static
    pub const CHANNEL_LABEL: ChildId = ChildId(1006);
    /// Octave up button - Class: Button
    pub const OCTAVE_UP: ChildId = ChildId(1007);
    /// Octave down button - Class: Button
    pub const OCTAVE_DOWN: ChildId = ChildId(1008);
    /// Velocity up button - Class: Button
    pub const VELOCITY_UP: ChildId = ChildId(1009);
    /// Velocity down button - Class: Button
    pub const VELOCITY_DOWN: ChildId = ChildId(1010);
    /// Close button - Class: Button
    pub const CLOSE: ChildId = ChildId(2);
    /// Send to dropdown - Class: ComboBox
    pub const SEND_TO: ChildId = ChildId(1011);
    /// Send to label - Class: Static
    pub const SEND_TO_LABEL: ChildId = ChildId(1012);
}
