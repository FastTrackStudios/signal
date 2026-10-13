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
    let path = std::env::args().nth(1).unwrap();
    let s = dawfile_protools::parse_raw(std::fs::read(&path)?)?;
    let mut regions = Vec::new();
    collect(&s.blocks, 0x2629, &mut regions);
    collect(&s.blocks, 0x1008, &mut regions);
    println!("found {} regions", regions.len());
    for (i, (start, end)) in regions.iter().take(5).enumerate() {
        let bytes = &s.data[*start..*end];
        println!("--- region {i} @ 0x{start:06x} ({} bytes) ---", end - start);
        for (j, chunk) in bytes.chunks(16).enumerate() {
            print!("  +{:>3}: ", j * 16);
            for b in chunk {
                print!("{:02x} ", b);
            }
            print!("  ");
            for b in chunk {
                print!(
                    "{}",
                    if b.is_ascii_graphic() {
                        *b as char
                    } else {
                        '.'
                    }
                );
            }
            println!();
        }
    }
    Ok(())
}
