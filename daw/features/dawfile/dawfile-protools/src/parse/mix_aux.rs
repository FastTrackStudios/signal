//! Fallback mix-state decoder reading volume/pan from `0x260a` (master-send
//! and channel-pan blocks) instead of `0x1029`.
//!
//! Pro Tools mirrors track volume and pan across SEVERAL block types:
//! - `0x1029 +1..+5` (i32 vol), `+13..+17` (i32 pan) — the canonical
//!   "TrackMixSettings" location written by the PT GUI editor.
//! - `0x260a[0] +26..+27` (i16 vol) — the master-send level.
//! - `0x260a[2] +26..+27` (i16 pan) — left-channel pan in the bus
//!   routing.
//! - `0x260d +407..+408` (i16 vol mirror), `+497..+498` (i16 pan mirror).
//! - `0x261b`, `0x261c`, `0x2624` mirrors at various offsets.
//!
//! The PT Reaper Converter, when generating PTX from RPP via
//! `--convert in.rpp out.ptx`, writes ONLY the `0x260a` / `0x260d`
//! paths — `0x1029` retains its default `0, -100` values. Our main
//! parser reads from `0x1029` and so misses these values.
//!
//! This pass fills in `volume_centibel` and `pan` from the
//! `0x260a`-side mirrors whenever `0x1029` reads as default. This
//! makes the parser handle converter-generated PTX correctly without
//! breaking the existing LotF-style reads (where `0x1029` carries
//! the real values).
//!
//! Verified via RPP→PTX probe + plaintext-diff (2026-05-17). See
//! `docs/pt-field-map.md`.

use crate::block::Block;
use crate::cursor::Cursor;
use crate::types::Track;
use std::collections::HashMap;

/// Each per-track wrapper `0x260d` has multiple `0x260a` children
/// (master-send, channel-pan, etc.). The first `0x260a` is the
/// master-send and carries the track volume at `+26..+27` (i16 LE).
fn collect_vol_pan_by_track(blocks: &[Block], data: &[u8]) -> HashMap<String, (i16, i16)> {
    let mut out: HashMap<String, (i16, i16)> = HashMap::new();

    fn walk_2624(blocks: &[Block], data: &[u8], out: &mut HashMap<String, (i16, i16)>) {
        for b in blocks {
            // 0x2624 is the per-track-mirror block whose 0x102d child holds the
            // track name (in 0x2619).
            if b.content_type_raw == 0x2624 {
                // Bounds-check against THIS block's size — multi-track
                // sessions have smaller per-track 0x2624 blocks where
                // +829 / +919 are past the payload end (would read
                // adjacent block bytes as garbage). The single-track
                // baseline probe has a ~8400-byte 0x2624; per-track
                // 0x2624s in multi-track sessions are shorter.
                let payload_len = (b.block_size as usize).saturating_sub(2);
                if payload_len < 921 {
                    walk_2624(&b.children, data, out);
                    continue;
                }
                let name = find_2619_name(b, data);
                let vol = read_i16(data, b.offset + 2 + 829);
                let pan = read_i16(data, b.offset + 2 + 919);
                if let (Some(name), Some(vol), Some(pan)) = (name, vol, pan) {
                    out.entry(name).or_insert((vol, pan));
                }
            }
            walk_2624(&b.children, data, out);
        }
    }

    walk_2624(blocks, data, &mut out);
    out
}

fn read_i16(data: &[u8], offset: usize) -> Option<i16> {
    if offset + 2 > data.len() {
        return None;
    }
    Some(i16::from_le_bytes([data[offset], data[offset + 1]]))
}

fn find_2619_name(b: &Block, data: &[u8]) -> Option<String> {
    for c in &b.children {
        if c.content_type_raw == 0x2619 {
            let p = c.offset + 2;
            if p + 4 > data.len() {
                return None;
            }
            let len = u32::from_le_bytes(data[p..p + 4].try_into().ok()?) as usize;
            if len == 0 || len > 64 || p + 4 + len > data.len() {
                return None;
            }
            return Some(
                String::from_utf8_lossy(&data[p + 4..p + 4 + len])
                    .trim_end_matches('\0')
                    .to_string(),
            );
        }
        if let Some(n) = find_2619_name(c, data) {
            return Some(n);
        }
    }
    None
}

/// Fill in `volume_centibel` and `pan` for any track whose `0x1029`
/// read returned the canonical defaults (vol=0, pan=-100) by falling
/// back to the `0x2624 +829/+919` mirror.
///
/// The "default" check is conservative: we only overwrite when both
/// vol == 0 AND pan == -100 (the PT default-state values). Sessions
/// where 0x1029 carries the real values won't be touched.
pub fn fill_vol_pan_from_2624(
    blocks: &[Block],
    cursor: &Cursor<'_>,
    audio_tracks: &mut [Track],
    midi_tracks: &mut [Track],
) {
    let data = cursor.data();
    // The 0x2624 +829/+919 offsets only line up with the FIRST track's
    // payload — multi-track sessions interleave tracks within the single
    // 0x2624 in a layout we haven't fully mapped. Skip the fallback if
    // there's more than one mixable track to avoid corrupting values.
    let total_tracks = audio_tracks.len() + midi_tracks.len();
    if total_tracks > 2 {
        return;
    }

    let vp = collect_vol_pan_by_track(blocks, data);

    for t in audio_tracks.iter_mut() {
        if t.volume_centibel != 0 || t.pan != -100 {
            continue;
        }
        let Some(&(vol, pan)) = vp.get(&t.name).or_else(|| {
            for suffix in [".01", ".02", ".03", ".04", ".05"] {
                if let Some(v) = vp.get(&format!("{}{suffix}", t.name)) {
                    return Some(v);
                }
            }
            None
        }) else {
            continue;
        };
        t.volume_centibel = vol as i32;
        t.pan = pan as i32;
    }
    for t in midi_tracks.iter_mut() {
        if t.volume_centibel != 0 || t.pan != -100 {
            continue;
        }
        if let Some(&(vol, pan)) = vp.get(&t.name) {
            t.volume_centibel = vol as i32;
            t.pan = pan as i32;
        }
    }
}
