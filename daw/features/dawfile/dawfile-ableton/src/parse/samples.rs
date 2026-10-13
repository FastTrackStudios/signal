//! Sample reference parsing.
//!
//! Ableton stores sample paths differently across versions:
//!
//! - **v11+**: Direct `<Path Value="..." />` and `<RelativePath Value="..." />`
//! - **Pre-v11**: Hex-encoded UTF-16LE in `<Data>` element, structured
//!   `<RelativePathElement>` children, and `<Name>` for filename.
//!
//! Mac files may use Apple Alias v2/v3 binary format in `<Data>` instead
//! of hex-encoded UTF-16LE paths.

use super::xml_helpers::*;
use crate::types::{AbletonVersion, SampleRef};
use roxmltree::Node;
use std::path::PathBuf;

/// Parse a `<SampleRef>` element.
pub fn parse_sample_ref(node: Node<'_, '_>, version: &AbletonVersion) -> SampleRef {
    let file_ref = child(node, "FileRef");

    let last_mod_date = child_value_parse::<u64>(node, "LastModDate");
    let default_duration = child_value_parse::<u64>(node, "DefaultDuration");
    let default_sample_rate = child_value_parse::<u32>(node, "DefaultSampleRate");

    let (path, relative_path, name, file_size, crc, live_pack_name, live_pack_id) =
        if let Some(fr) = file_ref {
            parse_file_ref(fr, version)
        } else {
            (None, None, None, None, None, None, None)
        };

    SampleRef {
        path,
        relative_path,
        name,
        file_size,
        crc,
        last_mod_date,
        default_duration,
        default_sample_rate,
        live_pack_name,
        live_pack_id,
    }
}

type FileRefParts = (
    Option<PathBuf>,
    Option<String>,
    Option<String>,
    Option<u64>,
    Option<u32>,
    Option<String>,
    Option<String>,
);

fn parse_file_ref(node: Node<'_, '_>, version: &AbletonVersion) -> FileRefParts {
    let live_pack_name = child_value(node, "LivePackName")
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let live_pack_id = child_value(node, "LivePackId")
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    if version.at_least(11, 0) {
        // v11+ direct path
        let path = child_value(node, "Path")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from);
        let relative_path = child_value(node, "RelativePath")
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let file_size = child_value_parse::<u64>(node, "OriginalFileSize");
        let crc = child_value_parse::<u32>(node, "OriginalCrc");
        let name = path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string());

        (
            path,
            relative_path,
            name,
            file_size,
            crc,
            live_pack_name,
            live_pack_id,
        )
    } else {
        // Pre-v11: hex-encoded path in <Data>, filename in <Name>
        let name = child_value(node, "Name")
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let path = parse_data_path(node);

        // Build relative path from RelativePathElement children
        let relative_path = child(node, "RelativePath").and_then(|rp| {
            let segments: Vec<&str> = rp
                .children()
                .filter(|n| n.has_tag_name("RelativePathElement"))
                .filter_map(|n| n.attribute("Dir"))
                .collect();
            if segments.is_empty() {
                None
            } else {
                let mut path = segments.join("/");
                if let Some(ref n) = name {
                    path.push('/');
                    path.push_str(n);
                }
                Some(path)
            }
        });

        let file_size = descend(node, "SearchHint")
            .and_then(|sh| child_value_parse::<u64>(sh, "FileSize"))
            .filter(|&s| s > 0);
        let crc = descend(node, "SearchHint")
            .and_then(|sh| child_value_parse::<u32>(sh, "Crc"))
            .filter(|&c| c > 0);

        (
            path,
            relative_path,
            name,
            file_size,
            crc,
            live_pack_name,
            live_pack_id,
        )
    }
}

/// Attempt to decode the hex-encoded path in the `<Data>` element (pre-v11).
///
/// The hex data is typically UTF-16LE on Windows or an Apple Alias on macOS.
fn parse_data_path(file_ref_node: Node<'_, '_>) -> Option<PathBuf> {
    let data_node = child(file_ref_node, "Data")?;
    let hex_text: String = data_node
        .text()
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    if hex_text.is_empty() {
        return None;
    }

    let bytes = hex_decode(&hex_text)?;

    // Try UTF-16LE first (Windows paths: every odd byte is 0x00)
    if is_utf16le(&bytes) {
        return decode_utf16le(&bytes);
    }

    // Try Mac Alias / Bookmark format
    if bytes.len() >= 4 {
        // Apple Alias v2 starts with 4 bytes of length, then "book" or specific patterns
        if let Some(path) = try_parse_mac_alias(&bytes) {
            return Some(path);
        }
    }

    None
}

/// Check if bytes look like UTF-16LE (every other byte is 0x00 for ASCII range).
fn is_utf16le(bytes: &[u8]) -> bool {
    if bytes.len() < 4 || !bytes.len().is_multiple_of(2) {
        return false;
    }
    // Check first 10 code units (or fewer)
    let check_len = (bytes.len() / 2).min(10);
    let zero_count = (0..check_len).filter(|&i| bytes[i * 2 + 1] == 0x00).count();
    // If most high bytes are zero, it's likely UTF-16LE ASCII
    zero_count > check_len / 2
}

/// Decode UTF-16LE bytes to a path, stopping at the first null terminator.
fn decode_utf16le(bytes: &[u8]) -> Option<PathBuf> {
    let u16s: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .take_while(|&c| c != 0)
        .collect();

    let s = String::from_utf16(&u16s).ok()?;
    let trimmed = s.trim_end_matches('\0');
    if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}

/// Try to parse a macOS Alias v2 binary blob and extract the POSIX path.
///
/// Mac Alias v2 format:
/// - Bytes 0-3: total record length (big-endian u32)
/// - Bytes 4-5: version (must be 0x0002 for v2)
/// - Bytes 6+: header fields
/// - After the fixed header (~150 bytes): tagged records (tag: u16, length: u16, data)
///
/// We look for tag 0x0012 (full POSIX path) or 0x000E (POSIX path).
/// Falls back to byte-scanning for path prefixes if tag parsing fails.
fn try_parse_mac_alias(bytes: &[u8]) -> Option<PathBuf> {
    // Try tag-based parsing for Alias v2
    if bytes.len() >= 8 {
        let version = u16::from_be_bytes([bytes[4], bytes[5]]);
        if version == 2
            && let Some(path) = parse_alias_v2_tags(bytes)
        {
            return Some(path);
        }
    }

    // Fallback: scan for POSIX path prefixes in the raw bytes
    for prefix in &[
        b"/Volumes/" as &[u8],
        b"/Users/",
        b"/Applications/",
        b"/Library/",
    ] {
        if let Some(start) = find_bytes(bytes, prefix) {
            let end = bytes[start..]
                .iter()
                .position(|&b| b == 0)
                .map(|p| start + p)
                .unwrap_or(bytes.len());
            if let Ok(path_str) = std::str::from_utf8(&bytes[start..end])
                && !path_str.is_empty()
            {
                return Some(PathBuf::from(path_str));
            }
        }
    }

    None
}

/// Parse tagged records in an Apple Alias v2 blob.
fn parse_alias_v2_tags(bytes: &[u8]) -> Option<PathBuf> {
    // Fixed header is ~150 bytes, tagged records follow
    let header_len = 150usize;
    if bytes.len() < header_len + 4 {
        return None;
    }

    let mut offset = header_len;
    let mut best_path: Option<String> = None;

    while offset + 4 <= bytes.len() {
        let tag = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]);
        let length = u16::from_be_bytes([bytes[offset + 2], bytes[offset + 3]]) as usize;
        offset += 4;

        if offset + length > bytes.len() {
            break;
        }

        if tag == 0xFFFF {
            break;
        }

        match tag {
            // 0x0012 = full POSIX path (preferred)
            0x0012 => {
                if let Ok(path) = std::str::from_utf8(&bytes[offset..offset + length]) {
                    let trimmed = path.trim_end_matches('\0');
                    if !trimmed.is_empty() {
                        return Some(PathBuf::from(trimmed));
                    }
                }
            }
            // 0x000E = POSIX path (may be volume-relative)
            0x000E => {
                if let Ok(path) = std::str::from_utf8(&bytes[offset..offset + length]) {
                    let trimmed = path.trim_end_matches('\0');
                    if !trimmed.is_empty() {
                        best_path = Some(trimmed.to_string());
                    }
                }
            }
            _ => {}
        }

        // Align to 2-byte boundary
        offset += length;
        if !offset.is_multiple_of(2) {
            offset += 1;
        }
    }

    best_path.map(PathBuf::from)
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn hex_decode(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    for chunk in hex.as_bytes().chunks(2) {
        let hi = hex_nibble(chunk[0])?;
        let lo = hex_nibble(chunk[1])?;
        bytes.push((hi << 4) | lo);
    }
    Some(bytes)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
