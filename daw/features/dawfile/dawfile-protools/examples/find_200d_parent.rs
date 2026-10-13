#![allow(clippy::all, dead_code)]
use dawfile_protools::raw_block::RawBlock;
fn main() {
    let path = std::env::args().nth(1).expect("path");
    let raw = std::fs::read(&path).unwrap();
    let session = dawfile_protools::parse_raw(raw).unwrap();
    fn walk(blocks: &[RawBlock], depth: u32, path_str: &mut Vec<String>) {
        for b in blocks {
            path_str.push(format!("0x{:04x}", b.content_type_raw));
            if b.content_type_raw == 0x200d {
                println!("Path: {}", path_str.join(" → "));
            }
            walk(&b.children, depth + 1, path_str);
            path_str.pop();
        }
    }
    let mut path = Vec::new();
    walk(&session.blocks, 0, &mut path);
}
