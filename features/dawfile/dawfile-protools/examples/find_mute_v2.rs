//! Round-2 mute hunt. Strategy:
//!
//! 1. Build the truth set of muted track names from user ground truth.
//! 2. For every block in the file, capture a stable "track key" if we can:
//!      - parent's 0x251a name if this block sits in a per-track wrapper,
//!      - or a `+18` track-index byte (0x251a inline carries one),
//!      - or, failing that, positional order among siblings of same CT.
//! 3. For every content_type, test: does "presence of any block of this CT
//!    on a track" partition the tracks exactly into muted vs not-muted?
//! 4. Separately: for every (ct, fixed_offset, byte_value) triple where
//!    block count == track count and per-block byte at that offset
//!    perfectly correlates with mute state, report.

use dawfile_protools::content_type::ContentType;
use dawfile_protools::raw_block::RawBlock;
use std::collections::{BTreeMap, BTreeSet, HashSet};

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
    let n_tracks = names.len();

    // Ground truth from PT Reaper Converter v1.5.4 output (Frida-captured).
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
    let n_muted_expected = expected.iter().filter(|x| **x).count();
    println!("tracks={} muted_expected={}", n_tracks, n_muted_expected);

    // Index every block by content_type_raw.
    let mut by_ct: BTreeMap<u16, Vec<&RawBlock>> = BTreeMap::new();
    walk(&session.blocks, &mut by_ct);
    println!("found {} distinct content types", by_ct.len());

    // ── Strategy A: per-track wrapper presence ────────────────────────────────
    //
    // For each per-track wrapper CT we know about (0x260d = AudioTrackInfo,
    // 0x251a/MidiTrackInfo entries themselves), look for a CHILD content_type
    // that's present iff track is muted.
    let wrapper_cts = [
        ("0x260d AudioTrackInfo", 0x260du16),
        ("0x251a MidiTrackInfo", 0x251au16),
        ("0x1029 mix wrapper", 0x1029u16),
    ];
    for (label, wct) in wrapper_cts {
        let wrappers: Vec<&RawBlock> = by_ct.get(&wct).cloned().unwrap_or_default();
        if wrappers.is_empty() {
            continue;
        }
        println!("\n=== {label}: {} wrappers ===", wrappers.len());
        // Try alignments 1:1 to names list (taking first n_tracks).
        if wrappers.len() < n_tracks {
            println!("  too few wrappers ({} < {})", wrappers.len(), n_tracks);
            continue;
        }
        // Collect set of child CTs per track index
        let aligned: Vec<&RawBlock> = wrappers.iter().take(n_tracks).copied().collect();
        let mut child_cts_per_track: Vec<BTreeSet<u16>> = Vec::with_capacity(n_tracks);
        for w in &aligned {
            let mut s = BTreeSet::new();
            for c in &w.children {
                s.insert(c.content_type_raw);
            }
            child_cts_per_track.push(s);
        }
        // Universe of CTs we saw
        let mut universe: BTreeSet<u16> = BTreeSet::new();
        for s in &child_cts_per_track {
            for ct in s {
                universe.insert(*ct);
            }
        }
        for ct in universe {
            let present: Vec<bool> = child_cts_per_track
                .iter()
                .map(|s| s.contains(&ct))
                .collect();
            if present == expected {
                println!("  MATCH presence: child 0x{ct:04x} (present iff muted)");
            }
            let inv: Vec<bool> = present.iter().map(|b| !b).collect();
            if inv == expected {
                println!("  MATCH absence: child 0x{ct:04x} (absent iff muted)");
            }
        }
    }

    // ── Strategy B: doubled-block alignment (stereo pairs) ──────────────────
    //
    // Many CTs occur as 2*n_audio + n_midi entries when audio tracks are
    // stereo. Try aligning by grouping every-other block.
    println!("\n=== Strategy B: doubled-alignment scan ===");
    for (ct, blocks) in &by_ct {
        // Only consider CTs whose count is plausibly per-track-doubled
        let n = blocks.len();
        if n == 0 {
            continue;
        }
        // Possible alignments:
        // (a) take every 2nd block, take(n_tracks)
        // (b) take first n_tracks
        // (c) take last n_tracks
        let candidates: Vec<Vec<&RawBlock>> = vec![
            blocks.iter().step_by(2).take(n_tracks).copied().collect(),
            blocks.iter().take(n_tracks).copied().collect(),
            blocks.iter().rev().take(n_tracks).rev().copied().collect(),
        ];
        for (variant, aligned) in candidates.iter().enumerate() {
            if aligned.len() != n_tracks {
                continue;
            }
            let min_len = aligned
                .iter()
                .map(|b| b.block_size.saturating_sub(2) as usize)
                .min()
                .unwrap_or(0);
            if min_len == 0 {
                continue;
            }
            // Limit to first 64 bytes per block for speed
            let scan = min_len.min(64);
            for i in 0..scan {
                let vals: Vec<u8> = aligned
                    .iter()
                    .map(|b| {
                        let p = b.start + 9 + i;
                        if p < data.len() { data[p] } else { 0 }
                    })
                    .collect();
                let bv: Vec<bool> = vals.iter().map(|v| *v != 0).collect();
                if bv == expected {
                    println!("  MATCH ct=0x{ct:04x} variant={variant} +{i:>3} (nonzero iff muted)");
                }
                let inv: Vec<bool> = bv.iter().map(|b| !b).collect();
                if inv == expected {
                    println!("  MATCH ct=0x{ct:04x} variant={variant} +{i:>3} (zero iff muted)");
                }
            }
        }
    }

    // ── Strategy C: list every per-track count by CT ────────────────────────
    println!("\n=== Per-CT block counts (to spot 17-count CTs) ===");
    let mut cts: Vec<(u16, usize)> = by_ct.iter().map(|(k, v)| (*k, v.len())).collect();
    cts.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (ct, n) in cts.iter().take(40) {
        let marker = if *n == n_muted_expected {
            " ←!17!"
        } else {
            ""
        };
        println!("  0x{ct:04x}: {n}{marker}");
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
