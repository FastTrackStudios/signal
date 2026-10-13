//! Block tree parser for Pro Tools session files.
//!
//! After decryption, a Pro Tools session is a tree of blocks. Each block
//! starts with magic byte `0x5A`, followed by type, size, content type,
//! and payload (which may contain child blocks).

use crate::content_type::ContentType;
use crate::cursor::Cursor;

/// Magic byte that marks the start of every block.
const BLOCK_MAGIC: u8 = 0x5A;

/// A parsed block from the session file.
#[derive(Debug, Clone)]
pub struct Block {
    /// Offset into the decrypted buffer where block content starts
    /// (points to the content_type field, i.e. `pos + 7`).
    pub offset: usize,
    /// Raw block type (u16).
    pub block_type: u16,
    /// Total size of block payload.
    pub block_size: u32,
    /// The content type identifier.
    pub content_type: Option<ContentType>,
    /// Raw content type value (preserved even if unknown).
    pub content_type_raw: u16,
    /// Child blocks nested within this block's payload.
    pub children: Vec<Block>,
}

impl Block {
    /// Find the first direct child with a given content type.
    pub fn find_child(&self, ct: ContentType) -> Option<&Block> {
        self.children.iter().find(|c| c.content_type == Some(ct))
    }

    /// Find all direct children with a given content type.
    pub fn find_children(&self, ct: ContentType) -> Vec<&Block> {
        self.children
            .iter()
            .filter(|c| c.content_type == Some(ct))
            .collect()
    }

    /// Recursively find all descendants with a given content type.
    pub fn find_all(&self, ct: ContentType) -> Vec<&Block> {
        let mut result = Vec::new();
        self.collect_all(ct, &mut result);
        result
    }

    fn collect_all<'a>(&'a self, ct: ContentType, out: &mut Vec<&'a Block>) {
        if self.content_type == Some(ct) {
            out.push(self);
        }
        for child in &self.children {
            child.collect_all(ct, out);
        }
    }
}

/// Parse the entire block tree from decrypted data.
///
/// Starts at byte 20 (after the cleartext header) and scans forward,
/// building a flat list of top-level blocks (each with nested children).
pub fn parse_blocks(data: &[u8], is_bigendian: bool) -> Vec<Block> {
    let mut blocks = Vec::new();
    parse_blocks_recursive(data, 20, data.len(), is_bigendian, &mut blocks);
    blocks
}

fn parse_blocks_recursive(
    data: &[u8],
    start: usize,
    end: usize,
    is_bigendian: bool,
    out: &mut Vec<Block>,
) {
    let mut pos = start;

    while pos < end {
        if data[pos] != BLOCK_MAGIC {
            pos += 1;
            continue;
        }

        // Need at least 9 bytes for the block header
        if pos + 9 > end {
            break;
        }

        let cursor = Cursor::new(data, is_bigendian);

        let block_type = cursor.u16_at(pos + 1);
        let block_size = cursor.u32_at(pos + 3);
        let content_type_raw = cursor.u16_at(pos + 7);

        // Validation: high byte of block_type must be zero
        if block_type & 0xFF00 != 0 {
            pos += 1;
            continue;
        }

        // Validation: block must fit within parent bounds
        let block_end = pos + 7 + block_size as usize;
        if block_end > end {
            pos += 1;
            continue;
        }

        let content_type = ContentType::from_raw(content_type_raw);

        // Offset points to content_type field (pos + 7), matching ptformat convention
        let offset = pos + 7;

        // Parse children within the payload area (after the 9-byte header)
        let children_start = pos + 9;
        let mut children = Vec::new();
        if children_start < block_end {
            parse_blocks_recursive(data, children_start, block_end, is_bigendian, &mut children);
        }

        out.push(Block {
            offset,
            block_type,
            block_size,
            content_type,
            content_type_raw,
            children,
        });

        pos = block_end;
    }
}
