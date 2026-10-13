//! Domain types for Pro Tools session data.
//!
//! These types represent the parsed contents of a Pro Tools session file.
//! They are format-specific (not yet mapped to `daw_proto` types).

/// A parsed Pro Tools session.
#[derive(Debug, Clone, PartialEq)]
pub struct ProToolsSession {
    /// Pro Tools version that created this session (5-12).
    pub version: u16,
    /// Session sample rate as stored in the file.
    pub session_sample_rate: u32,
    /// Initial (or only) BPM of the session.
    pub bpm: f64,
    /// All tempo change events, sorted by position.
    ///
    /// Contains at least one entry (the initial tempo). If the session has
    /// no tempo changes this is a single-element Vec.
    pub tempo_events: Vec<TempoEvent>,
    /// Time-signature change events, sorted by position.
    ///
    /// Empty for sessions with no meter changes (4/4 implicit).
    pub meter_events: Vec<MeterEvent>,
    /// User-defined markers (Pro Tools Memory Locations).
    pub markers: Vec<Marker>,
    /// Audio file references.
    pub audio_files: Vec<AudioFile>,
    /// Audio regions.
    pub audio_regions: Vec<AudioRegion>,
    /// Audio tracks with their region assignments.
    pub audio_tracks: Vec<Track>,
    /// MIDI regions.
    pub midi_regions: Vec<MidiRegion>,
    /// MIDI tracks with their region assignments.
    pub midi_tracks: Vec<Track>,
    /// Session-wide registry of all plugin types used anywhere in the session.
    ///
    /// This is a global list — Pro Tools stores one list for the whole session,
    /// not per-track. The same plugin may appear multiple times if used in
    /// different configurations (e.g., mono vs. stereo instance).
    pub plugins: Vec<PluginEntry>,
    /// I/O channels configured in the session's Hardware Setup and I/O Setup.
    pub io_channels: Vec<IoChannel>,
    /// I/O routing entries from `0x2602` blocks. PT sessions typically
    /// contain hundreds of entries; filter to `active == true` for the
    /// live routings. See [`RoutingEntry`].
    pub routing_entries: Vec<RoutingEntry>,
    /// Edit groups (PT 12+) decoded from the session's `0x4501` block.
    /// Empty when the session has no groups defined. Per-track membership
    /// not yet decoded — only group names + colors are populated.
    /// See [`EditGroup`].
    pub edit_groups: Vec<EditGroup>,
    /// Stem-mapping classifications (PT 12+) decoded from the session's
    /// `0x4702` block. Built-in entries start with `Dialog`, `Music`,
    /// `Effects`, `Narration`. Empty when the session has no mapping.
    pub stem_mappings: Vec<String>,
    /// Internal (non-audio) tracks of the session — aux returns, internal
    /// buses, the master fader, the click metronome track. PT stores these
    /// separately from the audio/MIDI track lists. Decoded from `0x261e`
    /// blocks; see [`InternalTrack`].
    pub internal_tracks: Vec<InternalTrack>,
    /// Per-instance plugin state blobs extracted from the session, keyed by
    /// the owning track name. Currently detects instruments whose state is a
    /// self-contained, format-portable blob (e.g. Omnisphere). See
    /// [`PluginInstanceState`].
    pub plugin_states: Vec<PluginInstanceState>,
}

/// Which plugin a [`PluginInstanceState`] blob belongs to. Determines how the
/// converter re-wraps the blob for the target host (Reaper VST3/AU).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginStateKind {
    /// Spectrasonics Omnisphere (also hosts Keyscape / Trilian / Stylus
    /// libraries). State is the `<SynthMaster …>` XML document.
    Omnisphere,
}

/// A single plugin instance's saved state, extracted from the PT session and
/// tagged with its owning track so the converter can re-emit it on the
/// matching Reaper track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginInstanceState {
    /// Name of the track that hosts this plugin instance.
    pub track_name: String,
    /// Which plugin this state belongs to.
    pub kind: PluginStateKind,
    /// The raw, format-portable state blob (for Omnisphere, the
    /// `<SynthMaster …>…</SynthMaster>` XML bytes).
    pub state: Vec<u8>,
}

/// An "internal" PT track — anything that's not an audio or MIDI playback
/// track. Covers Aux Input, Internal Bus, Master Fader, and the Click track.
///
/// Decoded from per-session `0x261e` blocks. Name lives at payload offset
/// `+0x24` (length-prefixed string). A 6-byte routing UID at `+0x32..+0x37`
/// links to a `0x2602` routing entry. The kind (aux/bus/master/click) is
/// not yet decoded — needs more probes to enumerate.
#[derive(Debug, Clone, PartialEq)]
pub struct InternalTrack {
    /// Track name as authored in PT (e.g. `"Master"`, `"DRUMS"`, `"Click 1"`).
    pub name: String,
    /// 6-byte routing UID. Maps to the `destination_uid` of a
    /// [`RoutingEntry`] when this track is a routing target.
    pub routing_uid: [u8; 6],
}

/// A Pro Tools edit group (PT 12+).
///
/// Decoded from the flat name list inside `0x4501`. Per-track membership
/// information lives in the same block's prefix and is not yet decoded.
#[derive(Debug, Clone, PartialEq)]
pub struct EditGroup {
    /// Group name as authored in PT.
    pub name: String,
    /// Group color palette index. `Some(-2)` (i.e. `0xFFFE` raw) is the
    /// default "no color" sentinel used elsewhere in the format. Other
    /// values index the same palette as `Track.color_byte`.
    pub color: Option<i16>,
}

/// An I/O routing entry decoded from a `0x2602` block in the session's
/// routing list. Each entry records one source→destination assignment.
///
/// Discovered via Frida byte-read trace on `routing-examples.ptx` and
/// the LotF session. The block has hundreds of entries on a typical
/// session — `+10` is the "active" flag (1 = used, 0 = unused); only
/// entries with `+10 == 1` represent live routings.
///
/// Other fields here are surfaced as raw bytes pending semantic
/// verification (see `docs/converter-frida-discovered-offsets.md`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutingEntry {
    /// File offset of the `0x2602` block (block magic position). Useful
    /// for cross-referencing with raw block tooling.
    pub block_start: usize,
    /// `0x2602 +10` u8 boolean — 1 means the routing entry is in use.
    pub active: bool,
    /// `0x2602 +33` u8 — secondary flag (varies; semantics TBD).
    pub flag_33: u8,
    /// `0x2602 +36` u8 — observed 0/1 alongside active entries.
    pub flag_36: u8,
    /// `0x2602 +47..+52` (6 bytes) — suspected destination UID.
    /// Pattern matches the source-file UID style seen in regions:
    /// each entry has a distinct 6-byte value when active.
    pub destination_uid: [u8; 6],
}

/// A reference to an audio file used in the session.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioFile {
    /// Filename (typically just the stem + extension, e.g. "kick.wav").
    pub filename: String,
    /// Index in the session's audio file list.
    pub index: u16,
    /// Length in samples.
    pub length: u64,
    /// 6-byte UID of the file entry (`0x1003` block payload `+38..+44`
    /// — i.e. `+45..+51` from block magic, between sentinels `0x2A`
    /// and `0x80`). Discovered via Frida byte-read trace on the LotF
    /// session — each file entry has a stable 6-byte UID. Used for
    /// region → file resolution alongside `AudioRegion.source_file_uid`.
    pub source_uid: Option<[u8; 6]>,
}

/// An audio region on the timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioRegion {
    /// Region name.
    pub name: String,
    /// Index in the session's region list.
    pub index: u16,
    /// Absolute start position on the timeline (in samples at target rate).
    pub start_pos: u64,
    /// Offset into the source audio file (in samples at target rate).
    pub sample_offset: u64,
    /// Length of the region (in samples at target rate).
    pub length: u64,
    /// Index of the source audio file in [`ProToolsSession::audio_files`].
    pub audio_file_index: u16,
    /// 6-byte UID identifying the source audio file. Regions that
    /// reference the same WAV share this UID. Decoded from payload
    /// `+54..+60` of the region's inner `0x2628` sub-block —
    /// discovered via Frida byte-read trace on the LotF user session.
    /// See `docs/converter-frida-discovered-offsets.md`.
    pub source_file_uid: Option<[u8; 6]>,
}

/// A MIDI event within a MIDI region.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MidiEvent {
    /// Position relative to region start (in ticks).
    pub position: u64,
    /// Note duration (in ticks).
    pub duration: u64,
    /// MIDI note number (0-127).
    pub note: u8,
    /// MIDI velocity (0-127).
    pub velocity: u8,
}

/// A MIDI region on the timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct MidiRegion {
    /// Region name.
    pub name: String,
    /// Index in the session's MIDI region list.
    pub index: u16,
    /// Absolute start position on the timeline (in samples at target rate).
    pub start_pos: u64,
    /// Offset into the source data.
    pub sample_offset: u64,
    /// Length of the region (in samples at target rate).
    pub length: u64,
    /// Clip timeline start in PT ticks (the three-point `start`). Used by the
    /// comp flatten to order a track's clips and cap overlaps.
    pub clip_start_ticks: u64,
    /// Clip source offset into the take in PT ticks (three-point `offset`,
    /// minus the 1e12 base). `events` are the FULL chunk; the comp flatten
    /// windows them per placement. When `clip_start_ticks == clip_src_ticks`
    /// the clip sits at its natural position and plays the whole take (PT only
    /// cuts off notes on clips that were actually trimmed).
    pub clip_src_ticks: u64,
    /// Clip length in PT ticks (the three-point `length`) — the clip's nominal
    /// trimmed extent, used as the upper source bound for cut clips.
    pub clip_len_ticks: u64,
    /// The take's timeline offset in PT ticks (`chunk.zero_ticks - ZERO_TICKS`).
    /// Note positions are chunk-relative; the timeline position of a note is
    /// `take_offset_ticks + position + (clip_start_ticks - clip_src_ticks)`.
    pub take_offset_ticks: u64,
    /// MIDI events in this region.
    pub events: Vec<MidiEvent>,
}

/// An alternate playlist for a track.
///
/// In Pro Tools, each track can have multiple playlists (named arrangements of
/// regions). The active playlist's regions are in [`Track::regions`]; any
/// inactive alternates are stored here.
#[derive(Debug, Clone, PartialEq)]
pub struct Playlist {
    /// Playlist name (e.g. `"Kick.01"`, `"Kick.02"`).
    pub name: String,
    /// Regions in this playlist.
    pub regions: Vec<TrackRegion>,
}

/// What kind of media a track carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackKind {
    /// Audio track — regions reference [`AudioRegion`] entries.
    Audio,
    /// MIDI track — regions reference [`MidiRegion`] entries.
    Midi,
}

/// A track (audio or MIDI) with its region assignments.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    /// Track name (from the track definition block).
    pub name: String,
    /// Whether this track is audio or MIDI.
    pub kind: TrackKind,
    /// Track channel index (ch_map value, used to match region assignments).
    pub index: u16,
    /// Name of the active playlist (from the region-to-track map block).
    ///
    /// For the main playlist this matches the track name; for a comp playlist
    /// this will have a suffix like `.01`.
    pub playlist_name: String,
    /// Regions on the active playlist, in timeline order.
    pub regions: Vec<TrackRegion>,
    /// Fade/crossfade entries on this track (PT marks these with byte `+46 == 0x01`
    /// in the region-track entry). Each fade overlaps one or two `regions` items
    /// and supplies a fade-in or fade-out length when emitting to a target DAW.
    pub fades: Vec<FadeRegion>,
    /// Fader value in 0.1 dB units. `0` = unity. Values `≤ -1440` are treated
    /// as `-∞ dB`. Decoded from the per-track `0x1029` (TrackMixSettings) block.
    pub volume_centibel: i32,
    /// `true` if the track is muted in PT's Mix window. Decoded from `0x1029` `+5`.
    pub mute: bool,
    /// `true` if the track is soloed in PT's Mix window.
    /// Decoded from `0x102d` `+162` (u8). Verified via RPP→PTX probe:
    /// `t.soloed()` produces `+162 = 0x01`; baseline `0x00`.
    /// See `docs/pt-field-map.md`.
    pub solo: bool,
    /// `true` if the track has solo-defeat set (track ignores other
    /// tracks' solo state — i.e. always plays even when something else
    /// is soloed).
    ///
    /// Decoded from `0x200b` `+268` (u8). Verified via RPP→PTX probe:
    /// `t.solo_defeated()` produces `+268 = 0x01`; baseline `0x00`.
    /// Mirror also at `0x200a +259`. See `docs/pt-field-map.md`.
    pub solo_defeat: bool,
    /// `true` if the track appears to be in PT's "Inactive"/bounced-source
    /// state.
    ///
    /// Heuristic: `(0x1029 +5 == 1) AND (0x260a[0] +8 == 1)`. PT sets
    /// the stored-mute bit AND keeps the send routing enabled for
    /// tracks that have been marked inactive (e.g. via "Make Inactive"
    /// menu or "Bounce to Disk → New Track"). Truly user-muted tracks
    /// have `+5=1 AND +8=0` (send cleared too).
    ///
    /// This is the COMPLEMENT of the `mute_resolver` filter: any
    /// track filtered OUT of `mute=true` because its `+8` was non-zero
    /// is reported here as `inactive`.
    ///
    /// Verified on LotF: the 12 tracks the converter reports as
    /// MUTESOLO 0 despite +5=1 (SYZ, AC GTR x2, El Gtr 1, Bass Demo,
    /// MIDI 1, Inst 1 + dups + .02 variants) all match this pattern.
    ///
    /// CAVEAT: ground-truth verification requires a PT-authored
    /// session that we know has explicit inactive tracks. Until then
    /// this is our best inference of PT's `PTXTrackStateSpec.active`.
    pub inactive: bool,
    /// Pan position. `-100` = full L, `0` = center, `+100` = full R.
    /// Decoded from `0x1029` `+13` (i32 LE).
    pub pan: i32,
    /// Alternate (inactive) playlists stored in the session.
    ///
    /// Empty unless the session was saved with alternate playlists.
    pub alternate_playlists: Vec<Playlist>,
    /// Name of the track's output assignment (e.g. `"Analog 1-2"`, `"Bus 1"`).
    ///
    /// Decoded from the per-track `0x260e` block, which carries a
    /// length-prefixed string at payload offset `+0x24..` naming either a
    /// hardware output or an internal bus. Empty when the track is
    /// unassigned or the block omits a destination (the 61-byte variant
    /// — payload begins `ff ff 01 01 ...`).
    pub output: String,
    /// Pro Tools track color palette index.
    ///
    /// Parsed from the `0x200b` (TrackAuxState) block: newer sessions frame
    /// it as `01 <idx> 00 01 00 03`; older sessions store it as an i16 at
    /// payload `+106` (`0xfffe` / -2 = no color, normalized to `0` here).
    /// `0` means "no color" / Reaper default.
    ///
    /// The index→RGB mapping is PT's genuine palette (recovered by sweeping
    /// every index 0..=255 through the official converter). See
    /// `daw_reaper::project_import::pt_color_to_rgb` and `PT_TRACK_PALETTE`.
    pub color_byte: u8,
    /// Track view height in pixels, parsed from the `0x200b` block. Pro Tools
    /// stores the height as a u16 right after the color field (new format:
    /// color-frame + 10; older format: after `c0 00 00 00 00 01`). The eight
    /// PT size presets are 16 (Micro), 23 (Mini), 43 (Small), 97 (Medium,
    /// default), 192 (Large), 300 (Jumbo), 600 (Extreme), 684 (Fit). Maps
    /// directly to Reaper's `TRACKHEIGHT`. `0` = not found / use default.
    pub height_px: u16,
    /// Volume-automation breakpoints decoded from this track's `0x260a[0]`
    /// (the first `0x260a` child of the per-track `0x260d` wrapper).
    ///
    /// Format: 22-byte header, 6-byte implicit breakpoint at +22, then
    /// `user_count` breakpoints at +28. Each is
    /// `u32 LE time_samples + i16 LE value_centibel`.
    pub volume_automation: Vec<VolumeAutomationBreakpoint>,
    /// Mute-automation breakpoints decoded from this track's `0x260a[1]`
    /// (the second `0x260a` under the per-track `0x260d` wrapper —
    /// position-paired by PT's track-list document order).
    ///
    /// Format: 28-byte header, then `N × 6` breakpoint bytes. Each
    /// breakpoint is `u32 LE time_samples + u8 muted (0/1) + u8 shape`.
    /// Header `+10` is total count (`1 + user_count`), `+16` is user
    /// count, `+4` is payload size. See `docs/pt-field-map.md` §
    /// "Additional probe findings — mute_envelope".
    ///
    /// Empty when the track has no automation envelope.
    pub mute_automation: Vec<MuteAutomationBreakpoint>,
    /// Pan-automation breakpoints decoded from `0x260d > 0x260c[0] >
    /// 0x260a[0]` (pan lives one level deeper than volume/mute, under a
    /// `0x260c` sub-wrapper). Same 6-byte layout as volume: `u32 LE
    /// time_samples + i16 LE value`, where `value` is pan × 100 (−100..100,
    /// left→right). Empty when the track has no pan envelope.
    pub pan_automation: Vec<PanAutomationBreakpoint>,
    /// Hierarchy / grouping marker decoded from `0x251a` payload byte
    /// (immediately after the length-prefixed track name).
    ///
    /// On the Routing Examples fixture this byte is `0x01` for every
    /// folder-parent track (nested folder hierarchies) and `0x00` for
    /// regular audio + leaf aux tracks. PT's `Master 1` is also `0x01`.
    ///
    /// On the Lord of the Fight session every track in the "02 LORD OF
    /// THE FIGHT" stem family ALSO reads `0x01`, even though those are
    /// flat audio tracks — suggesting the byte tracks "participates in a
    /// folder / stem-bus grouping" rather than strictly "is the container
    /// itself".
    ///
    /// `true` when this track is a FOLDER PARENT (a container that nests
    /// other tracks beneath it). Decoded from the per-track `0x210b` block's
    /// role byte at payload `+5`: `0x02` = folder parent. (`0x05` = Master,
    /// `0x07` = instrument, `0x00` = ordinary leaf.) See `folder_depth`.
    pub is_folder: bool,
    /// Nesting depth of this track in PT's folder hierarchy. `0` = top level.
    /// A folder parent has the same depth as a sibling leaf at its level; its
    /// children are at `depth + 1`.
    ///
    /// Reconstructed from the session's `0x210c` group block, which lists each
    /// folder's direct children by 6-byte UID (members may themselves be a
    /// nested group, encoding arbitrary depth). Tracks not in any group are
    /// depth `0`. When the session has no `0x210c` groups (PT users who only
    /// create flat "divider" tracks), every track is depth `0` and folders are
    /// not emitted — matching the official converter's behavior.
    pub folder_depth: u32,
    /// Zero-based position of this track in PT's on-screen track order,
    /// taken from the `0x2519`/`0x251a` track list (which interleaves audio,
    /// MIDI, master and folder/divider tracks in Edit-window order).
    ///
    /// This is PT's canonical display order — NOT the channel-map `index`
    /// (internal voice assignment) which scrambles the layout. Consumers
    /// should emit tracks sorted by this value to match the source session.
    /// `u32::MAX` when the track could not be matched to a `0x251a` entry, so
    /// unmatched tracks sink to the bottom while keeping their relative order.
    pub display_order: u32,
    /// Free-text track comment from PT's comment field, decoded from the
    /// per-track `0x261c > 0x200b > 0x200a > 0x2015` block (e.g. "Demo Vox
    /// STEM STRETCHED to 92 BPM", "U-47"). Empty when the track has no comment.
    pub comment: String,
    /// `true` for PT's Master track (`0x2519` kind byte `0x05`). It maps to
    /// REAPER's dedicated master track, not a regular track, so the converter
    /// must not emit it as a `<TRACK` (which would become a bus/folder parent).
    pub is_master: bool,
}

/// A single breakpoint in a volume-automation envelope.
///
/// Decoded from `0x260a[0]` per track. Six bytes per point:
/// `u32 LE time_samples + i16 LE value_centibel`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VolumeAutomationBreakpoint {
    /// Position in samples (at session sample rate).
    pub time_samples: u32,
    /// Volume in 0.1 dB units (centibel). `0` = unity; `-60` = -6 dB.
    pub value_centibel: i16,
}

/// A single breakpoint in a pan-automation envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanAutomationBreakpoint {
    /// Position in samples (at session sample rate).
    pub time_samples: u32,
    /// Pan × 100 (−100 = full left, 0 = center, 100 = full right).
    pub value: i16,
}

/// A single breakpoint in a mute-automation envelope.
///
/// Decoded from `0x260a[1]` per track. Six bytes per point:
/// `u32 LE time_samples + u8 muted + u8 shape`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MuteAutomationBreakpoint {
    /// Position in samples (at session sample rate).
    pub time_samples: u32,
    /// `true` = muted at this time, `false` = un-muted.
    pub muted: bool,
    /// Shape byte. `0` = step/square; values verified against REAPER
    /// `MUTEENV` Square shape only — non-zero shapes are passed
    /// through verbatim but their semantics are not yet decoded.
    pub shape: u8,
}

/// A fade region on a track.
///
/// Pro Tools stores fades as track entries marked with a fade flag (byte
/// `+46 == 0x01`) plus a parallel array of `0x262f` fade-definition blocks
/// holding the lengths and curve shape. The fade entry's `+4` field indexes
/// into that parallel array. See `docs/pt-fade-encoding.md` for the byte
/// layout.
#[derive(Debug, Clone, PartialEq)]
pub struct FadeRegion {
    /// Start position on the timeline, in samples at target rate.
    pub start_pos: u64,
    /// Fade-in length in samples. `0` for a pure fade-out entry.
    pub in_length: u64,
    /// Fade-out length in samples. `0` for a pure fade-in entry.
    /// When both `in_length` and `out_length` are non-zero this entry is a
    /// crossfade; symmetric if `in_length == out_length`.
    pub out_length: u64,
    /// Curve shape: `1` = linear, `2` = equal power, `3` = equal gain.
    pub shape: u8,
    /// Curve linearity flag (`0x262f` byte after `shape`): `0` = linear,
    /// non-zero = a non-linear (curved) fade. PT/the converter collapses all
    /// non-linear Reaper curves to a single value here, so only linear-vs-
    /// curved is recoverable (map non-linear → Bezier, like the official tool).
    pub curve: u8,
    /// Index into the per-session fade-definition list (`0x262f` blocks in
    /// document order). Useful for cross-referencing or debugging.
    pub fade_index: u32,
}

impl Track {
    /// True if this track has any alternate (comp) playlists in addition to the active one.
    pub fn has_alternate_playlists(&self) -> bool {
        !self.alternate_playlists.is_empty()
    }

    /// Total playlist count (active + alternates).
    pub fn playlist_count(&self) -> usize {
        1 + self.alternate_playlists.len()
    }
}

/// A region placed on a track.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackRegion {
    /// Index into either `audio_regions` or `midi_regions`.
    pub region_index: u16,
    /// Start position override (if the track assignment overrides the region's start).
    pub start_pos: u64,
    /// Per-clip flag from `0x1050 +53` (u8 boolean). Semantics not
    /// fully verified — observed value `1` on rare clips and `0` on
    /// most. Discovered via Frida byte-read trace on LotF.
    /// See `docs/converter-frida-discovered-offsets.md`.
    pub clip_flag_53: bool,
    /// Per-clip mute state from `0x104f +9` u8. `true` = muted clip.
    /// Verified via Frida byte-read trace: `clip_muted` probe
    /// (REAPER item with `.muted()` + real WAV source) produces
    /// `1` at this byte; `clip_with_wav` produces `0`.
    pub clip_muted: bool,
    /// Per-clip color palette index from the inner `0x104f` sub-block
    /// at payload `+16..+17` (i16 LE). `None` = no `0x104f` child
    /// present. `Some(-2)` = default/no color. Other values map to
    /// the same PT palette as `Track.color_byte`.
    ///
    /// Discovered via Frida byte-read trace; `0x104f` lives at start
    /// of `0x1050` payload. Field locations within `0x104f` partly
    /// verified (+25/+26 read as i16 LE `FE FF` = -2 for default).
    pub clip_color: Option<i16>,
    /// Lower bound (inclusive, in source-chunk ticks) for MIDI events from this
    /// placement. `clip_src` for a trimmed clip, `0` for a full-take clip. Audio
    /// placements set `0`.
    pub clip_lo_ticks: u64,
    /// Upper bound (exclusive, in source-chunk ticks) for MIDI events from this
    /// placement: the clip's trimmed end and/or the comp boundary (the next
    /// clip on the track). `u64::MAX` = play to take end. Audio sets `u64::MAX`.
    pub note_trim_ticks: u64,
}

/// A single constant-tempo segment on the session timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct TempoEvent {
    /// Position in ticks (relative to the session start, ZERO_TICKS-based).
    pub tick_start: u64,
    /// Position in samples at the session's target sample rate.
    pub sample_start: u64,
    /// Beats per minute.
    pub bpm: f64,
    /// Ticks per beat (960,000 in all observed sessions).
    pub ticks_per_beat: u64,
}

/// A time-signature change event.
#[derive(Debug, Clone, PartialEq)]
pub struct MeterEvent {
    /// Position in ticks (relative to session start).
    pub tick_start: u64,
    /// Position in samples at the session's target sample rate.
    pub sample_start: u64,
    /// Bar number where this meter begins (1-based).
    pub measure: u32,
    /// Time signature numerator (e.g. 6 in 6/8).
    pub numerator: u32,
    /// Time signature denominator (e.g. 8 in 6/8).
    pub denominator: u32,
}

/// A user-defined marker (Pro Tools Memory Location).
#[derive(Debug, Clone, PartialEq)]
pub struct Marker {
    /// Marker name.
    pub name: String,
    /// 1-based memory location number.
    pub number: u32,
    /// Position in ticks (relative to session start).
    pub tick_pos: u64,
    /// Position in samples at the session's target sample rate.
    pub sample_pos: u64,
    /// Marker color as RGB. `None` = default / uncolored. Decoded from
    /// the per-marker `0x4826` block at payload `+2..+8` (three u16 LE
    /// triplets, low byte = component).
    pub color_rgb: Option<(u8, u8, u8)>,
}

/// A plugin / insert registered in the session's global plugin list.
///
/// Pro Tools keeps a single session-wide list (block 0x1018) of all plugin
/// types used anywhere in the session.
#[derive(Debug, Clone, PartialEq)]
pub struct PluginEntry {
    /// Insert slot index within a track's insert chain (0-based).
    ///
    /// Note: the same slot index can appear on multiple tracks; this field
    /// identifies the position within a single track's chain, not a unique
    /// session-wide slot ID.
    pub slot_index: u8,
    /// Display name as shown in the Pro Tools insert selector.
    pub name: String,
    /// Manufacturer 4-character code (stored byte-reversed in the file).
    /// E.g., "Digi" for Digidesign/Avid, "Srdx" for SoundRadix.
    pub manufacturer_4cc: [u8; 4],
    /// Plugin type 4CC (e.g., "Rvrb" for reverb).
    pub type_4cc: [u8; 4],
    /// Plugin subtype / variant 4CC.
    pub subtype_4cc: [u8; 4],
    /// Number of input channels (1 = mono, 2 = stereo).
    pub input_channels: u8,
    /// Number of output channels (1 = mono, 2 = stereo).
    pub output_channels: u8,
    /// AAX bundle identifier, e.g. `"com.avid.aax.dverb"`.
    pub aax_bundle_id: String,
}

impl PluginEntry {
    /// Returns the manufacturer code as a printable ASCII string.
    pub fn manufacturer_name(&self) -> String {
        self.manufacturer_4cc
            .iter()
            .map(|&b| if b.is_ascii_graphic() { b as char } else { '?' })
            .collect()
    }
}

/// A hardware or bus I/O channel configured in the session's I/O setup.
#[derive(Debug, Clone, PartialEq)]
pub struct IoChannel {
    /// Channel display name (e.g., "Out 1-2", "MacBook Pro Speakers 1-2").
    pub name: String,
    /// I/O class: 0x01 = physical hardware interface, 0x02 = output bus.
    pub io_class: u8,
    /// Number of audio channels (1 = mono, 2 = stereo).
    pub channel_count: u8,
    /// 6-byte UID at `0x1021 +46`. Routing entries (`RoutingEntry`)
    /// reference channels by this UID via their `destination_uid`
    /// field — match by `IoChannel.uid == RoutingEntry.destination_uid`
    /// to resolve a routing assignment to a hardware channel.
    ///
    /// Discovered via Frida byte-read trace on LotF — the routing
    /// dest UIDs were found also occurring inside `0x1021` blocks
    /// at this offset.
    pub uid: Option<[u8; 6]>,
}

/// Strongly-typed view of [`IoChannel::io_class`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoClass {
    /// Physical hardware interface (`io_class == 0x01`).
    HardwareInterface,
    /// Output bus (`io_class == 0x02`).
    OutputBus,
    /// Any other / unknown I/O class.
    Other(u8),
}

impl IoChannel {
    /// Returns a strongly-typed view of [`Self::io_class`].
    pub fn class(&self) -> IoClass {
        match self.io_class {
            0x01 => IoClass::HardwareInterface,
            0x02 => IoClass::OutputBus,
            other => IoClass::Other(other),
        }
    }
}

impl ProToolsSession {
    /// Iterate over every track in the session, audio first then MIDI.
    pub fn all_tracks(&self) -> impl Iterator<Item = &Track> {
        self.audio_tracks.iter().chain(self.midi_tracks.iter())
    }

    /// Resolve a `RoutingEntry`'s destination UID to the matching
    /// `IoChannel`. Returns `None` if no channel has a matching UID
    /// (the entry may point at a bus or internal target not yet
    /// surfaced).
    ///
    /// Verified on LotF: routing entry[0] (`dest_uid=5ecac0f83a49`)
    /// resolves to `"Analog 1-2"`, entry[1] to `"Analog 3-4"`, etc.
    pub fn resolve_routing_destination(&self, entry: &RoutingEntry) -> Option<&IoChannel> {
        self.io_channels
            .iter()
            .find(|ch| ch.uid == Some(entry.destination_uid))
    }

    /// Iterate only the ACTIVE routing entries (those with
    /// `RoutingEntry.active == true`).
    pub fn active_routings(&self) -> impl Iterator<Item = &RoutingEntry> {
        self.routing_entries.iter().filter(|r| r.active)
    }

    /// Total active region placements across every audio + MIDI track.
    pub fn total_active_region_placements(&self) -> usize {
        self.all_tracks().map(|t| t.regions.len()).sum()
    }

    /// Total alternate-playlist count across every track.
    pub fn total_alternate_playlists(&self) -> usize {
        self.all_tracks().map(|t| t.alternate_playlists.len()).sum()
    }

    /// Look up an audio file by region.
    pub fn audio_file_for(&self, region: &AudioRegion) -> Option<&AudioFile> {
        self.audio_files
            .iter()
            .find(|f| f.index == region.audio_file_index)
    }

    /// Convert a sample position (at the session sample rate) to seconds.
    pub fn samples_to_seconds(&self, samples: u64) -> f64 {
        if self.session_sample_rate == 0 {
            return 0.0;
        }
        samples as f64 / self.session_sample_rate as f64
    }
}

/// The origin tick value for MIDI positions (10^12).
pub const ZERO_TICKS: u64 = 0xe8d4a51000;

/// The "no region assigned" sentinel value.
pub const NO_REGION: u16 = 0xFFFF;
