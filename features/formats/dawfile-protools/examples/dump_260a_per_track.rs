//! For each 0x260d wrapper, dump its 0x260a child payloads.

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
    for (i, w) in wrappers.iter().enumerate() {
        let name = names.get(i).cloned().unwrap_or_default();
        let a_blocks: Vec<&RawBlock> = w
            .children
            .iter()
            .filter(|c| c.content_type_raw == 0x260a)
            .collect();
        print!("[{i:02}] {:>40} |", &name[..name.len().min(40)]);
        for a in &a_blocks {
            let p = a.start + 9;
            let len = (a.block_size as usize).saturating_sub(2).min(30);
            let hex: String = data[p..p + len.min(data.len() - p)]
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<Vec<_>>()
                .join("");
            print!(" {hex}");
        }
        println!();
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
