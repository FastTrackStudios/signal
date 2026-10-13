#![allow(clippy::all, dead_code)]
//! Print every 0x1029 payload byte-by-byte across all tracks so we can
//! visually diff varying fields (looking for track color).

use dawfile_protools::content_type::ContentType;
use dawfile_protools::raw_block::RawBlock;

fn main() {
    let path = std::env::args().nth(1).expect("path");
    let raw = std::fs::read(&path).unwrap();
    let session = dawfile_protools::parse_raw(raw).unwrap();
    let data = session.cursor().data();

    // 0x251a names in order, doubled-list-aware
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

    let mix = collect(&session.blocks, ContentType::TrackMixSettings);
    println!("found {} mix blocks, {} names", mix.len(), names.len());

    // For each byte position, count distinct values
    let payload_len = 281;
    let mut distinct: Vec<std::collections::HashMap<u8, usize>> =
        vec![std::collections::HashMap::new(); payload_len];
    let payloads: Vec<&[u8]> = mix
        .iter()
        .map(|b| {
            let p = b.start + 9;
            &data[p..p + payload_len]
        })
        .collect();

    for p in &payloads {
        for (i, &b) in p.iter().enumerate() {
            *distinct[i].entry(b).or_insert(0) += 1;
        }
    }

    // Print only positions where ≥2 distinct values appear (= varying field).
    // Skip the known volume/pan/mute regions.
    let known_skip: Vec<std::ops::Range<usize>> = vec![
        1..5,     // vol A
        5..6,     // mute A
        13..17,   // pan A
        88..92,   // vol B
        92..93,   // mute B
        103..107, // pan B
    ];

    let in_known = |i: usize| known_skip.iter().any(|r| r.contains(&i));

    println!("\nposition | distinct vals | example values");
    for i in 0..payload_len {
        if distinct[i].len() < 2 || in_known(i) {
            continue;
        }
        let mut vals: Vec<(u8, usize)> = distinct[i].iter().map(|(k, v)| (*k, *v)).collect();
        vals.sort();
        let show: Vec<String> = vals
            .iter()
            .take(6)
            .map(|(b, c)| format!("0x{b:02x}×{c}"))
            .collect();
        println!(
            "+{i:>3} (0x{i:03x}) | {} | {}",
            distinct[i].len(),
            show.join(" ")
        );
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

fn collect(blocks: &[RawBlock], ct: ContentType) -> Vec<&RawBlock> {
    let mut out = Vec::new();
    fn rec<'a>(blocks: &'a [RawBlock], ct: ContentType, out: &mut Vec<&'a RawBlock>) {
        for b in blocks {
            if b.content_type == Some(ct) {
                out.push(b);
            }
            rec(&b.children, ct, out);
        }
    }
    rec(blocks, ct, &mut out);
    out
}
