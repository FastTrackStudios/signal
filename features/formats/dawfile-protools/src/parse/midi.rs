//! MIDI parsing: events, regions, and track assignments.
//!
//! MIDI data is stored as event chunks (block 0x2000), which are then
//! mapped to regions (0x2002/0x2634) and finally to tracks (0x1058).

use crate::block::Block;
use crate::content_type::ContentType;
use crate::cursor::{self, Cursor};
use crate::parse::tempo::{TempoSegment, tick_to_sample};
use crate::types::{MidiEvent, MidiRegion, NO_REGION, Track, TrackKind, TrackRegion, ZERO_TICKS};

/// Magic marker that precedes MIDI event data within a 0x2000 block.
const MIDI_MAGIC: &[u8] = b"MdNLB";

/// Fixed tick base added to a MIDI region's three-point source-offset field.
/// The real offset into the source chunk is `offset - MIDI_SRC_BASE`.
const MIDI_SRC_BASE: u64 = 1_000_000_000_000;

/// A raw MIDI event chunk before region assignment.
#[derive(Debug, Clone)]
struct MidiChunk {
    events: Vec<MidiEvent>,
    zero_ticks: u64,
    max_pos: u64,
}

/// Parse all MIDI data: events, regions, and tracks.
pub fn parse_midi(
    blocks: &[Block],
    cursor: &Cursor<'_>,
    version: u16,
    rate_factor: f64,
    tempo_segments: &[TempoSegment],
    target_sample_rate: u32,
) -> (Vec<MidiRegion>, Vec<Track>) {
    // Pass 1: Parse raw MIDI event chunks
    let chunks = parse_midi_chunks(blocks, cursor);

    // Pass 2: Map chunks to MIDI regions
    let regions = parse_midi_regions(
        blocks,
        cursor,
        &chunks,
        version,
        rate_factor,
        tempo_segments,
        target_sample_rate,
    );

    // Pass 3: Parse MIDI tracks and assign regions
    let tracks = parse_midi_tracks(
        blocks,
        cursor,
        &regions,
        rate_factor,
        tempo_segments,
        target_sample_rate,
    );

    (regions, tracks)
}

/// Parse raw MIDI event chunks from 0x2000 blocks.
///
/// ## Chunk layout (PT 10+)
///
/// ```text
/// [+0..+5]   magic "MdNLB"
/// [+11..+15] u32 n_events
/// [+15..+23] u64 zero_ticks (chunk position baseline, LE)
/// [+23..]    n_events × 35-byte event records
/// ```
///
/// ## Event record (35 bytes, relative to record start)
///
/// ```text
/// [+0]       u8     extra/source note (often equal to +9; semantics unclear)
/// [+1..+8]   bytes  flags / extra metadata
/// [+9]       u8     MIDI note number (0..127)
/// [+10]      u8     velocity (0..127)
/// [+11..+19] i64 LE duration in ticks (negative = paired note-off; skip)
/// [+19..+27] bytes  sign-extension / padding
/// [+27..+35] u64 LE absolute tick position (same encoding as chunk zero_ticks)
/// ```
///
/// Position relative to the chunk start = `position_u64 - zero_ticks_u64`.
/// Both share the same `0x4000_00e8_...` upper-byte pattern that PT uses for
/// timestamps; subtracting cancels the constant prefix.
fn parse_midi_chunks(blocks: &[Block], cursor: &Cursor<'_>) -> Vec<MidiChunk> {
    let mut chunks = Vec::new();
    let data = cursor.data();

    let midi_blocks = find_all_recursive(blocks, ContentType::MidiEventsBlock);

    const EVENTS_START_OFFSET: usize = 23;
    const EVENT_STRIDE: usize = 35;
    const POS_OFFSET: usize = 27;
    // The MIDI note number is byte +0 (verified against the reference exports:
    // ev[0] = 0x40/0x44/0x34 = 64/68/52 match exactly). Byte +9 is a secondary
    // "source note" field (the pre-edit pitch) — reading it there gave wrong,
    // often out-of-range pitches.
    const NOTE_OFFSET: usize = 0;
    const VEL_OFFSET: usize = 10;
    // The 35-byte record holds four baseline-2^62-encoded u64 fields (their
    // MSB `0x40` markers sit at byte offsets 8/18/26/34): duration at +1, two
    // unused fields at +11/+19, and the absolute note-on position at +27.
    const DUR_OFFSET: usize = 1;

    for block in midi_blocks {
        // Scan for ALL MdNLB magic markers within the block.
        let block_end = (block.offset + block.block_size as usize).min(data.len());
        let mut search_start = block.offset;

        while let Some(magic_pos) = find_magic(data, search_start, block_end) {
            search_start = magic_pos + MIDI_MAGIC.len();

            let n_events_offset = magic_pos + 11;
            if n_events_offset + 4 > data.len() {
                break;
            }
            let n_events = cursor.u32_at(n_events_offset) as usize;

            // zero_ticks is an 8-byte field (u64 LE) — same encoding as the
            // per-event position field, so subtracting yields relative ticks.
            let zt_offset = magic_pos + 15;
            if zt_offset + 8 > data.len() {
                break;
            }
            let zero_ticks_u64 =
                u64::from_le_bytes(data[zt_offset..zt_offset + 8].try_into().unwrap());

            let events_start = magic_pos + EVENTS_START_OFFSET;
            let mut events = Vec::with_capacity(n_events);
            let mut max_pos: u64 = 0;

            for i in 0..n_events {
                let ev_offset = events_start + i * EVENT_STRIDE;
                if ev_offset + EVENT_STRIDE > data.len() {
                    break;
                }

                // A record's +27 position is the ONSET for the note described by
                // the NEXT record: note/velocity/duration all live in record
                // i+1, while the onset comes from record i's +27. Pair them so
                // each note lands on the right onset with its own velocity and
                // length.
                let nrec = ev_offset + EVENT_STRIDE;
                if nrec + EVENT_STRIDE > data.len() {
                    break;
                }
                let note = data[nrec + NOTE_OFFSET];
                let velocity = data[nrec + VEL_OFFSET];

                // Duration field (8 bytes, baseline-2^62 encoded):
                //   * top byte 0x40 → `2^62 + ticks` (actual duration = value - 2^62)
                //   * top byte 0x00 → small positive u64, ticks directly
                //   * top byte 0xff → negative i64 = paired note-off record, skip
                //   * anything else → unknown, skip
                let dur_bytes: [u8; 8] = data[nrec + DUR_OFFSET..nrec + DUR_OFFSET + 8]
                    .try_into()
                    .unwrap();
                let dur_raw = u64::from_le_bytes(dur_bytes);
                const BASELINE: u64 = 0x4000_0000_0000_0000;
                let duration = match dur_bytes[7] {
                    0x00 => dur_raw,
                    0x40 => dur_raw.saturating_sub(BASELINE),
                    0xff => continue, // paired note-off
                    _ => continue,
                };
                // duration == 0 is legitimate (instantaneous click/drum hit)

                // Position is a u64 LE in absolute PT ticks. Subtract the
                // chunk's zero_ticks baseline (same encoding) to get the
                // tick offset from the chunk's reference point.
                let pos_bytes: [u8; 8] = data[ev_offset + POS_OFFSET..ev_offset + POS_OFFSET + 8]
                    .try_into()
                    .unwrap();
                let pos_abs = u64::from_le_bytes(pos_bytes);
                let relative_pos = pos_abs.saturating_sub(zero_ticks_u64);

                // Sanity check: drop events with implausible note numbers
                // (the dump tool showed a few records with byte values outside
                // 0..127 — likely format glitches or unused entries).
                if note > 127 || velocity > 127 {
                    continue;
                }

                if relative_pos > max_pos {
                    max_pos = relative_pos;
                }

                events.push(MidiEvent {
                    position: relative_pos,
                    duration,
                    note,
                    velocity,
                });
            }

            chunks.push(MidiChunk {
                events,
                // The legacy u40 form of zero_ticks is no longer used for
                // arithmetic, but downstream may inspect it for debugging.
                zero_ticks: zero_ticks_u64 & 0x0000_0000_ffff_ffff_ffff,
                max_pos,
            });
        }
    }

    chunks
}

/// Parse MIDI regions from region map blocks.
fn parse_midi_regions(
    blocks: &[Block],
    cursor: &Cursor<'_>,
    chunks: &[MidiChunk],
    version: u16,
    _rate_factor: f64,
    tempo_segments: &[TempoSegment],
    target_sample_rate: u32,
) -> Vec<MidiRegion> {
    let mut regions = Vec::new();

    // Choose block types based on version
    let (map_ct, region_ct) = if version < 10 {
        (ContentType::MidiRegionMapOld, ContentType::MidiRegionOld)
    } else {
        (ContentType::MidiRegionMapNew, ContentType::MidiRegionNew)
    };

    let region_map = match find_block_recursive(blocks, map_ct) {
        Some(b) => b,
        None => return regions,
    };

    let region_blocks = region_map.find_all(region_ct);

    for (idx, block) in region_blocks.iter().enumerate() {
        let data = cursor.data();

        // For PT 10+, the region data is inside a CompoundRegionGroup (0x2628) child.
        // For PT 5-9, the data is directly in the region block.
        let data_block = if version >= 10 {
            match block.find_child(ContentType::CompoundRegionGroup) {
                Some(child) => child,
                None => continue,
            }
        } else {
            block
        };

        // Region name at data_block.offset + 2
        let name_offset = data_block.offset + 2;
        if name_offset + 4 >= data.len() {
            continue;
        }
        let (name, str_consumed) = cursor.length_prefixed_string(name_offset);

        // The chunk index is a u32 right after the end of the data block
        // (at data_block.offset + data_block.block_size)
        let chunk_idx_offset = data_block.offset + data_block.block_size as usize;
        let chunk_idx = if chunk_idx_offset + 4 <= data.len() {
            cursor.u32_at(chunk_idx_offset) as usize
        } else {
            idx
        };

        // A MIDI region is a WINDOWED view of its source chunk, encoded with the
        // same "three-point" (start, source-offset, length) layout as audio
        // regions, right after the name. PT comps build a track from several
        // windowed clips of one take, so we must honour the window — otherwise
        // every clip of a shared take reports the take's full note count.
        //
        // `offset` carries a fixed 1e12 tick base; the real source offset is
        // `offset - MIDI_SRC_BASE`. `length` is the clip length in ticks. Only
        // the PT 10+ format (data in 0x2628) carries this; older formats keep
        // the whole chunk.
        let (clip_start, clip_src, clip_len) = if version >= 10 {
            let j = name_offset + str_consumed;
            let (start, offset, length) = cursor::parse_three_point(cursor, j);
            (start, offset.saturating_sub(MIDI_SRC_BASE), length)
        } else {
            (0u64, 0u64, u64::MAX)
        };

        // Keep the FULL source chunk (events stay at their chunk-relative tick
        // positions). All windowing is done per-placement in `parse_midi_tracks`
        // on the chunk-tick axis, because the same take can be referenced by
        // several clips with different windows, and because PT only "cuts off"
        // (excludes) notes for clips that were actually trimmed — a clip placed
        // at its natural position (`clip_start == clip_src`) plays the whole take.
        let events = if chunk_idx < chunks.len() {
            chunks[chunk_idx].events.clone()
        } else {
            Vec::new()
        };

        // The chunk's `zero_ticks` baseline encodes where the take sits on the
        // session timeline (relative to the session's ZERO_TICKS origin). Note
        // positions are stored chunk-relative, so the take's timeline offset
        // (`zero_ticks - ZERO_TICKS`) must be added back when placing the clip;
        // without it every take collapses to the session start.
        let take_offset_ticks = chunks
            .get(chunk_idx)
            .map(|c| c.zero_ticks.saturating_sub(ZERO_TICKS))
            .unwrap_or(0);

        // Region length in samples: the clip's nominal extent.
        let region_length_samples = if clip_len != u64::MAX {
            tick_to_sample(clip_len, tempo_segments, target_sample_rate)
        } else if chunk_idx < chunks.len() {
            tick_to_sample(
                chunks[chunk_idx].max_pos,
                tempo_segments,
                target_sample_rate,
            )
        } else {
            0
        };

        regions.push(MidiRegion {
            name,
            index: idx as u16,
            start_pos: tick_to_sample(clip_start, tempo_segments, target_sample_rate),
            sample_offset: 0,
            length: region_length_samples,
            clip_start_ticks: clip_start,
            clip_src_ticks: clip_src,
            clip_len_ticks: if clip_len == u64::MAX { 0 } else { clip_len },
            take_offset_ticks,
            events,
        });
    }

    regions
}

/// Parse the non-audio tracks (MIDI / instrument / master / click / folder
/// dividers) from the 0x2519 track list and assign each its ACTIVE-playlist
/// regions.
///
/// The 0x2519 list enumerates EVERY track in the session. Audio tracks are
/// parsed separately from 0x1015 and dropped from this set by the caller (by
/// name); what remains is the non-audio tracks PT shows in the Edit window —
/// master, click, dividers and instrument/MIDI tracks — all of which must be
/// emitted to match the source layout even when they hold no regions.
///
/// Regions are matched to tracks BY NAME (not the old positional 0x1058 walk,
/// which scattered every region onto the wrong track): a region
/// `"Inst 1.02-01"` belongs to the track named `"Inst 1.02"`. Regions whose
/// playlist is an inactive alternate (e.g. `"Inst 1.01-*"` when `.02` is the
/// active track) match no track name and are left out, mirroring the audio
/// active/alternate-playlist model.
fn parse_midi_tracks(
    blocks: &[Block],
    cursor: &Cursor<'_>,
    regions: &[MidiRegion],
    rate_factor: f64,
    tempo_segments: &[TempoSegment],
    target_sample_rate: u32,
) -> Vec<Track> {
    let _ = rate_factor;
    let mut tracks = Vec::new();

    // Parse track definitions from 0x2519
    let track_list = match find_block_recursive(blocks, ContentType::MidiTrackList) {
        Some(b) => b,
        None => return tracks,
    };

    // Parse the per-track clip-placement groups from 0x1058: each 0x1057 is one
    // instrument track's playlist (a list of 0x1056/0x104f clip placements,
    // each naming a windowed region by its array index). Groups are in document
    // order, 1:1 with the kind-0x07 instrument tracks in the 0x2519 list (any
    // trailing groups are inactive alternate playlists).
    let placement_groups: Vec<Vec<u16>> = {
        let mut groups = Vec::new();
        if let Some(map) = find_block_recursive(blocks, ContentType::MidiRegionTrackMap) {
            for entry in &map.children {
                if entry.content_type != Some(ContentType::MidiRegionTrackMapEntries) {
                    continue;
                }
                let mut idxs = Vec::new();
                for c in &entry.children {
                    if c.content_type != Some(ContentType::MidiRegionTrackEntry) {
                        continue;
                    }
                    for f in &c.children {
                        if f.content_type != Some(ContentType::AudioRegionTrackSubEntryNew) {
                            continue;
                        }
                        // Region array index: u32 at the sub-entry's payload +4.
                        let p = f.offset + 4;
                        if p + 4 <= cursor.data().len() {
                            idxs.push(u32::from_le_bytes(
                                cursor.data()[p..p + 4].try_into().unwrap(),
                            ) as u16);
                        }
                    }
                }
                groups.push(idxs);
            }
        }
        groups
    };
    let mut next_group = 0usize;

    let data = cursor.data();
    let mut seen = std::collections::HashSet::new();
    for child in track_list.find_children(ContentType::MidiTrackInfo) {
        let name_offset = child.offset + 4;
        if name_offset + 4 >= data.len() {
            continue;
        }

        let (name, _str_consumed) = cursor.length_prefixed_string(name_offset);
        if name.is_empty() {
            continue;
        }
        // 0x2519 may carry a 2× tail copy of the whole list (and interleaved
        // alternates in converter-authored PTX); keep the first occurrence of
        // each name only.
        if !seen.insert(name.clone()) {
            continue;
        }

        // Kind byte at child payload +2: 0x07 = instrument track, 0x01 = pure
        // MIDI track, 0x05 = master, 0x02 = click/folder divider, 0x00 = audio.
        // Both 0x07 and 0x01 carry MIDI note placements; everything else stays
        // empty.
        let kind = data.get(child.offset + 2).copied().unwrap_or(0);
        let is_note_track = kind == 0x07 || kind == 0x01;

        // Active-playlist clips: instrument tracks consume the next 0x1057 group
        // in order; each placement references a windowed region by array index,
        // placed at that region's own decoded timeline position.
        //
        // Comp flatten: a track's clips can overlap on the timeline (the later
        // clip wins). Sort clips by timeline start and cap each clip's events at
        // the next clip's start — `note_trim_ticks = min(clip_len, next_start -
        // start)` — so overlaps aren't double-counted. The last clip is uncapped
        // (plays to the take end). Gaps are preserved because we also cap by the
        // clip's own length.
        let track_regions: Vec<TrackRegion> = if is_note_track {
            let group = placement_groups
                .get(next_group)
                .cloned()
                .unwrap_or_default();
            next_group += 1;

            // Resolve each placement to (region_index, start, src, len) and sort
            // by timeline start for the comp flatten.
            let mut clips: Vec<(u16, u64, u64, u64)> = group
                .into_iter()
                .filter_map(|ri| {
                    regions
                        .get(ri as usize)
                        .map(|r| (ri, r.clip_start_ticks, r.clip_src_ticks, r.clip_len_ticks))
                })
                .collect();
            clips.sort_by_key(|c| c.1);

            // A track with a single clip plays its whole take (no comp, no
            // cut-off) — confirmed exact against the exports (Bass, Drums.dupN,
            // Presence). A comp (multiple clips) windows each clip to its source
            // span and caps it at the next clip on the track.
            let single = clips.len() == 1;

            clips
                .iter()
                .enumerate()
                .map(|(i, &(ri, start, src, len))| {
                    // Events are the full chunk at chunk-tick positions; bound
                    // them in chunk-tick space.
                    let (clip_lo_ticks, note_trim_ticks) = if single {
                        (0, u64::MAX)
                    } else {
                        // Source window [src, src+len), then capped at the comp
                        // boundary — the next clip with a strictly-greater start
                        // (equal-start clips are stacked takes, not a boundary).
                        // timeline = start + (chunk_pos - src), so the boundary
                        // in chunk-tick space is `next_start - start + src`.
                        let next_start = clips[i + 1..].iter().map(|c| c.1).find(|&s| s > start);
                        let comp_hi = match next_start {
                            Some(ns) => ns.saturating_sub(start).saturating_add(src),
                            None => u64::MAX,
                        };
                        (src, src.saturating_add(len).min(comp_hi))
                    };

                    // Item placement. The take's natural note timeline is
                    // `take_offset + chunk_pos + (start - src)`. We emit each
                    // clip's notes RELATIVE to its kept-window start
                    // (`chunk_pos - clip_lo`, done at emit time) so they sit
                    // inside the item, so the item itself must begin
                    // `clip_lo` later — at `take_offset + start - src +
                    // clip_lo`. For a single clip (`clip_lo == 0`) this is the
                    // take's recorded position, unchanged; for comp clips it
                    // spreads each window to its own spot instead of stacking
                    // every item on the take offset.
                    let take_offset = regions[ri as usize].take_offset_ticks;
                    let placement_ticks = take_offset
                        .saturating_add(start)
                        .saturating_sub(src)
                        .saturating_add(clip_lo_ticks);

                    TrackRegion {
                        region_index: ri,
                        start_pos: tick_to_sample(
                            placement_ticks,
                            tempo_segments,
                            target_sample_rate,
                        ),
                        clip_flag_53: false,
                        clip_muted: false,
                        clip_color: None,
                        clip_lo_ticks,
                        note_trim_ticks,
                    }
                })
                .collect()
        } else {
            Vec::new()
        };

        tracks.push(Track {
            name,
            kind: TrackKind::Midi,
            index: tracks.len() as u16,
            playlist_name: String::new(),
            fades: Vec::new(),
            regions: track_regions,
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
            is_master: kind == 0x05,
        });
    }

    tracks
}

/// Find the MdNLB magic bytes within a range.
fn find_magic(data: &[u8], start: usize, end: usize) -> Option<usize> {
    let end = end.min(data.len());
    if end < start + MIDI_MAGIC.len() {
        return None;
    }
    data[start..end - MIDI_MAGIC.len() + 1]
        .windows(MIDI_MAGIC.len())
        .position(|w| w == MIDI_MAGIC)
        .map(|p| start + p)
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

fn find_all_recursive(blocks: &[Block], ct: ContentType) -> Vec<&Block> {
    let mut result = Vec::new();
    for block in blocks {
        if block.content_type == Some(ct) {
            result.push(block);
        }
        result.extend(find_all_recursive(&block.children, ct));
    }
    result
}
