//! Exhaustive byte/bit scan over per-track 1:1 content types.
//! Strategies that may discriminate the mute state:
//! - Single byte at offset N (== 0x01 iff muted)
//! - Single bit at (offset N, bit B)
//! - i32 nonzero at offset N (envelope present iff muted)
//! - i32 ≥ threshold at offset N

use dawfile_protools::content_type::ContentType;
use dawfile_protools::raw_block::RawBlock;
use std::collections::{BTreeMap, HashSet};

fn main() {
    let path = std::env::args().nth(1).expect("path");
    let raw = std::fs::read(&path).unwrap();
    let session = dawfile_protools::parse_raw(raw).unwrap();
    let data = session.cursor().data();

    let list = find(&session.blocks, ContentType::MidiTrackList).unwrap();
    let mut names: Vec<String> = Vec::new();
    let mut seen = HashSet::new();
    for c in &list.children {
        if c.content_type != Some(ContentType::MidiTrackInfo) {
            continue;
        }
        let off = c.start + 11;
        let len = u32::from_le_bytes(data[off..off + 4].try_into().unwrap()) as usize;
        let n = String::from_utf8_lossy(&data[off + 4..off + 4 + len.min(64)]).to_string();
        if !seen.insert(n.clone()) {
            break;
        }
        names.push(n);
    }

    // Ground truth from PT Reaper Converter v1.5.4 output (Frida-captured RPP
    // on LotF — see docs/pt-reaper-converter-re.md). Earlier hardcoded list
    // (incl. Inst*/MIDI 1) was wrong — those tracks show MUTESOLO 0 0 0 in
    // the converter's output.
    let expected_muted: HashSet<&str> = [
        "ClickPrint",
        "02 LORD OF THE FIGHT.01",
        "02 LORD OF THE FIGHT_Vocals",
        "02 LORD OF THE FIGHT_Bass",
        "02 LORD OF THE FIGHT_Drums",
        "02 LORD OF THE FIGHT_Guitar",
        "02 LORD OF THE FIGHT_Other",
        "02 LORD OF THE FIGHT_Piano",
    ]
    .into_iter()
    .collect();
    let expected: Vec<bool> = names
        .iter()
        .map(|n| expected_muted.contains(n.as_str()))
        .collect();

    let mut by_ct: BTreeMap<u16, Vec<&RawBlock>> = BTreeMap::new();
    walk(&session.blocks, &mut by_ct);

    // Pick all CTs with count == names.len() OR (names.len()*2)
    let n = names.len();
    let mut candidates: Vec<(u16, Vec<&RawBlock>)> = Vec::new();
    for (ct, blocks) in &by_ct {
        if blocks.len() == n {
            candidates.push((*ct, blocks.clone()));
        } else if blocks.len() == n * 2 {
            // doubled — take first half
            candidates.push((*ct, blocks.iter().take(n).copied().collect()));
            // and also evens
            candidates.push((*ct, blocks.iter().step_by(2).take(n).copied().collect()));
        }
    }

    let read_u8 = |b: &RawBlock, off: usize| -> Option<u8> {
        let p = b.start + 9 + off;
        if p < data.len() && off < b.block_size as usize {
            Some(data[p])
        } else {
            None
        }
    };
    let read_u32 = |b: &RawBlock, off: usize| -> Option<u32> {
        let p = b.start + 9 + off;
        if p + 4 <= data.len() && off + 4 <= b.block_size as usize {
            Some(u32::from_le_bytes(data[p..p + 4].try_into().unwrap()))
        } else {
            None
        }
    };

    let mut matches = 0;
    for (ct, blocks) in &candidates {
        let min_len = blocks
            .iter()
            .map(|b| b.block_size.saturating_sub(2) as usize)
            .min()
            .unwrap_or(0);

        // Single byte == 0x01 / nonzero
        for off in 0..min_len {
            // bytes-equal-1 mode
            let actual: Vec<bool> = blocks.iter().map(|b| read_u8(b, off) == Some(1)).collect();
            if actual == expected {
                println!("MATCH ct=0x{ct:04x} byte+{off:>3} == 1 iff muted");
                matches += 1;
            }
            let inv: Vec<bool> = actual.iter().map(|b| !b).collect();
            if inv == expected {
                println!("MATCH ct=0x{ct:04x} byte+{off:>3} == 1 iff NOT muted");
                matches += 1;
            }
            // bytes-nonzero mode
            let nz: Vec<bool> = blocks
                .iter()
                .map(|b| read_u8(b, off).map(|x| x != 0).unwrap_or(false))
                .collect();
            if nz == expected {
                println!("MATCH ct=0x{ct:04x} byte+{off:>3} != 0 iff muted");
                matches += 1;
            }
            // bit-level
            for bit in 0..8u8 {
                let bv: Vec<bool> = blocks
                    .iter()
                    .map(|b| {
                        read_u8(b, off)
                            .map(|x| (x >> bit) & 1 == 1)
                            .unwrap_or(false)
                    })
                    .collect();
                if bv == expected {
                    println!("MATCH ct=0x{ct:04x} bit (+{off}, b{bit}) iff muted");
                    matches += 1;
                }
                let inv: Vec<bool> = bv.iter().map(|b| !b).collect();
                if inv == expected {
                    println!("MATCH ct=0x{ct:04x} bit (+{off}, b{bit}) iff NOT muted");
                    matches += 1;
                }
            }
        }

        // u32 nonzero
        for off in 0..min_len.saturating_sub(3) {
            let nz: Vec<bool> = blocks
                .iter()
                .map(|b| read_u32(b, off).map(|x| x != 0).unwrap_or(false))
                .collect();
            if nz == expected {
                println!("MATCH ct=0x{ct:04x} u32+{off:>3} != 0 iff muted");
                matches += 1;
            }
        }
    }
    println!(
        "done, {matches} matches across {} candidate blocksets",
        candidates.len()
    );
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

fn walk<'a>(blocks: &'a [RawBlock], by_ct: &mut BTreeMap<u16, Vec<&'a RawBlock>>) {
    for b in blocks {
        by_ct.entry(b.content_type_raw).or_default().push(b);
        walk(&b.children, by_ct);
    }
}
