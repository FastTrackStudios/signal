//! Primitives for constructing and inserting new blocks.
//!
//! The base [`splice`](super::splice::splice) operation can replace any byte
//! range; this module builds on top of it to provide block-aware operations
//! that are convenient for higher-level writers.
//!
//! # Block layout reminder
//!
//! ```text
//! +0      u8       magic byte (0x5A)
//! +1..+2  u16 LE   block_type (always 0x01..0xFF in PT; high byte == 0)
//! +3..+6  u32 LE   block_size (= byte count from +7 onward — i.e. content_type + payload)
//! +7..+8  u16 LE   content_type
//! +9..    bytes    payload (may contain nested 0x5A-prefixed children)
//! ```
//!
//! `end = start + 7 + block_size`. The full block including the magic header
//! occupies `9 + payload_bytes_after_content_type` bytes total.

use crate::content_type::ContentType;
use crate::raw_block::{RawBlock, RawSession};
use crate::write::splice::splice;

/// Wrap a content_type + payload pair into a full block frame.
///
/// Produces bytes ready for [`insert_block_at`] or [`splice`]. The block_type
/// is hard-coded to `0x01` (the value used by every observed block in the
/// format); pass a different value via [`wrap_as_block_with_type`] if needed.
///
/// The size field is computed automatically: `block_size = 2 (content_type)
/// + payload.len()`.
pub fn wrap_as_block(ct: u16, payload: &[u8]) -> Vec<u8> {
    wrap_as_block_with_type(0x01, ct, payload)
}

/// Like [`wrap_as_block`] but allows an explicit block_type field. The
/// caller must know what they're doing — only `0x01` has been observed in
/// PT 5..12 sessions.
pub fn wrap_as_block_with_type(block_type: u16, ct: u16, payload: &[u8]) -> Vec<u8> {
    let block_size = (2 + payload.len()) as u32;
    let mut out = Vec::with_capacity(9 + payload.len());
    out.push(0x5A);
    out.extend_from_slice(&block_type.to_le_bytes());
    out.extend_from_slice(&block_size.to_le_bytes());
    out.extend_from_slice(&ct.to_le_bytes());
    out.extend_from_slice(payload);
    out
}

/// Append a new block as a child of `parent`, placed at the end of the
/// parent's payload.
///
/// `parent` is identified by its `start` offset (matching `RawBlock.start`).
/// Returns the size delta (positive = grew). The session's block tree is
/// rebuilt; existing `&RawBlock` references are invalidated.
///
/// # Errors
///
/// Returns `None` if no block starts at `parent_start`.
pub fn append_child_block(
    session: &mut RawSession,
    parent_start: usize,
    new_block_bytes: &[u8],
) -> Option<i64> {
    let parent = find_block_at(&session.blocks, parent_start)?;
    let insertion_point = parent.end;
    Some(splice(session, insertion_point, 0, new_block_bytes))
}

/// Insert a new block immediately before the block at `target_start`.
///
/// Useful for prepending entries to a flat list block where sibling order
/// matters. The new block becomes a sibling, not a child, of the target.
pub fn insert_block_before(
    session: &mut RawSession,
    target_start: usize,
    new_block_bytes: &[u8],
) -> i64 {
    splice(session, target_start, 0, new_block_bytes)
}

/// Remove the entire block at `block_start` (including header).
///
/// Returns the size delta (negative). Ancestor block_size fields are updated
/// by [`splice`].
pub fn remove_block(session: &mut RawSession, block_start: usize) -> Option<i64> {
    let block = find_block_at(&session.blocks, block_start)?;
    let old_len = block.end - block.start;
    Some(splice(session, block.start, old_len, &[]))
}

/// Look up a block by its `start` byte position, anywhere in the tree.
pub fn find_block_at(blocks: &[RawBlock], start: usize) -> Option<&RawBlock> {
    for b in blocks {
        if b.start == start {
            return Some(b);
        }
        if start > b.start && start < b.end {
            return find_block_at(&b.children, start);
        }
    }
    None
}

/// Find the first top-level (or deeply nested) block of the given content
/// type. Convenience wrapper around `RawSession::find_block`.
pub fn find_first_block(session: &RawSession, ct: ContentType) -> Option<&RawBlock> {
    session.find_block(ct)
}

/// Collect every block of the given content type, descending recursively.
pub fn find_all_blocks(blocks: &[RawBlock], ct: u16) -> Vec<&RawBlock> {
    let mut out = Vec::new();
    fn walk<'a>(blocks: &'a [RawBlock], ct: u16, out: &mut Vec<&'a RawBlock>) {
        for b in blocks {
            if b.content_type_raw == ct {
                out.push(b);
            }
            walk(&b.children, ct, out);
        }
    }
    walk(blocks, ct, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_produces_correct_header() {
        // CT 0x4501, 4-byte payload "test"
        let block = wrap_as_block(0x4501, b"test");
        assert_eq!(block.len(), 9 + 4, "header (9) + payload (4)");
        assert_eq!(block[0], 0x5A, "magic byte");
        assert_eq!(&block[1..3], &[0x01, 0x00], "block_type = 1");
        assert_eq!(
            u32::from_le_bytes(block[3..7].try_into().unwrap()),
            2 + 4,
            "block_size = 2 (ct) + 4 (payload)"
        );
        assert_eq!(&block[7..9], &[0x01, 0x45], "content_type 0x4501 LE");
        assert_eq!(&block[9..], b"test", "payload");
    }

    #[test]
    fn wrap_empty_payload() {
        let block = wrap_as_block(0x2030, &[]);
        assert_eq!(block.len(), 9);
        assert_eq!(
            u32::from_le_bytes(block[3..7].try_into().unwrap()),
            2,
            "block_size = 2 even for empty payload"
        );
    }
}
