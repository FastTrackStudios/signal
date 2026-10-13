//! Project Settings dialog child window IDs.

use crate::ChildId;

/// Project Settings dialog child window IDs.
pub struct ProjectSettings;

impl ProjectSettings {
    /// OK - Class: Button
    pub const OK: ChildId = ChildId(1);
    /// Cancel - Class: Button
    pub const CANCEL: ChildId = ChildId(2);
    /// Settings tabs - Class: SysTabControl32
    pub const TABS: ChildId = ChildId(1000);
    /// Project sample rate inputbox - Class: Edit
    pub const SAMPLE_RATE: ChildId = ChildId(1001);
    /// Sample rate label - Class: Static
    pub const SAMPLE_RATE_LABEL: ChildId = ChildId(1002);
    /// Project BPM inputbox - Class: Edit
    pub const BPM: ChildId = ChildId(1003);
    /// BPM label - Class: Static
    pub const BPM_LABEL: ChildId = ChildId(1004);
    /// Time signature numerator inputbox - Class: Edit
    pub const TIME_SIG_NUM: ChildId = ChildId(1005);
    /// Time signature denominator inputbox - Class: Edit
    pub const TIME_SIG_DENOM: ChildId = ChildId(1006);
    /// Time signature label - Class: Static
    pub const TIME_SIG_LABEL: ChildId = ChildId(1007);
    /// Project start time inputbox - Class: Edit
    pub const START_TIME: ChildId = ChildId(1008);
    /// Start time label - Class: Static
    pub const START_TIME_LABEL: ChildId = ChildId(1009);
    /// Project start measure inputbox - Class: Edit
    pub const START_MEASURE: ChildId = ChildId(1010);
    /// Start measure label - Class: Static
    pub const START_MEASURE_LABEL: ChildId = ChildId(1011);
    /// Timebase for items/envelopes/markers dropdown - Class: ComboBox
    pub const TIMEBASE: ChildId = ChildId(1012);
    /// Timebase label - Class: Static
    pub const TIMEBASE_LABEL: ChildId = ChildId(1013);
    /// Pitch shift mode dropdown - Class: ComboBox
    pub const PITCH_SHIFT_MODE: ChildId = ChildId(1014);
    /// Pitch shift label - Class: Static
    pub const PITCH_SHIFT_LABEL: ChildId = ChildId(1015);
    /// Project pan law dropdown - Class: ComboBox
    pub const PAN_LAW: ChildId = ChildId(1016);
    /// Pan law label - Class: Static
    pub const PAN_LAW_LABEL: ChildId = ChildId(1017);
    /// Project path inputbox - Class: Edit
    pub const PROJECT_PATH: ChildId = ChildId(1018);
    /// Project path label - Class: Static
    pub const PROJECT_PATH_LABEL: ChildId = ChildId(1019);
    /// Browse project path - Class: Button
    pub const BROWSE_PROJECT_PATH: ChildId = ChildId(1020);
    /// Limit project length - Class: Button
    pub const LIMIT_PROJECT_LENGTH: ChildId = ChildId(1021);
    /// Project length inputbox - Class: Edit
    pub const PROJECT_LENGTH: ChildId = ChildId(1022);
    /// Length label - Class: Static
    pub const LENGTH_LABEL: ChildId = ChildId(1023);
    /// Project framerate dropdown - Class: ComboBox
    pub const FRAMERATE: ChildId = ChildId(1024);
    /// Framerate label - Class: Static
    pub const FRAMERATE_LABEL: ChildId = ChildId(1025);
    /// Notes inputbox - Class: Edit
    pub const NOTES: ChildId = ChildId(1100);
    /// Notes label - Class: Static
    pub const NOTES_LABEL: ChildId = ChildId(1101);
    /// Apply - Class: Button
    pub const APPLY: ChildId = ChildId(1144);
    /// Save as default project settings - Class: Button
    pub const SAVE_AS_DEFAULT: ChildId = ChildId(1200);
}
