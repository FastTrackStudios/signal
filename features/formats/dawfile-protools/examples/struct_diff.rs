#![allow(clippy::all, dead_code)]
//! Walk every `0x102d` (per-track display block), align inner children by
//! content_type, and report payload bytes that vary across tracks.
//!
//! Bypasses the "false 23-distinct" trap caused by variable-layout siblings:
//! each child's offset is computed relative to its own header, so we compare
//! semantically same data across tracks.

use dawfile_protools::content_type::ContentType;
use dawfile_protools::raw_block::RawBlock;

fn main() {
    let path = std::env::args().nth(1).expect("path");
    let raw = std::fs::read(&path).unwrap();
    let session = dawfile_protools::parse_raw(raw).unwrap();
    let data = session.cursor().data();

    let track_list = find(&session.blocks, ContentType::MidiTrackList).expect("no 0x2519");
    let mut names: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for c in &track_list.children {
        if c.content_type != Some(ContentType::MidiTrackInfo) {
            continue;
        }
        let off = c.start + 11;
        let len = u32::from_le_bytes(data[off..off + 4].try_into().unwrap()) as usize;
        let name = String::from_utf8_lossy(&data[off + 4..off + 4 + len.min(64)]).to_string();
        if !seen.insert(name.clone()) {
            break;
        }
        names.push(name);
    }

    // For each 0x102d, collect (child_ct, child_payload_bytes) tuples
    let displays: Vec<&RawBlock> = collect(&session.blocks, 0x102d);
    println!(
        "{} 0x102d blocks, {} track names",
        displays.len(),
        names.len()
    );

    // Build a per-track list of [(child_ct, payload_hex)]
    type ChildSig = Vec<(u16, Vec<u8>)>;
    let mut per_track: Vec<(String, ChildSig)> = Vec::new();
    for (i, d) in displays.iter().enumerate() {
        let name = names.get(i).cloned().unwrap_or_else(|| format!("#{i}"));
        let mut sig: ChildSig = Vec::new();
        walk_children(d, data, &mut sig);
        per_track.push((name, sig));
    }

    // Group tracks by ChildSig structure (same set of child_cts in same order)
    let mut by_shape: std::collections::HashMap<Vec<u16>, Vec<usize>> =
        std::collections::HashMap::new();
    for (i, (_, sig)) in per_track.iter().enumerate() {
        let shape: Vec<u16> = sig.iter().map(|(ct, _)| *ct).collect();
        by_shape.entry(shape).or_default().push(i);
    }

    println!("\nShapes (sequences of child content_types):");
    for (shape, members) in &by_shape {
        println!("  shape {:?} → {} tracks", shape, members.len());
    }

    // Inside each shape group, diff each child position's bytes across members
    println!("\nVarying child payloads per shape:");
    for (shape, members) in &by_shape {
        if members.len() < 2 {
            continue;
        }
        for child_idx in 0..shape.len() {
            // Collect payload bytes for this child position across all members
            let payloads: Vec<&[u8]> = members
                .iter()
                .map(|m| per_track[*m].1[child_idx].1.as_slice())
                .collect();
            let min_len = payloads.iter().map(|p| p.len()).min().unwrap_or(0);
            if min_len == 0 {
                continue;
            }
            // For each byte position, count distinct values
            for i in 0..min_len {
                let mut distinct: std::collections::BTreeMap<u8, usize> =
                    std::collections::BTreeMap::new();
                for p in &payloads {
                    *distinct.entry(p[i]).or_insert(0) += 1;
                }
                if distinct.len() < 2 {
                    continue;
                }
                let mut v: Vec<(u8, usize)> = distinct.into_iter().collect();
                v.sort_by(|a, b| b.1.cmp(&a.1));
                let preview: Vec<String> = v
                    .iter()
                    .take(8)
                    .map(|(b, c)| format!("0x{b:02x}×{c}"))
                    .collect();
                println!(
                    "  shape 0x{:04x} child[{child_idx}] (0x{:04x}) +{i:>3} | {} distinct | {}",
                    shape[0],
                    shape[child_idx],
                    v.len(),
                    preview.join(" ")
                );
            }
        }
    }
}

fn find(blocks: &[RawBlock], ct: ContentType) -> Option<&RawBlock> {
    for b in blocks {
        if b.content_type == Some(ct) {
            return Some(b);
        }
        if let Some(x) = find(&b.children, ct) {
            return Some(x);
        }
    }
    None
}

fn collect(blocks: &[RawBlock], ct: u16) -> Vec<&RawBlock> {
    let mut out = Vec::new();
    fn rec<'a>(blocks: &'a [RawBlock], ct: u16, out: &mut Vec<&'a RawBlock>) {
        for b in blocks {
            if b.content_type_raw == ct {
                out.push(b);
            }
            rec(&b.children, ct, out);
        }
    }
    rec(blocks, ct, &mut out);
    out
}

fn walk_children(b: &RawBlock, data: &[u8], out: &mut Vec<(u16, Vec<u8>)>) {
    for c in &b.children {
        let payload_start = c.start + 9;
        let payload_end = c.end.min(data.len());
        let payload = data[payload_start..payload_end].to_vec();
        out.push((c.content_type_raw, payload));
    }
}
