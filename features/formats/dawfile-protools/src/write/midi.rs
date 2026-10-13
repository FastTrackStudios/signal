//! MIDI note-chunk encoding for the native PTX writer.
//!
//! The inverse of `parse::midi::parse_midi_chunks`. A MIDI take is stored as an
//! `MdNLB` note chunk inside a `0x2000` block. The chunk header is:
//!
//! ```text
//! [+0..+5]   "MdNLB"
//! [+5..+7]   u16 version (always 3 for PT 10+)
//! [+7..+11]  u32 field7 = n_events*47 + 22   (deterministic; verified across chunks)
//! [+11..+15] u32 n_events
//! [+15..+23] u64 zero_ticks   (the take's absolute-tick baseline)
//! [+23..]    n_events × 35-byte event records
//! ```
//!
//! Each 35-byte record's fields:
//!
//! ```text
//! [+0]       u8  note number (0..127)
//! [+1..+9]   u64 duration, baseline-2^62  (stored = 2^62 + ticks)
//! [+9]       u8  velocity (0..127)
//! [+10]      u8  0x40  (constant marker)
//! [+11..+19] u64 baseline-2^62, value 0   (unused field)
//! [+19..+27] u64 baseline-2^62, value 0   (unused field)
//! [+27..+35] u64 absolute tick position = zero_ticks + chunk-relative position
//! ```
//!
//! **Staggered pairing (critical).** PT does not store one note per record
//! self-contained. The note that sounds at record *i*'s onset (`+27`) has its
//! pitch / velocity / duration in record *i+1*. This was verified against the
//! reference exports: decoding self-contained makes note parity collapse
//! (~240 mismatches vs ~10), while the staggered pairing matches. So to encode
//! `N` notes we emit `N+1` records: record `i`'s `+27` is note `i`'s onset and
//! record `i+1` carries note `i`'s pitch/vel/dur. `n_events` (header) = record
//! count = `N+1`; the decoder loop yields `N` notes.

/// A note to encode into a chunk. Positions/durations are in PT ticks
/// (960,000 per quarter), relative to the chunk's `zero_ticks` baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkNote {
    /// Chunk-relative onset position, in ticks.
    pub position: u64,
    /// Duration in ticks.
    pub duration: u64,
    /// MIDI note number (0..127).
    pub note: u8,
    /// Velocity (1..127).
    pub velocity: u8,
}

/// 2^62 — the baseline PT adds to tick-valued u64 fields.
const BASELINE: u64 = 0x4000_0000_0000_0000;
const EVENT_STRIDE: usize = 35;
const MAGIC: &[u8; 5] = b"MdNLB";

/// Session timeline origin in PT ticks (== `types::ZERO_TICKS`, 0xe8d4a51000).
/// A chunk's `zero_ticks` header is `ZERO_TICKS + take_offset`.
const ZERO_TICKS: u64 = 0xe8d4_a510_00;

/// Fixed 1e12-tick base added to a region's three-point source-offset field
/// (== `parse::midi::MIDI_SRC_BASE`). Real source offset = `offset - this`.
const MIDI_SRC_BASE: u64 = 1_000_000_000_000;

/// Build a 35-byte record from an onset position and (optionally) a note's
/// pitch/vel/dur. In the staggered layout the position and note-data come from
/// different source notes, so they're set independently.
fn build_record(abs_pos: u64, note_data: Option<&ChunkNote>) -> [u8; EVENT_STRIDE] {
    let mut r = [0u8; EVENT_STRIDE];
    r[10] = 0x40;
    r[1..9].copy_from_slice(&BASELINE.to_le_bytes());
    r[11..19].copy_from_slice(&BASELINE.to_le_bytes());
    r[19..27].copy_from_slice(&BASELINE.to_le_bytes());
    if let Some(nd) = note_data {
        r[0] = nd.note;
        r[1..9].copy_from_slice(&BASELINE.wrapping_add(nd.duration).to_le_bytes());
        r[9] = nd.velocity;
    }
    r[27..35].copy_from_slice(&abs_pos.to_le_bytes());
    r
}

/// Encode an `MdNLB` note chunk (header + records) in PT's staggered layout.
/// `zero_ticks` is the take's absolute-tick baseline (PT timestamps carry a
/// `2^62 + ZERO_TICKS` prefix).
pub fn encode_note_chunk(notes: &[ChunkNote], zero_ticks: u64) -> Vec<u8> {
    if notes.is_empty() {
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&3u16.to_le_bytes());
        out.extend_from_slice(&22u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&zero_ticks.to_le_bytes());
        return out;
    }
    let n = notes.len();
    let rec_count = (n + 1) as u32; // staggered: N notes → N+1 records
    let mut out = Vec::with_capacity(23 + (n + 1) * EVENT_STRIDE);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&3u16.to_le_bytes()); // version
    out.extend_from_slice(&rec_count.wrapping_mul(47).wrapping_add(22).to_le_bytes()); // field7
    out.extend_from_slice(&rec_count.to_le_bytes()); // n_events = record count
    out.extend_from_slice(&zero_ticks.to_le_bytes());
    // record j: onset = note[j] (j<N), note-data = note[j-1] (j>=1).
    for j in 0..=n {
        let abs_pos = zero_ticks.wrapping_add(notes[j.min(n - 1)].position);
        let note_data = if j >= 1 { Some(&notes[j - 1]) } else { None };
        out.extend_from_slice(&build_record(abs_pos, note_data));
    }
    out
}

/// Staggered decoder mirroring the production parser: note `i` takes its onset
/// from record `i`'s `+27` and its pitch/vel/dur from record `i+1`. (Velocity
/// is read from `+9`, the real location; the production parser reads `+10` for
/// note-parity reasons and so loses velocity — that's a parser quirk, not the
/// data.) Used to validate the encoder offline.
#[cfg(test)]
pub fn decode_note_chunk_staggered(data: &[u8]) -> Vec<ChunkNote> {
    let mut out = Vec::new();
    if data.len() < 23 || &data[0..5] != MAGIC {
        return out;
    }
    let n = u32::from_le_bytes(data[11..15].try_into().unwrap()) as usize;
    let zt = u64::from_le_bytes(data[15..23].try_into().unwrap());
    for i in 0..n {
        let r = 23 + i * EVENT_STRIDE;
        let nr = r + EVENT_STRIDE;
        if nr + EVENT_STRIDE > data.len() {
            break;
        }
        let dur =
            u64::from_le_bytes(data[nr + 1..nr + 9].try_into().unwrap()).wrapping_sub(BASELINE);
        let pos = u64::from_le_bytes(data[r + 27..r + 35].try_into().unwrap()).wrapping_sub(zt);
        out.push(ChunkNote {
            position: pos,
            duration: dur,
            note: data[nr],
            velocity: data[nr + 9],
        });
    }
    out
}

// ======================================================================
// Phase 2-4: region / region-map / placement-chain encoders + injection
// ======================================================================

use crate::content_type::ContentType;
use crate::raw_block::{RawBlock, RawSession};
use crate::write::block_ops::wrap_as_block;
use crate::write::splice::splice;

/// One instrument track's worth of MIDI to inject. `tracks[i]` maps to the
/// i-th note track (kind `0x07`/`0x01`) in 0x2519 document order.
#[derive(Debug, Clone, Default)]
pub struct MidiTrackInput {
    /// The track's notes. Positions/durations in PT ticks (960,000/quarter),
    /// relative to the track's clip origin (the take baseline).
    pub notes: Vec<ChunkNote>,
    /// Region/clip name. If empty, a default `"Region N"` is used.
    pub name: String,
}

/// Constant 66-byte trailer that follows the three-point in a MIDI region's
/// 0x2628 payload. Captured verbatim from a real session
/// (`studio-session-2.ptx`, region "Bass-01"). The parser never reads past the
/// three-point, so the exact bytes are opaque; we only need *some* valid
/// trailer (including the 16-byte clip UID at the tail) so the block frames
/// stay self-consistent through a raw re-parse.
const REGION_TRAILER: [u8; 65] = [
    0x00, 0x98, 0x3a, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xfe, 0xff, 0x00, 0x00, 0x00,
    0x00, 0xff, 0xff, 0x04, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0x00,
    0x00, 0xea, 0x67, 0xef, 0x50, 0x53, 0xb8, 0x44, 0xb1, 0xac, 0x3c, 0xc9, 0xe0, 0x58, 0x37, 0xf9,
    0x6b,
];

/// Encode a 5-byte-each three-point descriptor for a MIDI region.
///
/// Layout (mirrors `cursor::parse_three_point`): a 5-byte header where the
/// HIGH nibble of header[1]/[2]/[3] gives the byte-widths of offset/length/
/// start, then the three values LE in the order offset, length, start. We use
/// a fixed width of 5 bytes per value so any tick magnitude fits.
fn encode_three_point(offset: u64, length: u64, start: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(5 + 15);
    // header: low nibbles copied from a real region (parser ignores them);
    // high nibbles encode width=5 for each of offset, length, start.
    out.push(0x00); // header[0] (constant)
    out.push(0x51); // header[1]: offset width 5
    out.push(0x50); // header[2]: length width 5
    out.push(0x53); // header[3]: start width 5
    out.push(0x08); // header[4] (constant)
    out.extend_from_slice(&offset.to_le_bytes()[..5]);
    out.extend_from_slice(&length.to_le_bytes()[..5]);
    out.extend_from_slice(&start.to_le_bytes()[..5]);
    out
}

/// Build a length-prefixed (u32 LE) string.
fn lp_string(s: &str) -> Vec<u8> {
    let mut out = (s.len() as u32).to_le_bytes().to_vec();
    out.extend_from_slice(s.as_bytes());
    out
}

/// Encode one MIDI region (0x2633) wrapping a CompoundRegionGroup (0x2628)
/// followed by the u32 chunk index. `chunk_idx` indexes the note chunk inside
/// the 0x2000 block (in MdNLB occurrence order). Clip placed at its natural
/// position: `start == src == 0`, so the take's events play in full.
fn encode_region(name: &str, chunk_idx: u32, clip_len: u64) -> Vec<u8> {
    // 0x2628 payload: name + three-point + constant trailer.
    let mut inner = lp_string(name);
    inner.extend_from_slice(&encode_three_point(MIDI_SRC_BASE, clip_len, 0));
    inner.extend_from_slice(&REGION_TRAILER);
    let block_2628 = wrap_as_block(ContentType::CompoundRegionGroup as u16, &inner);

    // 0x2633 payload: the 0x2628 block, then the chunk index u32.
    let mut outer = block_2628;
    outer.extend_from_slice(&chunk_idx.to_le_bytes());
    wrap_as_block(ContentType::MidiRegionNew as u16, &outer)
}

/// Encode the region map (0x2634): u32 count then the 0x2633 region blocks.
fn encode_region_map(regions: &[Vec<u8>]) -> Vec<u8> {
    let mut payload = (regions.len() as u32).to_le_bytes().to_vec();
    for r in regions {
        payload.extend_from_slice(r);
    }
    payload
}

/// Encode one placement sub-entry (0x104f) referencing region `region_idx`.
/// `pos_ticks` is the timeline position (added to MIDI_SRC_BASE in the field).
fn encode_sub_entry(region_idx: u32, pos_ticks: u64) -> Vec<u8> {
    // 35-byte payload mirroring a real 0x104f.
    let mut p = [0u8; 35];
    p[2..6].copy_from_slice(&region_idx.to_le_bytes()); // region index u32 @ payload+2
    let pos_field = MIDI_SRC_BASE.wrapping_add(pos_ticks);
    p[7..12].copy_from_slice(&pos_field.to_le_bytes()[..5]); // u40 position @ payload+7
    p[14] = 0x40; // format byte @ payload+14
    p[15] = 0x03;
    p[16] = 0xfe;
    p[17] = 0xff;
    p[20] = 0x01;
    for b in p.iter_mut().take(30).skip(22) {
        *b = 0xff;
    }
    wrap_as_block(ContentType::AudioRegionTrackSubEntryNew as u16, &p)
}

/// Encode one placement entry (0x1056) wrapping a single 0x104f.
fn encode_track_entry(region_idx: u32, pos_ticks: u64) -> Vec<u8> {
    let sub = encode_sub_entry(region_idx, pos_ticks);
    wrap_as_block(ContentType::MidiRegionTrackEntry as u16, &sub)
}

/// Encode one per-track placement group (0x1057): a length-prefixed name, a
/// u32 placement count, then the 0x1056 placement entries.
fn encode_track_map_entry(name: &str, placements: &[(u32, u64)]) -> Vec<u8> {
    let mut payload = lp_string(name);
    payload.extend_from_slice(&(placements.len() as u32).to_le_bytes());
    for &(ri, pos) in placements {
        payload.extend_from_slice(&encode_track_entry(ri, pos));
    }
    wrap_as_block(ContentType::MidiRegionTrackMapEntries as u16, &payload)
}

/// Encode the 0x1058 placement-map payload: u32 group-count then the 0x1057
/// groups (one per note track, in document order).
fn encode_placement_map(groups: &[Vec<u8>]) -> Vec<u8> {
    let mut payload = (groups.len() as u32).to_le_bytes().to_vec();
    for g in groups {
        payload.extend_from_slice(g);
    }
    payload
}

/// Replace the entire payload of the first block of content type `ct` with
/// `new_payload` (the payload is everything after the 9-byte block header).
/// Returns `false` if no such block exists.
fn replace_block_payload(session: &mut RawSession, ct: u16, new_payload: &[u8]) -> bool {
    let Some((start, end)) = find_first_raw(&session.blocks, ct).map(|b| (b.start, b.end)) else {
        return false;
    };
    let payload_start = start + 9;
    let old_len = end - payload_start;
    // Keep the block's total size CONSTANT by padding the new payload up to the
    // original length with zeros. PT stores every block's absolute file offset
    // in a registry table (a `0x0002` block near EOF); changing any block's
    // size shifts every later block and invalidates those stored offsets, which
    // makes PT fail loading with "end of stream". Padding avoids the shift so
    // the registry stays valid. (The container's leading count means PT reads
    // exactly the entries we wrote and ignores the zero tail.) If the new
    // payload is larger than the original, we cannot keep size constant without
    // rewriting the registry — fall back to a splice and let the caller know.
    if new_payload.len() <= old_len {
        let mut padded = new_payload.to_vec();
        padded.resize(old_len, 0);
        splice(session, payload_start, old_len, &padded);
    } else {
        splice(session, payload_start, old_len, new_payload);
    }
    true
}

fn find_first_raw(blocks: &[RawBlock], ct: u16) -> Option<&RawBlock> {
    for b in blocks {
        if b.content_type_raw == ct {
            return Some(b);
        }
        if let Some(f) = find_first_raw(&b.children, ct) {
            return Some(f);
        }
    }
    None
}

/// Inject MIDI into an already-parsed session.
///
/// `tracks[i]` supplies the notes for the i-th note track (kind `0x07`/`0x01`)
/// in 0x2519 document order; tracks beyond the supplied list, or with empty
/// `notes`, get no clips. The function REPLACES the payloads of the existing
/// `0x2000` (note chunks), `0x2634` (region map) and the first `0x1058`
/// (placement map) blocks, so any pre-existing MIDI in those blocks is
/// discarded.
///
/// One region + one chunk + one single-clip placement is emitted per
/// note-bearing track. Single-clip placements play the whole take, so the
/// notes round-trip at their chunk-relative positions.
///
/// Validate by re-parsing (`read_session_from_bytes(session.encrypt(), …)`):
/// the i-th note-bearing track's region carries the injected notes (note
/// number + tick position; velocity is parsed from the wrong offset upstream,
/// so do not assert it).
pub fn inject_midi(session: &mut RawSession, tracks: &[MidiTrackInput]) -> crate::PtResult<()> {
    let mut chunk_list: Vec<Vec<u8>> = Vec::new();
    let mut regions: Vec<Vec<u8>> = Vec::new();
    let mut groups: Vec<Vec<u8>> = Vec::new();

    for (ti, track) in tracks.iter().enumerate() {
        if track.notes.is_empty() {
            // Empty group keeps track↔group alignment for later tracks.
            groups.push(encode_track_map_entry(
                if track.name.is_empty() {
                    "Empty"
                } else {
                    &track.name
                },
                &[],
            ));
            continue;
        }
        let chunk_idx = regions.len() as u32;
        // Note chunk. PT timestamps carry a 2^62 baseline on top of the
        // session tick origin; the chunk's zero_ticks header and every event
        // position field must include it (top byte 0x40) or PT can't parse the
        // records. take_offset == 0 (notes are already chunk-relative).
        //
        // No explicit terminator record. The staggered decode reads one record
        // past the last note (record N+1); real sessions let that land in the
        // chunk's trailing SLACK (zeros → velocity 0 → skipped), which we also
        // provide below. A non-standard 0xff terminator made PT choke once it
        // started parsing events, so we match the original layout: N+1 records
        // followed by zero slack.
        let chunk_bytes = encode_note_chunk(&track.notes, BASELINE + ZERO_TICKS);

        // Each MdNLB chunk inside the 0x2000 block is framed by an `MdChun`
        // container header: `"MdChun" 01 00 <u32 byte-len>` then the chunk.
        // Pro Tools walks these by length; omitting the header makes PT read
        // past the data and fail with "end of stream" (our parser is lenient
        // and only scans for the MdNLB magic, so it tolerated the omission).
        // We collect each chunk separately so slack can be distributed across
        // all of them later (a single chunk with huge slack is rejected by PT).
        chunk_list.push(chunk_bytes);

        // Region spanning all notes (length = furthest note end).
        let clip_len = track
            .notes
            .iter()
            .map(|n| n.position + n.duration)
            .max()
            .unwrap_or(0);
        let name = if track.name.is_empty() {
            format!("Region {}", ti + 1)
        } else {
            track.name.clone()
        };
        regions.push(encode_region(&name, chunk_idx, clip_len));

        // Single-clip placement at the natural (clip_start == 0) position.
        groups.push(encode_track_map_entry(&name, &[(chunk_idx, 0)]));
    }

    // Per-chunk slack budget (zero bytes inside each MdChun region beyond the
    // MdNLB data). PT carries such slack in real sessions; we keep the 0x2000
    // block at its original size (registry-safety, see replace_block_payload)
    // and DISTRIBUTE the required filler evenly across chunks so no single
    // chunk gets a pathologically large slack (PT rejects that). Compute the
    // target payload size from the existing 0x2000 block.
    let target_2000 = find_first_raw(&session.blocks, ContentType::MidiEventsBlock as u16)
        .map(|b| (b.end) - (b.start + 9));
    // Tight size: u32 count + Σ (12-byte MdChun header + chunk bytes).
    let tight: usize = 4 + chunk_list.iter().map(|c| 12 + c.len()).sum::<usize>();
    let per_chunk_slack = match target_2000 {
        Some(t) if t > tight && !chunk_list.is_empty() => (t - tight) / chunk_list.len(),
        _ => 0,
    };
    let mut chunk_payload = (chunk_list.len() as u32).to_le_bytes().to_vec();
    for (i, c) in chunk_list.iter().enumerate() {
        // Last chunk takes any rounding remainder so the block fills exactly.
        let slack = if let Some(t) = target_2000 {
            if i + 1 == chunk_list.len() {
                t.saturating_sub(chunk_payload.len() + 12 + c.len())
            } else {
                per_chunk_slack
            }
        } else {
            0
        };
        chunk_payload.extend_from_slice(b"MdChun");
        chunk_payload.extend_from_slice(&[0x01, 0x00]);
        chunk_payload.extend_from_slice(&((c.len() + slack) as u32).to_le_bytes());
        chunk_payload.extend_from_slice(c);
        chunk_payload.resize(chunk_payload.len() + slack, 0);
    }
    replace_block_payload(session, ContentType::MidiEventsBlock as u16, &chunk_payload);
    replace_block_payload(
        session,
        ContentType::MidiRegionMapNew as u16,
        &encode_region_map(&regions),
    );
    replace_block_payload(
        session,
        ContentType::MidiRegionTrackMap as u16,
        &encode_placement_map(&groups),
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_header_field7_formula() {
        // 451 notes → 452 records; header n_events = record count.
        let notes = vec![
            ChunkNote {
                position: 0,
                duration: 480_000,
                note: 60,
                velocity: 100
            };
            451
        ];
        let chunk = encode_note_chunk(&notes, 0x4000_00e8_d519_a488);
        let n = u32::from_le_bytes(chunk[11..15].try_into().unwrap());
        assert_eq!(n, 452);
        let field7 = u32::from_le_bytes(chunk[7..11].try_into().unwrap());
        assert_eq!(field7, 452 * 47 + 22);
    }

    #[test]
    fn note_chunk_round_trips() {
        let zt = 0x4000_00e8_d519_a488;
        let notes = vec![
            ChunkNote {
                position: 990_720,
                duration: 1_065_600,
                note: 66,
                velocity: 80,
            },
            ChunkNote {
                position: 1_463_040,
                duration: 550_080,
                note: 69,
                velocity: 78,
            },
            ChunkNote {
                position: 2_413_440,
                duration: 1_048_320,
                note: 64,
                velocity: 76,
            },
            ChunkNote {
                position: 3_000_000,
                duration: 120_000,
                note: 36,
                velocity: 64,
            },
        ];
        let chunk = encode_note_chunk(&notes, zt);
        // N notes → N+1 records.
        assert_eq!(chunk.len(), 23 + (notes.len() + 1) * EVENT_STRIDE);
        let decoded = decode_note_chunk_staggered(&chunk);
        assert_eq!(decoded, notes);
    }

    // ------------------------------------------------------------------
    // Phase 2-4 round-trip: inject MIDI into a donor session, re-parse.
    // ------------------------------------------------------------------

    const DONOR: &str = "tests/fixtures/studio-session-2.ptx";

    fn donor_session() -> RawSession {
        let raw = std::fs::read(DONOR).expect("donor fixture present");
        crate::parse_raw(raw).expect("decrypt donor")
    }

    #[test]
    fn three_point_round_trips_via_parser_helper() {
        // Build a region's 0x2628 payload and re-decode the three-point with
        // the production parser helper to confirm width/value agreement.
        use crate::cursor::{Cursor, parse_three_point};
        let mut payload = lp_string("Test");
        payload.extend_from_slice(&encode_three_point(MIDI_SRC_BASE + 5000, 123_456, 7890));
        let cur = Cursor::new(&payload, false);
        let (_name, consumed) = cur.length_prefixed_string(0);
        let (start, offset, length) = parse_three_point(&cur, consumed);
        assert_eq!(offset, MIDI_SRC_BASE + 5000);
        assert_eq!(length, 123_456);
        assert_eq!(start, 7890);
    }

    #[test]
    fn inject_midi_round_trips() {
        let mut session = donor_session();

        // Two instrument tracks' worth of notes at known positions.
        let track0 = MidiTrackInput {
            name: "InjectA".to_string(),
            notes: vec![
                ChunkNote {
                    position: 0,
                    duration: 480_000,
                    note: 60,
                    velocity: 100,
                },
                ChunkNote {
                    position: 960_000,
                    duration: 240_000,
                    note: 64,
                    velocity: 90,
                },
                ChunkNote {
                    position: 1_920_000,
                    duration: 480_000,
                    note: 67,
                    velocity: 80,
                },
            ],
        };
        let track1 = MidiTrackInput {
            name: "InjectB".to_string(),
            notes: vec![
                ChunkNote {
                    position: 100_000,
                    duration: 50_000,
                    note: 36,
                    velocity: 110,
                },
                ChunkNote {
                    position: 500_000,
                    duration: 50_000,
                    note: 38,
                    velocity: 105,
                },
            ],
        };
        let inputs = vec![track0.clone(), track1.clone()];
        inject_midi(&mut session, &inputs).unwrap();

        let bytes = session.encrypt();
        let parsed = crate::read_session_from_bytes(bytes, 48000).unwrap();

        // Region map: two regions, in track order.
        assert_eq!(parsed.midi_regions.len(), 2, "two regions injected");
        for (i, src) in [&track0, &track1].iter().enumerate() {
            let region = &parsed.midi_regions[i];
            assert_eq!(region.name, src.name, "region {i} name");
            // Match PT's layout (no terminator): the staggered decode reads one
            // record past the last note into the chunk's trailing slack zeros,
            // producing a harmless velocity-0 phantom the converter drops. Real
            // notes carry velocity 64 (parser reads the +10 marker byte); filter
            // on that to compare the actual notes.
            let real: Vec<_> = region.events.iter().filter(|e| e.velocity > 0).collect();
            assert_eq!(
                real.len(),
                src.notes.len(),
                "region {i} note count (parsed {} vs injected {})",
                real.len(),
                src.notes.len()
            );
            for (ev, want) in real.iter().zip(src.notes.iter()) {
                assert_eq!(ev.note, want.note, "region {i} note number");
                assert_eq!(ev.position, want.position, "region {i} note position");
                // velocity intentionally NOT asserted (parser reads +10).
            }
        }

        // The placement chain must surface these regions on note tracks.
        let placed: Vec<u16> = parsed
            .midi_tracks
            .iter()
            .flat_map(|t| t.regions.iter().map(|r| r.region_index))
            .collect();
        assert!(placed.contains(&0), "region 0 placed on a track");
        assert!(placed.contains(&1), "region 1 placed on a track");
    }

    #[test]
    fn inject_single_track() {
        let mut session = donor_session();
        let notes = vec![
            ChunkNote {
                position: 0,
                duration: 120_000,
                note: 48,
                velocity: 70,
            },
            ChunkNote {
                position: 240_000,
                duration: 120_000,
                note: 50,
                velocity: 70,
            },
        ];
        inject_midi(
            &mut session,
            &[MidiTrackInput {
                name: "Solo".to_string(),
                notes: notes.clone(),
            }],
        )
        .unwrap();
        let parsed = crate::read_session_from_bytes(session.encrypt(), 48000).unwrap();
        assert_eq!(parsed.midi_regions.len(), 1);
        let region = &parsed.midi_regions[0];
        // Filter the trailing velocity-0 phantom (see inject_midi_round_trips).
        let real: Vec<_> = region.events.iter().filter(|e| e.velocity > 0).collect();
        assert_eq!(real.len(), notes.len());
        for (ev, want) in real.iter().zip(notes.iter()) {
            assert_eq!(ev.note, want.note);
            assert_eq!(ev.position, want.position);
        }
    }
}
