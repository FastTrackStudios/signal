//! Plugin / insert list parsing.
//!
//! ## Block 0x1017 — PluginEntry
//!
//! Layout (relative to `block.offset`, which points at the content_type field):
//! ```text
//! [0..1]   u16  content_type = 0x1017
//! [2]      u8   slot_index (0xFF = empty / unoccupied slot)
//! [3]      u8   name_len
//! [4..6]   [u8;3]  padding (always zero)
//! [7..7+N] [u8;N]  display name (N = name_len)
//! [7+N..7+N+12]  [u8;12]  plugin type code (3 × 4-byte OSType, stored byte-reversed)
//!                           bytes  0-3: manufacturer code (e.g. "Digi", "Srdx")
//!                           bytes  4-7: plugin type 4CC   (e.g. "Rvrb", "muto")
//!                           bytes  8-11: plugin subtype   (e.g. "Poly")
//! [7+N+12]      u8  input_channel_count  (1=mono, 2=stereo)
//! [7+N+13]      u8  input_stereo_flag
//! [7+N+14]      u8  output_channel_count (1=mono, 2=stereo)
//! [7+N+15]      u8  output_stereo_flag
//! [7+N+16..18]  [u8;3]  unknown flags
//! [7+N+19..22]  u32     aax_id_len  (byte count of AAX bundle ID that follows)
//! [7+N+23..]    [u8]    AAX bundle ID (e.g. "com.avid.aax.dverb")
//! [7+N+23+M..]  [u8;12] trailing metadata
//! ```
//!
//! Empty slots: slot_index = 0xFF, name_len = 0, rest is zeros.
//!
//! ## Block 0x1018 — PluginList
//!
//! A single top-level container that holds all `PluginEntry` children for the
//! session (one global list, not per-track).

use crate::block::Block;
use crate::content_type::ContentType;
use crate::cursor::Cursor;
use crate::types::PluginEntry;

/// Minimum size of a non-empty plugin entry payload (beyond content_type).
const MIN_REAL_ENTRY_PAYLOAD: usize = 3 + 12 + 7 + 4; // flags + code + channels + aax_len

/// Parse the session-wide plugin list from the `0x1018` (PluginList) block.
///
/// Returns all occupied plugin entries (empty slots with `slot_index == 0xFF`
/// are skipped). The list is the global registry of all plugin types used
/// anywhere in the session.
pub fn parse_plugins(blocks: &[Block], cursor: &Cursor<'_>) -> Vec<PluginEntry> {
    let plugin_list = match find_block_recursive(blocks, ContentType::PluginList) {
        Some(b) => b,
        None => return Vec::new(),
    };

    let mut entries = Vec::new();
    for child in &plugin_list.children {
        if child.content_type != Some(ContentType::PluginEntry) {
            continue;
        }
        if let Some(entry) = parse_plugin_entry(child, cursor) {
            entries.push(entry);
        }
    }
    entries
}

fn parse_plugin_entry(block: &Block, cursor: &Cursor<'_>) -> Option<PluginEntry> {
    let data = cursor.data();
    let base = block.offset; // points at content_type field (u16)

    // Need at least 4 bytes for ct + slot + name_len
    if base + 4 > data.len() {
        return None;
    }

    let slot_index = data[base + 2];
    let name_len = data[base + 3] as usize;

    // 0xFF = empty slot
    if slot_index == 0xFF {
        return None;
    }

    // Validate we have enough room for the fixed fields
    let fixed_end = base + 7 + name_len + MIN_REAL_ENTRY_PAYLOAD;
    if fixed_end > data.len() {
        return None;
    }

    // Display name
    let name_start = base + 7;
    let name = String::from_utf8_lossy(&data[name_start..name_start + name_len]).into_owned();

    // 12-byte plugin type code (3 reversed 4CCs)
    let code_start = name_start + name_len;
    let mut manufacturer_4cc = [0u8; 4];
    let mut type_4cc = [0u8; 4];
    let mut subtype_4cc = [0u8; 4];
    if code_start + 12 <= data.len() {
        // Each 4CC is stored with bytes reversed; reverse to get canonical form.
        manufacturer_4cc.copy_from_slice(&data[code_start..code_start + 4]);
        manufacturer_4cc.reverse();
        type_4cc.copy_from_slice(&data[code_start + 4..code_start + 8]);
        type_4cc.reverse();
        subtype_4cc.copy_from_slice(&data[code_start + 8..code_start + 12]);
        subtype_4cc.reverse();
    }

    // Channel I/O info (7 bytes after the type code)
    let ch_base = code_start + 12;
    let input_channels = data.get(ch_base).copied().unwrap_or(1);
    let output_channels = data.get(ch_base + 2).copied().unwrap_or(1);

    // AAX bundle ID (u32 length + bytes)
    let aax_len_base = ch_base + 7;
    if aax_len_base + 4 > data.len() {
        return Some(PluginEntry {
            slot_index,
            name,
            manufacturer_4cc,
            type_4cc,
            subtype_4cc,
            input_channels,
            output_channels,
            aax_bundle_id: String::new(),
        });
    }

    let aax_id_len =
        u32::from_le_bytes(data[aax_len_base..aax_len_base + 4].try_into().unwrap()) as usize;

    let aax_id_start = aax_len_base + 4;
    let aax_bundle_id = if aax_id_len > 0 && aax_id_start + aax_id_len <= data.len() {
        String::from_utf8_lossy(&data[aax_id_start..aax_id_start + aax_id_len]).into_owned()
    } else {
        String::new()
    };

    Some(PluginEntry {
        slot_index,
        name,
        manufacturer_4cc,
        type_4cc,
        subtype_4cc,
        input_channels,
        output_channels,
        aax_bundle_id,
    })
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
