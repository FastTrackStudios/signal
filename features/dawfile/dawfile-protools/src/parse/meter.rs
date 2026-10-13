//! Meter map and marker parsing.
//!
//! ## Meter block (0x2029)
//!
//! Layout:
//! ```text
//! [0-4]   "Meter"  (5 bytes)
//! [5-6]   u16      (always 02 00)
//! [7-10]  u32      payload size
//! [11-14] u32      entry count  N
//! [15 .. 15+N*36]  primary entries (36 bytes each):
//!     [0-7]   u64  absolute tick position
//!     [8-11]  u32  bar number (1-based)
//!     [12-15] u32  numerator
//!     [16-19] u32  denominator
//!     [20-35] 16 bytes of other fields (ignored)
//! [..]    N * 16 bytes secondary back-reference entries (ignored)
//! [..]    10-byte trailing footer
//! ```
//!
//! ## Markers (0x271a / 0x2619)
//!
//! Pro Tools "Memory Locations" are children of the 0x271a block (type
//! `MarkerList`).  The first child (type 0x2619) named `"Markers"` is the
//! container; additional 0x2619 children within it would be the individual
//! marker entries.  None of the current fixture files contain user-defined
//! memory locations, so this returns an empty Vec for now.

use crate::block::Block;
use crate::content_type::ContentType;
use crate::cursor::Cursor;
use crate::parse::tempo::{TempoSegment, tick_to_sample};
use crate::types::{Marker, MeterEvent, ZERO_TICKS};

/// Byte size of each primary meter entry.
const ENTRY_SIZE: usize = 36;
/// Byte offset of the entry count within the "Meter" block.
const COUNT_OFFSET: usize = 11;
/// Byte offset of the first primary entry.
const FIRST_ENTRY_OFFSET: usize = 15;

/// Parse the meter (time-signature) map from the first `0x2029` block.
///
/// Returns a sorted list of meter events. Empty if the session has no meter
/// block or the block contains no entries.
pub fn parse_meter_events(
    blocks: &[Block],
    cursor: &Cursor<'_>,
    tempo_map: &[TempoSegment],
    target_sample_rate: u32,
) -> Vec<MeterEvent> {
    let meter_block = match find_block_recursive(blocks, ContentType::MeterBlock) {
        Some(b) => b,
        None => return Vec::new(),
    };

    let data = cursor.data();
    let block_start = meter_block.offset + 2;
    let block_end = (block_start + meter_block.block_size as usize).min(data.len());

    if block_start + COUNT_OFFSET + 4 > block_end {
        return Vec::new();
    }

    // Verify the "Meter" magic at the start.
    if data[block_start..].get(..5) != Some(b"Meter") {
        return Vec::new();
    }

    let count_bytes: [u8; 4] = data[block_start + COUNT_OFFSET..block_start + COUNT_OFFSET + 4]
        .try_into()
        .unwrap();
    let count = u32::from_le_bytes(count_bytes) as usize;

    if count == 0 {
        return Vec::new();
    }

    let mut events = Vec::with_capacity(count);

    for i in 0..count {
        let entry_start = block_start + FIRST_ENTRY_OFFSET + i * ENTRY_SIZE;
        if entry_start + ENTRY_SIZE > block_end {
            break;
        }

        // u64 absolute tick position
        let tick_bytes: [u8; 8] = data[entry_start..entry_start + 8].try_into().unwrap();
        let tick_abs = u64::from_le_bytes(tick_bytes);
        let tick_start = tick_abs.saturating_sub(ZERO_TICKS);

        // u32 bar number
        let measure_bytes: [u8; 4] = data[entry_start + 8..entry_start + 12].try_into().unwrap();
        let measure = u32::from_le_bytes(measure_bytes);

        // u32 numerator
        let numer_bytes: [u8; 4] = data[entry_start + 12..entry_start + 16].try_into().unwrap();
        let numerator = u32::from_le_bytes(numer_bytes);

        // u32 denominator
        let denom_bytes: [u8; 4] = data[entry_start + 16..entry_start + 20].try_into().unwrap();
        let denominator = u32::from_le_bytes(denom_bytes);

        if numerator == 0 || denominator == 0 {
            continue;
        }

        let sample_start = tick_to_sample(tick_start, tempo_map, target_sample_rate);

        events.push(MeterEvent {
            tick_start,
            sample_start,
            measure,
            numerator,
            denominator,
        });
    }

    events
}

/// Parse user-defined markers from the `0x271a` (MarkerList) block.
///
/// ## Block 0x2619 — MarkerEntry layout
///
/// ```text
/// [0..1]         u16   content_type = 0x2619
/// [2..5]         u32   name_len
/// [6..6+N]       [u8]  name (N = name_len bytes)
/// [6+N]          u8    marker_class:
///                        0x00 = system / built-in (Tempo, Meter, Key Signature, Chord Symbols)
///                        0x01 = user-defined memory location
/// [6+N+1]        u8    (unknown, 0x00)
/// [6+N+2]        u8    (unknown, 0x01)
/// [6+N+3..6+N+6] [u8;4] zeros
/// [6+N+7..6+N+10]  u32 = 42 (field-length constant)
/// [6+N+11..6+N+18] [u8;8] unique record identifier (UID, not a position)
/// [6+N+19..6+N+22] [u8;4] zeros
/// [6+N+23..6+N+26] u32 = 42
/// [6+N+27..6+N+34] [u8;8] same UID (duplicate cross-reference)
/// [6+N+35..6+N+42] [u8;8] zeros
/// [6+N+43..6+N+46] u32  sequential_number (1-based order in the session)
/// ...                    child block (0x4301, all zeros — position encoding TBD)
/// ```
///
/// The tick/sample position encoding in 0x2619 blocks has not been fully
/// reverse-engineered; `sample_pos` and `tick_pos` are reported as 0.
pub fn parse_markers(
    blocks: &[Block],
    cursor: &Cursor<'_>,
    tempo_map: &[TempoSegment],
    target_sample_rate: u32,
) -> Vec<Marker> {
    // Pro Tools 12 stores user memory locations under a 0x2030 (MarkerSectionV12)
    // container whose children are 0x2077 (MarkerEntryV12) entries with full
    // position data. Try that first.
    if let Some(markers) = parse_markers_v12(blocks, cursor, tempo_map, target_sample_rate) {
        return markers;
    }

    let user_marker_container = match find_block_recursive(blocks, ContentType::UserMarkerContainer)
    {
        Some(b) => b,
        None => return Vec::new(),
    };

    let data = cursor.data();
    let mut markers = Vec::new();
    let mut number: u32 = 0;

    for child in &user_marker_container.children {
        if child.content_type != Some(ContentType::MarkerEntry) {
            continue;
        }

        let base = child.offset; // points at content_type field

        // Need at least 6 bytes for content_type(2) + name_len(4)
        if base + 6 > data.len() {
            continue;
        }

        let name_len = u32::from_le_bytes(data[base + 2..base + 6].try_into().unwrap()) as usize;
        if name_len == 0 || base + 6 + name_len > data.len() {
            continue;
        }

        let name = String::from_utf8_lossy(&data[base + 6..base + 6 + name_len]).into_owned();

        // Marker class byte: 0x00 = system/built-in, 0x01 = user-defined
        let marker_class = data.get(base + 6 + name_len).copied().unwrap_or(0);
        if marker_class != 0x01 {
            // Skip system markers (Tempo, Meter, Key Signature, Chord Symbols)
            continue;
        }

        number += 1;

        // Try to read the sequential number from the fixed offset.
        // Formula: offset = 6(ct) + name_len + 43(fixed fields before number)
        let num_offset = base + 6 + name_len + 43;
        let seq_number = if num_offset + 4 <= data.len() {
            u32::from_le_bytes(data[num_offset..num_offset + 4].try_into().unwrap())
        } else {
            number
        };

        markers.push(Marker {
            name,
            number: if seq_number > 0 { seq_number } else { number },
            // Position encoding in 0x2619 blocks is not yet decoded.
            tick_pos: 0,
            sample_pos: 0,
            // 0x4826 color sub-block is PT12-specific; pre-PT12 markers
            // don't carry it via this path.
            color_rgb: None,
        });
    }

    markers
}

/// Parse user-defined markers from the PT 12 `0x2030`/`0x2077` layout.
///
/// ## Layout (per `0x2077` entry)
///
/// Relative to the content_type position (`block.offset`):
/// ```text
/// [0..2]                 u16   content_type = 0x2077
/// [2..4]                 u16   flags / sub-type
/// [4..8]                 u32   (unknown header field)
/// [8..12]                u32   name_len
/// [12..12+name_len]      utf8  name
/// [12+name_len..+8]      u64   encoded tick position (LE)
/// [..+8]                 u64   duplicate of the position field
/// ...                          remaining payload (sub-blocks, padding)
/// ```
///
/// The encoded position has a session-wide baseline; the relative tick position
/// is `encoded - min(encoded across all entries)`. Verified empirically against
/// known song-section positions (multiples of 960000 ticks/quarter).
///
/// Returns `None` if no `0x2030` container is present.
fn parse_markers_v12(
    blocks: &[Block],
    cursor: &Cursor<'_>,
    tempo_map: &[TempoSegment],
    target_sample_rate: u32,
) -> Option<Vec<Marker>> {
    // Gather every 0x2077 entry under any 0x2030 container.
    let mut entries: Vec<&Block> = Vec::new();
    collect_marker_section_entries(blocks, &mut entries);
    if entries.is_empty() {
        return None;
    }

    let data = cursor.data();
    // (encoded_tick, name) for each successfully parsed entry.
    let mut raw: Vec<(u64, String)> = Vec::new();

    for entry in &entries {
        let p = entry.offset;
        // Need header (12) + at least 1 byte of name + 8 bytes for position.
        if p + 12 + 1 + 8 > data.len() {
            continue;
        }
        let name_len = u32::from_le_bytes(data[p + 8..p + 12].try_into().unwrap()) as usize;
        if name_len == 0 || p + 12 + name_len + 8 > data.len() {
            continue;
        }
        let name = String::from_utf8_lossy(&data[p + 12..p + 12 + name_len]).into_owned();
        let pos_offset = p + 12 + name_len;
        let encoded = u64::from_le_bytes(data[pos_offset..pos_offset + 8].try_into().unwrap());
        raw.push((encoded, name));
    }

    if raw.is_empty() {
        return None;
    }

    // Baseline = session "tick 0" (start of bar 1) in PT's encoded form.
    // Marker positions are stored as `2^62 + ZERO_TICKS + actual_tick_offset`,
    // mirroring how PT stores all timestamps. Subtracting this constant gives
    // the tick offset from the session start, so a marker at bar 3 in PT
    // lands on bar 3 in the export (not bar 1).
    const MARKER_BASELINE: u64 = (1u64 << 62) + ZERO_TICKS;

    // Also collect per-entry color from the inner 0x4826 sub-block.
    let mut markers = Vec::with_capacity(raw.len());
    for (i, ((encoded, name), entry)) in raw.into_iter().zip(entries.iter()).enumerate() {
        let tick_pos = encoded.saturating_sub(MARKER_BASELINE);
        let sample_pos = tick_to_sample(tick_pos, tempo_map, target_sample_rate);
        let color_rgb = decode_marker_color_v12(entry, data);
        markers.push(Marker {
            name,
            number: (i as u32) + 1,
            tick_pos,
            sample_pos,
            color_rgb,
        });
    }

    Some(markers)
}

fn collect_marker_section_entries<'a>(blocks: &'a [Block], out: &mut Vec<&'a Block>) {
    for b in blocks {
        if b.content_type == Some(ContentType::MarkerSectionV12) {
            for child in &b.children {
                if child.content_type == Some(ContentType::MarkerEntryV12) {
                    out.push(child);
                }
            }
        }
        collect_marker_section_entries(&b.children, out);
    }
}

/// Decode a per-marker color from its `0x4826` sub-block.
///
/// Discovered via Frida byte-read tracing on the converter: the
/// `marker_colored` probe (REAPER color `0xD86E41`) produces reads at
/// payload `+2`, `+4`, `+6` of the inner `0x4826` block with values
/// `0xD8`, `0x6E`, `0x41` — i.e. each component is the LOW BYTE of a
/// u16 LE triplet at those offsets.
///
/// Returns `None` when no `0x4826` is present (uncolored marker).
fn decode_marker_color_v12(entry: &Block, data: &[u8]) -> Option<(u8, u8, u8)> {
    // Find the first 0x4826 child anywhere in the entry's subtree.
    fn find_4826(b: &Block) -> Option<&Block> {
        if b.content_type_raw == 0x4826 {
            return Some(b);
        }
        for c in &b.children {
            if let Some(f) = find_4826(c) {
                return Some(f);
            }
        }
        None
    }
    let color_block = find_4826(entry)?;
    let payload = color_block.offset + 2;
    if payload + 8 > data.len() {
        return None;
    }
    let r = data[payload + 2];
    let g = data[payload + 4];
    let b = data[payload + 6];
    // Uncolored markers store all-zeros in this field; treat that as None.
    if r == 0 && g == 0 && b == 0 {
        return None;
    }
    Some((r, g, b))
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
