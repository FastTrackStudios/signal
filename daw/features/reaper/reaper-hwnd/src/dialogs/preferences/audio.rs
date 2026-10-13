//! Preferences -> Audio page child window IDs.

use crate::ChildId;

/// Preferences -> Audio page child window IDs.
pub struct AudioPrefs;

impl AudioPrefs {
    /// Close audio device when stopped and application is inactive - Class: Button
    pub const CLOSE_WHEN_STOPPED_INACTIVE: ChildId = ChildId(1042);
    /// Close audio device when stopped - Class: Button
    pub const CLOSE_WHEN_STOPPED: ChildId = ChildId(1043);
    /// Channel naming style dropdown - Class: ComboBox
    pub const CHANNEL_NAMING_STYLE: ChildId = ChildId(1044);
    /// Fade-in on start/stop length inputbox - Class: Edit
    pub const FADE_IN_ON_START_STOP: ChildId = ChildId(1045);
    /// Fade-out on start/stop length inputbox - Class: Edit
    pub const FADE_OUT_ON_START_STOP: ChildId = ChildId(1046);
    /// Fade-in/out label - Class: Static
    pub const FADE_LABEL: ChildId = ChildId(1047);
    /// Prebuffer media on playback start - Class: Button
    pub const PREBUFFER_ON_PLAYBACK: ChildId = ChildId(1048);
    /// Anticipative FX processing - Class: Button
    pub const ANTICIPATIVE_FX: ChildId = ChildId(1049);
    /// Audio thread priority dropdown - Class: ComboBox
    pub const THREAD_PRIORITY: ChildId = ChildId(1050);
    /// Realtime FX processing threads inputbox - Class: Edit
    pub const FX_PROCESSING_THREADS: ChildId = ChildId(1051);
    /// FX processing threads label - Class: Static
    pub const FX_THREADS_LABEL: ChildId = ChildId(1052);
    /// Auto-detect number of threads - Class: Button
    pub const AUTO_DETECT_THREADS: ChildId = ChildId(1053);
    /// Use all available CPU cores - Class: Button
    pub const USE_ALL_CORES: ChildId = ChildId(1054);
    /// Audio device label - Class: Static
    pub const AUDIO_DEVICE_LABEL: ChildId = ChildId(1100);
    /// Performance label - Class: Static
    pub const PERFORMANCE_LABEL: ChildId = ChildId(1101);
    /// Apply settings - Class: Button
    pub const APPLY_SETTINGS: ChildId = ChildId(1400);
}
