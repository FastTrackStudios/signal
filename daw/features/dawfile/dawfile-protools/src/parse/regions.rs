//! Audio region parsing.
//!
//! Regions are parsed from block types 0x100b (PT 5-9) or 0x262a (PT 10+).
//! Each region has a name, index, and three-point position data (start,
//! sample offset, length) encoded with variable-width integers.

use crate::block::Block;
use crate::content_type::ContentType;
use crate::cursor::{self, Cursor};
use crate::types::AudioRegion;

/// Parse all audio regions from the block tree.
pub fn parse_audio_regions(
    blocks: &[Block],
    cursor: &Cursor<'_>,
    version: u16,
    rate_factor: f64,
) -> Vec<AudioRegion> {
    let mut regions = Vec::new();

    // Choose the region list block type based on version
    let (list_ct, region_ct) = if version < 10 {
        (ContentType::AudioRegionListOld, ContentType::AudioRegionOld)
    } else {
        (ContentType::AudioRegionListNew, ContentType::AudioRegionNew)
    };

    let region_list = match find_block_recursive(blocks, list_ct) {
        Some(b) => b,
        None => return regions,
    };

    let region_blocks = region_list.find_all(region_ct);

    for (idx, block) in region_blocks.iter().enumerate() {
        if let Some(region) = parse_single_region(block, cursor, idx as u16, rate_factor) {
            regions.push(region);
        }
    }

    regions
}

/// Parse a single audio region from its block.
fn parse_single_region(
    block: &Block,
    cursor: &Cursor<'_>,
    index: u16,
    rate_factor: f64,
) -> Option<AudioRegion> {
    let data = cursor.data();

    // Region name is a length-prefixed string at offset + 11
    let name_offset = block.offset + 11;
    if name_offset + 4 >= data.len() {
        return None;
    }

    let (name, str_consumed) = cursor.length_prefixed_string(name_offset);

    // Three-point data starts immediately after the name (str_consumed already includes the 4-byte prefix)
    let three_point_offset = name_offset + str_consumed;
    if three_point_offset + 20 >= data.len() {
        return None;
    }

    let (start, sample_offset, length) = cursor::parse_three_point(cursor, three_point_offset);

    // Audio file index: legacy zero placeholder. The real region→file
    // link is the 6-byte UID at payload `+54..+60` of the inner
    // `0x2628` sub-block (discovered via Frida byte-read trace on
    // the LotF session — see `docs/converter-frida-discovered-offsets.md`).
    let audio_file_index = 0u16;

    // Extract the 6-byte source-file UID from the inner 0x2628.
    // The UID lives at block-magic offset +54 (= block.offset + 47,
    // since block.offset = magic + 7). NOTE: the +54 is offset from
    // the 0x5A magic byte, NOT from payload start.
    let mut source_file_uid: Option<[u8; 6]> = None;
    fn find_2628_magic(b: &Block) -> Option<usize> {
        if b.content_type_raw == 0x2628 {
            return Some(b.offset.saturating_sub(7));
        }
        for c in &b.children {
            if let Some(p) = find_2628_magic(c) {
                return Some(p);
            }
        }
        None
    }
    if let Some(magic) = find_2628_magic(block) {
        let uid_at = magic + 54;
        if uid_at + 6 <= data.len() {
            let mut uid = [0u8; 6];
            uid.copy_from_slice(&data[uid_at..uid_at + 6]);
            source_file_uid = Some(uid);
        }
    }

    Some(AudioRegion {
        name,
        index,
        start_pos: (start as f64 * rate_factor) as u64,
        sample_offset: (sample_offset as f64 * rate_factor) as u64,
        length: (length as f64 * rate_factor) as u64,
        audio_file_index,
        source_file_uid,
    })
}

/// Recursively find a block by content type.
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
