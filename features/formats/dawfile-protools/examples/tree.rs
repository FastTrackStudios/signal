//! Print the block tree to a configurable depth.

use dawfile_protools::raw_block::RawBlock;

fn main() {
    let path = std::env::args().nth(1).expect("path");
    let depth: usize = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(3);
    let raw = std::fs::read(&path).unwrap();
    let session = dawfile_protools::parse_raw(raw).unwrap();
    walk(&session.blocks, 0, depth);
}

fn walk(blocks: &[RawBlock], cur: usize, max: usize) {
    for b in blocks {
        let pad = "  ".repeat(cur);
        println!(
            "{pad}0x{:04x}  @0x{:06x}  size={}  kids={}",
            b.content_type_raw,
            b.start,
            b.block_size,
            b.children.len()
        );
        if cur + 1 < max {
            walk(&b.children, cur + 1, max);
        }
    }
}
