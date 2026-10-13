//! Dump 0x251a entries with their 0x4420 child bytes to find folder parent info.

use dawfile_protools::content_type::ContentType;
use dawfile_protools::raw_block::RawBlock;

fn main() {
    let path = std::env::args().nth(1).expect("path");
    let raw = std::fs::read(&path).unwrap();
    let session = dawfile_protools::parse_raw(raw).unwrap();
    let data = session.cursor().data();

    let list = find(&session.blocks, ContentType::MidiTrackList).unwrap();
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
        let kind_byte = data[c.start + 9];
        // child 0x4420 (4-byte) gives a per-track 16-bit value
        let child_bytes: Vec<String> = c
            .children
            .iter()
            .map(|ch| {
                let p = ch.start + 9;
                let pl = (ch.block_size as usize).saturating_sub(2);
                let bytes = &data[p..p + pl.min(16)];
                format!(
                    "0x{:04x}({}): {}",
                    ch.content_type_raw,
                    ch.block_size,
                    bytes
                        .iter()
                        .map(|b| format!("{:02x}", b))
                        .collect::<Vec<_>>()
                        .join(" ")
                )
            })
            .collect();
        // Also bytes inline at the end of 0x251a payload (after the name)
        let end_off = c.start + 9 + (c.block_size as usize).saturating_sub(2);
        let extra_start = (c.start + 11 + 4 + len).min(end_off);
        let inline_after_name: String = data[extra_start..(extra_start + 24).min(end_off)]
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join(" ");
        println!(
            "kind=0x{kind_byte:02x} name={name:?}\n  inline after name: {inline_after_name}\n  children: {}",
            child_bytes.join(" | ")
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
