//! Solo + solo-defeat decoder — gated to converter/writer-authored PTX.
//!
//! The `0x102d +162` (solo) and `0x200b +268` (solo-defeat) offsets were found
//! via an RPP→PTX probe on converter-authored PTX. They hold there, but on real
//! PT-authored sessions those bytes mean something else and over-fire (every
//! click "Shake" reads soloed; the user confirmed none of the PNG Worship
//! sessions actually have soloed tracks). So `apply_solo_state` only reads them
//! when the file is converter-authored (≥ 8 `0x1029` per `0x261c`, the same
//! shape test the mute decoder uses); PT-authored sessions keep `solo =
//! solo_defeat = false` until their real offsets are found. See
//! `docs/pt-field-map.md`.

use crate::block::Block;
use crate::content_type::ContentType;
use crate::cursor::Cursor;
use crate::types::Track;
use std::collections::HashMap;

fn collect_solo_by_name(blocks: &[Block], data: &[u8]) -> HashMap<String, bool> {
    let mut out: HashMap<String, bool> = HashMap::new();

    fn walk(blocks: &[Block], data: &[u8], out: &mut HashMap<String, bool>) {
        for b in blocks {
            if b.content_type_raw == 0x102d {
                let p162 = b.offset + 2 + 162;
                let solo = p162 < data.len() && data[p162] != 0;
                let name = b.children.iter().find_map(|c| {
                    if c.content_type_raw != 0x2619 {
                        return None;
                    }
                    let p = c.offset + 2;
                    if p + 4 > data.len() {
                        return None;
                    }
                    let len = u32::from_le_bytes(data[p..p + 4].try_into().ok()?) as usize;
                    if len == 0 || len > 64 || p + 4 + len > data.len() {
                        return None;
                    }
                    Some(
                        String::from_utf8_lossy(&data[p + 4..p + 4 + len])
                            .trim_end_matches('\0')
                            .to_string(),
                    )
                });
                if let Some(name) = name {
                    out.entry(name).or_insert(solo);
                }
            }
            walk(&b.children, data, out);
        }
    }

    walk(blocks, data, &mut out);
    out
}

/// Collect `name → solo_defeat` from `0x200b +268`. Uses the same
/// ancestor-walking 0x2619 name resolution as the color decoder
/// (handles both flat and deeply-nested track-name structures).
fn collect_solo_defeat_by_name(blocks: &[Block], data: &[u8]) -> HashMap<String, bool> {
    let mut parents: HashMap<usize, Option<&Block>> = HashMap::new();
    fn build_parents<'a>(
        blocks: &'a [Block],
        parent: Option<&'a Block>,
        out: &mut HashMap<usize, Option<&'a Block>>,
    ) {
        for b in blocks {
            out.insert(b.offset, parent);
            build_parents(&b.children, Some(b), out);
        }
    }
    build_parents(blocks, None, &mut parents);

    fn find_2619(b: &Block, data: &[u8]) -> Option<String> {
        for c in &b.children {
            if c.content_type == Some(ContentType::MarkerEntry) {
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
            if let Some(n) = find_2619(c, data) {
                return Some(n);
            }
        }
        None
    }

    let mut out: HashMap<String, bool> = HashMap::new();
    let aux_blocks = crate::parse::collect_blocks_recursive(blocks, ContentType::TrackAuxState);
    for b in &aux_blocks {
        let p = b.offset + 2 + 268;
        if p >= data.len() {
            continue;
        }
        let defeat = data[p] != 0;

        let mut anc = parents.get(&b.offset).copied().flatten();
        let mut depth = 0;
        let mut name: Option<String> = None;
        while let Some(a) = anc {
            if let Some(n) = find_2619(a, data) {
                name = Some(n);
                break;
            }
            anc = parents.get(&a.offset).copied().flatten();
            depth += 1;
            if depth > 10 {
                break;
            }
        }
        if let Some(name) = name {
            out.entry(name).or_insert(defeat);
        }
    }
    out
}

/// Apply per-track solo / solo-defeat state.
///
/// The `0x102d +162` (solo) and `0x200b +268` (solo-defeat) offsets hold only
/// for *converter/writer-authored* PTX (where they were probe-verified). On
/// real *PT-authored* sessions those bytes mean something else and over-fire
/// (every click "Shake" reads soloed; the user confirmed none of the PNG
/// Worship sessions have any soloed track). So we only read them when the file
/// has the converter-authored shape — detected exactly as the mute decoder
/// does: ≥ 8 `0x1029` mix blocks per `0x261c` track container. PT-authored
/// sessions keep `solo = solo_defeat = false` until their real offsets are
/// found. (Mute, from `0x1029 +5`, is accurate on both and is unaffected.)
pub fn apply_solo_state(
    blocks: &[Block],
    cursor: &Cursor<'_>,
    audio_tracks: &mut [Track],
    midi_tracks: &mut [Track],
) {
    let containers = crate::parse::collect_blocks_recursive(blocks, ContentType::TrackContainer);
    let mix_blocks = crate::parse::collect_blocks_recursive(blocks, ContentType::TrackMixSettings);
    let converter_authored = !containers.is_empty() && mix_blocks.len() >= containers.len() * 8;
    if !converter_authored {
        return; // PT-authored: +162/+268 are unreliable, leave solo state false.
    }

    let data = cursor.data();
    let solo_by_name = collect_solo_by_name(blocks, data);
    let defeat_by_name = collect_solo_defeat_by_name(blocks, data);

    let lookup = |map: &HashMap<String, bool>, name: &str| -> Option<bool> {
        if let Some(v) = map.get(name).copied() {
            return Some(v);
        }
        for suffix in [".01", ".02", ".03", ".04", ".05"] {
            if let Some(v) = map.get(&format!("{name}{suffix}")).copied() {
                return Some(v);
            }
        }
        None
    };

    for t in audio_tracks.iter_mut().chain(midi_tracks.iter_mut()) {
        if let Some(s) = lookup(&solo_by_name, &t.name) {
            t.solo = s;
        }
        if let Some(d) = lookup(&defeat_by_name, &t.name) {
            t.solo_defeat = d;
        }
    }
}
