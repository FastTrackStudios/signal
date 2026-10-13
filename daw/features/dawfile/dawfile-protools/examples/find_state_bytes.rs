//! Search 0x1029 payload for ALL byte positions whose values across
//! tracks match boolean field semantics (only 0/1 across all tracks),
//! then print per-track values aligned with the user's mute list so we
//! can pick which byte is `mute`, `active`, `hidden`, `solo`, `soloSafe`.

use dawfile_protools::content_type::ContentType;
use dawfile_protools::raw_block::RawBlock;
use std::collections::HashSet;

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

    let mix_blocks = collect(&session.blocks, ContentType::TrackMixSettings);

    // Expected static mute per user — only ClickPrint and 02 LORD family
    // are CONFIRMED to be true-muted (no automation override).
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

    // For each byte position in 0x1029 (281 bytes), check if it's
    // strictly 0/1 across all blocks, then compare its pattern to
    // the expected mute.
    const PAYLOAD_LEN: usize = 281;
    let payloads: Vec<&[u8]> = mix_blocks
        .iter()
        .take(names.len())
        .map(|b| {
            let p = b.start + 9;
            &data[p..p + PAYLOAD_LEN.min(data.len() - p)]
        })
        .collect();
    let aligned_names: Vec<&str> = names
        .iter()
        .take(payloads.len())
        .map(|s| s.as_str())
        .collect();
    let expected: Vec<bool> = aligned_names
        .iter()
        .map(|n| expected_muted.contains(n))
        .collect();

    println!(
        "scanning {} 0x1029 blocks × {} bytes",
        payloads.len(),
        PAYLOAD_LEN
    );
    println!(
        "expected mute = {} tracks",
        expected.iter().filter(|x| **x).count()
    );

    // For each byte position, count distinct values
    for i in 0..PAYLOAD_LEN {
        let vals: Vec<u8> = payloads
            .iter()
            .map(|p| p.get(i).copied().unwrap_or(0))
            .collect();
        let distinct: HashSet<u8> = vals.iter().copied().collect();
        if distinct.len() != 2 || !distinct.contains(&0) || !distinct.contains(&1) {
            continue;
        }
        let bool_vals: Vec<bool> = vals.iter().map(|v| *v != 0).collect();
        // hamming distance to expected
        let matches = bool_vals
            .iter()
            .zip(expected.iter())
            .filter(|(a, b)| a == b)
            .count();
        let total = expected.len();
        println!("+{i:>3}  matches {matches}/{total}");
        if matches >= total - 2 {
            // Show diff
            for (idx, (a, b)) in bool_vals.iter().zip(expected.iter()).enumerate() {
                if a != b {
                    println!(
                        "    DIFF [{idx}] {} parser={} expected={}",
                        aligned_names[idx], a, b
                    );
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
