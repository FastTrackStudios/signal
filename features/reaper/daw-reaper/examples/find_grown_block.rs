//! For two PTX files, find each pair of blocks of the same CT where
//! the size differs. Reports the position and delta.

use dawfile_protools::raw_block::RawBlock;

fn flatten(blocks: &[RawBlock], out: &mut Vec<(u16, usize, u32, usize)>) {
    fn walk(blocks: &[RawBlock], depth: usize, out: &mut Vec<(u16, usize, u32, usize)>) {
        for b in blocks {
            out.push((b.content_type_raw, b.start, b.block_size, depth));
            walk(&b.children, depth + 1, out);
        }
    }
    walk(blocks, 0, out);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a = dawfile_protools::parse_raw(std::fs::read(std::env::args().nth(1).unwrap())?)?;
    let b = dawfile_protools::parse_raw(std::fs::read(std::env::args().nth(2).unwrap())?)?;
    let ct: u16 = u16::from_str_radix(
        std::env::args()
            .nth(3)
            .expect("ct hex")
            .trim_start_matches("0x"),
        16,
    )?;

    let mut av = Vec::new();
    let mut bv = Vec::new();
    flatten(&a.blocks, &mut av);
    flatten(&b.blocks, &mut bv);

    av.retain(|x| x.0 == ct);
    bv.retain(|x| x.0 == ct);
    println!(
        "a has {} blocks of 0x{ct:04x}, b has {}",
        av.len(),
        bv.len()
    );

    if av.len() != bv.len() {
        return Ok(());
    }

    for (i, ((_, _, asz, _), (_, _, bsz, _))) in av.iter().zip(bv.iter()).enumerate() {
        if asz != bsz {
            println!(
                "  [{i}]: a size={asz}  b size={bsz}  delta={}",
                *bsz as i64 - *asz as i64
            );
        }
    }
    Ok(())
}
