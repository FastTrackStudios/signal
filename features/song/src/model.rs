//! Core durable-storage types: [`Song`], [`Arrangement`], and the
//! reference / manifest value types they carry.
//!
//! These are the STORED counterpart to the runtime `session_proto::Song`
//! (which is hydrated from this form later, out of scope here). Nothing in
//! this module depends on `keyflow`: a chart is a [`ChartRef`] (a relative
//! path within the song folder and/or an attachment id), never an embedded
//! `keyflow::Chart`.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::key::Key;

/// Stable identity of a [`Song`].
pub type SongId = Uuid;

/// Stable identity of an [`Arrangement`] within a song.
pub type ArrangementId = Uuid;

/// A portable, self-contained song.
///
/// On disk this is a folder (see the crate docs and
/// `docs/song-folder-format.md`): a `song.md` index plus a per-arrangement
/// resource folder under `arrangements/`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Song {
    /// Stable identity. Generated on creation; never re-derived from the
    /// folder name, so renaming the folder doesn't orphan references.
    pub id: SongId,

    /// Human title (e.g. `"Great Are You Lord"`).
    pub title: String,

    /// Free-form tags for organisation / filtering.
    #[serde(default)]
    pub tags: Vec<String>,

    /// The arrangement used by default. MUST match the `id` of one of
    /// [`Song::arrangements`].
    pub default_arrangement: ArrangementId,

    /// One or more arrangements. Always non-empty in a well-formed song
    /// (the default arrangement is one of these).
    pub arrangements: Vec<Arrangement>,
}

impl Song {
    /// Look up an arrangement by id.
    #[must_use]
    pub fn arrangement(&self, id: ArrangementId) -> Option<&Arrangement> {
        self.arrangements.iter().find(|a| a.id == id)
    }

    /// The default arrangement, if present.
    #[must_use]
    pub fn default(&self) -> Option<&Arrangement> {
        self.arrangement(self.default_arrangement)
    }
}

/// One arrangement of a [`Song`] — a specific key + chart + parts layout.
///
/// A song has a default arrangement and may accrue alternates over time
/// (e.g. an acoustic version, a different key for a different vocalist).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Arrangement {
    /// Stable identity within the song.
    pub id: ArrangementId,

    /// Human name (e.g. `"Default"`, `"Acoustic"`, `"Key of A"`).
    pub name: String,

    /// The musical key of this arrangement.
    pub key: Key,

    /// Base tempo in BPM. Individual [`Part`]s may override it.
    ///
    /// Lives on the arrangement rather than the [`Song`] for the same reason
    /// [`Arrangement::key`] does: a second arrangement of the same song is
    /// routinely a different tempo, and pinning it to the song would make one
    /// of them lie.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tempo_bpm: Option<f32>,

    /// Base time signature. Individual [`Part`]s may override it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_signature: Option<TimeSignature>,

    /// Reference to this arrangement's chart, if any. Keyflow-agnostic:
    /// a relative path and/or attachment id — never an embedded chart.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart_ref: Option<ChartRef>,

    /// The arrangement's parts in running order — Intro, Verse 1, Chorus …
    #[serde(default)]
    pub parts: PartsManifest,

    /// References to attachments (audio, PDFs, images) that belong to this
    /// arrangement. The real `AttachmentService` is wired elsewhere; here
    /// these are just references.
    #[serde(default)]
    pub attachment_refs: Vec<AttachmentRef>,
}

/// A reference to a chart resource. At least one of the two fields is
/// expected to be set; both may be, in which case `path` is authoritative
/// and `attachment_id` is a durable pointer that survives moves.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartRef {
    /// Relative path within the song folder, e.g.
    /// `arrangements/default/chart.kf`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    /// Content-addressed / attachment-service id, if the chart is tracked
    /// as an attachment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachment_id: Option<String>,
}

impl ChartRef {
    /// A chart referenced purely by relative path.
    #[must_use]
    pub fn from_path(path: impl Into<String>) -> Self {
        Self {
            path: Some(path.into()),
            attachment_id: None,
        }
    }
}

/// A reference to an attachment file that lives alongside the song.
///
/// This is intentionally a *reference* type only — no I/O, no
/// `AttachmentService` coupling (a separate workstream owns that).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentRef {
    /// Attachment id (content-addressed or service-assigned).
    pub id: String,

    /// Relative path within the song folder, if the bytes live in-folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    /// SHA-256 hex digest of the attachment bytes, for integrity /
    /// content addressing. Optional until verified.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,

    /// Free-form kind hint (`"audio"`, `"pdf"`, `"image"`, …). Open set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

/// The arrangement's parts, **in running order**.
///
/// Order is musical, not cosmetic: this is the sequence the band plays, so
/// index 0 is what the song opens with.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PartsManifest {
    #[serde(default)]
    pub parts: Vec<Part>,
}

impl PartsManifest {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    /// Total length in bars, when every part declares one.
    #[must_use]
    pub fn total_bars(&self) -> Option<u32> {
        self.parts.iter().map(|p| p.bars).sum()
    }
}

/// One part of the song — `"Intro"`, `"Verse 1"`, `"Chorus"`, `"Bridge"`,
/// `"Turnaround"`. The section of the arrangement you can point at and say
/// "take it from there".
///
/// The name is free-form and org-defined, never a closed enum: a worship set
/// and a musical will name their parts differently and both are right.
///
/// Tempo and time signature are **overrides**. A part that does not set them
/// inherits the arrangement's, which is the common case — only the half-time
/// bridge or the 6/8 turnaround needs its own.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    /// Part name. Free-form / org-defined; not a closed enum.
    pub name: String,

    /// Length in bars, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bars: Option<u32>,

    /// Tempo override for this part, in BPM. `None` = inherit the
    /// arrangement's tempo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tempo_bpm: Option<f32>,

    /// Time-signature override for this part. `None` = inherit the
    /// arrangement's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_signature: Option<TimeSignature>,

    /// The sound this part calls for, by name — a patch / preset / scene in
    /// whatever rig plays it ("Worship Energy", "Dry Piano").
    ///
    /// Deliberately a **string**, not a typed reference: this crate stays
    /// portable storage and must not learn about any particular rig's patch
    /// ids, the same way it stores charts as references rather than embedding
    /// `keyflow::Chart`. Whoever plays the song resolves the name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch: Option<String>,

    /// Free-form performance note for this part ("build", "drop to pad",
    /// "leader speaks").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,

    /// Resources associated with this part (charts, stems, notes …).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resource_refs: Vec<ResourceRef>,
}

impl Part {
    /// A bare named part — everything else inherited or unknown.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }
}

/// A time signature, e.g. 4/4 or 6/8.
///
/// `denominator` is a note value (4 = crotchet, 8 = quaver), so it is always
/// a power of two; [`TimeSignature::is_valid`] is the check, deliberately not
/// enforced in the type so a malformed stored song still round-trips and can
/// be reported rather than refusing to load.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeSignature {
    /// Beats per bar (the 6 in 6/8).
    pub numerator: u8,
    /// Beat note value (the 8 in 6/8).
    pub denominator: u8,
}

impl TimeSignature {
    /// Common time.
    pub const COMMON: Self = Self {
        numerator: 4,
        denominator: 4,
    };

    #[must_use]
    pub const fn new(numerator: u8, denominator: u8) -> Self {
        Self {
            numerator,
            denominator,
        }
    }

    /// Whether this is musically well-formed: a non-zero beat count over a
    /// power-of-two note value.
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        self.numerator > 0 && self.denominator.is_power_of_two() && self.denominator > 0
    }
}

impl Default for TimeSignature {
    fn default() -> Self {
        Self::COMMON
    }
}

impl std::fmt::Display for TimeSignature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.numerator, self.denominator)
    }
}

/// A generic resource reference used by [`Part`]s — a relative path
/// and/or attachment id, plus an optional kind hint.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachment_id: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}
