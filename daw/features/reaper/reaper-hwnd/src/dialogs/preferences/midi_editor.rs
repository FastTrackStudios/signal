//! Preferences -> MIDI Editor page child window IDs.

use crate::ChildId;

/// Preferences -> MIDI Editor page child window IDs.
pub struct MidiEditorPrefs;

impl MidiEditorPrefs {
    /// Open MIDI editor behavior dropdown - Class: ComboBox
    pub const OPEN_BEHAVIOR: ChildId = ChildId(1000);
    /// Open behavior label - Class: Static
    pub const OPEN_BEHAVIOR_LABEL: ChildId = ChildId(1001);
    /// Events per quarter note inputbox - Class: Edit
    pub const EVENTS_PER_QN: ChildId = ChildId(1002);
    /// Events label - Class: Static
    pub const EVENTS_LABEL: ChildId = ChildId(1003);
    /// Color map dropdown - Class: ComboBox
    pub const COLOR_MAP: ChildId = ChildId(1004);
    /// Color map label - Class: Static
    pub const COLOR_MAP_LABEL: ChildId = ChildId(1005);
    /// Secondary MIDI item display dropdown - Class: ComboBox
    pub const SECONDARY_ITEMS: ChildId = ChildId(1006);
    /// Secondary items label - Class: Static
    pub const SECONDARY_ITEMS_LABEL: ChildId = ChildId(1007);
    /// Default note length dropdown - Class: ComboBox
    pub const DEFAULT_NOTE_LENGTH: ChildId = ChildId(1008);
    /// Note length label - Class: Static
    pub const NOTE_LENGTH_LABEL: ChildId = ChildId(1009);
    /// Default note velocity inputbox - Class: Edit
    pub const DEFAULT_VELOCITY: ChildId = ChildId(1010);
    /// Velocity label - Class: Static
    pub const VELOCITY_LABEL: ChildId = ChildId(1011);
    /// Default channel dropdown - Class: ComboBox
    pub const DEFAULT_CHANNEL: ChildId = ChildId(1012);
    /// Channel label - Class: Static
    pub const CHANNEL_LABEL: ChildId = ChildId(1013);
    /// Show MIDI preview notes - Class: Button
    pub const SHOW_PREVIEW_NOTES: ChildId = ChildId(1014);
    /// Close editor on project switch - Class: Button
    pub const CLOSE_ON_PROJECT_SWITCH: ChildId = ChildId(1015);
    /// One editor per project - Class: Button
    pub const ONE_EDITOR_PER_PROJECT: ChildId = ChildId(1016);
    /// Use inline editor - Class: Button
    pub const USE_INLINE_EDITOR: ChildId = ChildId(1017);
    /// Draw notes in MIDI editor - Class: Button
    pub const DRAW_NOTES: ChildId = ChildId(1018);
    /// Default piano roll timebase dropdown - Class: ComboBox
    pub const PIANO_ROLL_TIMEBASE: ChildId = ChildId(1100);
    /// Timebase label - Class: Static
    pub const TIMEBASE_LABEL: ChildId = ChildId(1101);
    /// MIDI editor settings label - Class: Static
    pub const MIDI_EDITOR_LABEL: ChildId = ChildId(1701);
}
