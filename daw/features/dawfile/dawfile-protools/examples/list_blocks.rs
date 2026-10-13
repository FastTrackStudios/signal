use dawfile_protools::{block, decrypt};
use std::{collections::BTreeMap, env};

fn count_all(blocks: &[block::Block], counts: &mut BTreeMap<u16, usize>) {
    for b in blocks {
        *counts.entry(b.content_type_raw).or_insert(0) += 1;
        count_all(&b.children, counts);
    }
}

fn main() {
    let path = env::args().nth(1).expect("usage: list_blocks <file>");
    let mut data = std::fs::read(&path).expect("read");
    let _ = decrypt::decrypt(&mut data).expect("decrypt");
    let is_be = data.get(0x11).copied().unwrap_or(0) != 0;
    let blocks = block::parse_blocks(&data, is_be);

    let mut counts = BTreeMap::new();
    count_all(&blocks, &mut counts);

    println!("Block content types found in {}:", path);
    for (ct, n) in &counts {
        println!("  0x{ct:04x}  × {n}");
    }
}
