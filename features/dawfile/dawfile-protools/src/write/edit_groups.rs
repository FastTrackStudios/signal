//! Write-side encoders for edit groups + stem mapping.
//!
//! Wire format (read-side: `crates/dawfile-protools/src/parse/mod.rs` and
//! `docs/converter-frida-discovered-offsets.md` §"`0x4501` / `0x4702`"):
//!
//! - `0x4501` (EditGroupList): per-track membership prefix (~hundreds of
//!   bytes; not yet decoded for write) followed by a flat sequence of
//!   `[u32 LE namelen][utf-8 name][i16 LE color]` entries.
//! - `0x4702` (StemMappingList): same `[u32 namelen][utf-8 name]` shape
//!   minus the i16 color trailer.
//!
//! # Implementation status
//!
//! `replace_stem_mappings` is implemented end-to-end and tested. The two
//! edit-group helpers (`add_edit_group_name`, `replace_edit_groups`)
//! intentionally return `Err(WriteError::Unimplemented(_))` because the
//! per-track membership table preceding the name list is not yet decoded;
//! writing only the names would produce a file where the names are out of
//! sync with the membership prefix. The trait shape is committed so a
//! future contributor can fill in `replace_edit_groups` without rewriting
//! the surface.

use crate::content_type::ContentType;
use crate::raw_block::{RawBlock, RawSession};
use crate::types::EditGroup;
use crate::write::WriteError;
use crate::write::block_ops::{find_all_blocks, wrap_as_block};
use crate::write::splice::splice;

/// Append a new stem-mapping entry to the session's `0x4702` block.
///
/// Returns the byte delta the splice introduced. If no `0x4702` exists in
/// the session, a new one is created at the end of the file as a top-level
/// sibling (PT accepts this placement).
pub fn add_stem_mapping(session: &mut RawSession, name: &str) -> Result<i64, WriteError> {
    if name.is_empty() || name.len() > 255 {
        return Err(WriteError::InvalidArgument(format!(
            "stem-mapping name length out of range (1..=255): {} bytes",
            name.len()
        )));
    }

    // Build the new entry: [u32 namelen][utf-8 name]
    let mut entry = Vec::with_capacity(4 + name.len());
    entry.extend_from_slice(&(name.len() as u32).to_le_bytes());
    entry.extend_from_slice(name.as_bytes());

    if let Some(existing) = session.find_block(ContentType::StemMappingList) {
        // Append to existing block payload (just before block.end).
        let insertion_point = existing.end;
        let delta = splice(session, insertion_point, 0, &entry);
        return Ok(delta);
    }

    // No existing 0x4702 — synthesize one with this single entry as its
    // entire payload.
    let new_block = wrap_as_block(0x4702, &entry);
    let insertion_point = session.data.len();
    Ok(splice(session, insertion_point, 0, &new_block))
}

/// Replace the full stem-mapping list with the given names. Removes any
/// existing `0x4702` block(s) and inserts a fresh one.
///
/// PT accepts entries with arbitrary names — there is no validation that
/// the names match the built-in stem types (`Dialog`/`Music`/`Effects`/
/// `Narration`), nor that they reference existing tracks.
pub fn replace_stem_mappings(session: &mut RawSession, names: &[&str]) -> Result<i64, WriteError> {
    for n in names {
        if n.is_empty() || n.len() > 255 {
            return Err(WriteError::InvalidArgument(format!(
                "stem-mapping name length out of range (1..=255): {} bytes",
                n.len()
            )));
        }
    }

    // Remove every existing 0x4702 block first.
    let mut total_delta = 0i64;
    loop {
        let existing_start = session
            .find_block(ContentType::StemMappingList)
            .map(|b| b.start);
        let Some(start) = existing_start else { break };
        let block = session
            .find_block(ContentType::StemMappingList)
            .expect("just found it");
        let len = block.end - block.start;
        total_delta += splice(session, start, len, &[]);
    }

    // Build the combined payload.
    let mut payload = Vec::new();
    for n in names {
        payload.extend_from_slice(&(n.len() as u32).to_le_bytes());
        payload.extend_from_slice(n.as_bytes());
    }

    if names.is_empty() {
        // Removal-only: no fresh block to insert.
        return Ok(total_delta);
    }

    let new_block = wrap_as_block(0x4702, &payload);
    let insertion_point = session.data.len();
    total_delta += splice(session, insertion_point, 0, &new_block);
    Ok(total_delta)
}

/// **Not yet implemented.** The per-track membership table that precedes the
/// name list inside `0x4501` is not decoded; appending a name without
/// updating the membership table would leave the file internally
/// inconsistent.
///
/// Trait shape kept so callers can compile-time depend on this surface.
pub fn add_edit_group_name(
    _session: &mut RawSession,
    _group: &EditGroup,
) -> Result<i64, WriteError> {
    Err(WriteError::Unimplemented(
        "0x4501 edit-group write blocked on per-track membership-table decode \
         (see docs/converter-frida-discovered-offsets.md \
         §\"Membership decode status (blocked)\")",
    ))
}

/// **Not yet implemented.** See [`add_edit_group_name`].
pub fn replace_edit_groups(
    _session: &mut RawSession,
    _groups: &[EditGroup],
) -> Result<i64, WriteError> {
    Err(WriteError::Unimplemented(
        "0x4501 edit-group write blocked on per-track membership-table decode",
    ))
}

/// Return the parsed stem-mapping names from the session, in order.
///
/// Helper for symmetry with read-side `ProToolsSession.stem_mappings` so
/// writers can read-modify-write at the raw-block level without going
/// through a full `read_session()` round-trip.
pub fn read_stem_mappings(session: &RawSession) -> Vec<String> {
    let mut out = Vec::new();
    for b in find_all_blocks(&session.blocks, 0x4702) {
        parse_flat_namelist(b, &session.data, &mut out, false);
    }
    out
}

fn parse_flat_namelist(b: &RawBlock, data: &[u8], out: &mut Vec<String>, has_color_trailer: bool) {
    let payload_start = b.start + 9;
    let block_end = b.end.min(data.len());
    if payload_start >= block_end {
        return;
    }
    let mut p = payload_start;
    // Skip into the payload until we find the first plausible [namelen][name] pair.
    let mut found = false;
    while p + 4 < block_end && !found {
        let nlen = u32::from_le_bytes(data[p..p + 4].try_into().unwrap_or([0; 4])) as usize;
        let trailer = if has_color_trailer { 2 } else { 0 };
        if (2..=64).contains(&nlen) && p + 4 + nlen + trailer <= block_end {
            let name = &data[p + 4..p + 4 + nlen];
            if name.iter().all(|c| (0x20..0x7f).contains(c)) {
                found = true;
                break;
            }
        }
        p += 1;
    }
    while p + 4 < block_end {
        let nlen = u32::from_le_bytes(data[p..p + 4].try_into().unwrap_or([0; 4])) as usize;
        let trailer = if has_color_trailer { 2 } else { 0 };
        if !(1..=64).contains(&nlen) || p + 4 + nlen + trailer > block_end {
            break;
        }
        let name_bytes = &data[p + 4..p + 4 + nlen];
        if !name_bytes.iter().all(|c| (0x20..0x7f).contains(c)) {
            break;
        }
        out.push(String::from_utf8_lossy(name_bytes).into_owned());
        p += 4 + nlen + trailer;
    }
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
    fn add_stem_mapping_to_fixture_without_block() {
        // HeyLady has no 0x4702 block; adding one should synthesize it.
        let bytes = std::fs::read(HEYLADY_PTX).expect("fixture readable");
        let mut session = crate::parse_raw(bytes).expect("parses");
        assert!(
            session.find_block(ContentType::StemMappingList).is_none(),
            "HeyLady should have no 0x4702 baseline"
        );
        let delta = add_stem_mapping(&mut session, "Dialog").expect("add succeeds");
        assert!(delta > 0, "splice should grow the file");
        assert!(
            session.find_block(ContentType::StemMappingList).is_some(),
            "new 0x4702 should be present after add"
        );
        let names = read_stem_mappings(&session);
        assert_eq!(names, vec!["Dialog"]);
    }

    #[test]
    fn replace_stem_mappings_with_built_in_set() {
        let bytes = std::fs::read(WORSHIP_PTX).expect("fixture readable");
        let mut session = crate::parse_raw(bytes).expect("parses");
        let original = read_stem_mappings(&session);
        assert!(!original.is_empty(), "worship has a baseline 0x4702");

        replace_stem_mappings(
            &mut session,
            &["Dialog", "Music", "Effects", "Narration", "Walla"],
        )
        .expect("replace succeeds");

        let after = read_stem_mappings(&session);
        assert_eq!(
            after,
            vec!["Dialog", "Music", "Effects", "Narration", "Walla"]
        );
    }

    #[test]
    fn replace_stem_mappings_empty_removes_block() {
        let bytes = std::fs::read(WORSHIP_PTX).expect("fixture readable");
        let mut session = crate::parse_raw(bytes).expect("parses");
        assert!(session.find_block(ContentType::StemMappingList).is_some());

        replace_stem_mappings(&mut session, &[]).expect("empty replace ok");
        assert!(
            session.find_block(ContentType::StemMappingList).is_none(),
            "empty replace should remove the block entirely"
        );
        assert_eq!(read_stem_mappings(&session), Vec::<String>::new());
    }

    #[test]
    fn add_edit_group_returns_unimplemented() {
        let bytes = std::fs::read(WORSHIP_PTX).expect("fixture readable");
        let mut session = crate::parse_raw(bytes).expect("parses");
        let result = add_edit_group_name(
            &mut session,
            &EditGroup {
                name: "TestGroup".into(),
                color: None,
            },
        );
        assert!(matches!(result, Err(WriteError::Unimplemented(_))));
    }
}
