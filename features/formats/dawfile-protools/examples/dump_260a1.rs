use dawfile_protools::raw_block::RawBlock;
fn collect_260a<'a>(blocks: &'a [RawBlock], out: &mut Vec<&'a RawBlock>) {
    for b in blocks {
        if b.content_type_raw == 0x260a {
            out.push(b);
        }
        collect_260a(&b.children, out);
    }
}
fn main() {
    let path = std::env::args().nth(1).expect("path");
    let s = dawfile_protools::parse_raw(std::fs::read(&path).unwrap()).unwrap();
    let data = s.cursor().data();
    let mut bs = Vec::new();
    collect_260a(&s.blocks, &mut bs);
    let b = bs[1]; // 0x260a[1] = master-send mute
    let p = b.start + 9;
    let len = (b.block_size as usize).saturating_sub(2);
    println!("0x260a[1] @ 0x{:x} size {} payload:", b.start, b.block_size);
    let hex: Vec<String> = (p..p + len).map(|i| format!("{:02x}", data[i])).collect();
    for chunk in hex.chunks(16) {
        println!("  {}", chunk.join(" "));
    }
}
