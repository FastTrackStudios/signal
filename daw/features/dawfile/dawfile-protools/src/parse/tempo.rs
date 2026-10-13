//! Tempo map parsing from Pro Tools block 0x2028.
//!
//! ## Block layout
//!
//! The 0x2028 block starts with "Tempo" followed by a count of constant-tempo
//! segments. Each segment is encoded as a "Const" sub-record at a fixed stride
//! of 61 bytes, beginning at byte 19 of the block content (offset+2).
//!
//! Within each "Const" record (offsets relative to the "Const" byte):
//! ```text
//!   +0  ..  +4   "Const" (5 bytes)
//!   +5  ..  +10  sub-block header (u16 + u32 size)
//!  +11  .. +13   "TMS" (3 bytes)
//!  +14  .. +19   TMS header (u16 + u32 size = 20)
//!  +20  .. +39   TMS payload (20 bytes):
//!                  byte 0: flags (0x01 for first segment, 0x00 for others)
//!                  bytes 10-14: tick_pos (u40 LE) — absolute tick position
//!  +40  .. +47   f64 BPM (little-endian)
//!  +48  .. +51   u32 ticks_per_beat (little-endian; always 960,000)
//!  +52  .. +60   padding
//! ```

use crate::block::Block;
use crate::content_type::ContentType;
use crate::cursor::Cursor;
use crate::types::ZERO_TICKS;

const CONST_TAG: &[u8] = b"Const";
/// Byte offset of f64 BPM from the start of a "Const" record.
const BPM_OFFSET: usize = 40;
/// Byte offset of u32 ticks_per_beat from the start of a "Const" record.
const TPB_OFFSET: usize = 48;
/// Byte offset of the u40 tick position from the start of a "Const" record.
const TICK_OFFSET: usize = 30;

/// A single constant-tempo segment.
#[derive(Debug, Clone)]
pub struct TempoSegment {
    /// Tick position (relative to `ZERO_TICKS`) where this segment starts.
    pub tick_start: u64,
    /// Pre-computed sample position where this segment starts.
    pub sample_start: u64,
    /// Beats per minute.
    pub bpm: f64,
    /// Ticks per beat (960,000 in all observed sessions).
    pub ticks_per_beat: u64,
}

/// Parse the tempo map from the first `0x2028` block found in the block tree.
///
/// Returns a sorted, non-empty list of tempo segments. If no tempo block is
/// found or it cannot be parsed, falls back to a single 120 BPM segment.
pub fn parse_tempo_map(
    blocks: &[Block],
    cursor: &Cursor<'_>,
    sample_rate: u32,
) -> Vec<TempoSegment> {
    let tempo_block = match find_block_recursive(blocks, ContentType::TempoBlock) {
        Some(b) => b,
        None => return default_tempo_map(sample_rate),
    };

    let data = cursor.data();
    let block_start = tempo_block.offset + 2; // skip 2-byte content-type header
    let block_end = (block_start + tempo_block.block_size as usize).min(data.len());

    if block_start >= block_end {
        return default_tempo_map(sample_rate);
    }

    // Scan for every "Const" occurrence within this block's data range.
    let mut segments: Vec<TempoSegment> = Vec::new();
    let mut search = block_start;

    while search + CONST_TAG.len() <= block_end {
        match data[search..block_end]
            .windows(CONST_TAG.len())
            .position(|w| w == CONST_TAG)
        {
            None => break,
            Some(rel) => {
                let const_pos = search + rel;
                search = const_pos + CONST_TAG.len();

                // tick_pos at const_pos + TICK_OFFSET (u40 LE)
                let tick_end = const_pos + TICK_OFFSET + 5;
                if tick_end > block_end {
                    continue;
                }
                let tick_bytes = &data[const_pos + TICK_OFFSET..const_pos + TICK_OFFSET + 5];
                let tick_abs = u64::from_le_bytes([
                    tick_bytes[0],
                    tick_bytes[1],
                    tick_bytes[2],
                    tick_bytes[3],
                    tick_bytes[4],
                    0,
                    0,
                    0,
                ]);
                let tick_start = tick_abs.saturating_sub(ZERO_TICKS);

                // f64 BPM at const_pos + BPM_OFFSET
                let bpm_end = const_pos + BPM_OFFSET + 8;
                if bpm_end > block_end {
                    continue;
                }
                let bpm_bytes: [u8; 8] = data[const_pos + BPM_OFFSET..bpm_end].try_into().unwrap();
                let bpm = f64::from_le_bytes(bpm_bytes);

                if !(10.0..=400.0).contains(&bpm) {
                    continue;
                }

                // u32 ticks_per_beat at const_pos + TPB_OFFSET
                let tpb_end = const_pos + TPB_OFFSET + 4;
                let ticks_per_beat = if tpb_end <= block_end {
                    let tpb_bytes: [u8; 4] =
                        data[const_pos + TPB_OFFSET..tpb_end].try_into().unwrap();
                    u32::from_le_bytes(tpb_bytes) as u64
                } else {
                    960_000
                };

                if ticks_per_beat == 0 {
                    continue;
                }

                segments.push(TempoSegment {
                    tick_start,
                    sample_start: 0, // filled in below
                    bpm,
                    ticks_per_beat,
                });
            }
        }
    }

    if segments.is_empty() {
        return default_tempo_map(sample_rate);
    }

    // Sort by tick_start and remove duplicates (the file stores two identical
    // copies of the 0x2028 block; we'll hit each "Const" twice if we searched
    // the full block tree — deduplicate).
    segments.sort_by_key(|s| s.tick_start);
    segments.dedup_by_key(|s| s.tick_start);

    // Pre-compute sample_start for each segment by accumulating from previous.
    compute_sample_starts(&mut segments, sample_rate);

    segments
}

/// Convert an absolute tick position (relative to `ZERO_TICKS`) to samples.
pub fn tick_to_sample(relative_tick: u64, segments: &[TempoSegment], sample_rate: u32) -> u64 {
    if segments.is_empty() {
        return 0;
    }

    // Find the last segment whose tick_start <= relative_tick.
    let seg = segments
        .iter()
        .rev()
        .find(|s| s.tick_start <= relative_tick)
        .unwrap_or(&segments[0]);

    let elapsed_ticks = relative_tick.saturating_sub(seg.tick_start);
    let elapsed_samples = (elapsed_ticks as f64 / (seg.ticks_per_beat as f64 * seg.bpm / 60.0)
        * sample_rate as f64) as u64;

    seg.sample_start + elapsed_samples
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn compute_sample_starts(segments: &mut [TempoSegment], sample_rate: u32) {
    let mut cumulative_samples: u64 = 0;
    let mut prev_tick: u64 = 0;
    let mut prev_bpm: f64 = segments[0].bpm;
    let mut prev_tpb: u64 = segments[0].ticks_per_beat;

    for seg in segments.iter_mut() {
        let elapsed_ticks = seg.tick_start.saturating_sub(prev_tick);
        cumulative_samples += (elapsed_ticks as f64 / (prev_tpb as f64 * prev_bpm / 60.0)
            * sample_rate as f64) as u64;

        seg.sample_start = cumulative_samples;
        prev_tick = seg.tick_start;
        prev_bpm = seg.bpm;
        prev_tpb = seg.ticks_per_beat;
    }
}

fn default_tempo_map(sample_rate: u32) -> Vec<TempoSegment> {
    let _ = sample_rate;
    vec![TempoSegment {
        tick_start: 0,
        sample_start: 0,
        bpm: 120.0,
        ticks_per_beat: 960_000,
    }]
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
