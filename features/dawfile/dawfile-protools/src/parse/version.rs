//! Pro Tools version detection.
//!
//! The version is determined from the first block in the file. There are
//! two formats: old (content_type 0x0003) and new (content_type 0x2067).

use crate::block::Block;
use crate::content_type::ContentType;
use crate::cursor::Cursor;
use crate::error::{PtError, PtResult};

/// The BITCODE signature string found in valid PT files.
const BITCODE: &[u8] = b"0010111100101011";

/// Detect the Pro Tools version from the decrypted file data.
///
/// The XOR type byte determines which format to expect:
/// - `0x01`: PT 5-9 (old format, uses VersionInfoOld block 0x0003)
/// - `0x05`: PT 10-12 (new format, uses SessionInfo block 0x2067)
pub fn parse_version(cursor: &Cursor<'_>, blocks: &[Block], xor_type: u8) -> PtResult<u16> {
    let data = cursor.data();

    // Validate signature: byte 0 must be 0x03, or BITCODE must appear near the start
    let has_signature = data[0] == 0x03;
    let has_bitcode = data.len() > 256 && data[1..257].windows(BITCODE.len()).any(|w| w == BITCODE);

    if !has_signature && !has_bitcode {
        return Err(PtError::InvalidSignature);
    }

    // Try to find a version block — strategy depends on xor_type
    if let Some(version) = try_block_version(cursor, blocks, xor_type) {
        return Ok(version);
    }

    // Fallback: read version directly from fixed offsets
    let candidates = [0x40usize, 0x3D, 0x3A];
    for &offset in &candidates {
        if offset < data.len() {
            let v = data[offset] as u16;
            if v > 0 {
                let version = if offset == 0x3A { v + 2 } else { v };
                if (5..=12).contains(&version) {
                    return Ok(version);
                }
            }
        }
    }

    Err(PtError::ParseError {
        offset: 0,
        message: "could not determine Pro Tools version".into(),
    })
}

/// Try to extract version from a block at the start of the file.
///
/// For old format (xor_type 0x01), look for VersionInfoOld (0x0003) first.
/// For new format (xor_type 0x05), look for SessionInfo (0x2067) first.
/// Both block types may be present in any file — the xor_type tells us which
/// one to trust.
fn try_block_version(cursor: &Cursor<'_>, blocks: &[Block], xor_type: u8) -> Option<u16> {
    // For old format, try VersionInfoOld first
    if xor_type == 0x01
        && let Some(v) = try_version_info_old(cursor, blocks)
    {
        return Some(v);
    }

    // For new format (or old format fallback), try SessionInfo
    if let Some(v) = try_session_info(cursor, blocks) {
        return Some(v);
    }

    // For new format, try VersionInfoOld as fallback
    if xor_type != 0x01
        && let Some(v) = try_version_info_old(cursor, blocks)
    {
        return Some(v);
    }

    None
}

/// Extract version from VersionInfoOld block (0x0003).
///
/// Layout: content_type(2) + flag(1) + length_prefixed_string + u32(skip) + u32(version)
fn try_version_info_old(cursor: &Cursor<'_>, blocks: &[Block]) -> Option<u16> {
    for block in blocks.iter().take(5) {
        if block.content_type != Some(ContentType::VersionInfoOld) {
            continue;
        }
        // String starts at offset + 3 (after content_type + 1 flag byte)
        let str_start = block.offset + 3;
        if str_start + 4 >= cursor.len() {
            continue;
        }
        let (_, str_consumed) = cursor.length_prefixed_string(str_start);
        // After the string: 4 bytes of unknown field, then 4 bytes of version
        let version_offset = str_start + str_consumed + 4;
        if version_offset + 4 <= cursor.len() {
            let v = cursor.u32_at(version_offset) as u16;
            if (5..=12).contains(&v) {
                return Some(v);
            }
        }
    }
    None
}

/// Extract version from SessionInfo block (0x2067).
fn try_session_info(cursor: &Cursor<'_>, blocks: &[Block]) -> Option<u16> {
    for block in blocks.iter().take(5) {
        if block.content_type != Some(ContentType::SessionInfo) {
            continue;
        }
        let version_offset = block.offset + 20;
        if version_offset + 4 <= cursor.len() {
            let v = cursor.u32_at(version_offset) as u16 + 2;
            if (5..=12).contains(&v) {
                return Some(v);
            }
        }
    }
    None
}
