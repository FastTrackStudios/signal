//! For two PTX files, report all block-type counts AND sum of total
//! payload sizes per type. Find which block grew between probes.

use dawfile_protools::raw_block::RawBlock;
use std::collections::BTreeMap;

fn walk(blocks: &[RawBlock], counts: &mut BTreeMap<u16, (usize, usize)>) {
    for b in blocks {
        let e = counts.entry(b.content_type_raw).or_insert((0, 0));
        e.0 += 1;
        e.1 += b.block_size as usize;
        walk(&b.children, counts);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a = dawfile_protools::parse_raw(std::fs::read(std::env::args().nth(1).unwrap())?)?;
    let b = dawfile_protools::parse_raw(std::fs::read(std::env::args().nth(2).unwrap())?)?;

    let mut ac = BTreeMap::new();
    let mut bc = BTreeMap::new();
    walk(&a.blocks, &mut ac);
    walk(&b.blocks, &mut bc);

    let all_cts: std::collections::BTreeSet<u16> = ac.keys().chain(bc.keys()).copied().collect();
    println!("CT          a_count a_size  b_count b_size  Δsize");
    for ct in all_cts {
        let (an, asz) = ac.get(&ct).copied().unwrap_or((0, 0));
        let (bn, bsz) = bc.get(&ct).copied().unwrap_or((0, 0));
        let delta = bsz as i64 - asz as i64;
        if delta != 0 || an != bn {
            println!("0x{ct:04x}      {an:>4}    {asz:>6}    {bn:>4}    {bsz:>6}    {delta:>+5}");
        }
    }
    Ok(())
}
