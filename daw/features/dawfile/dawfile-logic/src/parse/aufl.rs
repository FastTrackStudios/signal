//! AuFl chunk parser — audio file pool entry.
//!
//! ## Binary layout (sequential, no padding between fields)
//!
//! | Bytes   | Field                  | Type       | Notes                                   |
//! |---------|------------------------|------------|-----------------------------------------|
//! | 0–1     | `size`                 | u16 LE     | In-memory struct size (0x390 = 912)     |
//! | 2–3     | `_unused_isort`        | u16 LE     | Old sort field, always 0                |
//! | 4–7     | `lSize`                | i32 LE     | Same as `size` cast to i32              |
//! | 8–9     | filename `length`      | u16 LE     | Number of UTF-16 code units             |
//! | 10..    | filename `unicode`     | UTF-16LE   | `length * 2` bytes                      |
//! | +0      | `magic`                | i32 LE     | `0x4155464c` = 'AUFL' reversed          |
//! | +4      | `usable`               | u8         |                                         |
//! | +5      | `flags`                | u8         |                                         |
//! | +6      | `legacy_machine`       | u8         |                                         |
//! | +7      | `mSelected`            | u8         |                                         |
//! | +8–9    | unused                 | 2 bytes    |                                         |
//! | +10     | `__unused_bytes`       | 124 bytes  | Padding                                 |
//! | +134    | `compressionType`      | i32 LE     |                                         |
//! | +138    | `volname`              | 256 bytes  | UTF-8 relative path to audio folder     |
//! | …       | flags, format, etc.    | various    | Not parsed further here                 |
//!
//! Variable-length blobs (mElasticPitchAnalysis, pFileIO, etc.) appear later;
//! we stop parsing at the fixed header fields we need.

use crate::error::{LogicError, LogicResult};

/// Key metadata extracted from an `AuFl` chunk payload.
#[derive(Debug, Clone)]
pub struct AudioFileEntry {
    /// Filename as stored in the bundle (e.g. `"Audio Track 1 #01.wav"`).
    pub filename: String,
    /// Audio folder path (from volname, e.g. `"Audio Files"`).
    pub vol_name: String,
    /// Whether this file entry is marked usable.
    pub usable: bool,
}

/// Parse an `AuFl` (audio file pool) chunk payload.
///
/// Returns `None` if the payload is too short to contain a filename.
pub fn parse_aufl(data: &[u8]) -> Option<AudioFileEntry> {
    // Minimum: 8 bytes header + 2 bytes length field
    if data.len() < 10 {
        return None;
    }

    // Bytes 8–9: UTF-16 filename length (in code units, not bytes)
    let len_chars = u16::from_le_bytes([data[8], data[9]]) as usize;
    let name_start = 10;
    let name_end = name_start + len_chars * 2;

    if data.len() < name_end {
        return None;
    }

    let filename = decode_utf16le(&data[name_start..name_end]);

    // After the filename: magic(4) + usable(1) + flags(1) + legacy_machine(1) + mSelected(1) + unused(2) + __unused_bytes(124) + compressionType(4) + volname(256)
    let after_name = name_end;
    let usable_offset = after_name + 4; // skip magic (4 bytes)
    let volname_offset = after_name + 4 + 1 + 1 + 1 + 1 + 2 + 124 + 4; // magic+usable+flags+legacy+selected+unused+pad+compressionType

    let usable = data.get(usable_offset).copied().unwrap_or(0) != 0;

    let vol_name = if data.len() >= volname_offset + 256 {
        let raw = &data[volname_offset..volname_offset + 256];
        // Null-terminated UTF-8 path
        let end = raw.iter().position(|&b| b == 0).unwrap_or(256);
        String::from_utf8_lossy(&raw[..end]).into_owned()
    } else {
        String::new()
    };

    Some(AudioFileEntry {
        filename,
        vol_name,
        usable,
    })
}

fn decode_utf16le(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}
