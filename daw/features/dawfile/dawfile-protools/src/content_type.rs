//! Pro Tools block content type identifiers.
//!
//! Each block in a Pro Tools session file has a `content_type` field that
//! identifies what kind of data the block contains. These were reverse-engineered
//! from the binary format by the ptformat project.

/// Known content types found in Pro Tools session files.
///
/// Values are the raw `u16` content_type from block headers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum ContentType {
    // ── Session metadata ────────────────────────────────────────────────
    /// Old-format version info (PT 5-9)
    VersionInfoOld = 0x0003,
    /// Product name and version string
    ProductVersion = 0x0030,
    /// Session sample rate
    SessionSampleRate = 0x1028,
    /// Session path info / new-format version (PT 10+)
    SessionInfo = 0x2067,

    // ── Audio files ─────────────────────────────────────────────────────
    /// WAV sample rate and size
    WavInfo = 0x1001,
    /// WAV metadata container
    WavMetadata = 0x1003,
    /// WAV file list (full)
    WavList = 0x1004,
    /// WAV names sub-list
    WavNames = 0x103a,

    // ── Regions (PT 5-9) ────────────────────────────────────────────────
    /// Region name + number (generic)
    RegionName = 0x1007,
    /// Audio region name + number (v5-9)
    AudioRegionOld = 0x1008,
    /// Audio region list (v5-9)
    AudioRegionListOld = 0x100b,

    // ── Regions (PT 10+) ────────────────────────────────────────────────
    /// Audio region name + number (v10+)
    AudioRegionNew = 0x2629,
    /// Audio region list (v10+)
    AudioRegionListNew = 0x262a,

    // ── Region-to-track mapping (old) ───────────────────────────────────
    /// Region-to-track assignment entry
    RegionTrackEntry = 0x100e,
    /// Audio region-to-track entry
    AudioRegionTrackEntry = 0x100f,
    /// Audio region-to-track map entries
    AudioRegionTrackMapEntries = 0x1011,
    /// Audio region-to-track full map
    AudioRegionTrackMap = 0x1012,

    // ── Region-to-track mapping (v8+) ───────────────────────────────────
    /// Audio region-to-track sub-entry (v8+)
    AudioRegionTrackSubEntryNew = 0x104f,
    /// Audio region-to-track entry (v8+)
    AudioRegionTrackEntryNew = 0x1050,
    /// Audio region-to-track map entries (v8+)
    AudioRegionTrackMapEntriesNew = 0x1052,
    /// Audio region-to-track full map (v8+)
    AudioRegionTrackMapNew = 0x1054,

    // ── Tracks ──────────────────────────────────────────────────────────
    /// Audio track name + number
    AudioTrackInfo = 0x1014,
    /// Audio tracks container
    AudioTrackList = 0x1015,
    /// Per-track mix-state block (volume, pan, mute). 281-byte payload.
    /// One block per mixable track (Master output is excluded).
    /// See `docs/pt-track-properties.md` for the byte layout.
    TrackMixSettings = 0x1029,

    /// Per-track mix wrapper. Groups `0x1029` plus routing (`0x260e`) and
    /// send entries (`0x260a`) under one container. One per mixable track,
    /// in `0x251a` document order (29 entries in the user session).
    TrackMixWrapper = 0x260d,

    /// Track output / routing assignment. Carries a length-prefixed string
    /// at payload offset `+0x24..` naming the destination (e.g.
    /// `"Analog 1-2"` for a hardware output, `"Bus 1"` for an internal
    /// bus). The 61-byte variant (payload begins `ff ff 01 01 ...`) has no
    /// destination.
    TrackRouting = 0x260e,

    /// Per-colored-track container. Holds `0x261b` (the main per-track
    /// container) plus `0x200b` whose payload `+163` byte is the track's
    /// color palette position in PT's 23-column × 3-row palette. Folder
    /// tracks have no `0x261c`.
    TrackContainer = 0x261c,

    /// Per-colored-track auxiliary state block. Lives inside `0x261c`.
    /// Payload byte `+163` = color palette position.
    TrackAuxState = 0x200b,

    /// Per-session list of fade definitions (lengths + curve shape). Wraps
    /// one `FadeDef` (0x262f) per fade-marked track entry.
    FadeDefList = 0x2630,
    /// A single fade definition: 24..36-byte payload encoding in-length,
    /// out-length, and curve shape. Indexed by the fade entry's `+4` field.
    /// See `docs/pt-fade-encoding.md`.
    FadeDef = 0x262f,

    // ── FX / Plugins ────────────────────────────────────────────────────
    /// Plugin entry
    PluginEntry = 0x1017,
    /// Plugin full list
    PluginList = 0x1018,

    // ── I/O Routing ─────────────────────────────────────────────────────
    /// I/O channel entry
    IoChannelEntry = 0x1021,
    /// I/O channel list
    IoChannelList = 0x1022,
    /// I/O route entry
    IoRoute = 0x2602,
    /// I/O routing table
    IoRoutingTable = 0x2603,

    // ── MIDI events ─────────────────────────────────────────────────────
    /// MIDI events data block
    MidiEventsBlock = 0x2000,

    // ── MIDI regions (PT 5-9) ───────────────────────────────────────────
    /// MIDI region name + number (v5-9)
    MidiRegionOld = 0x2001,
    /// MIDI regions map (v5-9)
    MidiRegionMapOld = 0x2002,

    // ── MIDI regions (PT 10+) ───────────────────────────────────────────
    /// MIDI region name + number (v10+)
    MidiRegionNew = 0x2633,
    /// MIDI regions map (v10+)
    MidiRegionMapNew = 0x2634,

    // ── MIDI tracks ─────────────────────────────────────────────────────
    /// MIDI track full list
    MidiTrackList = 0x2519,
    /// MIDI track name + number
    MidiTrackInfo = 0x251a,

    // ── MIDI region-to-track ────────────────────────────────────────────
    /// MIDI region-to-track entry
    MidiRegionTrackEntry = 0x1056,
    /// MIDI region-to-track map entries
    MidiRegionTrackMapEntries = 0x1057,
    /// MIDI region-to-track full map
    MidiRegionTrackMap = 0x1058,

    // ── Compound regions ────────────────────────────────────────────────
    /// Compound region element
    CompoundRegionElement = 0x2523,
    /// Compound region group
    CompoundRegionGroup = 0x2628,
    /// Compound MIDI region container
    CompoundMidiRegionContainer = 0x262b,
    /// Compound MIDI region full map
    CompoundMidiRegionMap = 0x262c,

    // ── Alternate playlists ─────────────────────────────────────────────
    /// Alternate playlist map container (wraps a 0x1054 for inactive playlists)
    AlternatePlaylistMap = 0x2428,
    /// Alternate playlist map container, secondary variant
    AlternatePlaylistMapAlt = 0x2429,

    // ── Markers ─────────────────────────────────────────────────────────
    /// Marker list (system container: Tempo, Meter, Key Sig, Chord Symbols)
    MarkerList = 0x271a,
    /// User-defined memory location container (user markers live here)
    UserMarkerContainer = 0x263b,
    /// Song-section marker section (PT 12 layout). Holds `MarkerEntryV12` children
    /// — each one is a user-defined memory location with name + tick position.
    MarkerSectionV12 = 0x2030,
    /// Song-section marker entry (PT 12 layout). Payload: header(8B) + u32 name_len
    /// + name + u64 encoded tick position + duplicate(8B) + remaining fields.
    MarkerEntryV12 = 0x2077,

    // ── Snaps ───────────────────────────────────────────────────────────
    /// Snaps block
    SnapsBlock = 0x2511,

    // ── Tempo / meter ────────────────────────────────────────────────────
    /// Tempo map block (contains "Tempo"/"Const"/"TMS" structure with f64 BPM)
    TempoBlock = 0x2028,
    /// Meter map block (contains "Meter" + per-measure time signature changes)
    MeterBlock = 0x2029,

    // ── Markers ─────────────────────────────────────────────────────────
    /// Individual entry block within the marker / track-list hierarchy
    MarkerEntry = 0x2619,

    // ── Groups (PT 12+) ─────────────────────────────────────────────────
    /// Edit-groups list block (one per session when any groups are defined).
    /// Contains a per-track membership table followed by a flat sequence of
    /// `[u32 namelen][utf-8 name][i16 color]` entries. See
    /// `docs/converter-frida-discovered-offsets.md` §"`0x4501` / `0x4702`".
    EditGroupList = 0x4501,
    /// Stem-mapping list (PT 12+'s "Stem Mapping" feature). Flat list of
    /// `[u32 namelen][utf-8 name]` entries; starts with built-in stem types
    /// `Dialog`/`Music`/`Effects`/`Narration`. Used to categorize tracks
    /// for stem export.
    StemMappingList = 0x4702,

    /// Internal (non-audio) track entry — Aux Input / Internal Bus / Master
    /// Fader / Click track. One block per internal track. Name lives at
    /// payload `+0x1d` (= magic + `0x24`) as a length-prefixed string;
    /// 6-byte routing UID at `+0x29..+0x2e`.
    InternalTrackEntry = 0x261e,
}

impl ContentType {
    /// Try to parse a raw u16 into a known content type.
    pub fn from_raw(raw: u16) -> Option<Self> {
        // Safety: We match exhaustively rather than transmute
        match raw {
            0x0003 => Some(Self::VersionInfoOld),
            0x0030 => Some(Self::ProductVersion),
            0x1028 => Some(Self::SessionSampleRate),
            0x2067 => Some(Self::SessionInfo),

            0x1001 => Some(Self::WavInfo),
            0x1003 => Some(Self::WavMetadata),
            0x1004 => Some(Self::WavList),
            0x103a => Some(Self::WavNames),

            0x1007 => Some(Self::RegionName),
            0x1008 => Some(Self::AudioRegionOld),
            0x100b => Some(Self::AudioRegionListOld),
            0x2629 => Some(Self::AudioRegionNew),
            0x262a => Some(Self::AudioRegionListNew),

            0x100e => Some(Self::RegionTrackEntry),
            0x100f => Some(Self::AudioRegionTrackEntry),
            0x1011 => Some(Self::AudioRegionTrackMapEntries),
            0x1012 => Some(Self::AudioRegionTrackMap),
            0x104f => Some(Self::AudioRegionTrackSubEntryNew),
            0x1050 => Some(Self::AudioRegionTrackEntryNew),
            0x1052 => Some(Self::AudioRegionTrackMapEntriesNew),
            0x1054 => Some(Self::AudioRegionTrackMapNew),

            0x1014 => Some(Self::AudioTrackInfo),
            0x1015 => Some(Self::AudioTrackList),
            0x1029 => Some(Self::TrackMixSettings),
            0x260d => Some(Self::TrackMixWrapper),
            0x260e => Some(Self::TrackRouting),
            0x261c => Some(Self::TrackContainer),
            0x200b => Some(Self::TrackAuxState),
            0x2630 => Some(Self::FadeDefList),
            0x262f => Some(Self::FadeDef),

            0x1017 => Some(Self::PluginEntry),
            0x1018 => Some(Self::PluginList),

            0x1021 => Some(Self::IoChannelEntry),
            0x1022 => Some(Self::IoChannelList),
            0x2602 => Some(Self::IoRoute),
            0x2603 => Some(Self::IoRoutingTable),

            0x2000 => Some(Self::MidiEventsBlock),
            0x2001 => Some(Self::MidiRegionOld),
            0x2002 => Some(Self::MidiRegionMapOld),
            0x2633 => Some(Self::MidiRegionNew),
            0x2634 => Some(Self::MidiRegionMapNew),

            0x2519 => Some(Self::MidiTrackList),
            0x251a => Some(Self::MidiTrackInfo),

            0x1056 => Some(Self::MidiRegionTrackEntry),
            0x1057 => Some(Self::MidiRegionTrackMapEntries),
            0x1058 => Some(Self::MidiRegionTrackMap),

            0x2523 => Some(Self::CompoundRegionElement),
            0x2628 => Some(Self::CompoundRegionGroup),
            0x262b => Some(Self::CompoundMidiRegionContainer),
            0x262c => Some(Self::CompoundMidiRegionMap),

            0x2428 => Some(Self::AlternatePlaylistMap),
            0x2429 => Some(Self::AlternatePlaylistMapAlt),

            0x271a => Some(Self::MarkerList),
            0x263b => Some(Self::UserMarkerContainer),
            0x2030 => Some(Self::MarkerSectionV12),
            0x2077 => Some(Self::MarkerEntryV12),
            0x2511 => Some(Self::SnapsBlock),

            0x2028 => Some(Self::TempoBlock),
            0x2029 => Some(Self::MeterBlock),

            0x2619 => Some(Self::MarkerEntry),

            0x4501 => Some(Self::EditGroupList),
            0x4702 => Some(Self::StemMappingList),
            0x261e => Some(Self::InternalTrackEntry),

            _ => None,
        }
    }

    /// Get the raw u16 value.
    pub fn as_raw(self) -> u16 {
        self as u16
    }
}

impl core::fmt::Display for ContentType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?} (0x{:04x})", self.as_raw())
    }
}
