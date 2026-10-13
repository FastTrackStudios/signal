//! Diff the block-type counts between two PTX files.
//! Reports which CTs grew, shrank, or are new.

use dawfile_protools::raw_block::RawBlock;
use std::collections::BTreeMap;

fn count(blocks: &[RawBlock], out: &mut BTreeMap<u16, usize>) {
    for b in blocks {
        *out.entry(b.content_type_raw).or_default() += 1;
        count(&b.children, out);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a_path = std::env::args().nth(1).expect("a.ptx");
    let b_path = std::env::args().nth(2).expect("b.ptx");
    let a = dawfile_protools::parse_raw(std::fs::read(&a_path)?)?;
    let b = dawfile_protools::parse_raw(std::fs::read(&b_path)?)?;

    let mut ac = BTreeMap::new();
    let mut bc = BTreeMap::new();
    count(&a.blocks, &mut ac);
    count(&b.blocks, &mut bc);

    let all_cts: std::collections::BTreeSet<u16> = ac.keys().chain(bc.keys()).copied().collect();
    println!(" CT      a    b   delta");
    for ct in all_cts {
        let an = *ac.get(&ct).unwrap_or(&0);
        let bn = *bc.get(&ct).unwrap_or(&0);
        let d = bn as i64 - an as i64;
        if d != 0 {
            println!("0x{ct:04x}  {an:>3}  {bn:>3}   {d:+}");
        }
    }
    Ok(())
}
