//! Render to File dialog and format sub-page child window IDs.

use crate::ChildId;

/// Render to File dialog child window IDs (excluding output format settings).
pub struct RenderDialog;

impl RenderDialog {
    /// Render button - Class: Button
    pub const RENDER: ChildId = ChildId(1);
    /// Cancel - Class: Button
    pub const CANCEL: ChildId = ChildId(2);
    /// Resample mode dropdown - Class: ComboBox
    pub const RESAMPLE_MODE: ChildId = ChildId(1000);
    /// Render speed dropdown - Class: ComboBox
    pub const RENDER_SPEED: ChildId = ChildId(1001);
    /// Time range dropdown - Class: ComboBox
    pub const TIME_RANGE: ChildId = ChildId(1002);
    /// Source dropdown - Class: ComboBox
    pub const SOURCE: ChildId = ChildId(1003);
    /// Directory inputbox - Class: Edit
    pub const DIRECTORY: ChildId = ChildId(1007);
    /// Render to inputbox - Class: Edit
    pub const RENDER_TO: ChildId = ChildId(1008);
    /// Start inputbox - Class: Edit
    pub const START: ChildId = ChildId(1009);
    /// End inputbox - Class: Edit
    pub const END: ChildId = ChildId(1010);
    /// Length inputbox - Class: Edit
    pub const LENGTH: ChildId = ChildId(1011);
    /// Tail inputbox - Class: Edit
    pub const TAIL: ChildId = ChildId(1012);
    /// Filename inputbox - Class: Edit
    pub const FILENAME: ChildId = ChildId(1013);
    /// Browse button - Class: Button
    pub const BROWSE: ChildId = ChildId(1030);
    /// Samplerate dropdown - Class: ComboBox
    pub const SAMPLERATE: ChildId = ChildId(1041);
    /// Silently increment filenames - Class: Button
    pub const SILENTLY_INCREMENT_FILENAMES: ChildId = ChildId(1042);
    /// Start label - Class: Static
    pub const START_LABEL: ChildId = ChildId(1043);
    /// End label - Class: Static
    pub const END_LABEL: ChildId = ChildId(1044);
    /// Length label - Class: Static
    pub const LENGTH_LABEL: ChildId = ChildId(1045);
    /// Tail checkbox - Class: Button
    pub const TAIL_CHECKBOX: ChildId = ChildId(1046);
    /// Region Manager button - Class: Button
    pub const REGION_MANAGER: ChildId = ChildId(1056);
    /// Presets button - Class: Button
    pub const PRESETS: ChildId = ChildId(1057);
    /// x file button - Class: Button
    pub const X_FILE: ChildId = ChildId(1058);
    /// Multichannel tracks to multichannel files - Class: Button
    pub const MULTICHANNEL_TO_MULTICHANNEL: ChildId = ChildId(1059);
    /// Save copy of project to outfile.wav.RPP - Class: Button
    pub const SAVE_PROJECT_COPY: ChildId = ChildId(1060);
    /// Noise shaping - Class: Button
    pub const NOISE_SHAPING: ChildId = ChildId(1061);
    /// Use project sample rate for mixing - Class: Button
    pub const USE_PROJECT_SAMPLE_RATE: ChildId = ChildId(1062);
    /// Tracks with only mono media to mono files - Class: Button
    pub const MONO_TRACKS_TO_MONO: ChildId = ChildId(1063);
    /// Region Matrix button - Class: Button
    pub const REGION_MATRIX: ChildId = ChildId(1064);
    /// Wildcards button - Class: Button
    pub const WILDCARDS: ChildId = ChildId(1065);
    /// Static label - Class: Static
    pub const STATIC_LABEL: ChildId = ChildId(1115);
    /// Output format dropdown - Class: ComboBox
    pub const OUTPUT_FORMAT: ChildId = ChildId(1116);
    /// Output formats tabs - Class: SysTabControl32
    pub const OUTPUT_FORMAT_TABS: ChildId = ChildId(1118);
    /// Do not render likely silent files - Class: Button
    pub const DO_NOT_RENDER_SILENT: ChildId = ChildId(1155);
    /// Add rendered items to new tracks - Class: Button
    pub const ADD_TO_NEW_TRACKS: ChildId = ChildId(1154);
    /// Stretch markers/transient guides - Class: Button
    pub const STRETCH_MARKERS: ChildId = ChildId(1173);
    /// Metadata button - Class: Button
    pub const METADATA: ChildId = ChildId(1178);
    /// Take markers - Class: Button
    pub const TAKE_MARKERS: ChildId = ChildId(1179);
    /// ms label - Class: Static
    pub const MS_LABEL: ChildId = ChildId(1240);
    /// Master mix label - Class: Static
    pub const MASTER_MIX_LABEL: ChildId = ChildId(1241);
    /// Dither - Class: Button
    pub const DITHER: ChildId = ChildId(1307);
    /// Channels dropdown - Class: ComboBox
    pub const CHANNELS: ChildId = ChildId(1397);
    /// Add to render queue - Class: Button
    pub const ADD_TO_RENDER_QUEUE: ChildId = ChildId(1447);
    /// Open render queue button - Class: Button
    pub const OPEN_RENDER_QUEUE: ChildId = ChildId(1448);
    /// Save changes and close - Class: Button
    pub const SAVE_AND_CLOSE: ChildId = ChildId(1449);
    /// Region/media items count - Class: Edit
    pub const REGION_MEDIA_ITEMS_COUNT: ChildId = ChildId(1692);
    /// Delay queued render - Class: Button
    pub const DELAY_QUEUED_RENDER: ChildId = ChildId(1808);
    /// Output format settings HWND - Class: #32770
    pub const OUTPUT_FORMAT_SETTINGS: ChildId = ChildId(58000);
    /// Secondary output format settings HWND - Class: #32770
    pub const SECONDARY_OUTPUT_FORMAT_SETTINGS: ChildId = ChildId(58002);
}

/// Render to File -> WAV format sub-page child IDs (children of ID 58000).
pub struct RenderWav;

impl RenderWav {
    /// Bit depth dropdown - Class: ComboBox
    pub const BIT_DEPTH: ChildId = ChildId(1000);
    /// Markers/regions dropdown - Class: ComboBox
    pub const MARKERS_REGIONS: ChildId = ChildId(1001);
    /// Large files dropdown - Class: ComboBox
    pub const LARGE_FILES: ChildId = ChildId(1002);
    /// Write BWF chunk - Class: Button
    pub const WRITE_BWF_CHUNK: ChildId = ChildId(1042);
    /// Include project filename in BWF - Class: Button
    pub const INCLUDE_PROJECT_FILENAME_IN_BWF: ChildId = ChildId(1044);
    /// Embed tempo - Class: Button
    pub const EMBED_TEMPO: ChildId = ChildId(1059);
    /// WAV bit depth label - Class: Static
    pub const BIT_DEPTH_LABEL: ChildId = ChildId(1185);
    /// Large files label - Class: Static
    pub const LARGE_FILES_LABEL: ChildId = ChildId(1816);
}

/// Render to File -> AIFF format sub-page child IDs.
pub struct RenderAiff;

impl RenderAiff {
    /// Bit depth dropdown - Class: ComboBox
    pub const BIT_DEPTH: ChildId = ChildId(1000);
    /// AIFF bit depth label - Class: Static
    pub const BIT_DEPTH_LABEL: ChildId = ChildId(1185);
}

/// Render to File -> AudioCD Image format sub-page child IDs.
pub struct RenderAudioCd;

impl RenderAudioCd {
    /// Extra lead-in silence inputbox - Class: Edit
    pub const EXTRA_LEAD_IN_SILENCE: ChildId = ChildId(1001);
    /// Lead-in silence for tracks - Class: Edit
    pub const LEAD_IN_SILENCE_FOR_TRACKS: ChildId = ChildId(1002);
    /// Markers define new tracks dropdown - Class: ComboBox
    pub const MARKERS_DEFINE_NEW_TRACKS: ChildId = ChildId(1014);
    /// Burn CD image after render - Class: Button
    pub const BURN_CD_IMAGE: ChildId = ChildId(1016);
    /// Only markers starting with # - Class: Button
    pub const ONLY_HASH_MARKERS: ChildId = ChildId(1017);
    /// Lead-in silence label - Class: Static
    pub const LEAD_IN_SILENCE_LABEL: ChildId = ChildId(65535);
}

/// Render to File -> DDP format sub-page child IDs.
pub struct RenderDdp;

impl RenderDdp {
    /// Help button - Class: Button
    pub const HELP: ChildId = ChildId(1000);
    /// DDP compatibility note - Class: Static
    pub const COMPATIBILITY_NOTE: ChildId = ChildId(65535);
}

/// Render to File -> FLAC format sub-page child IDs.
pub struct RenderFlac;

impl RenderFlac {
    /// Encoding depth dropdown - Class: ComboBox
    pub const ENCODING_DEPTH: ChildId = ChildId(1002);
    /// Data compression dropdown - Class: ComboBox
    pub const DATA_COMPRESSION: ChildId = ChildId(1003);
    /// FLAC encoding depth label - Class: Static
    pub const ENCODING_DEPTH_LABEL: ChildId = ChildId(65535);
}

/// Render to File -> MP3 format sub-page child IDs.
pub struct RenderMp3;

impl RenderMp3 {
    /// Bitrate dropdown - Class: ComboBox
    pub const BITRATE: ChildId = ChildId(1003);
    /// Quality dropdown - Class: ComboBox
    pub const QUALITY: ChildId = ChildId(1006);
    /// CBR mode dropdown - Class: ComboBox
    pub const CBR_MODE: ChildId = ChildId(1014);
    /// Quality label - Class: Static
    pub const QUALITY_LABEL: ChildId = ChildId(1017);
    /// Bitrate label - Class: Static
    pub const BITRATE_LABEL: ChildId = ChildId(1018);
    /// Approximate label - Class: Static
    pub const APPROXIMATE_LABEL_1: ChildId = ChildId(1019);
    /// Approximate label - Class: Static
    pub const APPROXIMATE_LABEL_2: ChildId = ChildId(1020);
    /// Encode dropdown - Class: ComboBox
    pub const ENCODE: ChildId = ChildId(1024);
    /// LAME version label - Class: Static
    pub const LAME_VERSION_LABEL: ChildId = ChildId(1025);
    /// No joint stereo - Class: Button
    pub const NO_JOINT_STEREO: ChildId = ChildId(1039);
    /// Write ReplayGain tag - Class: Button
    pub const WRITE_REPLAY_GAIN: ChildId = ChildId(1040);
    /// Mode label - Class: Static
    pub const MODE_LABEL: ChildId = ChildId(65535);
}

/// Render to File -> OGG Vorbis format sub-page child IDs.
pub struct RenderOggVorbis;

impl RenderOggVorbis {
    /// VBR quality inputbox - Class: Edit
    pub const VBR_QUALITY: ChildId = ChildId(1001);
    /// VBR quality radio - Class: Button
    pub const VBR_QUALITY_RADIO: ChildId = ChildId(1002);
    /// CBR radio - Class: Button
    pub const CBR_RADIO: ChildId = ChildId(1003);
    /// CBR bitrate inputbox - Class: Edit
    pub const CBR_BITRATE: ChildId = ChildId(1004);
    /// ABR radio - Class: Button
    pub const ABR_RADIO: ChildId = ChildId(1005);
    /// ABR bitrate inputbox - Class: Edit
    pub const ABR_BITRATE: ChildId = ChildId(1006);
    /// ABR min bitrate inputbox - Class: Edit
    pub const ABR_MIN_BITRATE: ChildId = ChildId(1007);
    /// ABR max bitrate inputbox - Class: Edit
    pub const ABR_MAX_BITRATE: ChildId = ChildId(1008);
    /// Quality range label - Class: Static
    pub const QUALITY_RANGE_LABEL: ChildId = ChildId(65535);
}

/// Render to File -> Video (ffmpeg/libav) AVI format sub-page child IDs.
pub struct RenderVideoFfmpeg;

impl RenderVideoFfmpeg {
    /// Video bitrate inputbox - Class: Edit
    pub const VIDEO_BITRATE: ChildId = ChildId(1002);
    /// Audio bitrate inputbox - Class: Edit
    pub const AUDIO_BITRATE: ChildId = ChildId(1003);
    /// Width inputbox - Class: Edit
    pub const WIDTH: ChildId = ChildId(1004);
    /// Height inputbox - Class: Edit
    pub const HEIGHT: ChildId = ChildId(1005);
    /// FPS inputbox - Class: Edit
    pub const FPS: ChildId = ChildId(1006);
    /// Video codec dropdown - Class: ComboBox
    pub const VIDEO_CODEC: ChildId = ChildId(1007);
    /// Audio codec dropdown - Class: ComboBox
    pub const AUDIO_CODEC: ChildId = ChildId(1008);
    /// Settings button - Class: Button
    pub const SETTINGS: ChildId = ChildId(1009);
    /// Format dropdown - Class: ComboBox
    pub const FORMAT: ChildId = ChildId(1014);
    /// MJPEG quality inputbox - Class: Edit
    pub const MJPEG_QUALITY: ChildId = ChildId(1018);
    /// kbps or % label - Class: Static
    pub const KBPS_OR_PERCENT_LABEL: ChildId = ChildId(1019);
    /// kbps label - Class: Static
    pub const KBPS_LABEL: ChildId = ChildId(1020);
}

/// Render to File -> Video (GIF) format sub-page child IDs.
pub struct RenderVideoGif;

impl RenderVideoGif {
    /// Preserve aspect ratio - Class: Button
    pub const PRESERVE_ASPECT_RATIO: ChildId = ChildId(1003);
    /// Width inputbox - Class: Edit
    pub const WIDTH: ChildId = ChildId(1004);
    /// Height inputbox - Class: Edit
    pub const HEIGHT: ChildId = ChildId(1005);
    /// FPS inputbox - Class: Edit
    pub const FPS: ChildId = ChildId(1006);
    /// Ignore low color bits inputbox - Class: Edit
    pub const IGNORE_LOW_COLOR_BITS: ChildId = ChildId(1007);
    /// Encode transparency - Class: Button
    pub const ENCODE_TRANSPARENCY: ChildId = ChildId(1010);
}

/// Render to File -> Video (LCF) format sub-page child IDs.
pub struct RenderVideoLcf;

impl RenderVideoLcf {
    /// Preserve aspect ratio - Class: Button
    pub const PRESERVE_ASPECT_RATIO: ChildId = ChildId(1003);
    /// Width inputbox - Class: Edit
    pub const WIDTH: ChildId = ChildId(1004);
    /// Height inputbox - Class: Edit
    pub const HEIGHT: ChildId = ChildId(1005);
    /// FPS inputbox - Class: Edit
    pub const FPS: ChildId = ChildId(1006);
    /// LCF options tweak inputbox - Class: Edit
    pub const LCF_OPTIONS_TWEAK: ChildId = ChildId(1012);
}

/// Render to File -> WavPack format sub-page child IDs.
pub struct RenderWavPack;

impl RenderWavPack {
    /// Mode dropdown - Class: ComboBox
    pub const MODE: ChildId = ChildId(1000);
    /// Bit depth dropdown - Class: ComboBox
    pub const BIT_DEPTH: ChildId = ChildId(1002);
    /// Write BWF chunk - Class: Button
    pub const WRITE_BWF_CHUNK: ChildId = ChildId(1042);
    /// Include project filename in BWF - Class: Button
    pub const INCLUDE_PROJECT_FILENAME_IN_BWF: ChildId = ChildId(1044);
    /// Only markers starting with # - Class: Button
    pub const ONLY_HASH_MARKERS: ChildId = ChildId(1045);
    /// Write markers as cues - Class: Button
    pub const WRITE_MARKERS_AS_CUES: ChildId = ChildId(1057);
    /// Bit depth label - Class: Static
    pub const BIT_DEPTH_LABEL: ChildId = ChildId(65535);
}
