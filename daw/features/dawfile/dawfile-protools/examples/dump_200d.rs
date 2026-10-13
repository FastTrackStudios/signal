use dawfile_protools::raw_block::RawBlock;
fn main() {
    let path = std::env::args().nth(1).expect("path");
    let raw = std::fs::read(&path).unwrap();
    let session = dawfile_protools::parse_raw(raw).unwrap();
    let data = session.cursor().data();
    fn walk<'a>(blocks: &'a [RawBlock], ct: u16, out: &mut Vec<&'a RawBlock>) {
        for b in blocks {
            if b.content_type_raw == ct {
                out.push(b);
            }
            walk(&b.children, ct, out);
        }
    }
    let mut blks = Vec::new();
    walk(&session.blocks, 0x200d, &mut blks);
    println!("Found {} 0x200d blocks", blks.len());
    for (i, b) in blks.iter().enumerate() {
        let p = b.start + 9;
        let len = b.block_size.saturating_sub(2) as usize;
        let end = (p + len).min(data.len());
        let hex: Vec<String> = (p..end).map(|i| format!("{:02x}", data[i])).collect();
        println!("[{i}] @ 0x{:x} sz={} payload:", b.start, b.block_size);
        // chunks of 16
        for chunk in hex.chunks(16) {
            println!("    {}", chunk.join(" "));
        }
        // children
        for c in &b.children {
            println!("  child 0x{:04x} sz={}", c.content_type_raw, c.block_size);
        }
    }
}
