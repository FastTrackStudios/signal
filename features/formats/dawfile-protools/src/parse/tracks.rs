//! Audio track parsing and region-to-track assignment.
//!
//! Tracks are parsed from block 0x1015 (audio track list). Region-to-track
//! assignments come from blocks 0x1012 (old, PT 5-7) or 0x1054 (new, PT 8+).
//!
//! ## Playlist model
//!
//! The top-level `0x1054` block holds ALL playlists (active and alternate) as a
//! flat list of `0x1052` children, in the same order as the corresponding
//! `0x1014` entries in `0x1015`. Each `0x1014` / `0x1052` pair is one playlist
//! (which may be a multi-channel stereo track).
//!
//! To recover the "track with alternates" model:
//! - Strip the version suffix (`.01`, `.02`, …) from each playlist name → base name.
//! - Group by `(base_name, channel_position_within_0x1014_block)`.
//! - The **first** `0x1014` entry for each base name = the active playlist.
//! - Subsequent entries for the same base name = alternate playlists.
//! - Stereo channels are distinguished by their channel position (0 = L, 1 = R),
//!   so the two channels of a stereo alternate are handled independently.

use crate::block::Block;
use crate::content_type::ContentType;
use crate::cursor::Cursor;
use crate::parse::tempo::{TempoSegment, tick_to_sample};
use crate::types::{
    AudioRegion, FadeRegion, NO_REGION, Playlist, Track, TrackKind, TrackRegion, ZERO_TICKS,
};

/// Internal track entry that carries the channel position used for grouping.
struct TrackEntry {
    track: Track,
    /// Zero-based channel index within its `0x1014` block (0 for mono, 0/1 for stereo).
    channel_pos: usize,
}

/// Parse audio tracks and their region assignments (active + alternate playlists).
pub fn parse_audio_tracks(
    blocks: &[Block],
    cursor: &Cursor<'_>,
    regions: &[AudioRegion],
    version: u16,
    rate_factor: f64,
    tempo_map: &[TempoSegment],
    target_sample_rate: u32,
) -> Vec<Track> {
    // Step 1: Parse track definitions from 0x1015
    let mut entries = parse_track_definitions(blocks, cursor);

    // Step 2: Assign regions from 0x1054 (or 0x1012 for old format)
    if version < 8 {
        let mut tracks: Vec<Track> = entries.into_iter().map(|e| e.track).collect();
        assign_regions_old(blocks, cursor, regions, &mut tracks);
        tracks.retain(|t| !t.regions.is_empty());
        tracks.sort_by_key(|t| t.index);
        for (i, track) in tracks.iter_mut().enumerate() {
            track.index = i as u16;
        }
        return tracks;
    }

    // Collect every fade-definition block (0x262f) in document order so each
    // fade entry's `+4` index can be resolved to a real (in_len, out_len, shape).
    // See `docs/pt-fade-encoding.md`.
    let fade_defs = collect_fade_defs(blocks);

    assign_regions_new(
        blocks,
        cursor,
        regions,
        &mut entries,
        rate_factor,
        tempo_map,
        target_sample_rate,
        &fade_defs,
    );

    // Keep all defined tracks, including empty ones (no regions). PT
    // sessions used as palette tests (Color Testing.ptx) or routing-only
    // skeletons have legitimate empty tracks that still carry color,
    // routing, and mix state. Filtering them dropped 72/72 tracks on
    // Color Testing.ptx. If a downstream caller wants only tracks with
    // content, they can filter on `regions.is_empty()` themselves.

    // Step 4: Group alternates under their primary track
    let mut tracks = group_alternate_playlists(entries);

    // Step 5: Sort by index and renumber
    tracks.sort_by_key(|t| t.index);
    for (i, track) in tracks.iter_mut().enumerate() {
        track.index = i as u16;
    }

    tracks
}

// ── Track definitions ──────────────────────────────────────────────────────

/// Parse track definitions from the AudioTrackList block (0x1015).
///
/// Returns one `TrackEntry` per channel, with `channel_pos` tracking which
/// channel within its `0x1014` block the entry came from (for stereo grouping).
fn parse_track_definitions(blocks: &[Block], cursor: &Cursor<'_>) -> Vec<TrackEntry> {
    let mut entries = Vec::new();

    let track_list = match find_block_recursive(blocks, ContentType::AudioTrackList) {
        Some(b) => b,
        None => return entries,
    };

    for child in track_list.find_children(ContentType::AudioTrackInfo) {
        let data = cursor.data();
        let name_offset = child.offset + 2;
        if name_offset + 4 >= data.len() {
            continue;
        }

        let (name, str_consumed) = cursor.length_prefixed_string(name_offset);

        // Number of channels is a u32 after name + 1 separator byte.
        let nch_offset = name_offset + str_consumed + 1;
        if nch_offset + 4 >= data.len() {
            continue;
        }
        let nch = cursor.u32_at(nch_offset) as usize;

        // Read channel map indices (nch × u16)
        let ch_offset = nch_offset + 4;
        for ch in 0..nch {
            let idx_offset = ch_offset + ch * 2;
            if idx_offset + 2 > data.len() {
                break;
            }
            let track_index = cursor.u16_at(idx_offset);

            entries.push(TrackEntry {
                track: Track {
                    name: name.clone(),
                    kind: TrackKind::Audio,
                    index: track_index,
                    playlist_name: String::new(),
                    regions: Vec::new(),
                    fades: Vec::new(),
                    volume_centibel: 0,
                    mute: false,
                    solo: false,
                    solo_defeat: false,
                    inactive: false,
                    pan: 0,
                    alternate_playlists: Vec::new(),
                    output: String::new(),
                    color_byte: 0,
                    height_px: 0,
                    volume_automation: Vec::new(),
                    mute_automation: Vec::new(),
                    pan_automation: Vec::new(),
                    is_folder: false,
                    folder_depth: 0,
                    display_order: u32::MAX,
                    comment: String::new(),
                    is_master: false,
                },
                channel_pos: ch,
            });
        }
    }

    entries
}

// ── Active playlist assignment ─────────────────────────────────────────────

/// Assign regions to tracks using the old format (block 0x1012, PT 5-7).
fn assign_regions_old(
    blocks: &[Block],
    cursor: &Cursor<'_>,
    regions: &[AudioRegion],
    tracks: &mut [Track],
) {
    let map_block = match find_block_recursive(blocks, ContentType::AudioRegionTrackMap) {
        Some(b) => b,
        None => return,
    };

    let entries = map_block.find_all(ContentType::RegionTrackEntry);
    let mut track_idx = 0;

    for entry in entries {
        if track_idx >= tracks.len() {
            break;
        }

        let data = cursor.data();
        let raw_offset = entry.offset + 4;
        if raw_offset + 4 > data.len() {
            continue;
        }

        let raw_index = cursor.u32_at(raw_offset) as u16;

        if raw_index == NO_REGION {
            track_idx += 1;
            continue;
        }

        if let Some(region) = regions.iter().find(|r| r.index == raw_index) {
            tracks[track_idx].regions.push(TrackRegion {
                region_index: raw_index,
                start_pos: region.start_pos,
                clip_flag_53: false,
                clip_muted: false,
                clip_color: None,
                clip_lo_ticks: 0,
                note_trim_ticks: u64::MAX,
            });
        }

        track_idx += 1;
    }
}

/// Assign regions from the top-level 0x1054 block.
///
/// All `0x1052` entries map 1:1 to `entries` by position — both active and
/// alternate playlists are in the same flat list. Grouping into alternates
/// happens later in `group_alternate_playlists`.
#[allow(clippy::too_many_arguments)]
fn assign_regions_new(
    blocks: &[Block],
    cursor: &Cursor<'_>,
    regions: &[AudioRegion],
    entries: &mut [TrackEntry],
    rate_factor: f64,
    tempo_map: &[TempoSegment],
    target_sample_rate: u32,
    fade_defs: &[&Block],
) {
    let map_block = match find_top_level_block(blocks, ContentType::AudioRegionTrackMapNew) {
        Some(b) => b,
        None => return,
    };

    let map_entries = map_block.find_all(ContentType::AudioRegionTrackMapEntriesNew);
    let data = cursor.data();

    for (idx, map_entry) in map_entries.iter().enumerate() {
        if idx >= entries.len() {
            break;
        }

        let playlist_name = {
            let no = map_entry.offset + 2;
            if no + 4 < data.len() {
                let (n, _) = cursor.length_prefixed_string(no);
                n
            } else {
                String::new()
            }
        };

        let (slot_regions, slot_fades) = collect_slot_regions(
            map_entry,
            cursor,
            regions,
            rate_factor,
            tempo_map,
            target_sample_rate,
            fade_defs,
        );
        entries[idx].track.playlist_name = playlist_name;
        entries[idx].track.regions.extend(slot_regions);
        entries[idx].track.fades.extend(slot_fades);
    }
}

/// Collect all region placements from a single 0x1052 block.
///
/// Returns `(main_regions, fades)`. Fade entries — track entries whose byte at
/// offset +46 is `0x01` — are collected separately along with the referenced
/// audio region's length, which empirically equals the fade duration when the
/// referenced region is a PT-generated fade clip.
fn collect_slot_regions(
    map_entry: &Block,
    cursor: &Cursor<'_>,
    regions: &[AudioRegion],
    rate_factor: f64,
    tempo_map: &[TempoSegment],
    target_sample_rate: u32,
    fade_defs: &[&Block],
) -> (Vec<TrackRegion>, Vec<FadeRegion>) {
    let data = cursor.data();
    let mut slot_regions = Vec::new();
    let mut slot_fades = Vec::new();

    for track_entry in &map_entry.find_all(ContentType::AudioRegionTrackEntryNew) {
        let is_fade =
            track_entry.offset + 47 <= data.len() && cursor.u8_at(track_entry.offset + 46) == 0x01;

        // Per-clip flag at 0x1050 +53 (u8). Discovered via Frida.
        // 0x1050 is the parent (the track_entry itself).
        // `track_entry.offset` points at content_type (= magic + 7),
        // so +53 from offset = +46 from payload start in our parser's
        // terms = file offset block.offset.saturating_sub(7) + 53.
        let clip_flag_53 = {
            let magic = track_entry.offset.saturating_sub(7);
            data.get(magic + 53).copied().unwrap_or(0) != 0
        };
        // Per-clip state from inner 0x104f sub-block. Verified via
        // Frida probe-and-diff:
        //   - clip mute at `0x104f +9` u8
        //   - clip color (palette index) at `0x104f +25..+26` i16 LE
        let (clip_color, clip_muted) = {
            let mut color: Option<i16> = None;
            let mut muted = false;
            for child in &track_entry.children {
                if child.content_type_raw == 0x104f {
                    let magic = child.offset.saturating_sub(7);
                    if magic + 27 <= data.len() {
                        muted = data[magic + 9] != 0;
                        color = Some(i16::from_le_bytes([data[magic + 25], data[magic + 26]]));
                    }
                    break;
                }
            }
            (color, muted)
        };

        for sub_entry in &track_entry.find_all(ContentType::AudioRegionTrackSubEntryNew) {
            let raw_offset = sub_entry.offset + 4;
            if raw_offset + 4 > data.len() {
                continue;
            }
            let raw_index_u32 = cursor.u32_at(raw_offset);
            let raw_index = raw_index_u32 as u16;
            if !is_fade && raw_index == NO_REGION {
                continue;
            }

            let start_offset = sub_entry.offset + 9;
            // Byte at offset+16 distinguishes position format:
            //   0x00 → u32 sample position at offset+9 (normal audio tracks)
            //   0x40 → u40 tick position at offset+9 (instrument/MIDI-derived tracks)
            let is_tick_based =
                sub_entry.offset + 17 <= data.len() && cursor.u8_at(sub_entry.offset + 16) == 0x40;

            let start = if is_tick_based {
                if start_offset + 5 <= data.len() {
                    let tick_pos = cursor.u40_le(start_offset);
                    let relative_tick = tick_pos.saturating_sub(ZERO_TICKS);
                    tick_to_sample(relative_tick, tempo_map, target_sample_rate)
                } else {
                    0
                }
            } else if start_offset + 4 <= data.len() {
                let raw_start = cursor.u32_at(start_offset) as u64;
                (raw_start as f64 * rate_factor) as u64
            } else if let Some(r) = regions.iter().find(|r| r.index == raw_index) {
                r.start_pos
            } else {
                0
            };

            if is_fade {
                // The fade entry's `+4..+7` field is an index into the
                // session-wide fade-definition list (0x262f blocks), NOT into
                // `audio_regions`. Look it up and decode the lengths + shape.
                let fade_index = raw_index_u32;
                let (in_length, out_length, shape, curve) = fade_defs
                    .get(fade_index as usize)
                    .and_then(|def| decode_fade_def(def, data))
                    .unwrap_or((0, 0, 1, 0));

                slot_fades.push(FadeRegion {
                    start_pos: start,
                    in_length,
                    out_length,
                    shape,
                    curve,
                    fade_index,
                });
            } else {
                slot_regions.push(TrackRegion {
                    region_index: raw_index,
                    start_pos: start,
                    clip_flag_53,
                    clip_color,
                    clip_muted,
                    clip_lo_ticks: 0,
                    note_trim_ticks: u64::MAX,
                });
            }
        }
    }

    (slot_regions, slot_fades)
}

/// Decode a `0x262f` fade-definition block.
///
/// Layout (per `docs/pt-fade-encoding.md`):
/// - `+7`: type byte. High nibble = byte width of in-length, low nibble = byte
///   width of out-length. Observed values: 0x30, 0x33, 0x22, 0x20, 0x11.
/// - `+10..`: in-length (LE, width per type byte high nibble), then out-length
///   (LE, width per low nibble), then a single byte for shape (1 = linear,
///   2 = equal power, 3 = equal gain).
///
/// Returns `(in_len_samples, out_len_samples, shape)`. `out_len == 0` denotes a
/// single-direction fade; `in_len == out_len` denotes a symmetric crossfade.
fn decode_fade_def(def: &Block, data: &[u8]) -> Option<(u64, u64, u8, u8)> {
    let base = def.offset;
    if base + 11 > data.len() {
        return None;
    }
    let type_byte = data[base + 7];
    let n_in = (type_byte >> 4) as usize;
    let n_out = (type_byte & 0x0f) as usize;
    let payload_end = base + 10 + n_in + n_out;
    if payload_end >= data.len() {
        return None;
    }
    let read_le = |o: usize, n: usize| -> u64 {
        let mut v = 0u64;
        for i in 0..n {
            v |= (data[o + i] as u64) << (8 * i);
        }
        v
    };
    let in_len = if n_in > 0 {
        read_le(base + 10, n_in)
    } else {
        0
    };
    let out_len = if n_out > 0 {
        read_le(base + 10 + n_in, n_out)
    } else {
        0
    };
    let shape = data[base + 10 + n_in + n_out];
    // The byte right after `shape` flags non-linear curves (0 = linear).
    let curve = data.get(base + 10 + n_in + n_out + 1).copied().unwrap_or(0);
    Some((in_len, out_len, shape, curve))
}

/// Walk the block tree and collect every `0x262f` fade-definition block in
/// document order. The fade entry's `+4` field indexes into this list.
fn collect_fade_defs(blocks: &[Block]) -> Vec<&Block> {
    let mut out = Vec::new();
    fn walk<'a>(b: &'a Block, out: &mut Vec<&'a Block>) {
        if b.content_type == Some(ContentType::FadeDef) {
            out.push(b);
        }
        for c in &b.children {
            walk(c, out);
        }
    }
    for b in blocks {
        walk(b, &mut out);
    }
    out
}

// ── Alternate playlist grouping ────────────────────────────────────────────

/// Group alternate playlists under their primary track.
///
/// Groups by `(playlist_base_name, channel_pos)`. The first `TrackEntry` seen
/// for each `(base, channel)` key is the active track; subsequent entries with
/// the same key are alternate playlists that get attached to the primary.
///
/// Using `channel_pos` as a secondary key ensures that the two channels of a
/// stereo alternate are associated with the correct stereo-channel primary,
/// rather than being mistaken for mono alternates.
fn group_alternate_playlists(entries: Vec<TrackEntry>) -> Vec<Track> {
    // (base_name, channel_pos) → index in `result`
    let mut seen: std::collections::HashMap<(String, usize), usize> =
        std::collections::HashMap::new();
    let mut result: Vec<Track> = Vec::new();

    for entry in entries {
        let base = playlist_base_name(&entry.track.playlist_name).to_string();
        let key = (base.clone(), entry.channel_pos);

        if let Some(&primary_idx) = seen.get(&key) {
            // Alternate playlist — attach to the primary track
            result[primary_idx].alternate_playlists.push(Playlist {
                name: entry.track.playlist_name,
                regions: entry.track.regions,
            });
        } else {
            // Active (primary) track — set the track name to the base name so
            // it matches what the user sees in the Pro Tools UI.
            let result_idx = result.len();
            seen.insert(key, result_idx);
            let mut track = entry.track;
            track.name = base;
            result.push(track);
        }
    }

    result
}

/// Strip the trailing version suffix (`.01`, `.02`, …) from a playlist name.
///
/// `"OH.04"` → `"OH"`, `"kick.13"` → `"kick"`, `"snaps 1"` → `"snaps 1"`,
/// `"Juno.dup1.cm.01"` → `"Juno.dup1.cm"` (`.cm` has letters so it's not a suffix).
fn playlist_base_name(name: &str) -> &str {
    if let Some(dot_pos) = name.rfind('.')
        && name[dot_pos + 1..].chars().all(|c| c.is_ascii_digit())
    {
        return &name[..dot_pos];
    }
    name
}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Find the first top-level (non-nested) block with the given content type.
///
/// Used to find the active `0x1054` without descending into `0x2428`/`0x2429`
/// alternate-playlist containers.
fn find_top_level_block(blocks: &[Block], ct: ContentType) -> Option<&Block> {
    blocks
        .iter()
        .find(|&block| block.content_type == Some(ct))
        .map(|v| v as _)
}

fn find_block_recursive(blocks: &[Block], ct: ContentType) -> Option<&Block> {
    for block in blocks {
        if block.content_type == Some(ct) {
            return Some(block);
        }
        if let Some(found) = find_block_recursive(&block.children, ct) {
            return Some(found);
        }
    }
    None
}
