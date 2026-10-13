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
    let s = dawfile_protools::parse_raw(std::fs::read(std::env::args().nth(1).unwrap())?)?;
    // Find each 0x260d wrapper, then its 0x260a[1] child.
    let mut wraps = Vec::new();
    find(&s.blocks, 0x260d, &mut wraps);
    for (i, (start, end)) in wraps.iter().enumerate().take(2) {
        // Find first wrapper, then 2nd 0x260a child.
        // We need access to children — re-walk blocks tree to find this wrapper as a RawBlock ref.
        // Use parse_raw_blocks_pub on the data directly.
        let blocks = dawfile_protools::raw_block::parse_raw_blocks_pub(&s.data, s.is_bigendian);
        fn find_wrapper(blocks: &[RawBlock], start: usize) -> Option<&RawBlock> {
            for b in blocks {
                if b.start == start {
                    return Some(b);
                }
                if let Some(f) = find_wrapper(&b.children, start) {
                    return Some(f);
                }
            }
            None
        }
        if let Some(w) = find_wrapper(&blocks, *start) {
            let envs: Vec<&RawBlock> = w
                .children
                .iter()
                .filter(|c| c.content_type_raw == 0x260a)
                .collect();
            println!(
                "wrapper [{i}] @ 0x{start:06x}..0x{end:06x} has {} 0x260a children",
                envs.len()
            );
            for (k, env) in envs.iter().enumerate() {
                println!(
                    "  --- [{}] @ 0x{:06x}..0x{:06x} ({} bytes) ---",
                    k,
                    env.start,
                    env.end,
                    env.end - env.start
                );
                let payload_start = env.start + 7;
                let end = env.end.min(s.data.len());
                for (j, chunk) in s.data[payload_start..end].chunks(16).enumerate() {
                    print!("    +{:>3}: ", j * 16);
                    for b in chunk {
                        print!("{:02x} ", b);
                    }
                    println!();
                }
            }
        }
    }
    Ok(())
}
