use dawfile_protools::raw_block::RawBlock;
fn main() {
    let path = std::env::args().nth(1).expect("path");
    let raw = std::fs::read(&path).unwrap();
    let session = dawfile_protools::parse_raw(raw).unwrap();
    let data = session.cursor().data();
    fn walk(blocks: &[RawBlock], depth: u32, data: &[u8], in_2624: bool) {
        let pad = " ".repeat(depth as usize * 2);
        for b in blocks {
            let mut info = format!("{pad}0x{:04x} sz={}", b.content_type_raw, b.block_size);
            // For 0x2619, also show name
            if b.content_type_raw == 0x2619 {
                let p = b.start + 9;
                if p + 4 <= data.len() {
                    let len = u32::from_le_bytes(data[p..p + 4].try_into().unwrap()) as usize;
                    if len > 0 && len < 64 && p + 4 + len <= data.len() {
                        let name = String::from_utf8_lossy(&data[p + 4..p + 4 + len]);
                        info.push_str(&format!(" name={name:?}"));
                    }
                }
            }
            if b.content_type_raw == 0x200d {
                info.push_str(" ★FOLDER★");
            }
            println!("{info}");
            // Only descend into 0x2624 and stay within
            let entering = b.content_type_raw == 0x2624 || in_2624;
            if entering {
                walk(&b.children, depth + 1, data, true);
            }
        }
    }
    walk(&session.blocks, 0, data, false);
}
