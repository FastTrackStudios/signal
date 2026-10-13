use dawfile_protools::raw_block::RawBlock;
fn flatten(blocks: &[RawBlock], depth: usize, out: &mut Vec<(u16, usize, usize, usize)>) {
    for b in blocks {
        out.push((b.content_type_raw, b.start, b.end, depth));
        flatten(&b.children, depth + 1, out);
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let s = dawfile_protools::parse_raw(std::fs::read(std::env::args().nth(1).unwrap())?)?;
    let mut v = Vec::new();
    flatten(&s.blocks, 0, &mut v);
    v.retain(|x| {
        matches!(
            x.0,
            0x1014 | 0x1015 | 0x1052 | 0x1054 | 0x2519 | 0x251a | 0x2107 | 0x210b
        )
    });
    v.sort_by_key(|x| x.1);
    for (ct, s, e, d) in v {
        println!("{:>2}: 0x{ct:04x} [0x{s:06x}..0x{e:06x}] ({} B)", d, e - s);
    }
    Ok(())
}
