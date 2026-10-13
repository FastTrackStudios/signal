// Map decrypted PTX offsets back to (CT, offset-within-block).
use dawfile_protools::raw_block::RawBlock;

fn find(blocks: &[RawBlock], target: usize, out: &mut Vec<(u16, usize, usize, usize)>) {
    for b in blocks {
        if target >= b.start && target < b.end {
            out.push((b.content_type_raw, b.start, b.end, target - b.start));
            // Also walk children so we get the innermost block
            find(&b.children, target, out);
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).unwrap();
    let s = dawfile_protools::parse_raw(std::fs::read(&path)?)?;
    for arg in std::env::args().skip(2) {
        let target: usize = arg.parse()?;
        print!("offset {} (0x{:x}): ", target, target);
        let mut path = Vec::new();
        find(&s.blocks, target, &mut path);
        for (ct, start, _end, rel) in path {
            print!("[0x{:04x}@0x{:x}+{}] ", ct, start, rel);
        }
        println!();
    }
    Ok(())
}
