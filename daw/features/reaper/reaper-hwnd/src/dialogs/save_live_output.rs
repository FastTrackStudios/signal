//! Save Live Output to Disk dialog and format sub-page child window IDs.

use crate::ChildId;

/// Save Live Output to Disk dialog child window IDs.
pub struct SaveLiveOutput;

impl SaveLiveOutput {
    /// Save button - Class: Button
    pub const SAVE: ChildId = ChildId(1);
    /// Cancel button - Class: Button
    pub const CANCEL: ChildId = ChildId(2);
    /// Output file path inputbox - Class: Edit
    pub const OUTPUT_PATH: ChildId = ChildId(1000);
    /// Browse output path - Class: Button
    pub const BROWSE: ChildId = ChildId(1001);
    /// Output format dropdown - Class: ComboBox
    pub const OUTPUT_FORMAT: ChildId = ChildId(1002);
    /// Output format settings HWND - Class: #32770
    pub const FORMAT_SETTINGS: ChildId = ChildId(58000);
    /// Sample rate dropdown - Class: ComboBox
    pub const SAMPLE_RATE: ChildId = ChildId(1003);
    /// Channels dropdown - Class: ComboBox
    pub const CHANNELS: ChildId = ChildId(1004);
    /// Source dropdown - Class: ComboBox
    pub const SOURCE: ChildId = ChildId(1005);
    /// Output path label - Class: Static
    pub const OUTPUT_PATH_LABEL: ChildId = ChildId(1100);
    /// Format label - Class: Static
    pub const FORMAT_LABEL: ChildId = ChildId(1101);
    /// Source label - Class: Static
    pub const SOURCE_LABEL: ChildId = ChildId(1102);
}

/// Save Live Output -> WAV format sub-page child IDs (children of format settings HWND).
pub struct SaveLiveWav;

impl SaveLiveWav {
    /// Bit depth dropdown - Class: ComboBox
    pub const BIT_DEPTH: ChildId = ChildId(1000);
    /// Write BWF chunk - Class: Button
    pub const WRITE_BWF_CHUNK: ChildId = ChildId(1042);
    /// Large files dropdown - Class: ComboBox
    pub const LARGE_FILES: ChildId = ChildId(1002);
    /// Bit depth label - Class: Static
    pub const BIT_DEPTH_LABEL: ChildId = ChildId(1185);
}

/// Save Live Output -> FLAC format sub-page child IDs.
pub struct SaveLiveFlac;

impl SaveLiveFlac {
    /// Encoding depth dropdown - Class: ComboBox
    pub const ENCODING_DEPTH: ChildId = ChildId(1002);
    /// Data compression dropdown - Class: ComboBox
    pub const DATA_COMPRESSION: ChildId = ChildId(1003);
    /// Encoding depth label - Class: Static
    pub const ENCODING_DEPTH_LABEL: ChildId = ChildId(65535);
}

/// Save Live Output -> MP3 format sub-page child IDs.
pub struct SaveLiveMp3;

impl SaveLiveMp3 {
    /// Bitrate dropdown - Class: ComboBox
    pub const BITRATE: ChildId = ChildId(1003);
    /// Quality dropdown - Class: ComboBox
    pub const QUALITY: ChildId = ChildId(1006);
    /// Mode dropdown - Class: ComboBox
    pub const MODE: ChildId = ChildId(1014);
    /// Encode dropdown - Class: ComboBox
    pub const ENCODE: ChildId = ChildId(1024);
    /// Quality label - Class: Static
    pub const QUALITY_LABEL: ChildId = ChildId(1017);
    /// Bitrate label - Class: Static
    pub const BITRATE_LABEL: ChildId = ChildId(1018);
}

/// Save Live Output -> OGG Vorbis format sub-page child IDs.
pub struct SaveLiveOggVorbis;

impl SaveLiveOggVorbis {
    /// VBR quality inputbox - Class: Edit
    pub const VBR_QUALITY: ChildId = ChildId(1001);
    /// VBR quality radio - Class: Button
    pub const VBR_QUALITY_RADIO: ChildId = ChildId(1002);
    /// CBR radio - Class: Button
    pub const CBR_RADIO: ChildId = ChildId(1003);
    /// CBR bitrate inputbox - Class: Edit
    pub const CBR_BITRATE: ChildId = ChildId(1004);
    /// Quality range label - Class: Static
    pub const QUALITY_RANGE_LABEL: ChildId = ChildId(65535);
}
