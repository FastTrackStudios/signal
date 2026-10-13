use dawfile_protools::raw_block::RawBlock;
fn main() {
    let path = std::env::args().nth(1).expect("path");
    let raw = std::fs::read(&path).unwrap();
    let session = dawfile_protools::parse_raw(raw).unwrap();
    let data = session.cursor().data();
    fn walk<'a>(blocks: &'a [RawBlock], out: &mut Vec<&'a RawBlock>) {
        for b in blocks {
            // 0x102d is per-track state (solo flag at +162, etc.) — not
            // currently surfaced as a named ContentType variant.
            if b.content_type_raw == 0x102d {
                out.push(b);
            }
            walk(&b.children, out);
        }
    }
    let mut ts = Vec::new();
    walk(&session.blocks, &mut ts);
    for (i, b) in ts.iter().enumerate() {
        let p162 = b.start + 9 + 162;
        let v = if p162 < data.len() { data[p162] } else { 0 };
        println!(
            "[{i}] 0x102d @ 0x{:x} sz={} +162=0x{v:02x}",
            b.start, b.block_size
        );
        for c in &b.children {
            println!("  child 0x{:04x} sz={}", c.content_type_raw, c.block_size);
            if c.content_type_raw == 0x2619 {
                let p = c.start + 9;
                if p + 4 <= data.len() {
                    let len = u32::from_le_bytes(data[p..p + 4].try_into().unwrap()) as usize;
                    if p + 4 + len <= data.len() {
                        let name = String::from_utf8_lossy(&data[p + 4..p + 4 + len]);
                        println!("    name={name:?}");
                    }
                }
            }
        }
    }
}
