//! Dump 0x1029 bytes for named tracks side by side.

use dawfile_protools::content_type::ContentType;
use dawfile_protools::raw_block::RawBlock;

fn main() {
    let path = std::env::args().nth(1).expect("path");
    let raw = std::fs::read(&path).unwrap();
    let session = dawfile_protools::parse_raw(raw).unwrap();
    let data = session.cursor().data();

    // 0x251a names in order
    let list = find(&session.blocks, ContentType::MidiTrackList).unwrap();
    let mut names: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for c in &list.children {
        if c.content_type != Some(ContentType::MidiTrackInfo) {
            continue;
        }
        let off = c.start + 11;
        let len = u32::from_le_bytes(data[off..off + 4].try_into().unwrap()) as usize;
        let name = String::from_utf8_lossy(&data[off + 4..off + 4 + len.min(64)]).to_string();
        if !seen.insert(name.clone()) {
            break;
        }
        names.push(name);
    }

    let mix_blocks = collect(&session.blocks, ContentType::TrackMixSettings);

    let want: Vec<String> = std::env::args().skip(2).collect();
    let want_refs: Vec<&str> = want.iter().map(|s| s.as_str()).collect();
    let want = want_refs;

    println!("Bytes for tracks: {:?}", want);
    println!(
        "Pos  | {}",
        want.iter()
            .map(|n| format!("{:>15}", n.chars().take(15).collect::<String>()))
            .collect::<Vec<_>>()
            .join(" ")
    );

    let mut payloads: Vec<&[u8]> = Vec::new();
    for w in &want {
        let idx = names.iter().position(|n| n == w);
        match idx {
            Some(i) if i < mix_blocks.len() => {
                let b = mix_blocks[i];
                let p = b.start + 9;
                payloads.push(&data[p..p + 281.min(data.len() - p)]);
            }
            _ => {
                println!("# {} not found", w);
                return;
            }
        }
    }
    for i in 0..281 {
        let line: Vec<String> = payloads
            .iter()
            .map(|p| {
                if i < p.len() {
                    format!("{:>15}", format!("0x{:02x}", p[i]))
                } else {
                    "  ?".to_string()
                }
            })
            .collect();
        // only print if at least one differs
        let first = payloads.first().and_then(|p| p.get(i).copied());
        let differs = payloads.iter().any(|p| p.get(i).copied() != first);
        if differs {
            println!("+{i:>3}  | {}", line.join(" "));
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
