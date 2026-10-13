//! Chunk interpreter — builds a [`LogicSession`] from raw [`LogicChunk`]s.
//!
//! ## Arrangement position encoding
//!
//! `MSeq`, `Trak`, and `EvSq` chunks all store the arrangement clock position
//! in `header_meta[4..8]` (i32 LE). The tick unit is **65 536 ticks per beat**
//! (at the session BPM), so:
//!
//! ```text
//! beats = meta[4..8] as i32 / 65_536.0
//! ```
//!
//! This was empirically confirmed against the `FileDecrypt.logicx` fixture:
//! - MSeq "Untitled" at `meta[4]=0` -> bar 1 beat 1
//! - MSeq "FileDecrypt" at `meta[4]=262_144` -> bar 2 beat 1 (1 bar = 4x65_536)
//! - MSeq "Untitled" at `meta[4]=3_932_160` -> bar 16 beat 1 (15 bars = 60 beats)
//!
//! ## Clip type detection
//!
//! The Trak chunk immediately following each MSeq in file order determines the
//! clip type:
//! - `data_len == 0`  → MIDI region (events stored in the EvSq that follows)
//! - `data_len == 58` → Audio region (references the audio pool via GUID)
//!
//! ## Audio pool entries
//!
//! `AuFl` chunks name the source audio files; `AuRg` chunks describe the
//! source segment (start offset, duration in sample frames, region name).  The
//! AuFl/AuRg meta[4..8] value is a pool index (not an arrangement position).
//! Pool entries are matched to placed clips by name prefix heuristic.

use crate::parse::aufl::parse_aufl;
use crate::parse::aurg::parse_aurg;
use crate::parse::bundle::BundleMeta;
use crate::types::{
    ClipKind, LogicAudioFile, LogicChunk, LogicClip, LogicCompRange, LogicMarker, LogicMidiNote,
    LogicSession, LogicSummingGroup, LogicTake, LogicTakeFolder, LogicTempoEvent, LogicTrack,
    TrackKind,
};

/// Ticks per beat in the arrangement clock used by MSeq / Trak / EvSq headers.
const TICKS_PER_BEAT: f64 = 65_536.0;

/// On-disk tags (reversed from human-readable form).
const TAG_MSEQ: [u8; 4] = *b"qeSM";
const TAG_TRAK: [u8; 4] = *b"karT";
const TAG_EVSQ: [u8; 4] = *b"qSvE";
const TAG_AUFL: [u8; 4] = *b"lFuA";
const TAG_AURG: [u8; 4] = *b"gRuA";
const TAG_ENVI: [u8; 4] = *b"ivnE";

/// EvSq event sentinel: bytes[4..8] == 0x88000000 marks a non-positioned event.
const EVSQ_SENTINEL: u32 = 0x8800_0000;
/// EvSq end-of-sequence marker type byte.
const EVSQ_END: u8 = 0xf1;

// ── Public entry point ────────────────────────────────────────────────────────

/// Build a [`LogicSession`] from parsed bundle metadata and the flat chunk list.
pub fn build_session(meta: BundleMeta, chunks: Vec<LogicChunk>) -> LogicSession {
    let (mut tracks, summing_groups) = extract_tracks_and_groups(&chunks);

    // Walk the MSeq → Trak → EvSq groups to produce clips.
    let clips = collect_clips(&chunks, meta.sample_rate, meta.bpm);

    // Attach clips to tracks by name matching.
    attach_clips_to_tracks(&mut tracks, clips);

    // Audio file pool (one LogicAudioFile per AuFl chunk).
    let audio_files: Vec<LogicAudioFile> = chunks
        .iter()
        .filter(|c| c.tag == TAG_AUFL)
        .filter_map(|c| parse_aufl(&c.data))
        .map(|f| LogicAudioFile {
            filename: f.filename,
            vol_name: f.vol_name,
            usable: f.usable,
        })
        .collect();

    LogicSession {
        creator_version: meta.creator_version,
        variant_name: meta.variant_name,
        sample_rate: meta.sample_rate,
        bpm: meta.bpm,
        time_sig_numerator: meta.time_sig_numerator,
        time_sig_denominator: meta.time_sig_denominator,
        key: meta.key,
        key_gender: meta.key_gender,
        tracks,
        markers: Vec::new(),      // TODO: parse from EvSq (RE pending)
        tempo_events: Vec::new(), // TODO: parse from EvSq (RE pending)
        summing_groups,
        audio_files,
        chunks,
    }
}

// ── MSeq / Trak / EvSq clip extraction ───────────────────────────────────────

/// A clip gathered from a MSeq→Trak→EvSq group, ready to attach to a track.
struct PendingClip {
    /// User-visible clip name (from MSeq payload).
    name: String,
    /// Arrangement start in beats (from MSeq header_meta[4..8]).
    position_beats: f64,
    /// Clip content.
    kind: ClipKind,
    /// Duration in beats (from AuRg for audio; from EvSq for MIDI).
    length_beats: f64,
}

/// Walk the chunk list and extract all placed clips (audio + MIDI).
fn collect_clips(chunks: &[LogicChunk], sample_rate: u32, bpm: f64) -> Vec<PendingClip> {
    // Build audio pool for matching clip names to source files.
    let audio_pool = build_audio_pool(chunks, sample_rate, bpm);

    let mut clips = Vec::new();
    let mut i = 0;

    while i < chunks.len() {
        if chunks[i].tag != TAG_MSEQ {
            i += 1;
            continue;
        }

        let mseq = &chunks[i];
        let position_beats = mseq_position_beats(mseq);
        let name = mseq_name(&mseq.data);

        // Skip system / internal MSeq names.
        if is_system_mseq_name(&name) {
            i += 1;
            continue;
        }

        // Look ahead to find Trak and EvSq.
        let mut j = i + 1;
        let mut has_audio_trak = false;
        let mut evsq_idx = None;

        while j < chunks.len() {
            match chunks[j].tag {
                TAG_TRAK
                    // 58 bytes = FileDecrypt/Logic ≤11.x; 57 bytes = Fire/Logic 12.x.
                    if chunks[j].data_len >= 57 => {
                        has_audio_trak = true;
                    }
                TAG_EVSQ => {
                    evsq_idx = Some(j);
                    break;
                }
                TAG_MSEQ => break,
                _ => {}
            }
            j += 1;
        }

        if has_audio_trak {
            // Audio clip — match against the audio pool by name prefix.
            if let Some(pending) = make_audio_clip(&name, position_beats, &audio_pool) {
                clips.push(pending);
            }
        } else {
            // MIDI clip — parse notes from the EvSq.
            let notes = evsq_idx
                .map(|idx| parse_evsq_notes(&chunks[idx].data))
                .unwrap_or_default();

            let length_beats = estimate_midi_length(&notes);

            clips.push(PendingClip {
                name: name.clone(),
                position_beats,
                kind: ClipKind::Midi { notes },
                length_beats,
            });
        }

        // Advance past the EvSq (or past the next MSeq boundary).
        i = if let Some(idx) = evsq_idx { idx + 1 } else { j };
    }

    clips
}

/// Read the arrangement clock position (beats) from a MSeq/Trak/EvSq chunk.
///
/// `header_meta[4..8]` is i32 LE in units of [`TICKS_PER_BEAT`].
fn mseq_position_beats(chunk: &LogicChunk) -> f64 {
    let ticks = i32::from_le_bytes(chunk.header_meta[4..8].try_into().unwrap_or([0; 4]));
    ticks as f64 / TICKS_PER_BEAT
}

/// Extract the user-visible name from a MSeq payload.
///
/// Layout: bytes 0–7 (flags), 8–15 (track index i64 LE),
///         16–17 (name length u16 LE), 18+ (UTF-8 name).
fn mseq_name(payload: &[u8]) -> String {
    if payload.len() < 18 {
        return String::new();
    }
    let name_len = u16::from_le_bytes([payload[16], payload[17]]) as usize;
    let name_end = 18 + name_len;
    if payload.len() < name_end {
        return String::new();
    }
    String::from_utf8_lossy(&payload[18..name_end]).into_owned()
}

/// Returns true for Logic-internal MSeq names that should not become clips.
fn is_system_mseq_name(name: &str) -> bool {
    matches!(
        name,
        "Untitled"
            | "TRASH"
            | "Track Automation Root Folder"
            | "Track Alternatives"
            | "Global Harmonies"
            | "*Automation"
    ) || name.is_empty()
}

// ── EvSq / MIDI note parsing ─────────────────────────────────────────────────

/// Parse MIDI-like note events from an EvSq payload.
///
/// Each record is 16 bytes:
/// - byte 0:    event type
/// - bytes 4–7: arrangement position as i32 LE ticks (or 0x88000000 = sentinel)
/// - bytes 8–15: event-specific data (note, velocity, duration, etc.)
///
/// We extract positioned events (non-sentinel, non-end) as [`LogicMidiNote`]
/// records.  The tick unit inside EvSq differs from the MSeq arrangement unit
/// and is not yet fully characterised.
fn parse_evsq_notes(data: &[u8]) -> Vec<LogicMidiNote> {
    let mut notes = Vec::new();

    for record in data.chunks_exact(16) {
        let ev_type = record[0];
        if ev_type == EVSQ_END {
            break;
        }

        // Skip sentinel (non-positioned) records.
        let pos_raw = u32::from_le_bytes(record[4..8].try_into().unwrap());
        if pos_raw == EVSQ_SENTINEL {
            continue;
        }

        let position_ticks = i32::from_le_bytes(record[4..8].try_into().unwrap());

        // Best-effort decode of note data from bytes 8–15.
        // Logic encodes pitch/velocity/channel here; exact byte positions vary
        // by event type.  We extract byte[11] as pitch and byte[12] as velocity
        // based on observed patterns for type 0x30 events.
        let pitch = record[11];
        let velocity = record[12];
        let channel = 0u8; // channel encoding not yet decoded

        notes.push(LogicMidiNote {
            event_type: ev_type,
            position_ticks,
            pitch,
            velocity,
            channel,
            raw_data: record[8..16].try_into().unwrap_or([0; 8]),
        });
    }

    notes
}

/// Estimate MIDI clip length from the last note position (rough heuristic).
fn estimate_midi_length(notes: &[LogicMidiNote]) -> f64 {
    let max_ticks = notes.iter().map(|n| n.position_ticks).max().unwrap_or(0);
    // EvSq ticks are not the same as MSeq ticks; store as 0 until decoded.
    let _ = max_ticks;
    0.0
}

// ── Audio pool ────────────────────────────────────────────────────────────────

struct AudioPoolEntry {
    name: String,              // AuRg region name (e.g. "Audio Track 1 #01")
    track_name_hint: String,   // prefix before " #NN"
    take_number: u8,           // 0 = comp result, ≥1 = recorded take
    take_number_on_lane: u8,   // 0-based UI lane index within take folder
    source_offset_frames: i32, // source file start in sample frames
    file_path: Option<String>,
    duration_beats: f64,
    comp_start_ticks: i64, // non-zero only for take_number ≥ 1
    comp_end_ticks: i64,
    uuid: Option<[u8; 16]>,           // RFC 4122 UUID for this region
    offset_to_take_folder_start: i32, // -1 = not in a take folder
}

fn build_audio_pool(chunks: &[LogicChunk], sample_rate: u32, bpm: f64) -> Vec<AudioPoolEntry> {
    let sr = sample_rate.max(1) as f64;
    let bpm = bpm.max(1.0);

    // Map filename → AuFl entry for file-path decoration.
    let aufl_entries: Vec<_> = chunks
        .iter()
        .filter(|c| c.tag == TAG_AUFL)
        .filter_map(|c| parse_aufl(&c.data))
        .collect();

    chunks
        .iter()
        .filter(|c| c.tag == TAG_AURG)
        .filter_map(|c| {
            let region = parse_aurg(&c.data)?;

            let duration_beats = region.duration_frames as f64 / sr / 60.0 * bpm;

            let file_path = aufl_entries
                .iter()
                .find(|f| {
                    region.name.starts_with(
                        &f.filename[..f.filename.rfind('.').unwrap_or(f.filename.len())],
                    )
                })
                .map(|f| {
                    if f.vol_name.is_empty() {
                        f.filename.clone()
                    } else {
                        format!("{}/{}", f.vol_name, f.filename)
                    }
                });

            let track_name_hint = if let Some(pos) = region.name.rfind(" #") {
                region.name[..pos].to_owned()
            } else {
                region.name.clone()
            };

            Some(AudioPoolEntry {
                name: region.name,
                track_name_hint,
                take_number: region.take_number,
                take_number_on_lane: region.take_number_on_lane,
                source_offset_frames: region.source_offset_frames,
                file_path,
                duration_beats,
                comp_start_ticks: region.start_ticks,
                comp_end_ticks: region.end_ticks,
                uuid: region.uuid,
                offset_to_take_folder_start: region.offset_to_take_folder_start,
            })
        })
        .collect()
}

/// Try to match an audio MSeq name to a pool entry and build a clip.
///
/// If the pool contains any `take_number ≥ 1` entries whose name or
/// `track_name_hint` matches `clip_name`, this clip is a **Take Folder** and
/// the function returns `ClipKind::TakeFolder`.  Otherwise it returns a plain
/// `ClipKind::Audio`.
fn make_audio_clip(
    clip_name: &str,
    position_beats: f64,
    pool: &[AudioPoolEntry],
) -> Option<PendingClip> {
    // Collect all pool entries that match this clip name (exact or hint).
    let matching: Vec<&AudioPoolEntry> = pool
        .iter()
        .filter(|e| e.name == clip_name || e.track_name_hint == clip_name)
        .collect();

    // If any matching entries are recorded takes, build a Take Folder.
    let has_takes = matching.iter().any(|e| e.take_number >= 1);

    if has_takes {
        return Some(make_take_folder_clip(clip_name, position_beats, &matching));
    }

    // Single audio clip — prefer exact name match, fall back to hint match.
    let entry = matching.into_iter().next();

    Some(PendingClip {
        name: clip_name.to_owned(),
        position_beats,
        kind: ClipKind::Audio {
            file_path: entry.and_then(|e| e.file_path.clone()),
        },
        length_beats: entry.map(|e| e.duration_beats).unwrap_or(0.0),
    })
}

/// Build a `PendingClip` with `ClipKind::TakeFolder` from a set of matching pool entries.
fn make_take_folder_clip(
    clip_name: &str,
    position_beats: f64,
    matching: &[&AudioPoolEntry],
) -> PendingClip {
    // Takes: all entries with take_number ≥ 1, deduplicated by take number.
    let mut takes: Vec<LogicTake> = {
        let mut seen = std::collections::HashSet::new();
        matching
            .iter()
            .filter(|e| e.take_number >= 1)
            .filter(|e| seen.insert(e.take_number))
            .map(|e| LogicTake {
                number: e.take_number,
                duration_beats: e.duration_beats,
                source_offset_frames: e.source_offset_frames,
                file_path: e.file_path.clone(),
            })
            .collect()
    };
    takes.sort_by_key(|t| t.number);

    // Comp ranges: take entries with non-zero clock spans define the active selection.
    let mut comp_ranges: Vec<LogicCompRange> = matching
        .iter()
        .filter(|e| e.take_number >= 1 && (e.comp_start_ticks != 0 || e.comp_end_ticks != 0))
        .map(|e| LogicCompRange {
            take_number: e.take_number,
            comp_start_ticks: e.comp_start_ticks,
            comp_end_ticks: e.comp_end_ticks,
        })
        .collect();
    comp_ranges.sort_by_key(|r| r.comp_start_ticks);

    // Folder length: from the take_number=0 comp-result entry, or max of takes.
    let length_beats = matching
        .iter()
        .find(|e| e.take_number == 0)
        .map(|e| e.duration_beats)
        .unwrap_or_else(|| {
            matching
                .iter()
                .map(|e| e.duration_beats)
                .fold(0.0_f64, f64::max)
        });

    PendingClip {
        name: clip_name.to_owned(),
        position_beats,
        length_beats,
        kind: ClipKind::TakeFolder(LogicTakeFolder { takes, comp_ranges }),
    }
}

// ── Clip → Track attachment ───────────────────────────────────────────────────

fn attach_clips_to_tracks(tracks: &mut [LogicTrack], clips: Vec<PendingClip>) {
    for pending in clips {
        let track_name = pending.name.clone();

        let clip = LogicClip {
            name: pending.name.clone(),
            position_beats: pending.position_beats,
            length_beats: pending.length_beats,
            kind: pending.kind,
        };

        if let Some(track) = tracks.iter_mut().find(|t| t.name == track_name) {
            track.clips.push(clip);
        }
        // If no matching track, the clip is dropped — we don't yet have the
        // full track↔region mapping from the binary.
    }
}

// ── Envi chunk parsing ────────────────────────────────────────────────────────
//
// `ivnE` (Envi) chunks represent mixer objects: tracks, buses, aux sends,
// the master, etc.  Each chunk carries a name string that is the user-visible
// label for that mixer channel.

fn extract_tracks_and_groups(chunks: &[LogicChunk]) -> (Vec<LogicTrack>, Vec<LogicSummingGroup>) {
    let mut tracks = Vec::new();
    let mut summing_groups = Vec::new();

    for chunk in chunks {
        if chunk.tag != TAG_ENVI {
            continue;
        }

        let name = extract_name_from_envi(&chunk.data);
        let fader_db = extract_fader_from_envi(&chunk.data);
        let kind = classify_envi_channel(&name);

        match kind {
            EnviKind::SummingGroup { child_hint } => {
                summing_groups.push(LogicSummingGroup {
                    name,
                    member_names: child_hint.into_iter().collect(),
                });
            }
            EnviKind::SystemChannel => {}
            EnviKind::Track(track_kind) => {
                tracks.push(LogicTrack {
                    name,
                    kind: track_kind,
                    channel: 0,
                    fader_db,
                    muted: false,
                    soloed: false,
                    parent_group: None,
                    clips: Vec::new(),
                });
            }
        }
    }

    tracks.retain(|t| !t.name.is_empty());

    // Assign 1-based channel numbers in display order.
    for (i, t) in tracks.iter_mut().enumerate() {
        t.channel = (i + 1) as u32;
    }

    (tracks, summing_groups)
}

#[derive(Debug)]
enum EnviKind {
    Track(TrackKind),
    SummingGroup { child_hint: Option<String> },
    SystemChannel,
}

fn classify_envi_channel(name: &str) -> EnviKind {
    let lower = name.to_ascii_lowercase();

    if matches!(
        lower.as_str(),
        "stereo out"
            | "master"
            | "click"
            | "preview"
            | "not assigned"
            | "midi click"
            | "sequencer input"
            | "physical input"
            | "input view"
            | "input notes"
    ) {
        return EnviKind::SystemChannel;
    }

    // Logic exposes folder tracks via Envi entries named exactly "(Folder)" or
    // "Folder". Surface them as Folder kind so consumers can build hierarchy.
    if lower == "folder" || lower == "(folder)" {
        return EnviKind::Track(TrackKind::Folder);
    }

    if lower.contains("sum") || lower.contains("group") || lower.contains("indented") {
        return EnviKind::SummingGroup { child_hint: None };
    }

    if lower.contains("audio") {
        return EnviKind::Track(TrackKind::Audio);
    }
    if lower.contains("midi") {
        return EnviKind::Track(TrackKind::Midi);
    }
    if lower.contains("aux") || lower.contains("bus") || lower.contains("send") {
        return EnviKind::Track(TrackKind::Aux);
    }

    EnviKind::Track(TrackKind::Audio)
}

/// Extract the fader level (dB) from an `ivnE` (Envi) payload.
///
/// The fader raw value is a u8 at offset 0x52 in user-track Envi chunks
/// (empirically confirmed from Fire.logicx hex dumps; byte 0x51 = 0x11 is a
/// preceding parameter marker).
///
/// Scale (0–255 logarithmic, reference at 128 = 0 dB):
/// - `0`   → silence (returns `None`)
/// - `128` → 0.0 dB (unity gain)
/// - `255` → ~+6.0 dB (maximum)
///
/// Confirmed against Fire.logicx:
/// - "Kick" raw=61 → −6.4 dB (quiet drum)
/// - "Snare" raw=127 → −0.1 dB (near unity)
/// - "PLUCK - Mau5" raw=217 → +4.6 dB (loud lead)
/// - "Kick Sidechain" raw=249 → +5.8 dB (near-max sidechain send)
///
/// Returns `None` for system-channel chunks (data_len < 0x53) or silence (raw=0).
fn extract_fader_from_envi(data: &[u8]) -> Option<f64> {
    const FADER_MARKER_OFFSET: usize = 0x51;
    const FADER_VALUE_OFFSET: usize = 0x52;
    const FADER_MARKER: u8 = 0x11;

    if data.len() <= FADER_VALUE_OFFSET {
        return None;
    }
    // Validate the marker byte to avoid false matches in system-channel chunks.
    if data[FADER_MARKER_OFFSET] != FADER_MARKER {
        return None;
    }

    let raw = data[FADER_VALUE_OFFSET];
    if raw == 0 {
        return None; // silence / -infinity
    }

    // Logarithmic scale: 128 = 0 dB, 255 ≈ +6 dB, 64 ≈ −6 dB.
    let db = 20.0 * (raw as f64 / 128.0).log10();
    Some(db)
}

/// Extract the user-visible name from an `ivnE` (Envi) payload.
///
/// ## Layout (user-track Envi chunks, data_len ~467–491)
///
/// Empirically confirmed from Fire.logicx and FileDecrypt.logicx hex dumps:
///
/// | Offset | Size | Field          |
/// |--------|------|----------------|
/// | 0x51   | 1    | param marker (0x11) |
/// | 0x52   | 1    | fader_raw (0–127)   |
/// | 0x9E   | 1    | name_length    |
/// | 0x9F   | 1    | (padding/zero) |
/// | 0xA0   | N    | name bytes (ASCII, NOT null-terminated) |
///
/// The byte immediately after the name is the first byte of the next field and
/// may be any value — including printable ASCII — so we must use name_length to
/// know exactly where the name ends.
///
/// System-channel Envi chunks (data_len < 0xA1) use a different layout and
/// fall back to a longest-ASCII-run scan.
fn extract_name_from_envi(data: &[u8]) -> String {
    const NAME_LEN_OFFSET: usize = 0x9E; // 158
    const NAME_START: usize = 0xA0; // 160

    // Primary strategy: fixed-offset length-prefixed string.
    if data.len() > NAME_LEN_OFFSET {
        let name_len = data[NAME_LEN_OFFSET] as usize;
        let name_end = NAME_START + name_len;
        if name_len > 0 && name_len < 128 && name_end <= data.len() {
            let candidate = &data[NAME_START..name_end];
            if candidate.iter().all(|&b| (0x20..=0x7e).contains(&b)) {
                return String::from_utf8_lossy(candidate).into_owned();
            }
        }
    }

    // Fallback for system-channel chunks (shorter data, different layout):
    // scan for the longest printable-ASCII run of 3–64 bytes.
    let mut best: &[u8] = &[];
    let mut i = 0;
    while i < data.len() {
        if data[i] >= 0x20 && data[i] <= 0x7e {
            let start = i;
            while i < data.len() && data[i] >= 0x20 && data[i] <= 0x7e {
                i += 1;
            }
            let run = &data[start..i];
            if run.len() >= 3 && run.len() > best.len() && run.len() <= 64 {
                best = run;
            }
        } else {
            i += 1;
        }
    }

    String::from_utf8_lossy(best).into_owned()
}
