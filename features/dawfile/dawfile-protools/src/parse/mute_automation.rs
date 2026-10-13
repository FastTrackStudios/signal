//! Decode mute-automation envelopes from `0x260a[1]` per track.
//!
//! Each track's per-track wrapper (`0x260d`) contains multiple `0x260a`
//! children. The SECOND `0x260a` under each wrapper carries the mute
//! automation envelope:
//!
//! - 28-byte header
//!   - `+4..+8`   u32 LE: payload size (excluding 28-byte header? format
//!     is "bytes-after-+4"; updated via the splice cascade on write)
//!   - `+10`     u8: total breakpoint count (= `1 + user_count`)
//!   - `+16`     u8: user breakpoint count
//! - `N × 6` breakpoint bytes — each is
//!   - `u32 LE time_samples`
//!   - `u8 muted` (0 or 1)
//!   - `u8 shape` (0 = step/square)
//!
//! Pairs with tracks positionally via PT's `0x251a` document order — the
//! same convention used by `mute_resolver`.

use crate::block::Block;
use crate::content_type::ContentType;
use crate::cursor::Cursor;
use crate::types::{
    MuteAutomationBreakpoint, PanAutomationBreakpoint, Track, VolumeAutomationBreakpoint,
};

/// Walk every per-track `0x260d` wrapper, find its second `0x260a`
/// child, and decode the envelope. Apply to `tracks` by positional
/// pairing with the `0x251a` document order — falling back to "first
/// wrapper for first track, second for second, ..." when names don't
/// resolve.
pub fn apply_mute_automation(
    blocks: &[Block],
    cursor: &Cursor<'_>,
    audio_tracks: &mut [Track],
    midi_tracks: &mut [Track],
) {
    let data = cursor.data();

    // Collect wrappers in document order; for each, find its 2nd 0x260a
    // and decode any envelope.
    let mut wrappers: Vec<&Block> = Vec::new();
    collect_recursive(blocks, ContentType::TrackMixWrapper, &mut wrappers);

    // Build a name index using the MidiTrackList (matches mute_resolver
    // pairing): wrappers[i] ↔ ith unique 0x251a entry name.
    let names = collect_track_names(blocks, data);

    for (i, wrapper) in wrappers.iter().enumerate() {
        let Some(name) = names.get(i) else {
            continue;
        };

        // Mute envelope at 0x260a[1].
        if let Some(envelope_block) = nth_child_by_raw(wrapper, 0x260a, 1) {
            let bps = decode_mute_envelope(envelope_block, data);
            if !bps.is_empty() {
                apply_to_track(audio_tracks, midi_tracks, name, |t| {
                    t.mute_automation = bps.clone();
                });
            }
        }
        // Volume envelope at 0x260a[0].
        if let Some(envelope_block) = nth_child_by_raw(wrapper, 0x260a, 0) {
            let bps = decode_volume_envelope(envelope_block, data);
            if !bps.is_empty() {
                apply_to_track(audio_tracks, midi_tracks, name, |t| {
                    t.volume_automation = bps.clone();
                });
            }
        }
        // Pan envelope lives one level deeper: 0x260d > 0x260c[0] > 0x260a[0].
        // (Two 0x260c sub-wrappers exist for a dual panner; the first carries
        // the pan automation — identical to the second for a mono track.)
        if let Some(sub) = nth_child_by_raw(wrapper, 0x260c, 0)
            && let Some(envelope_block) = nth_child_by_raw(sub, 0x260a, 0)
        {
            let bps = decode_pan_envelope(envelope_block, data);
            if !bps.is_empty() {
                apply_to_track(audio_tracks, midi_tracks, name, |t| {
                    t.pan_automation = bps.clone();
                });
            }
        }
    }
}

fn decode_pan_envelope(block: &Block, data: &[u8]) -> Vec<PanAutomationBreakpoint> {
    let payload_start = block.offset + 2;
    if payload_start + 28 > data.len() {
        return Vec::new();
    }
    let user_count = data[payload_start + 16] as usize;
    let bp_start = payload_start + 28;
    if user_count == 0 || bp_start + user_count * 6 > data.len() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(user_count);
    for i in 0..user_count {
        let p = bp_start + i * 6;
        out.push(PanAutomationBreakpoint {
            time_samples: u32::from_le_bytes([data[p], data[p + 1], data[p + 2], data[p + 3]]),
            value: i16::from_le_bytes([data[p + 4], data[p + 5]]),
        });
    }
    out
}

fn apply_to_track<F: FnMut(&mut Track)>(
    audio_tracks: &mut [Track],
    midi_tracks: &mut [Track],
    name: &str,
    mut f: F,
) {
    if let Some(t) = audio_tracks
        .iter_mut()
        .find(|t| t.name == name || strip_suffix(&t.playlist_name) == name)
    {
        f(t);
        return;
    }
    if let Some(t) = midi_tracks
        .iter_mut()
        .find(|t| t.name == name || strip_suffix(&t.playlist_name) == name)
    {
        f(t);
    }
}

fn decode_volume_envelope(block: &Block, data: &[u8]) -> Vec<VolumeAutomationBreakpoint> {
    let payload_start = block.offset + 2;
    if payload_start + 28 > data.len() {
        return Vec::new();
    }
    let user_count = data[payload_start + 16] as usize;
    let bp_start = payload_start + 28;
    let bp_end = bp_start + user_count * 6;
    if user_count == 0 || bp_end > data.len() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(user_count);
    for i in 0..user_count {
        let p = bp_start + i * 6;
        let time_samples = u32::from_le_bytes([data[p], data[p + 1], data[p + 2], data[p + 3]]);
        let value_centibel = i16::from_le_bytes([data[p + 4], data[p + 5]]);
        out.push(VolumeAutomationBreakpoint {
            time_samples,
            value_centibel,
        });
    }
    out
}

fn decode_mute_envelope(block: &Block, data: &[u8]) -> Vec<MuteAutomationBreakpoint> {
    // Payload starts at block.offset + 2 (post content-type bytes).
    // Header occupies the first 28 payload bytes, followed by an
    // implicit t=0 breakpoint (6 bytes) that PT always stores, then
    // `user_count` user breakpoints. We surface only the user ones.
    let payload_start = block.offset + 2;
    if payload_start + 28 > data.len() {
        return Vec::new();
    }
    let user_count = data[payload_start + 16] as usize;
    // Header is 22 bytes, then a 6-byte implicit (t=0, default-state)
    // breakpoint at +22, then `user_count` user breakpoints at +28.
    let bp_start = payload_start + 28;
    let bp_end = bp_start + user_count * 6;
    if user_count == 0 || bp_end > data.len() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(user_count);
    for i in 0..user_count {
        let p = bp_start + i * 6;
        let time_samples = u32::from_le_bytes([data[p], data[p + 1], data[p + 2], data[p + 3]]);
        let muted = data[p + 4] != 0;
        let shape = data[p + 5];
        out.push(MuteAutomationBreakpoint {
            time_samples,
            muted,
            shape,
        });
    }
    out
}

fn collect_recursive<'a>(blocks: &'a [Block], ct: ContentType, out: &mut Vec<&'a Block>) {
    for b in blocks {
        if b.content_type == Some(ct) {
            out.push(b);
        }
        collect_recursive(&b.children, ct, out);
    }
}

fn nth_child_by_raw(parent: &Block, ct_raw: u16, n: usize) -> Option<&Block> {
    parent
        .children
        .iter()
        .filter(|c| c.content_type_raw == ct_raw)
        .nth(n)
}

fn collect_track_names(blocks: &[Block], data: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let Some(list) = find_first(blocks, ContentType::MidiTrackList) else {
        return names;
    };
    let mut seen = std::collections::HashSet::new();
    for c in &list.children {
        if c.content_type != Some(ContentType::MidiTrackInfo) {
            continue;
        }
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
        names.push(name);
    }
    names
}

fn find_first(blocks: &[Block], ct: ContentType) -> Option<&Block> {
    for b in blocks {
        if b.content_type == Some(ct) {
            return Some(b);
        }
        if let Some(found) = find_first(&b.children, ct) {
            return Some(found);
        }
    }
    None
}

fn strip_suffix(s: &str) -> &str {
    if let Some(idx) = s.rfind('.')
        && s[idx + 1..].chars().all(|c| c.is_ascii_digit())
    {
        return &s[..idx];
    }
    s
}
