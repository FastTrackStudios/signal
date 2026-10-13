//! Diff the FULL 0x260d (TrackMixWrapper) and 0x200b (TrackAuxState)
//! payloads between two named tracks aligned via 0x251a order.

use dawfile_protools::content_type::ContentType;
use dawfile_protools::raw_block::RawBlock;

fn main() {
    let path = std::env::args().nth(1).expect("path");
    let raw = std::fs::read(&path).unwrap();
    let session = dawfile_protools::parse_raw(raw).unwrap();
    let data = session.cursor().data();

    let list = find(&session.blocks, ContentType::MidiTrackList).unwrap();
    let mut names: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
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

    // 0x260d wrappers (in 0x251a order)
    let wrappers = collect(&session.blocks, ContentType::TrackMixWrapper);
    // 0x261c containers (only colored tracks)
    let containers = collect(&session.blocks, ContentType::TrackContainer);

    let args: Vec<String> = std::env::args().skip(2).collect();
    if args.len() < 2 {
        eprintln!("usage: diff_track_wrapper <ptx> <track-a> <track-b> [more...]");
        return;
    }

    println!("=== 0x260d (TrackMixWrapper) ===");
    diff_blocks(&args, &names, &wrappers, data);

    println!("\n=== 0x200b (TrackAuxState) inside 0x261c ===");
    let mut aux: Vec<&RawBlock> = Vec::new();
    for c in &containers {
        if let Some(b) = c.children.iter().find(|x| x.content_type_raw == 0x200b) {
            aux.push(b);
        }
    }
    // 0x261c is only for non-folder tracks. Names list for these:
    let aux_names: Vec<String> = containers
        .iter()
        .map(|c| find_2619_name(c, data).unwrap_or_default())
        .collect();
    diff_blocks(&args, &aux_names, &aux, data);
}

fn diff_blocks(want: &[String], names: &[String], blocks: &[&RawBlock], data: &[u8]) {
    let mut payloads: Vec<(String, &[u8])> = Vec::new();
    for w in want {
        let idx = names.iter().position(|n| n == w);
        match idx {
            Some(i) if i < blocks.len() => {
                let b = blocks[i];
                let p = b.start + 9;
                let end = b.end.min(data.len());
                payloads.push((w.clone(), &data[p..end]));
            }
            _ => {
                println!("  # '{}' not found in this CT", w);
            }
        }
    }
    if payloads.len() < 2 {
        return;
    }
    let min_len = payloads.iter().map(|(_, p)| p.len()).min().unwrap_or(0);
    let header: Vec<String> = payloads
        .iter()
        .map(|(n, _)| format!("{:>10}", n.chars().take(10).collect::<String>()))
        .collect();
    println!("  pos  | {}", header.join(" "));
    let mut diffs = 0;
    for i in 0..min_len {
        let first = payloads[0].1[i];
        if payloads.iter().any(|(_, p)| p[i] != first) {
            let vals: Vec<String> = payloads
                .iter()
                .map(|(_, p)| format!("{:>10}", format!("0x{:02x}", p[i])))
                .collect();
            println!("  +{i:>3}  | {}", vals.join(" "));
            diffs += 1;
            if diffs > 80 {
                println!("  ... (truncated)");
                return;
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

fn find_2619_name(b: &RawBlock, data: &[u8]) -> Option<String> {
    for c in &b.children {
        if c.content_type_raw == 0x2619 {
            let p = c.start + 9;
            if p + 4 > data.len() {
                return None;
            }
            let len = u32::from_le_bytes(data[p..p + 4].try_into().unwrap()) as usize;
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
