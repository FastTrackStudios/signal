//! List every content type observed in a session with its count, marking
//! which ones our parser knows about.

use std::collections::BTreeMap;

fn main() {
    let path = std::env::args().nth(1).expect("path");
    let raw = std::fs::read(&path).unwrap();
    let session = dawfile_protools::parse_raw(raw).unwrap();

    let mut counts: BTreeMap<u16, (usize, bool, usize)> = BTreeMap::new();
    walk(&session.blocks, &mut counts);

    println!(" ct   count  known   total_bytes");
    for (ct, (count, known, total_bytes)) in &counts {
        let flag = if *known { "Y" } else { "." };
        println!("0x{ct:04x}  {count:>5}     {flag}     {total_bytes:>10}");
    }
}

fn walk(
    blocks: &[dawfile_protools::raw_block::RawBlock],
    counts: &mut BTreeMap<u16, (usize, bool, usize)>,
) {
    for b in blocks {
        let entry = counts.entry(b.content_type_raw).or_insert((0, false, 0));
        entry.0 += 1;
        entry.1 = b.content_type.is_some();
        entry.2 += b.block_size as usize;
        walk(&b.children, counts);
    }
}
