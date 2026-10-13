//! Preferences -> MIDI Devices page child window IDs.

use crate::ChildId;

/// Preferences -> MIDI Devices page child window IDs.
pub struct MidiDevicesPrefs;

impl MidiDevicesPrefs {
    /// Enable MIDI playback on stop - Class: Button
    pub const PLAY_ON_STOP: ChildId = ChildId(1042);
    /// Reset MIDI on stop - Class: Button
    pub const RESET_ON_STOP: ChildId = ChildId(1043);
    /// Joystick as MIDI - Class: Button
    pub const JOYSTICK_AS_MIDI: ChildId = ChildId(1044);
    /// MIDI input devices list - Class: SysListView32
    pub const INPUT_DEVICES_LIST: ChildId = ChildId(1100);
    /// MIDI output devices list - Class: SysListView32
    pub const OUTPUT_DEVICES_LIST: ChildId = ChildId(1101);
    /// Enable input device - Class: Button
    pub const ENABLE_INPUT: ChildId = ChildId(1200);
    /// Enable output device - Class: Button
    pub const ENABLE_OUTPUT: ChildId = ChildId(1201);
    /// Reset input - Class: Button
    pub const RESET_INPUT: ChildId = ChildId(1300);
    /// Reset output - Class: Button
    pub const RESET_OUTPUT: ChildId = ChildId(1301);
    /// Hardware input label - Class: Static
    pub const HW_INPUT_LABEL: ChildId = ChildId(1400);
    /// Hardware output label - Class: Static
    pub const HW_OUTPUT_LABEL: ChildId = ChildId(1401);
    /// MIDI devices settings label - Class: Static
    pub const MIDI_DEVICES_LABEL: ChildId = ChildId(1453);
}
