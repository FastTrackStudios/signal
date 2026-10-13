//! Preferences -> Media page child window IDs.

use crate::ChildId;

/// Preferences -> Media page child window IDs.
pub struct MediaPrefs;

impl MediaPrefs {
    /// Default media format dropdown - Class: ComboBox
    pub const DEFAULT_FORMAT: ChildId = ChildId(1000);
    /// Format label - Class: Static
    pub const FORMAT_LABEL: ChildId = ChildId(1001);
    /// Import media at project tempo - Class: Button
    pub const IMPORT_AT_PROJECT_TEMPO: ChildId = ChildId(1002);
    /// Video import behavior dropdown - Class: ComboBox
    pub const VIDEO_IMPORT_BEHAVIOR: ChildId = ChildId(1003);
    /// Video label - Class: Static
    pub const VIDEO_LABEL: ChildId = ChildId(1004);
    /// Audio media label - Class: Static
    pub const AUDIO_LABEL: ChildId = ChildId(1005);
    /// Media settings label - Class: Static
    pub const MEDIA_LABEL: ChildId = ChildId(1006);
    /// Video decoder priority dropdown - Class: ComboBox
    pub const VIDEO_DECODER_PRIORITY: ChildId = ChildId(1007);
}
