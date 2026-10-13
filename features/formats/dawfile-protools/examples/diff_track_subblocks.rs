//! For two tracks, walk every child block of their 0x260d wrappers
//! and produce a per-CT byte diff. Goal: find blocks (or specific
//! bytes within blocks) that ONLY differ between effective-muted and
//! over-muted tracks.

use dawfile_protools::content_type::ContentType;
use dawfile_protools::raw_block::{RawBlock, RawSession};
use std::collections::{HashMap, HashSet};

fn name_to_wrapper(session: &RawSession) -> HashMap<String, &RawBlock> {
    let data = session.cursor().data();
    let mut out = HashMap::new();

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
    let list = first(&session.blocks, ContentType::MidiTrackList).expect("0x2519");

    let mut wrappers: Vec<&RawBlock> = Vec::new();
    fn walk<'a>(bs: &'a [RawBlock], out: &mut Vec<&'a RawBlock>) {
        for b in bs {
            if b.content_type == Some(ContentType::TrackMixWrapper) {
                out.push(b);
            }
            walk(&b.children, out);
        }
    }
    walk(&session.blocks, &mut wrappers);

    let mut idx = 0usize;
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
        let name = String::from_utf8_lossy(&data[p + 4..p + 4 + len]).to_string();
        if !seen.insert(name.clone()) {
            break;
        }
        if let Some(w) = wrappers.get(idx) {
            out.insert(name, *w);
            idx += 1;
        } else {
            break;
        }
    }
    out
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).expect("ptx");
    let a = std::env::args().nth(2).expect("track A");
    let b = std::env::args().nth(3).expect("track B");

    let raw = std::fs::read(&path)?;
    let session = dawfile_protools::parse_raw(raw)?;
    let data = session.cursor().data();
    let m = name_to_wrapper(&session);

    let wa = m.get(&a).expect("track A not found");
    let wb = m.get(&b).expect("track B not found");

    println!(
        "wrapper A {}: 0x260d @ 0x{:x} size {}",
        a, wa.start, wa.block_size
    );
    println!(
        "wrapper B {}: 0x260d @ 0x{:x} size {}",
        b, wb.start, wb.block_size
    );

    // Group children by content_type → list of (a_child, b_child) pairs
    let mut a_by_ct: HashMap<u16, Vec<&RawBlock>> = HashMap::new();
    for c in &wa.children {
        a_by_ct.entry(c.content_type_raw).or_default().push(c);
    }
    let mut b_by_ct: HashMap<u16, Vec<&RawBlock>> = HashMap::new();
    for c in &wb.children {
        b_by_ct.entry(c.content_type_raw).or_default().push(c);
    }

    let all_cts: std::collections::BTreeSet<u16> =
        a_by_ct.keys().chain(b_by_ct.keys()).copied().collect();

    for ct in all_cts {
        let av = a_by_ct.get(&ct).cloned().unwrap_or_default();
        let bv = b_by_ct.get(&ct).cloned().unwrap_or_default();
        if av.len() != bv.len() {
            println!("\n0x{ct:04x}: COUNT DIFFERS  a={} b={}", av.len(), bv.len());
            continue;
        }
        for (i, (cha, chb)) in av.iter().zip(bv.iter()).enumerate() {
            let sa = cha.block_size as usize;
            let sb = chb.block_size as usize;
            if sa != sb {
                println!("\n0x{ct:04x}[{i}]: SIZE DIFFERS  a={sa} b={sb}");
                continue;
            }
            let pay_len = sa.saturating_sub(2);
            let mut diffs = Vec::new();
            for off in 0..pay_len {
                let av_b = data.get(cha.start + 9 + off).copied().unwrap_or(0);
                let bv_b = data.get(chb.start + 9 + off).copied().unwrap_or(0);
                if av_b != bv_b {
                    diffs.push((off, av_b, bv_b));
                }
            }
            if diffs.is_empty() {
                continue;
            }
            // Skip noise blocks (>30% bytes differ)
            if diffs.len() * 3 > pay_len {
                continue;
            }
            println!(
                "\n0x{ct:04x}[{i}] (size {pay_len}): {} diff bytes",
                diffs.len()
            );
            for (off, av_b, bv_b) in diffs.iter().take(40) {
                println!("  +{off:>4}: a=0x{av_b:02x}  b=0x{bv_b:02x}");
            }
        }
    }
    Ok(())
}
