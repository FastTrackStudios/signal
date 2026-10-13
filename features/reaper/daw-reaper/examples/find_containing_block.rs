use dawfile_protools::raw_block::RawBlock;
fn find_containing(
    blocks: &[RawBlock],
    offset: usize,
    path: &mut Vec<(u16, usize, usize)>,
) -> bool {
    for b in blocks {
        if offset >= b.start && offset < b.end {
            path.push((b.content_type_raw, b.start, b.end));
            find_containing(&b.children, offset, path);
            return true;
        }
    }
    false
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let s = dawfile_protools::parse_raw(std::fs::read(std::env::args().nth(1).unwrap())?)?;
    for arg in std::env::args().skip(2) {
        let off = usize::from_str_radix(arg.trim_start_matches("0x"), 16)?;
        let mut path = Vec::new();
        find_containing(&s.blocks, off, &mut path);
        println!("0x{off:06x}: {:?}", path);
    }
    Ok(())
}
