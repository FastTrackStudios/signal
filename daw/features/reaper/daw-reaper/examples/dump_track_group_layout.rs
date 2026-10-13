//! Dump the per-track block group layout: 0x261b wrapper plus the
//! 0x200a/0x200b/0x2015 trailing color blocks, in file order.

use dawfile_protools::raw_block::RawBlock;

fn flatten(blocks: &[RawBlock], out: &mut Vec<(u16, usize, usize, u32)>) {
    for b in blocks {
        out.push((b.content_type_raw, b.start, b.end, b.block_size));
        flatten(&b.children, out);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let s = dawfile_protools::parse_raw(std::fs::read(std::env::args().nth(1).unwrap())?)?;
    let mut v = Vec::new();
    flatten(&s.blocks, &mut v);
    v.retain(|x| matches!(x.0, 0x261b | 0x261c | 0x200a | 0x200b | 0x2015 | 0x2624));
    v.sort_by_key(|x| x.1);
    println!(" CT      start     end        size       len");
    for (ct, s, e, sz) in v {
        println!("0x{ct:04x}  0x{s:06x}  0x{e:06x}  {sz:>6}  {:>6}", e - s);
    }
    Ok(())
}
