//! For two tracks in a session, find their per-track 0x260d wrapper and
//! produce a byte-level diff of the wrapper payloads.
//!
//! Usage: diff_two_tracks <session.ptx> <track_A> <track_B>
//!
//! Used to compare ClickPrint (truly muted per converter) vs SYZ
//! (+5=1 in PTX but converter outputs MUTESOLO 0) to find the
//! discriminator byte.

use dawfile_protools::content_type::ContentType;
use dawfile_protools::raw_block::RawBlock;
use std::collections::HashSet;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).expect("ptx");
    let track_a = std::env::args().nth(2).expect("track A name");
    let track_b = std::env::args().nth(3).expect("track B name");

    let raw = std::fs::read(&path)?;
    let session = dawfile_protools::parse_raw(raw)?;
    let data = session.cursor().data();

    // Walk 0x251a track list in document order to get name -> position.
    fn first(blocks: &[RawBlock], ct: ContentType) -> Option<&RawBlock> {
        for b in blocks {
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
        let name = String::from_utf8_lossy(&data[p + 4..p + 4 + len]).to_string();
        if !seen.insert(name.clone()) {
            break;
        }
        names.push(name);
    }

    let mut wrappers: Vec<&RawBlock> = Vec::new();
    fn collect_260d<'a>(blocks: &'a [RawBlock], out: &mut Vec<&'a RawBlock>) {
        for b in blocks {
            if b.content_type_raw == 0x260d {
                out.push(b);
            }
            collect_260d(&b.children, out);
        }
    }
    collect_260d(&session.blocks, &mut wrappers);

    // 0x260d count = names.len() - 1 (Master has no 0x260d)
    // align names[1..] with wrappers
    let find_wrapper = |name: &str| -> Option<&&RawBlock> {
        names.iter().position(|n| n == name).and_then(|idx| {
            if idx == 0 {
                None
            } else {
                wrappers.get(idx - 1)
            }
        })
    };

    let a = find_wrapper(&track_a).expect("track A not found");
    let b = find_wrapper(&track_b).expect("track B not found");

    let a_payload = a.start + 9;
    let b_payload = b.start + 9;
    let a_len = (a.block_size as usize).saturating_sub(2);
    let b_len = (b.block_size as usize).saturating_sub(2);
    println!(
        "Track A: {track_a:<25} 0x260d @ 0x{:x}  payload size {}",
        a.start, a_len
    );
    println!(
        "Track B: {track_b:<25} 0x260d @ 0x{:x}  payload size {}",
        b.start, b_len
    );

    let min_len = a_len.min(b_len);
    let mut diff_count = 0;
    for off in 0..min_len {
        let av = data[a_payload + off];
        let bv = data[b_payload + off];
        if av != bv {
            println!("  +{off:>4}: {track_a}=0x{av:02x}  {track_b}=0x{bv:02x}");
            diff_count += 1;
            if diff_count > 100 {
                println!("  ... (truncated; >100 diffs)");
                break;
            }
        }
    }
    println!("\ntotal diff bytes: {diff_count}");
    Ok(())
}
