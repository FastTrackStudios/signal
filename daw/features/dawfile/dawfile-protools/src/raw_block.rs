//! Raw block tree that preserves original bytes for round-trip fidelity.
//!
//! Unlike [`crate::block::Block`] which extracts structured fields, [`RawBlock`]
//! keeps the original byte spans so that unmodified blocks can be written
//! back verbatim. This is essential for round-trip editing: we only need
//! to understand the blocks we modify; everything else passes through
//! unchanged.

use crate::content_type::ContentType;
use crate::cursor::Cursor;

/// Magic byte that marks the start of every block.
const BLOCK_MAGIC: u8 = 0x5A;

/// A block in the decrypted session file, preserving byte-level fidelity.
///
/// The block occupies bytes `[start..end)` in the decrypted buffer, where:
/// - `start` is the position of the 0x5A magic byte
/// - `end = start + 7 + block_size`
///
/// The payload (between the 9-byte header and `end`) may contain child blocks
/// interspersed with non-block data. Both are preserved.
#[derive(Debug, Clone)]
pub struct RawBlock {
    /// Byte position of the 0x5A magic in the decrypted buffer.
    pub start: usize,
    /// First byte after this block (start + 7 + block_size).
    pub end: usize,
    /// Raw block type (u16).
    pub block_type: u16,
    /// Total size of block payload (from the header).
    pub block_size: u32,
    /// Parsed content type (None if unknown).
    pub content_type: Option<ContentType>,
    /// Raw content type u16 (preserved for unknown types).
    pub content_type_raw: u16,
    /// Child blocks within this block's payload.
    pub children: Vec<RawBlock>,
    /// Byte ranges within the payload that are NOT child blocks (inter-block data).
    /// These are the "gaps" between children that contain raw data fields.
    /// Stored as `(offset, length)` pairs relative to the decrypted buffer.
    pub data_spans: Vec<(usize, usize)>,
}

impl RawBlock {
    /// Get the header bytes (7 bytes: magic + type + size).
    pub fn header_range(&self) -> std::ops::Range<usize> {
        self.start..self.start + 7
    }

    /// Get the content type bytes (2 bytes at start + 7).
    pub fn content_type_range(&self) -> std::ops::Range<usize> {
        self.start + 7..self.start + 9
    }

    /// Get the entire payload range (after 7-byte header, before end).
    pub fn payload_range(&self) -> std::ops::Range<usize> {
        self.start + 7..self.end
    }

    /// Get the full byte range of this block.
    pub fn full_range(&self) -> std::ops::Range<usize> {
        self.start..self.end
    }

    /// Find the first direct child with a given content type.
    pub fn find_child(&self, ct: ContentType) -> Option<&RawBlock> {
        self.children.iter().find(|c| c.content_type == Some(ct))
    }

    /// Find the first direct child with a given content type (mutable).
    pub fn find_child_mut(&mut self, ct: ContentType) -> Option<&mut RawBlock> {
        self.children
            .iter_mut()
            .find(|c| c.content_type == Some(ct))
    }

    /// Recursively find all descendants with a given content type.
    pub fn find_all(&self, ct: ContentType) -> Vec<&RawBlock> {
        let mut result = Vec::new();
        self.collect_all(ct, &mut result);
        result
    }

    fn collect_all<'a>(&'a self, ct: ContentType, out: &mut Vec<&'a RawBlock>) {
        if self.content_type == Some(ct) {
            out.push(self);
        }
        for child in &self.children {
            child.collect_all(ct, out);
        }
    }
}

/// A raw-preserving block tree of an entire decrypted session file.
///
/// This holds the decrypted bytes plus the parsed block tree. Unmodified
/// regions are written back verbatim; only modified blocks need re-serialization.
#[derive(Debug, Clone)]
pub struct RawSession {
    /// The decrypted file bytes (mutable for in-place editing).
    pub data: Vec<u8>,
    /// Whether the file uses big-endian byte order.
    pub is_bigendian: bool,
    /// XOR encryption type (0x01 or 0x05).
    pub xor_type: u8,
    /// XOR encryption seed.
    pub xor_value: u8,
    /// Top-level blocks.
    pub blocks: Vec<RawBlock>,
}

impl RawSession {
    /// Create a cursor for reading values from the decrypted data.
    pub fn cursor(&self) -> Cursor<'_> {
        Cursor::new(&self.data, self.is_bigendian)
    }

    /// Find a block by content type (recursive search).
    pub fn find_block(&self, ct: ContentType) -> Option<&RawBlock> {
        find_recursive(&self.blocks, ct)
    }

    /// Find a block by content type (recursive, mutable).
    pub fn find_block_mut(&mut self, ct: ContentType) -> Option<&mut RawBlock> {
        find_recursive_mut(&mut self.blocks, ct)
    }

    /// Write a u16 at the given offset in the decrypted buffer, respecting endianness.
    pub fn write_u16(&mut self, offset: usize, value: u16) {
        let bytes = if self.is_bigendian {
            value.to_be_bytes()
        } else {
            value.to_le_bytes()
        };
        self.data[offset] = bytes[0];
        self.data[offset + 1] = bytes[1];
    }

    /// Write a u32 at the given offset in the decrypted buffer, respecting endianness.
    pub fn write_u32(&mut self, offset: usize, value: u32) {
        let bytes = if self.is_bigendian {
            value.to_be_bytes()
        } else {
            value.to_le_bytes()
        };
        self.data[offset..offset + 4].copy_from_slice(&bytes);
    }

    /// Write a u64 at the given offset in the decrypted buffer, respecting endianness.
    pub fn write_u64(&mut self, offset: usize, value: u64) {
        let bytes = if self.is_bigendian {
            value.to_be_bytes()
        } else {
            value.to_le_bytes()
        };
        self.data[offset..offset + 8].copy_from_slice(&bytes);
    }

    /// Encrypt the decrypted data back and return the encrypted bytes.
    ///
    /// This is the inverse of decryption. Since XOR is its own inverse,
    /// this is identical to the decrypt operation.
    pub fn encrypt(&self) -> Vec<u8> {
        let mut output = self.data.clone();
        // XOR is self-inverse: encrypt == decrypt with the same key
        let _ = crate::decrypt::decrypt(&mut output);
        output
    }
}

/// Parse a raw session from file bytes, preserving byte-level fidelity.
pub fn parse_raw(mut data: Vec<u8>) -> crate::error::PtResult<RawSession> {
    let xor_type = crate::decrypt::decrypt(&mut data)?;
    let xor_value = data[0x13];
    let is_bigendian = data.get(0x11).copied().unwrap_or(0) != 0;

    let blocks = parse_raw_blocks(&data, 20, data.len(), is_bigendian);

    Ok(RawSession {
        data,
        is_bigendian,
        xor_type,
        xor_value,
        blocks,
    })
}

/// Parse raw blocks from decrypted data (public entry point for rebuild after splice).
pub fn parse_raw_blocks_pub(data: &[u8], is_bigendian: bool) -> Vec<RawBlock> {
    parse_raw_blocks(data, 20, data.len(), is_bigendian)
}

/// Parse raw blocks recursively, tracking data spans between children.
fn parse_raw_blocks(data: &[u8], start: usize, end: usize, is_bigendian: bool) -> Vec<RawBlock> {
    let mut blocks = Vec::new();
    let cursor = Cursor::new(data, is_bigendian);
    let mut pos = start;

    while pos < end {
        if data[pos] != BLOCK_MAGIC {
            pos += 1;
            continue;
        }

        if pos + 9 > end {
            break;
        }

        let block_type = cursor.u16_at(pos + 1);
        let block_size = cursor.u32_at(pos + 3);
        let content_type_raw = cursor.u16_at(pos + 7);

        if block_type & 0xFF00 != 0 {
            pos += 1;
            continue;
        }

        let block_end = pos + 7 + block_size as usize;
        if block_end > end {
            pos += 1;
            continue;
        }

        let content_type = ContentType::from_raw(content_type_raw);
        let children_start = pos + 9;

        // Parse children
        let children = if children_start < block_end {
            parse_raw_blocks(data, children_start, block_end, is_bigendian)
        } else {
            Vec::new()
        };

        // Compute data spans (gaps between children)
        let data_spans = compute_data_spans(children_start, block_end, &children);

        blocks.push(RawBlock {
            start: pos,
            end: block_end,
            block_type,
            block_size,
            content_type,
            content_type_raw,
            children,
            data_spans,
        });

        pos = block_end;
    }

    blocks
}

/// Compute the byte ranges between child blocks (the "raw data" portions).
fn compute_data_spans(
    payload_start: usize,
    payload_end: usize,
    children: &[RawBlock],
) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut cursor = payload_start;

    for child in children {
        if child.start > cursor {
            spans.push((cursor, child.start - cursor));
        }
        cursor = child.end;
    }

    // Trailing data after last child
    if cursor < payload_end {
        spans.push((cursor, payload_end - cursor));
    }

    spans
}

fn find_recursive(blocks: &[RawBlock], ct: ContentType) -> Option<&RawBlock> {
    for block in blocks {
        if block.content_type == Some(ct) {
            return Some(block);
        }
        if let Some(found) = find_recursive(&block.children, ct) {
            return Some(found);
        }
    }
    None
}

fn find_recursive_mut(blocks: &mut [RawBlock], ct: ContentType) -> Option<&mut RawBlock> {
    for block in blocks {
        if block.content_type == Some(ct) {
            return Some(block);
        }
        if let Some(found) = find_recursive_mut(&mut block.children, ct) {
            return Some(found);
        }
    }
    None
}
