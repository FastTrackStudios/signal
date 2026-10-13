//! For each 0x200b block, walk its ancestors looking for a 0x2619 name child.
//! Reports the implied (track_name, color_byte) mapping.

use dawfile_protools::content_type::ContentType;
use dawfile_protools::raw_block::RawBlock;

fn main() {
    let path = std::env::args().nth(1).expect("path");
    let raw = std::fs::read(&path).unwrap();
    let session = dawfile_protools::parse_raw(raw).unwrap();
    let data = session.cursor().data();

    // Build parent-pointer map by walking the tree
    fn walk<'a>(
        blocks: &'a [RawBlock],
        parent: Option<&'a RawBlock>,
        parents: &mut std::collections::HashMap<usize, Option<&'a RawBlock>>,
        out200b: &mut Vec<&'a RawBlock>,
    ) {
        for b in blocks {
            parents.insert(b.start, parent);
            if b.content_type == Some(ContentType::TrackAuxState) {
                out200b.push(b);
            }
            walk(&b.children, Some(b), parents, out200b);
        }
    }
    let mut parents: std::collections::HashMap<usize, Option<&RawBlock>> =
        std::collections::HashMap::new();
    let mut blocks_200b = Vec::new();
    walk(&session.blocks, None, &mut parents, &mut blocks_200b);

    println!("found {} 0x200b blocks", blocks_200b.len());

    fn find_2619_name(b: &RawBlock, data: &[u8]) -> Option<String> {
        for c in &b.children {
            if c.content_type_raw == 0x2619 {
                let p = c.start + 9;
                if p + 4 <= data.len() {
                    let len = u32::from_le_bytes(data[p..p + 4].try_into().unwrap()) as usize;
                    if len > 0 && len < 64 && p + 4 + len <= data.len() {
                        return Some(
                            String::from_utf8_lossy(&data[p + 4..p + 4 + len]).into_owned(),
                        );
                    }
                }
            }
            if let Some(n) = find_2619_name(c, data) {
                return Some(n);
            }
        }
        None
    }

    for (i, b) in blocks_200b.iter().enumerate().take(75) {
        let p163 = b.start + 9 + 163;
        let color = if p163 < data.len() { data[p163] } else { 0 };
        // Verified palette field at +106..+107 (i16 LE).
        let p106 = b.start + 9 + 106;
        let c106 = if p106 + 2 <= data.len() {
            i16::from_le_bytes([data[p106], data[p106 + 1]])
        } else {
            0
        };

        // Walk ancestors looking for first 0x2619 name in *any* descendant
        let mut name: Option<String> = None;
        let mut p = parents.get(&b.start).cloned().flatten();
        let mut depth = 0;
        while let Some(ancestor) = p {
            if let Some(n) = find_2619_name(ancestor, data) {
                name = Some(n);
                break;
            }
            p = parents.get(&ancestor.start).cloned().flatten();
            depth += 1;
            if depth > 10 {
                break;
            }
        }

        println!(
            "[{:02}] 0x200b @ 0x{:x} +163=0x{:02x} +106={:>4} ancestor={:?}",
            i, b.start, color, c106, name
        );
    }
}
