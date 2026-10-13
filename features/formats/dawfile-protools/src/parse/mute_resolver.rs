//! Resolve effective mute by combining the stored mix-bit (`0x1029 +5`)
//! with the send-routing flag (`0x260a[0] +8`).
//!
//! Pro Tools sets the `+5` mute-mix bit for BOTH user-muted tracks and
//! for inactive/bounced-source tracks. The converter discriminates by
//! checking the first `0x260a` (master-send) block under the track's
//! `0x260d` wrapper: if `+8 == 0` the send is disabled (user muted),
//! if `+8 == 1` the send is routed (inactive/bounced — stored mute is
//! incidental and the track is NOT effective-muted).
//!
//! Rule: effective_mute = (mix +5 == 1) AND (send +8 == 0)
//!
//! Verified on Lord of the Fight (2026-05-17): correctly identifies
//! the 8 truly-muted tracks (ClickPrint + 7 LORD family stems) without
//! the 12 over-mutes from the +5-only read (SYZ, AC GTR x2, El Gtr 1,
//! Bass Demo, MIDI 1, Inst stack).

use crate::block::Block;
use crate::content_type::ContentType;
use crate::cursor::Cursor;
use crate::types::Track;
use std::collections::HashSet;

/// Walk every `0x260d` (track mix wrapper) and find the first `0x260a`
/// child. If its payload `+8` byte is non-zero, the track has an
/// active send route — collect its 0x251a-list name so we can clear
/// the stored mute bit on it.
fn collect_routed_track_names(blocks: &[Block], data: &[u8]) -> HashSet<String> {
    let mut routed: HashSet<String> = HashSet::new();

    // Walk the track list to get names in document order, paired with
    // their corresponding 0x260d wrapper (positional).
    let track_list = find_first(blocks, ContentType::MidiTrackList);
    let Some(list) = track_list else {
        return routed;
    };

    let mut wrappers: Vec<&Block> = Vec::new();
    collect_recursive(blocks, ContentType::TrackMixWrapper, &mut wrappers);

    let mut wrap_idx = 0usize;
    let mut seen: HashSet<String> = HashSet::new();
    for c in &list.children {
        if c.content_type != Some(ContentType::MidiTrackInfo) {
            continue;
        }
        // 0x251a payload begins at offset + 2 (block convention here),
        // but the children parse keeps offset relative differently.
        // Read the length-prefixed name at c.offset + 4 (consistent
        // with the existing color/folder decoders).
        let p = c.offset + 4;
        if p + 4 > data.len() {
            continue;
        }
        let Ok(arr) = data[p..p + 4].try_into() else {
            continue;
        };
        let len = u32::from_le_bytes(arr) as usize;
        if len == 0 || len > 64 || p + 4 + len > data.len() {
            continue;
        }
        let name = String::from_utf8_lossy(&data[p + 4..p + 4 + len])
            .trim_end_matches('\0')
            .to_string();
        if !seen.insert(name.clone()) {
            break;
        }

        // Pair with the wrapper at wrap_idx. The wrappers list is
        // 1:1 with the 0x251a entries in document order (skipping
        // any entries that lack a wrapper — typically Master has no
        // wrapper but is FIRST in the name list).
        let Some(w) = wrappers.get(wrap_idx) else {
            continue;
        };
        wrap_idx += 1;

        // Find the FIRST 0x260a child of the wrapper.
        let send_b8 = w
            .children
            .iter()
            .find(|c| c.content_type_raw == 0x260a)
            .and_then(|s| {
                let p = s.offset + 2 + 8;
                data.get(p).copied()
            })
            .unwrap_or(0);

        if send_b8 != 0 {
            routed.insert(name);
        }
    }

    routed
}

fn find_first(blocks: &[Block], ct: ContentType) -> Option<&Block> {
    for b in blocks {
        if b.content_type == Some(ct) {
            return Some(b);
        }
        if let Some(x) = find_first(&b.children, ct) {
            return Some(x);
        }
    }
    None
}

fn collect_recursive<'a>(blocks: &'a [Block], ct: ContentType, out: &mut Vec<&'a Block>) {
    for b in blocks {
        if b.content_type == Some(ct) {
            out.push(b);
        }
        collect_recursive(&b.children, ct, out);
    }
}

/// For every track currently flagged `mute=true`, clear it if the
/// track has an active send (`0x260a[0] +8 != 0`). This converts the
/// stored "mix-bit mute" to "effective playback mute".
pub fn resolve_effective_mute(
    blocks: &[Block],
    cursor: &Cursor<'_>,
    audio_tracks: &mut [Track],
    midi_tracks: &mut [Track],
) {
    let data = cursor.data();
    let routed = collect_routed_track_names(blocks, data);
    if routed.is_empty() {
        return;
    }

    for t in audio_tracks.iter_mut() {
        if !t.mute {
            continue;
        }
        let matched = routed.contains(&t.name)
            || [".01", ".02", ".03", ".04", ".05"]
                .iter()
                .any(|s| routed.contains(&format!("{}{s}", t.name)));
        if matched {
            // Track had `+5=1 AND +8=1` — stored mute set, but send
            // still routed. This is PT's inactive/bouncedSource state,
            // not user-mute. Clear effective mute, mark inactive.
            t.mute = false;
            t.inactive = true;
        }
    }
    for t in midi_tracks.iter_mut() {
        if t.mute && routed.contains(&t.name) {
            t.mute = false;
            t.inactive = true;
        }
    }
}
