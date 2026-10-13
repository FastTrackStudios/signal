//! Preferences -> Audio -> Recording page child window IDs.

use crate::ChildId;

/// Preferences -> Audio -> Recording page child window IDs.
pub struct RecordingPrefs;

impl RecordingPrefs {
    /// Build peaks during recording - Class: Button
    pub const BUILD_PEAKS: ChildId = ChildId(1004);
    /// Start new file every (minutes) inputbox - Class: Edit
    pub const NEW_FILE_INTERVAL: ChildId = ChildId(1005);
    /// Start new file label - Class: Static
    pub const NEW_FILE_LABEL: ChildId = ChildId(1006);
    /// Disk space check interval dropdown - Class: ComboBox
    pub const DISK_CHECK_INTERVAL: ChildId = ChildId(1007);
    /// Manual latency offset inputbox - Class: Edit
    pub const MANUAL_OFFSET: ChildId = ChildId(1008);
    /// Manual offset label - Class: Static
    pub const MANUAL_OFFSET_LABEL: ChildId = ChildId(1009);
    /// Recording filename pattern inputbox - Class: Edit
    pub const FILENAME_PATTERN: ChildId = ChildId(1010);
    /// Wildcards help button - Class: Button
    pub const WILDCARDS_HELP: ChildId = ChildId(1011);
    /// Show notification on recording start - Class: Button
    pub const SHOW_NOTIFICATION: ChildId = ChildId(1012);
    /// Record format dropdown - Class: ComboBox
    pub const RECORD_FORMAT: ChildId = ChildId(1013);
    /// Pre-roll (seconds) inputbox - Class: Edit
    pub const PRE_ROLL: ChildId = ChildId(1100);
    /// Pre-roll label - Class: Static
    pub const PRE_ROLL_LABEL: ChildId = ChildId(1101);
    /// Automatically monitor when recording armed - Class: Button
    pub const AUTO_MONITOR_ARMED: ChildId = ChildId(1200);
    /// Monitor input only when recording - Class: Button
    pub const MONITOR_ONLY_RECORDING: ChildId = ChildId(1201);
    /// Tape auto mode - Class: Button
    pub const TAPE_AUTO_MODE: ChildId = ChildId(1300);
    /// Record path label - Class: Static
    pub const RECORD_PATH_LABEL: ChildId = ChildId(1400);
    /// Use alternate record path inputbox - Class: Edit
    pub const ALTERNATE_RECORD_PATH: ChildId = ChildId(1401);
    /// Browse alternate path - Class: Button
    pub const BROWSE_ALTERNATE_PATH: ChildId = ChildId(1402);
    /// Recording settings label - Class: Static
    pub const RECORDING_SETTINGS_LABEL: ChildId = ChildId(1489);
}
