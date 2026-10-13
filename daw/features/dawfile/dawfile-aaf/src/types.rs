//! Domain types for AAF session data.
//!
//! These types represent the parsed contents of an AAF session file.
//! They are format-specific and not yet mapped to `daw_proto` types.

/// A parsed AAF session.
#[derive(Debug, Clone)]
pub struct AafSession {
    /// AAF object model version (from the Header object).
    pub object_model_version: u32,
    /// Session sample rate derived from audio essence descriptors.
    /// Defaults to 48000 if no audio essence is present.
    pub session_sample_rate: u32,
    /// Top-level tracks from the primary CompositionMob.
    /// (Convenience accessor — same as `compositions[0].tracks`.)
    pub tracks: Vec<AafTrack>,
    /// Timeline markers from the primary CompositionMob.
    pub markers: Vec<AafMarker>,
    /// Timecode start, if a timecode track is present.
    pub timecode_start: Option<AafTimecode>,
    /// All CompositionMobs in the file (usually exactly one).
    pub compositions: Vec<AafComposition>,
}

/// A CompositionMob — the "main timeline" of an AAF session.
#[derive(Debug, Clone)]
pub struct AafComposition {
    /// Mob name (often empty string for the top-level composition).
    pub name: String,
    /// All timeline tracks on this composition.
    pub tracks: Vec<AafTrack>,
    /// Markers carried by EventMobSlots on this composition.
    pub markers: Vec<AafMarker>,
    /// Timecode track, if present.
    pub timecode: Option<AafTimecode>,
}

/// A timeline track (from a `TimelineMobSlot` on a `CompositionMob`).
#[derive(Debug, Clone)]
pub struct AafTrack {
    /// The `SlotID` from the `MobSlot` — used to reference this slot.
    pub slot_id: u32,
    /// User-visible track name.
    pub name: String,
    /// Physical output number (1-based channel/output assignment), if set.
    pub physical_track_number: Option<u32>,
    /// Edit rate of this track. All clip positions and lengths are in edit units
    /// at this rate. Convert to samples via [`EditRate::to_samples`].
    pub edit_rate: EditRate,
    /// What kind of data this track carries.
    pub kind: TrackKind,
    /// Clips and gaps on this track, in timeline order.
    pub clips: Vec<AafClip>,
}

/// The data type carried by a track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackKind {
    /// Audio waveform data.
    Audio,
    /// Video/picture data.
    Video,
    /// MIDI or data track.
    Midi,
    /// SMPTE timecode.
    Timecode,
    /// Unrecognised or auxiliary data type.
    Other,
}

/// A placed clip or gap on a track.
#[derive(Debug, Clone)]
pub struct AafClip {
    /// Absolute start position on the timeline, in edit units of the enclosing
    /// track's `edit_rate`. Includes the `MobSlot.Origin` offset.
    pub start_position: i64,
    /// Length in edit units of the enclosing track's `edit_rate`.
    pub length: i64,
    /// The content of this clip.
    pub kind: ClipKind,
}

/// The content type of a clip.
#[derive(Debug, Clone)]
pub enum ClipKind {
    /// A reference to source media on disk.
    SourceClip {
        /// Resolved file URL/path from the `NetworkLocator`, if any.
        /// Typically a `file:///` URL (e.g. `file:///recordings/kick.wav`).
        source_file: Option<String>,
        /// Raw 32-byte UMID of the ultimately resolved `SourceMob`.
        source_mob_id: [u8; 32],
        /// Slot ID in the `SourceMob` that this clip references.
        source_slot_id: u32,
        /// Offset into the source in edit units of the *source mob's* slot rate.
        source_start: i64,
        /// Audio format information from the `PCMDescriptor`, if available.
        audio_info: Option<AudioEssenceInfo>,
    },
    /// Empty space / silence (from an AAF `Filler` segment).
    Filler,
    /// A dissolve or other transition between two adjacent clips.
    Transition {
        /// The cut point within the transition, in edit units.
        cut_point: i64,
    },
    /// An operation group (speed change, gain ramp, plug-in effect) applied to
    /// the contained input clips. The inner clips are already in `AafTrack.clips`
    /// at the appropriate positions; this variant records the wrapping operation.
    Operation {
        /// Number of input segments inside this operation group.
        input_count: usize,
    },
    /// A segment class we don't explicitly model.
    Other,
}

/// Audio format information harvested from a `PCMDescriptor` / `SoundDescriptor`.
#[derive(Debug, Clone, Copy)]
pub struct AudioEssenceInfo {
    /// Native sample rate of the audio file.
    pub sample_rate: u32,
    /// Number of audio channels.
    pub channels: u32,
    /// Bit depth (typically 16, 24, or 32).
    pub quantization_bits: u32,
    /// Total duration of the audio file in samples.
    pub length_samples: i64,
}

/// A timeline marker (from a `CommentMarker` event on an `EventMobSlot`).
#[derive(Debug, Clone)]
pub struct AafMarker {
    /// Position in edit units of the slot's edit rate.
    pub position: i64,
    /// Edit rate of the `EventMobSlot` carrying this marker.
    pub edit_rate: EditRate,
    /// Marker text / comment.
    pub comment: String,
}

/// SMPTE timecode start information.
#[derive(Debug, Clone, Copy)]
pub struct AafTimecode {
    /// Timecode start value expressed as total frames from midnight.
    pub start: i64,
    /// Frames per second (integer; use `drop_frame` for 29.97).
    pub fps: u16,
    /// Whether this is drop-frame timecode (e.g. 29.97 drop).
    pub drop_frame: bool,
    /// Edit rate of the timecode slot.
    pub edit_rate: EditRate,
}

// ─── EditRate ────────────────────────────────────────────────────────────────

/// A rational edit rate: `numerator / denominator` edit units per second.
///
/// Examples:
/// - Audio 48 kHz: `48000/1`
/// - Video 25 fps: `25/1`
/// - Video 29.97 drop: `30000/1001`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditRate {
    pub numerator: i32,
    pub denominator: i32,
}

impl EditRate {
    pub const AUDIO_48K: Self = Self {
        numerator: 48000,
        denominator: 1,
    };
    pub const AUDIO_44K: Self = Self {
        numerator: 44100,
        denominator: 1,
    };
    pub const VIDEO_25: Self = Self {
        numerator: 25,
        denominator: 1,
    };
    pub const VIDEO_30: Self = Self {
        numerator: 30,
        denominator: 1,
    };
    pub const VIDEO_2997_DROP: Self = Self {
        numerator: 30000,
        denominator: 1001,
    };
    pub const VIDEO_24: Self = Self {
        numerator: 24,
        denominator: 1,
    };

    /// Returns true if this is a plausible audio sample rate (numerator ≥ 8000,
    /// denominator == 1).
    pub fn is_audio_rate(self) -> bool {
        self.denominator == 1 && self.numerator >= 8000
    }

    /// Convert a count of edit units to seconds.
    pub fn to_seconds(self, units: i64) -> f64 {
        if self.numerator == 0 {
            return 0.0;
        }
        units as f64 * self.denominator as f64 / self.numerator as f64
    }

    /// Convert a count of edit units to audio samples at `sample_rate`.
    ///
    /// Uses `i128` arithmetic to avoid overflow on long timelines.
    pub fn to_samples(self, units: i64, sample_rate: u32) -> i64 {
        if self.numerator == 0 || self.denominator == 0 {
            return 0;
        }
        let r =
            units as i128 * sample_rate as i128 * self.denominator as i128 / self.numerator as i128;
        r as i64
    }

    /// Convert an audio sample count at `from_sample_rate` into edit units at
    /// this rate.
    pub fn from_samples(self, samples: i64, from_sample_rate: u32) -> i64 {
        if from_sample_rate == 0 || self.denominator == 0 {
            return 0;
        }
        let r = samples as i128 * self.numerator as i128
            / (self.denominator as i128 * from_sample_rate as i128);
        r as i64
    }
}

impl std::fmt::Display for EditRate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.numerator, self.denominator)
    }
}
