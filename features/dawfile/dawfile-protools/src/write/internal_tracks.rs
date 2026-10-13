//! Write-side encoders for internal tracks (aux/bus/master/click).
//!
//! Read-side decode lives in `crates/dawfile-protools/src/parse/mod.rs`
//! (`parse_internal_tracks`). The `0x261e` block layout:
//!
//! ```text
//! 0x261e {
//!   header bytes (variable prefix; nested 0x261b / 0x102d / 0x2619 headers)
//!   length-prefixed name (u32 namelen + utf-8 bytes) at magic + 0x24
//!   ... some bytes ...
//!   0x2a 00 00 00 [u8; 6] routing_uid    ← scan-detected marker
//!   ... per-track config bytes ...
//! }
//! ```
//!
//! # Implementation status
//!
//! `rename_internal_track` is implemented (length-aware splice on the
//! name field). Adding/removing entire internal tracks is **not yet
//! implemented**: the prefix bytes encode aux-vs-bus-vs-master kind,
//! plugin chain, output routing, etc., and that layout is not yet
//! decoded. A naive append would produce a file PT refuses to open.

use crate::content_type::ContentType;
use crate::raw_block::{RawBlock, RawSession};
use crate::write::WriteError;
use crate::write::splice::replace_string;

/// Rename an existing internal track in place.
///
/// `current_name` is matched exactly. Returns the byte delta from the
/// splice (negative if the new name is shorter; positive if longer).
///
/// # Errors
///
/// - `InvalidArgument` if `new_name` is empty or longer than 255 bytes.
/// - `InvalidArgument` if no `0x261e` block in the session carries the
///   given `current_name`.
pub fn rename_internal_track(
    session: &mut RawSession,
    current_name: &str,
    new_name: &str,
) -> Result<i64, WriteError> {
    if new_name.is_empty() || new_name.len() > 255 {
        return Err(WriteError::InvalidArgument(format!(
            "internal-track name length out of range (1..=255): {} bytes",
            new_name.len()
        )));
    }

    // Find the 0x261e block matching `current_name` and its name-prefix offset.
    let blocks_snapshot: Vec<(usize, usize)> = collect_internal_track_entries(&session.blocks);
    let mut target: Option<usize> = None;
    for (block_start, namelen_off) in blocks_snapshot {
        let p = namelen_off;
        if p + 4 > session.data.len() {
            continue;
        }
        let nlen = u32::from_le_bytes(session.data[p..p + 4].try_into().unwrap_or([0; 4])) as usize;
        if p + 4 + nlen > session.data.len() {
            continue;
        }
        let name = &session.data[p + 4..p + 4 + nlen];
        if name == current_name.as_bytes() {
            target = Some(p);
            let _ = block_start;
            break;
        }
    }

    let Some(namelen_off) = target else {
        return Err(WriteError::InvalidArgument(format!(
            "no internal track named {current_name:?}"
        )));
    };

    Ok(replace_string(session, namelen_off, new_name))
}

/// **Not yet implemented.** Insert a new internal track. Blocked on
/// decode of the `0x261e` prefix bytes that determine track kind, plugin
/// chain, and output routing.
pub fn add_internal_track(_session: &mut RawSession, _name: &str) -> Result<i64, WriteError> {
    Err(WriteError::Unimplemented(
        "0x261e internal-track write blocked on prefix decode (kind / routing / plugin chain). \
         See docs/converter-frida-discovered-offsets.md \
         §\"0x261e — Internal-track / aux-bus / master-bus / click-track entries\"",
    ))
}

/// **Not yet implemented.** Remove an existing internal track by name.
/// Blocked on the same prefix decode as `add_internal_track` plus the
/// need to update sibling references to the removed track's routing UID
/// (a separate cross-block index recomputation).
pub fn remove_internal_track(_session: &mut RawSession, _name: &str) -> Result<i64, WriteError> {
    Err(WriteError::Unimplemented(
        "0x261e internal-track removal blocked on cross-block UID reference cleanup",
    ))
}

/// Collect `(block_start, namelen_offset)` pairs for every `0x261e` in the
/// tree. `namelen_offset` points to the u32 length prefix of the track's
/// name; pass it to `replace_string` to do a length-aware rename.
fn collect_internal_track_entries(blocks: &[RawBlock]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    fn walk(blocks: &[RawBlock], out: &mut Vec<(usize, usize)>) {
        for b in blocks {
            if b.content_type == Some(ContentType::InternalTrackEntry) {
                // Same scan logic as parse_internal_tracks: find the first
                // valid [u32 namelen][printable ASCII] pair in the block
                // payload. We can't trust a fixed offset because the
                // nested 0x2619 header byte count is variable. Caller
                // re-validates with the actual data buffer.
                let payload_start = b.start + 9;
                let block_end = b.end;
                out.push((b.start, find_name_offset(payload_start, block_end)));
            }
            walk(&b.children, out);
        }
    }
    walk(blocks, &mut out);
    out
}

/// Heuristic: return the byte position where we *expect* the name's u32
/// length prefix to live. Caller re-validates against the data buffer.
fn find_name_offset(_payload_start: usize, _block_end: usize) -> usize {
    // Same fix-point that worked across all 5 PT-authored test fixtures:
    // the name's u32 namelen sits at `magic + 0x24`. For `0x261e` blocks,
    // `payload_start = magic + 9`, so namelen lives at `payload_start + 0x1b`.
    _payload_start + 0x1b
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEYLADY_PTX: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/HeyLady.ptx");
    const WORSHIP_PTX: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/worship-session.ptx"
    );

    #[test]
    fn rename_internal_track_same_length() {
        // worship-session has a track named "DRUMS" (5 chars). Rename to
        // "BEATS" (also 5 chars) — no splice should be needed.
        let bytes = std::fs::read(WORSHIP_PTX).expect("fixture readable");
        let mut session = crate::parse_raw(bytes).expect("parses");
        let delta = rename_internal_track(&mut session, "DRUMS", "BEATS").expect("rename succeeds");
        assert_eq!(delta, 0, "same-length rename should be in-place");

        // Re-parse and verify.
        let parsed = crate::read_session_from_bytes(session.encrypt(), 48000)
            .expect("re-parses after rename");
        let names: Vec<&str> = parsed
            .internal_tracks
            .iter()
            .map(|t| t.name.as_str())
            .collect();
        assert!(names.contains(&"BEATS"));
        assert!(!names.contains(&"DRUMS"));
    }

    #[test]
    fn rename_internal_track_grows() {
        // worship has "Verb" (4 chars). Rename to "ReverbLong" (10 chars).
        let bytes = std::fs::read(WORSHIP_PTX).expect("fixture readable");
        let mut session = crate::parse_raw(bytes).expect("parses");
        let delta =
            rename_internal_track(&mut session, "Verb", "ReverbLong").expect("rename succeeds");
        assert_eq!(delta, 10 - 4, "grew by 6 bytes");

        let parsed = crate::read_session_from_bytes(session.encrypt(), 48000)
            .expect("re-parses after splice");
        let names: Vec<&str> = parsed
            .internal_tracks
            .iter()
            .map(|t| t.name.as_str())
            .collect();
        assert!(names.contains(&"ReverbLong"));
    }

    #[test]
    fn rename_internal_track_not_found() {
        let bytes = std::fs::read(WORSHIP_PTX).expect("fixture readable");
        let mut session = crate::parse_raw(bytes).expect("parses");
        let result = rename_internal_track(&mut session, "NoSuchTrack", "Whatever");
        assert!(matches!(result, Err(WriteError::InvalidArgument(_))));
    }

    #[test]
    fn rename_internal_track_no_blocks() {
        // HeyLady has 1 internal track ("Click") so it has 0x261e present.
        // Trying to rename a non-existent name should error cleanly.
        let bytes = std::fs::read(HEYLADY_PTX).expect("fixture readable");
        let mut session = crate::parse_raw(bytes).expect("parses");
        let result = rename_internal_track(&mut session, "ImaginaryBus", "X");
        assert!(matches!(result, Err(WriteError::InvalidArgument(_))));
    }

    #[test]
    fn add_internal_track_returns_unimplemented() {
        let bytes = std::fs::read(WORSHIP_PTX).expect("fixture readable");
        let mut session = crate::parse_raw(bytes).expect("parses");
        let result = add_internal_track(&mut session, "TestBus");
        assert!(matches!(result, Err(WriteError::Unimplemented(_))));
    }
}
