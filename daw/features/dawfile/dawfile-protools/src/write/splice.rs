//! Byte-level splice operations on the decrypted buffer.
//!
//! The fundamental challenge of modifying binary formats: inserting or removing
//! bytes shifts everything after the edit point. Every parent block's `block_size`
//! field must be updated, and the block tree's offset fields must be refreshed.
//!
//! This module provides a [`splice`] operation that handles all of this:
//! 1. Replace a byte range with new bytes (possibly different length)
//! 2. Walk up the block tree, adjusting every ancestor's `block_size` field
//! 3. Rebuild the block tree from the modified buffer

use crate::cursor::Cursor;
use crate::raw_block::{self, RawSession};

/// Replace bytes at `range` with `replacement` in the session buffer.
///
/// This is the core primitive for variable-length modifications. It:
/// 1. Splices the bytes in the buffer
/// 2. Finds all ancestor blocks that contain the edit point
/// 3. Updates their `block_size` fields in the buffer
/// 4. Rebuilds the block tree
///
/// Returns the size delta (positive = grew, negative = shrunk).
pub fn splice(session: &mut RawSession, offset: usize, old_len: usize, replacement: &[u8]) -> i64 {
    let new_len = replacement.len();
    let delta = new_len as i64 - old_len as i64;

    if delta == 0 {
        // Same size — just overwrite, no structural changes needed
        session.data[offset..offset + old_len].copy_from_slice(replacement);
        return 0;
    }

    // Find all ancestor blocks that contain this offset BEFORE we modify the buffer.
    // An ancestor is any block where `block.start < offset < block.end`.
    let ancestors = find_ancestors(&session.blocks, offset);

    // Perform the splice on the raw buffer
    let end = offset + old_len;
    let mut new_data = Vec::with_capacity((session.data.len() as i64 + delta) as usize);
    new_data.extend_from_slice(&session.data[..offset]);
    new_data.extend_from_slice(replacement);
    new_data.extend_from_slice(&session.data[end..]);
    session.data = new_data;

    // Update block_size fields for all ancestors.
    // Each ancestor's size grows/shrinks by delta.
    // The block_size field is at `block.start + 3` (4 bytes, after magic + block_type).
    for ancestor_start in &ancestors {
        let size_offset = ancestor_start + 3;
        if size_offset + 4 <= session.data.len() {
            let cursor = Cursor::new(&session.data, session.is_bigendian);
            let old_size = cursor.u32_at(size_offset);
            let new_size = (old_size as i64 + delta) as u32;
            session.write_u32(size_offset, new_size);
        }
    }

    // Rebuild the block tree from the modified buffer
    session.blocks = raw_block::parse_raw_blocks_pub(&session.data, session.is_bigendian);

    delta
}

/// Replace a length-prefixed string in the buffer.
///
/// Handles the common pattern: `u32(length) + bytes`. Updates the length
/// prefix and splices the string data, fixing all ancestor block sizes.
///
/// `string_offset` should point to the u32 length prefix.
///
/// Returns the size delta.
pub fn replace_string(session: &mut RawSession, string_offset: usize, new_value: &str) -> i64 {
    let cursor = Cursor::new(&session.data, session.is_bigendian);
    let old_len = cursor.u32_at(string_offset) as usize;
    let data_start = string_offset + 4;

    let new_bytes = new_value.as_bytes();
    let new_len = new_bytes.len();

    // Update the length prefix
    session.write_u32(string_offset, new_len as u32);

    // Splice the string data
    splice(session, data_start, old_len, new_bytes)
}

/// Find all ancestor block `start` positions that contain the given offset.
/// Returns them from outermost (root) to innermost (leaf).
fn find_ancestors(blocks: &[crate::raw_block::RawBlock], offset: usize) -> Vec<usize> {
    let mut result = Vec::new();
    find_ancestors_recursive(blocks, offset, &mut result);
    result
}

fn find_ancestors_recursive(
    blocks: &[crate::raw_block::RawBlock],
    offset: usize,
    out: &mut Vec<usize>,
) {
    for block in blocks {
        if offset > block.start && offset < block.end {
            out.push(block.start);
            find_ancestors_recursive(&block.children, offset, out);
            return; // offset can only be inside one block at each level
        }
    }
}
