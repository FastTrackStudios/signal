use dawfile_protools::raw_block::RawBlock;
fn find(blocks: &[RawBlock], ct: u16, out: &mut Vec<(usize, usize)>) {
    for b in blocks {
        if b.content_type_raw == ct {
            out.push((b.start, b.end));
        }
        find(&b.children, ct, out);
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).unwrap();
    let s = dawfile_protools::parse_raw(std::fs::read(&path)?)?;
    let cts: Vec<u16> = std::env::args()
        .skip(2)
        .map(|s| u16::from_str_radix(s.trim_start_matches("0x"), 16).unwrap())
        .collect();
    for ct in cts {
        let mut v = Vec::new();
        find(&s.blocks, ct, &mut v);
        println!("=== 0x{:04x} × {} ===", ct, v.len());
        for (i, (start, end)) in v.iter().enumerate().take(40) {
            println!("  [{}] start=0x{:x} end=0x{:x}", i, start, end);
        }
    }
    Ok(())
}
