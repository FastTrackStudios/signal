//! Walk each per-track 0x260d wrapper. For each track, collect the
//! SET of child content_types present anywhere in its subtree. Then
//! find which CT is present iff the track is in the user-confirmed
//! "actually muted" set (per the converter's MUTESOLO output on LotF).

use dawfile_protools::content_type::ContentType;
use dawfile_protools::raw_block::RawBlock;
use std::collections::{BTreeSet, HashSet};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).expect("ptx path");
    let raw = std::fs::read(&path)?;
    let session = dawfile_protools::parse_raw(raw)?;
    let data = session.cursor().data();

    // Ground truth from Frida: only these 2 LotF tracks have an
    // explicit PTXMutePoint object. The 6 LORD-family stems below
    // them inherit mute via folder walk.
    let explicitly_muted: HashSet<&str> = ["ClickPrint", "02 LORD OF THE FIGHT.01"]
        .into_iter()
        .collect();

    // Walk 0x251a (MIDI track list) to get track names IN ORDER.
    let track_list = collect_first(&session.blocks, ContentType::MidiTrackList);
    let Some(list) = track_list else {
        eprintln!("no 0x2519");
        return Ok(());
    };

    let mut names: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for c in &list.children {
        if c.content_type != Some(ContentType::MidiTrackInfo) {
            continue;
        }
        let p = c.start + 11;
        if p + 4 > data.len() {
            continue;
        }
        let len = u32::from_le_bytes(data[p..p + 4].try_into().unwrap()) as usize;
        if len == 0 || len > 64 || p + 4 + len > data.len() {
            continue;
        }
        let name = String::from_utf8_lossy(&data[p + 4..p + 4 + len.min(64)]).to_string();
        if !seen.insert(name.clone()) {
            break;
        }
        names.push(name);
    }
    let n = names.len();

    // Collect 0x260d wrappers in document order. Should be 1:1 with track
    // names (minus Master, which has no 0x260d).
    let mut wrappers: Vec<&RawBlock> = Vec::new();
    fn collect_ct<'a>(blocks: &'a [RawBlock], ct: u16, out: &mut Vec<&'a RawBlock>) {
        for b in blocks {
            if b.content_type_raw == ct {
                out.push(b);
            }
            collect_ct(&b.children, ct, out);
        }
    }
    collect_ct(&session.blocks, 0x260d, &mut wrappers);
    println!("0x260d count: {}, names count: {n}", wrappers.len());

    // Build a "track index → set of all descendant CTs" map.
    fn collect_descendants(b: &RawBlock, out: &mut BTreeSet<u16>) {
        for c in &b.children {
            out.insert(c.content_type_raw);
            collect_descendants(c, out);
        }
    }

    // Master usually has no 0x260d, so align with names[1..] (skip Master).
    // OR maybe align with names[0..]. Let's try both.
    for skip in [0, 1] {
        println!("\n=== alignment: track[{skip}+i] ↔ wrapper[i] ===");
        let aligned: Vec<(&str, &RawBlock)> = names
            .iter()
            .skip(skip)
            .map(|s| s.as_str())
            .zip(wrappers.iter().copied())
            .collect();

        // Compute per-track CT sets
        let per_track: Vec<(String, BTreeSet<u16>)> = aligned
            .iter()
            .map(|(name, w)| {
                let mut s = BTreeSet::new();
                collect_descendants(w, &mut s);
                (name.to_string(), s)
            })
            .collect();

        // Find CTs that distinguish: present for "ClickPrint" &
        // "02 LORD OF THE FIGHT.01", absent for all others.
        let mut universe: BTreeSet<u16> = BTreeSet::new();
        for (_, s) in &per_track {
            universe.extend(s);
        }
        let mut matches: Vec<u16> = Vec::new();
        for ct in &universe {
            let present_set: HashSet<&str> = per_track
                .iter()
                .filter(|(_, s)| s.contains(ct))
                .map(|(n, _)| n.as_str())
                .collect();
            if present_set == explicitly_muted {
                matches.push(*ct);
            }
        }
        if matches.is_empty() {
            println!("  no exact match for {{ClickPrint, 02 LORD OF THE FIGHT.01}}");
        } else {
            for ct in matches {
                println!("  ★ 0x{ct:04x} present iff track is explicitly muted!");
            }
        }

        // Also report CTs present for ANY subset overlapping the muted set
        println!("  CT presence by name (only for muted-set members):");
        for (name, s) in &per_track {
            if explicitly_muted.contains(name.as_str()) || name.starts_with("02 LORD OF THE FIGHT")
            {
                let cts: Vec<String> = s.iter().map(|x| format!("0x{x:04x}")).collect();
                println!("    {name}: {} CTs", cts.len());
            }
        }
    }
    Ok(())
}

fn collect_first(blocks: &[RawBlock], ct: ContentType) -> Option<&RawBlock> {
    for b in blocks {
        if b.content_type == Some(ct) {
            return Some(b);
        }
        if let Some(x) = collect_first(&b.children, ct) {
            return Some(x);
        }
    }
    None
}
