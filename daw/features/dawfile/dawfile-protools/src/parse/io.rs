//! I/O channel list parsing.
//!
//! ## Block 0x1021 — IoChannelEntry
//!
//! Layout (relative to `block.offset`):
//! ```text
//! [0..1]  u16   content_type = 0x1021
//! [2]     u8    io_class  (0x01 = hardware physical interface, 0x02 = output bus)
//! [3]     u8    is_stereo (0x00 = mono, 0x01 = stereo pair)
//! [4..7]  u32   name_len
//! [8..]   [u8]  name (name_len bytes)
//! [8+N]   u8    channel_count (1 = mono, 2 = stereo)
//! [8+N+1..] remaining routing data (variable, not parsed)
//! ```
//!
//! ## Block 0x1022 — IoChannelList
//!
//! Top-level container with `IoChannelEntry` children.

use crate::block::Block;
use crate::content_type::ContentType;
use crate::cursor::Cursor;
use crate::types::IoChannel;

/// Parse the I/O channel list from the `0x1022` (IoChannelList) block.
///
/// Returns names and channel counts for all hardware and bus I/O channels
/// configured in the session.
pub fn parse_io_channels(blocks: &[Block], cursor: &Cursor<'_>) -> Vec<IoChannel> {
    let io_list = match find_block_recursive(blocks, ContentType::IoChannelList) {
        Some(b) => b,
        None => return Vec::new(),
    };

    let mut channels = Vec::new();
    for child in &io_list.children {
        if child.content_type != Some(ContentType::IoChannelEntry) {
            continue;
        }
        if let Some(ch) = parse_io_channel(child, cursor) {
            channels.push(ch);
        }
    }
    channels
}

fn parse_io_channel(block: &Block, cursor: &Cursor<'_>) -> Option<IoChannel> {
    let data = cursor.data();
    let base = block.offset;

    // Need at least 8 bytes: ct(2) + class(1) + stereo(1) + name_len(4)
    if base + 8 > data.len() {
        return None;
    }

    let io_class = data[base + 2];
    let is_stereo = data[base + 3];
    let name_len = u32::from_le_bytes(data[base + 4..base + 8].try_into().unwrap()) as usize;

    if name_len == 0 || base + 8 + name_len > data.len() {
        return None;
    }

    let name = String::from_utf8_lossy(&data[base + 8..base + 8 + name_len]).into_owned();

    // Channel count is the first byte after the name
    let channel_count = if base + 8 + name_len < data.len() {
        data[base + 8 + name_len]
    } else if is_stereo != 0 {
        2
    } else {
        1
    };

    // Clamp to sane values (1 or 2 channels for standard hardware)
    let channel_count = channel_count.clamp(1, 8);

    // UID layout: a 15-byte fixed control region follows the name,
    // then a `2A 00 00 00` sentinel (4 bytes), then 2 bytes, then the
    // 6-byte UID. So UID is at `name_end + 21`.
    //
    // Discovered via Frida byte-read trace on LotF: routing entries'
    // `destination_uid` matched bytes at this offset inside `0x1021`.
    // Verified for "Analog 1-2" (UID at +46) and "Front Left / Front
    // Right" (UID at +60) — both equal `name_end + 21`.
    let magic = block.offset.saturating_sub(7);
    let name_end = base + 8 + name_len;
    let uid_at = name_end + 21;
    let uid = if uid_at + 6 <= data.len()
        // Optional sanity check: the `2A 00 00 00` sentinel just before
        && data.get(uid_at.saturating_sub(6)) == Some(&0x2A)
    {
        let mut u = [0u8; 6];
        u.copy_from_slice(&data[uid_at..uid_at + 6]);
        Some(u)
    } else {
        None
    };
    let _ = magic;

    Some(IoChannel {
        name,
        io_class,
        channel_count,
        uid,
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
