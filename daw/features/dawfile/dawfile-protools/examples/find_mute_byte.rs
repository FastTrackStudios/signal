#![allow(clippy::all, dead_code)]
//! Brute-force search for any byte in any per-track block that
//! perfectly matches the user-provided mute list for Lord of the Fight.

use dawfile_protools::content_type::ContentType;
use dawfile_protools::raw_block::RawBlock;
use std::collections::{BTreeMap, HashSet};

fn main() {
    let path = std::env::args().nth(1).expect("path");
    let raw = std::fs::read(&path).unwrap();
    let session = dawfile_protools::parse_raw(raw).unwrap();
    let data = session.cursor().data();

    // 0x251a names in order (doubled-list-aware)
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

    // Expected mute set per user
    let expected_muted: HashSet<&str> = [
        "ClickPrint",
        "02 LORD OF THE FIGHT.01",
        "02 LORD OF THE FIGHT_Vocals",
        "02 LORD OF THE FIGHT_Bass",
        "02 LORD OF THE FIGHT_Drums",
        "02 LORD OF THE FIGHT_Guitar",
        "02 LORD OF THE FIGHT_Other",
        "02 LORD OF THE FIGHT_Piano",
        "MIDI 1",
        "Inst 1",
        "Inst 1.dup1",
        "Inst 1.dup2",
        "Inst 1.dup1.02",
        "Inst 1.dup2.02",
        "Inst 1.dup2.04",
        "Inst 1.dup3.02",
        "Inst 1.dup4.02",
    ]
    .into_iter()
    .collect();

    // Two variants: full (audio + MIDI), and audio-only (drops Inst tracks).
    // The PT file might split audio mute and MIDI mute into different byte
    // positions, so try both.
    let audio_only: HashSet<&str> = expected_muted
        .iter()
        .filter(|n| !n.starts_with("Inst ") && **n != "MIDI 1")
        .copied()
        .collect();

    let expected_full: Vec<bool> = names
        .iter()
        .map(|n| expected_muted.contains(n.as_str()))
        .collect();
    let expected_audio: Vec<bool> = names
        .iter()
        .map(|n| audio_only.contains(n.as_str()))
        .collect();
    println!(
        "expected: full={} muted, audio-only={} muted, total tracks={}",
        expected_full.iter().filter(|x| **x).count(),
        expected_audio.iter().filter(|x| **x).count(),
        expected_full.len()
    );

    // For every content type that has ~per-track count, brute force byte
    // positions looking for one that maps to expected.
    let mut by_ct: BTreeMap<u16, Vec<&RawBlock>> = BTreeMap::new();
    walk(&session.blocks, &mut by_ct);

    for (ct, blocks) in &by_ct {
        let n = blocks.len();
        // Match CT counts: per-track (30) or doubled (60) or per-non-master (29).
        if !(20..=names.len() * 2 + 4).contains(&n) {
            continue;
        }
        // Align by truncating to len(names). For CTs with 29 entries (missing
        // Master), we accept that the first track-name might not have a block
        // and check both alignments.
        let aligned_a: Vec<&RawBlock> = blocks.iter().take(names.len()).copied().collect();
        // shift-by-1: skip first track-name, align block[0] with name[1]
        let aligned_b: Vec<&RawBlock> = if blocks.len() < names.len() {
            // pad with first block to keep length
            blocks.iter().take(names.len()).copied().collect()
        } else {
            blocks.iter().skip(0).take(names.len()).copied().collect()
        };
        let aligned = aligned_a;
        let _ = aligned_b;
        let min_len = aligned
            .iter()
            .map(|b| b.block_size.saturating_sub(2) as usize)
            .min()
            .unwrap_or(0);
        if min_len == 0 {
            continue;
        }
        for i in 0..min_len {
            let vals: Vec<u8> = aligned
                .iter()
                .map(|b| {
                    let p = b.start + 9 + i;
                    if p < data.len() { data[p] } else { 0 }
                })
                .collect();
            // does this byte's nonzero positions match `expected` exactly?
            let bool_vec: Vec<bool> = vals.iter().map(|v| *v != 0).collect();
            // Try both directions: byte set when muted, OR byte cleared when muted
            for (label, expected) in [("FULL", &expected_full), ("AUDIO-ONLY", &expected_audio)] {
                if bool_vec == *expected {
                    println!("MATCH ({label}): 0x{ct:04x} +{i:>4} (byte=1 when muted)");
                }
                let inv: Vec<bool> = bool_vec.iter().map(|b| !b).collect();
                if inv == *expected {
                    println!("MATCH ({label}, INVERTED): 0x{ct:04x} +{i:>4} (byte=0 when muted)");
                }
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

fn walk<'a>(blocks: &'a [RawBlock], by_ct: &mut BTreeMap<u16, Vec<&'a RawBlock>>) {
    for b in blocks {
        by_ct.entry(b.content_type_raw).or_default().push(b);
        walk(&b.children, by_ct);
    }
}
