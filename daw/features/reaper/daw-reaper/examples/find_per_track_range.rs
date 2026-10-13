//! Locate the byte ranges of each per-track block group.
//!
//! Strategy: find every 0x261b block (top-level per-track wrapper) and
//! report its start..end range. Also find every 0x260d (track wrapper)
//! and 0x1029 (mix settings) for cross-reference.

use dawfile_protools::raw_block::RawBlock;

fn find_ct(blocks: &[RawBlock], ct: u16, out: &mut Vec<(usize, usize, u32)>) {
    for b in blocks {
        if b.content_type_raw == ct {
            out.push((b.start, b.end, b.block_size));
        }
        find_ct(&b.children, ct, out);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).expect("ptx");
    let s = dawfile_protools::parse_raw(std::fs::read(&path)?)?;
    let total = s.data.len();
    println!("file size: {} bytes", total);
    for ct in [0x261bu16, 0x260d, 0x1029, 0x102d, 0x2619, 0x200a] {
        let mut v = Vec::new();
        find_ct(&s.blocks, ct, &mut v);
        println!("0x{ct:04x}: {} blocks", v.len());
        for (i, (start, end, sz)) in v.iter().enumerate() {
            println!(
                "  [{i}] start=0x{start:06x} end=0x{end:06x} size={sz} ({} bytes payload)",
                end - start
            );
        }
    }
    Ok(())
}
