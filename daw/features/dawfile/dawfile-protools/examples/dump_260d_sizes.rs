//! Print every 0x260d wrapper's size + child counts, aligned with track names.

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

    let wrappers = collect(&session.blocks, ContentType::TrackMixWrapper);
    println!("idx  size  kids  | track");
    for (i, w) in wrappers.iter().enumerate() {
        let name = names.get(i).cloned().unwrap_or_else(|| "?".into());
        let kid_summary: Vec<String> = w
            .children
            .iter()
            .map(|c| format!("0x{:04x}({}B)", c.content_type_raw, c.block_size))
            .collect();
        let n_260a = w
            .children
            .iter()
            .filter(|c| c.content_type_raw == 0x260a)
            .count();
        let n_260c = w
            .children
            .iter()
            .filter(|c| c.content_type_raw == 0x260c)
            .count();
        println!(
            "[{i:02}] {:>5}   {} (260a={n_260a} 260c={n_260c}) | {name}\n     {}",
            w.block_size,
            w.children.len(),
            kid_summary.join(", ")
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
