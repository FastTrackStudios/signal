//! AAF property identifier (PID) constants.
//!
//! Values from SMPTE ST 2001-1 (AAF Object Model) and the AAF SDK
//! `AAFPropertyIDs.h`. PIDs are 16-bit unsigned integers; each is unique within
//! the global AAF property namespace.

// ─── InterchangeObject (base class of every AAF object) ──────────────────────

/// Class AUID of this object (16-byte `Auid`). Present on **every** AAF object.
pub const PID_OBJ_CLASS: u16 = 0x0101;
/// Generation UID (optional, 16 bytes).
pub const PID_OBJ_GENERATION: u16 = 0x0102;

// ─── Component ───────────────────────────────────────────────────────────────

/// DataDefinition (weak ref / 16-byte AUID) — indicates audio, video, TC, …
pub const PID_COMPONENT_DATA_DEFINITION: u16 = 0x0B01;
/// DataDefinition — Avid internal format PID (same semantic as 0x0B01).
pub const PID_COMPONENT_DATA_DEFINITION_AVID: u16 = 0x0201;
/// Length in edit units (i64).
pub const PID_COMPONENT_LENGTH: u16 = 0x0B02;
/// Length in edit units (i64) — Avid internal format PID (same semantic as 0x0B02).
pub const PID_COMPONENT_LENGTH_AVID: u16 = 0x0202;

// ─── Sequence ────────────────────────────────────────────────────────────────

/// Ordered vector of Component strong references.
pub const PID_SEQUENCE_COMPONENTS: u16 = 0x1001;

// ─── SourceReference (base for SourceClip) ───────────────────────────────────

/// Source MobID (32-byte UMID). All-zero means "no source" / original recording.
pub const PID_SOURCE_REF_SOURCE_ID: u16 = 0x1101;
/// Slot ID within the referenced Mob (u32).
pub const PID_SOURCE_REF_MOB_SLOT_ID: u16 = 0x1102;

// ─── SourceClip ──────────────────────────────────────────────────────────────

/// Start position within the source (i64 edit units at source slot rate).
pub const PID_SOURCE_CLIP_START_POSITION: u16 = 0x1201;
/// Fade-in length (i64).
pub const PID_SOURCE_CLIP_FADE_IN_LENGTH: u16 = 0x1202;
/// Fade-in type (u32 enum).
pub const PID_SOURCE_CLIP_FADE_IN_TYPE: u16 = 0x1203;
/// Fade-out length (i64).
pub const PID_SOURCE_CLIP_FADE_OUT_LENGTH: u16 = 0x1204;
/// Fade-out type (u32 enum).
pub const PID_SOURCE_CLIP_FADE_OUT_TYPE: u16 = 0x1205;

// ─── Event (base for CommentMarker) ──────────────────────────────────────────

/// Event position (i64 edit units within the EventMobSlot).
pub const PID_EVENT_POSITION: u16 = 0x0C01;
/// Event position — Avid internal format PID (same semantic as 0x0C01).
pub const PID_EVENT_POSITION_AVID: u16 = 0x0601;
/// Event comment text (UTF-16LE string).
pub const PID_EVENT_COMMENT: u16 = 0x0C02;

// ─── CommentMarker ───────────────────────────────────────────────────────────

/// Annotation text on a CommentMarker (UTF-16LE string).
pub const PID_COMMENT_MARKER_ANNOTATION: u16 = 0x0603;
/// Color of the marker (optional).
pub const PID_COMMENT_MARKER_COLOR: u16 = 0x0604;

// ─── Transition ──────────────────────────────────────────────────────────────

/// Strong ref to the OperationGroup inside this Transition.
pub const PID_TRANSITION_OPERATION_GROUP: u16 = 0x1401;
/// Cut point within the Transition (i64).
pub const PID_TRANSITION_CUT_POINT: u16 = 0x1402;

// ─── OperationGroup ──────────────────────────────────────────────────────────

/// Weak ref to the OperationDefinition (AUID).
pub const PID_OPERATION_GROUP_OPERATION: u16 = 0x0D01;
/// Optional strong ref to a Rendering segment (for rendered effects).
pub const PID_OPERATION_GROUP_RENDERING: u16 = 0x0D02;
/// Bypass override flag (bool / i32).
pub const PID_OPERATION_GROUP_BYPASS_OVERRIDE: u16 = 0x0D03;
/// Vector of input Segment strong refs.
pub const PID_OPERATION_GROUP_INPUT_SEGMENTS: u16 = 0x0D05;
/// Vector of Parameter strong refs.
pub const PID_OPERATION_GROUP_PARAMETERS: u16 = 0x0D06;

// ─── Timecode ────────────────────────────────────────────────────────────────

/// Timecode start value in frames (i64).
pub const PID_TIMECODE_START: u16 = 0x1501;
/// Frames per second (u16).
pub const PID_TIMECODE_FPS: u16 = 0x1502;
/// Drop-frame flag (u8 / bool).
pub const PID_TIMECODE_DROP: u16 = 0x1503;

// ─── Mob ─────────────────────────────────────────────────────────────────────

/// Mob's 32-byte UMID (MobID).
pub const PID_MOB_MOB_ID: u16 = 0x4401;
/// Mob name (UTF-16LE string, optional).
pub const PID_MOB_NAME: u16 = 0x4402;
/// Ordered vector of MobSlot strong refs.
pub const PID_MOB_SLOTS: u16 = 0x4403;
/// Last-modified timestamp.
pub const PID_MOB_LAST_MODIFIED: u16 = 0x4404;
/// Creation timestamp.
pub const PID_MOB_CREATION_TIME: u16 = 0x4405;
/// User comment set (TaggedValues).
pub const PID_MOB_USER_COMMENTS: u16 = 0x4406;
/// KLV data set.
pub const PID_MOB_KLV_DATA: u16 = 0x4407;
/// Attributes set.
pub const PID_MOB_ATTRIBUTES: u16 = 0x4408;
/// Usage code (u32 enum).
pub const PID_MOB_USAGE_CODE: u16 = 0x4409;
/// Annotations set.
pub const PID_MOB_ANNOTATIONS: u16 = 0x440B;

// ─── MobSlot ─────────────────────────────────────────────────────────────────

/// Slot ID (u32).
pub const PID_MOB_SLOT_SLOT_ID: u16 = 0x1011;
/// Slot ID (u32) — Avid internal format PID.
pub const PID_MOB_SLOT_SLOT_ID_AVID: u16 = 0x4801;
/// Slot name (UTF-16LE string, optional).
pub const PID_MOB_SLOT_SLOT_NAME: u16 = 0x1012;
/// Slot name (UTF-16LE string, optional) — Avid internal format PID.
pub const PID_MOB_SLOT_SLOT_NAME_AVID: u16 = 0x4802;
/// Physical track / output number (u32, optional).
pub const PID_MOB_SLOT_PHYSICAL_TRACK_NUMBER: u16 = 0x1014;
/// Physical track / output number (u32, optional) — Avid internal format PID.
pub const PID_MOB_SLOT_PHYSICAL_TRACK_NUMBER_AVID: u16 = 0x4804;
/// Strong ref to the Segment occupying this slot (SMPTE standard).
pub const PID_MOB_SLOT_SEGMENT: u16 = 0x1B02;
/// Strong ref to the Segment occupying this slot (Avid internal format).
pub const PID_MOB_SLOT_SEGMENT_AVID: u16 = 0x4803;

// ─── TimelineMobSlot ─────────────────────────────────────────────────────────

/// Edit rate of this slot (rational: 2× i32 — numerator then denominator).
pub const PID_TIMELINE_MOB_SLOT_EDIT_RATE: u16 = 0x4B01;
/// Origin: offset of sample 0 from the composition origin (i64).
pub const PID_TIMELINE_MOB_SLOT_ORIGIN: u16 = 0x4B02;
/// Mark-in point (optional, i64).
pub const PID_TIMELINE_MOB_SLOT_MARK_IN: u16 = 0x4B03;
/// Mark-out point (optional, i64).
pub const PID_TIMELINE_MOB_SLOT_MARK_OUT: u16 = 0x4B04;
/// User playback position (optional, i64).
pub const PID_TIMELINE_MOB_SLOT_USER_POS: u16 = 0x4B05;

// ─── EventMobSlot ────────────────────────────────────────────────────────────

/// Edit rate of this event slot (rational: 2× i32).
pub const PID_EVENT_MOB_SLOT_EDIT_RATE: u16 = 0x4901;
/// Event type (u32 enum, optional).
pub const PID_EVENT_MOB_SLOT_EVENT_TYPE: u16 = 0x4902;

// ─── Header ──────────────────────────────────────────────────────────────────

/// Byte order of this file (u16: 0x4949 = LE, 0x4D4D = BE).
pub const PID_HEADER_BYTE_ORDER: u16 = 0x3B01;
/// File last-modified timestamp.
pub const PID_HEADER_LAST_MODIFIED: u16 = 0x3B02;
/// AAF file format version (u16 major + u16 minor struct).
pub const PID_HEADER_VERSION: u16 = 0x3B04;
/// Identification list (vector of Identification objects).
pub const PID_HEADER_IDENTIFICATION_LIST: u16 = 0x3B05;
/// Strong ref to the ContentStorage object (SMPTE standard).
pub const PID_HEADER_CONTENT_STORAGE: u16 = 0x3B06;
/// Strong ref to the ContentStorage object (Avid internal format).
pub const PID_HEADER_CONTENT_STORAGE_AVID: u16 = 0x3B03;
/// Strong ref to the Dictionary object.
pub const PID_HEADER_DICTIONARY: u16 = 0x3B07;
/// Object model version (u32).
pub const PID_HEADER_OBJECT_MODEL_VERSION: u16 = 0x3B08;
/// Operational pattern AUID (16 bytes).
pub const PID_HEADER_OPERATIONAL_PATTERN: u16 = 0x3B09;
/// Set of essence container AUIDs.
pub const PID_HEADER_ESSENCE_CONTAINERS: u16 = 0x3B0A;
/// Set of descriptive scheme AUIDs.
pub const PID_HEADER_DESCRIPTIVE_SCHEMES: u16 = 0x3B0B;

// ─── ContentStorage ──────────────────────────────────────────────────────────

/// Unordered set of all Mob strong refs in the file.
pub const PID_CONTENT_STORAGE_MOBS: u16 = 0x1901;
/// Unordered set of EssenceData strong refs.
pub const PID_CONTENT_STORAGE_ESSENCE_DATA: u16 = 0x1902;

// ─── EssenceDescriptor ───────────────────────────────────────────────────────

/// Vector of Locator strong refs.
pub const PID_ESSENCE_DESCRIPTOR_LOCATOR: u16 = 0x2F01;
/// Vector of SubDescriptor strong refs.
pub const PID_ESSENCE_DESCRIPTOR_SUB_DESCRIPTORS: u16 = 0x2F02;

// ─── FileDescriptor ──────────────────────────────────────────────────────────

/// Sample rate of the underlying file (rational: 2× i32).
pub const PID_FILE_DESCRIPTOR_SAMPLE_RATE: u16 = 0x3001;
/// Total length in samples (i64).
pub const PID_FILE_DESCRIPTOR_LENGTH: u16 = 0x3002;
/// Weak ref to the container format definition (AUID).
pub const PID_FILE_DESCRIPTOR_CONTAINER_FORMAT: u16 = 0x3004;
/// Weak ref to the codec definition (AUID).
pub const PID_FILE_DESCRIPTOR_CODEC_DEFINITION: u16 = 0x3005;

// ─── SoundDescriptor ─────────────────────────────────────────────────────────

/// Compression type AUID (weak ref / 16 bytes).
pub const PID_SOUND_DESCRIPTOR_COMPRESSION: u16 = 0x3D01;
/// Number of audio channels (u32).
pub const PID_SOUND_DESCRIPTOR_CHANNELS: u16 = 0x3D07;
/// Audio sampling rate (rational: 2× i32).
pub const PID_SOUND_DESCRIPTOR_AUDIO_SAMPLING_RATE: u16 = 0x3D03;
/// Locked flag (bool / u8).
pub const PID_SOUND_DESCRIPTOR_LOCKED: u16 = 0x3D04;
/// Audio reference level (i32, dBu).
pub const PID_SOUND_DESCRIPTOR_AUDIO_REF_LEVEL: u16 = 0x3D05;
/// Electro-spatial formulation (u32 enum).
pub const PID_SOUND_DESCRIPTOR_ELECTRO_SPATIAL: u16 = 0x3D06;
/// Quantization bits per sample (u32).
pub const PID_SOUND_DESCRIPTOR_QUANTIZATION_BITS: u16 = 0x3D09;
/// Dial norm (i32).
pub const PID_SOUND_DESCRIPTOR_DIAL_NORM: u16 = 0x3D0A;

// ─── PCMDescriptor (extends SoundDescriptor) ─────────────────────────────────

/// Block alignment in bytes (u32).
pub const PID_PCM_DESCRIPTOR_BLOCK_ALIGN: u16 = 0x3D0B;
/// Sequence offset (u8).
pub const PID_PCM_DESCRIPTOR_SEQUENCE_OFFSET: u16 = 0x3D0C;
/// Average bytes per second (u32).
pub const PID_PCM_DESCRIPTOR_AVERAGE_BPS: u16 = 0x3D0D;
/// Channel assignment AUID (16 bytes).
pub const PID_PCM_DESCRIPTOR_CHANNEL_ASSIGNMENT: u16 = 0x3D32;

// ─── Locator ─────────────────────────────────────────────────────────────────

/// NetworkLocator: URL string (UTF-16LE).
pub const PID_NETWORK_LOCATOR_URL: u16 = 0x4001;

/// TextLocator: human-readable path description (UTF-16LE).
pub const PID_TEXT_LOCATOR_NAME: u16 = 0x4101;

// ─── SourceMob ───────────────────────────────────────────────────────────────

/// Strong ref to the EssenceDescriptor for this SourceMob.
pub const PID_SOURCE_MOB_ESSENCE_DESCRIPTION: u16 = 0x4701;
