use dawfile_protools::{block, content_type::ContentType, cursor::Cursor, decrypt};
use std::env;

fn main() {
    let path = env::args().nth(1).expect("usage: dump <file>");
    let mut data = std::fs::read(&path).expect("read");
    let _ = decrypt::decrypt(&mut data).expect("decrypt");

    let is_be = data.get(0x11).copied().unwrap_or(0) != 0;
    let cursor = Cursor::new(&data, is_be);
    let blocks = block::parse_blocks(&data, is_be);

    println!("is_bigendian: {is_be}");
    println!("total top-level blocks: {}", blocks.len());

    // Find AudioTrackList
    fn find_recursive(blocks: &[block::Block], ct: ContentType) -> Option<&block::Block> {
        for b in blocks {
            if b.content_type == Some(ct) {
                return Some(b);
            }
            if let Some(f) = find_recursive(&b.children, ct) {
                return Some(f);
            }
        }
        None
    }

    if let Some(tl) = find_recursive(&blocks, ContentType::AudioTrackList) {
        println!(
            "\nAudioTrackList at offset 0x{:x}, {} children",
            tl.offset,
            tl.children.len()
        );
        let info_children = tl.find_children(ContentType::AudioTrackInfo);
        println!("  AudioTrackInfo children: {}", info_children.len());

        for (i, child) in info_children.iter().enumerate().take(5) {
            let name_offset = child.offset + 2;
            let hex: Vec<String> = data[name_offset..std::cmp::min(name_offset + 32, data.len())]
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            println!(
                "  [{}] child.offset=0x{:x} payload[0..32]: {}",
                i,
                child.offset,
                hex.join(" ")
            );
            let (name, _) = cursor.length_prefixed_string(name_offset);
            println!("       name: {:?}", name);
        }
    } else {
        println!("No AudioTrackList found");
        // Print all top-level content types
        for b in &blocks {
            println!(
                "  block ct={:?} children={}",
                b.content_type,
                b.children.len()
            );
            for c in &b.children {
                println!(
                    "    child ct={:?} children={}",
                    c.content_type,
                    c.children.len()
                );
            }
        }
    }
}
