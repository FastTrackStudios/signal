//! 3-way byte diff of per-track 0x1029 blocks. Uses the same name-based
//! alignment the parser does.

use dawfile_protools::content_type::ContentType;
use dawfile_protools::raw_block::{RawBlock, RawSession};
use std::collections::{HashMap, HashSet};

fn name_to_1029_map(session: &RawSession) -> HashMap<String, &RawBlock> {
    let data = session.cursor().data();
    let mut out = HashMap::new();
    let track_list = {
        fn first(bs: &[RawBlock], ct: ContentType) -> Option<&RawBlock> {
            for b in bs {
                if b.content_type == Some(ct) {
                    return Some(b);
                }
                if let Some(x) = first(&b.children, ct) {
                    return Some(x);
                }
            }
            None
        }
        first(&session.blocks, ContentType::MidiTrackList).expect("0x2519")
    };
    let mut mix_blocks: Vec<&RawBlock> = Vec::new();
    fn walk<'a>(bs: &'a [RawBlock], out: &mut Vec<&'a RawBlock>) {
        for b in bs {
            if b.content_type == Some(ContentType::TrackMixSettings) {
                out.push(b);
            }
            walk(&b.children, out);
        }
    }
    walk(&session.blocks, &mut mix_blocks);

    let mut idx = 0usize;
    let mut seen: HashSet<String> = HashSet::new();
    for c in &track_list.children {
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
        let name = String::from_utf8_lossy(&data[p + 4..p + 4 + len]).to_string();
        if !seen.insert(name.clone()) {
            break;
        }
        if let Some(b) = mix_blocks.get(idx) {
            out.insert(name, *b);
            idx += 1;
        } else {
            break;
        }
    }
    out
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).expect("ptx");
    let muted_name = std::env::args().nth(2).expect("muted track");
    let not_muted_name = std::env::args().nth(3).expect("not muted track");
    let ambig_name = std::env::args().nth(4).expect("ambig track");

    let raw = std::fs::read(&path)?;
    let session = dawfile_protools::parse_raw(raw)?;
    let data = session.cursor().data();
    let map = name_to_1029_map(&session);

    let bm = map.get(&muted_name).expect("muted not found");
    let bnm = map.get(&not_muted_name).expect("not_muted not found");
    let ba = map.get(&ambig_name).expect("ambig not found");

    let pay_len = (bm.block_size as usize)
        .min(bnm.block_size as usize)
        .min(ba.block_size as usize)
        .saturating_sub(2);

    let p = |b: &RawBlock, off: usize| -> u8 {
        let i = b.start + 9 + off;
        if i < data.len() { data[i] } else { 0 }
    };

    println!("0x1029 payload (size {pay_len}):");
    println!("  muted={muted_name} not_muted={not_muted_name} ambig={ambig_name}");
    println!();

    let mut storage = Vec::new();
    let mut effective = Vec::new();
    for off in 0..pay_len {
        let m = p(bm, off);
        let n = p(bnm, off);
        let a = p(ba, off);
        if m != n && a == m {
            storage.push((off, m, n, a));
        }
        if m != a && n == a {
            effective.push((off, m, n, a));
        }
    }

    println!(
        "STORAGE (muted==ambig, differs from not_muted) [{}]:",
        storage.len()
    );
    for (off, m, n, a) in &storage {
        println!("  +{off:>4}: muted=0x{m:02x}  not_muted=0x{n:02x}  ambig=0x{a:02x}");
    }
    println!();
    println!(
        "EFFECTIVE (muted!=ambig, not_muted==ambig) [{}]:",
        effective.len()
    );
    for (off, m, n, a) in &effective {
        println!("  +{off:>4}: muted=0x{m:02x}  not_muted=0x{n:02x}  ambig=0x{a:02x}");
    }
    Ok(())
}
