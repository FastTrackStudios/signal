use dawfile_protools::raw_block::RawBlock;
fn collect(blocks: &[RawBlock], ct: u16, out: &mut Vec<(usize, usize)>) {
    for b in blocks {
        if b.content_type_raw == ct {
            out.push((b.start, b.end));
        }
        collect(&b.children, ct, out);
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let s = dawfile_protools::parse_raw(std::fs::read(std::env::args().nth(1).unwrap())?)?;
    let mut v = Vec::new();
    collect(&s.blocks, 0x260c, &mut v);
    println!("0x260c × {}", v.len());
    for (i, (start, end)) in v.iter().enumerate().take(4) {
        let sz = end - start;
        if !(40..=60).contains(&sz) {
            continue;
        }
        println!("--- [{i}] @ 0x{start:06x} ({sz} bytes) ---");
        for (j, chunk) in s.data[*start..*end].chunks(16).enumerate() {
            print!("  +{:>3}: ", j * 16);
            for b in chunk {
                print!("{:02x} ", b);
            }
            println!();
        }
    }
    Ok(())
}
