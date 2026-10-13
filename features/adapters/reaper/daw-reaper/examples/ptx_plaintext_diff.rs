//! Diff two .ptx files at the DECRYPTED block-payload level. For each block
//! pair, walks payload bytes and reports differences. Filters out blocks
//! that only differ in content_type-XX or whole encrypted blobs that aren't
//! decoded.
//!
//! Usage: ptx_plaintext_diff <baseline.ptx> <other.ptx>

use dawfile_protools::raw_block::RawBlock;
use std::collections::BTreeMap;

fn walk<'a>(blocks: &'a [RawBlock], out: &mut Vec<&'a RawBlock>) {
    for b in blocks {
        out.push(b);
        walk(&b.children, out);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a_path = std::env::args().nth(1).expect("a.ptx");
    let b_path = std::env::args().nth(2).expect("b.ptx");

    let a_raw = std::fs::read(&a_path)?;
    let b_raw = std::fs::read(&b_path)?;
    let a = dawfile_protools::parse_raw(a_raw)?;
    let b = dawfile_protools::parse_raw(b_raw)?;
    let a_data = a.cursor().data().to_vec();
    let b_data = b.cursor().data().to_vec();

    let mut a_blocks = Vec::new();
    let mut b_blocks = Vec::new();
    walk(&a.blocks, &mut a_blocks);
    walk(&b.blocks, &mut b_blocks);

    // Group by content_type → list in document order
    let mut a_by_ct: BTreeMap<u16, Vec<&RawBlock>> = BTreeMap::new();
    for blk in &a_blocks {
        a_by_ct.entry(blk.content_type_raw).or_default().push(blk);
    }
    let mut b_by_ct: BTreeMap<u16, Vec<&RawBlock>> = BTreeMap::new();
    for blk in &b_blocks {
        b_by_ct.entry(blk.content_type_raw).or_default().push(blk);
    }

    let all_cts: std::collections::BTreeSet<u16> =
        a_by_ct.keys().chain(b_by_ct.keys()).copied().collect();

    for ct in all_cts {
        let av = a_by_ct.get(&ct).cloned().unwrap_or_default();
        let bv = b_by_ct.get(&ct).cloned().unwrap_or_default();
        if av.len() != bv.len() {
            println!("0x{ct:04x}: COUNT DIFFERS  a={} b={}", av.len(), bv.len());
            continue;
        }
        for (i, (ab, bb)) in av.iter().zip(bv.iter()).enumerate() {
            let a_start = ab.start + 9;
            let a_end = a_start + (ab.block_size as usize).saturating_sub(2);
            let b_start = bb.start + 9;
            let b_end = b_start + (bb.block_size as usize).saturating_sub(2);
            let a_len = a_end - a_start;
            let b_len = b_end - b_start;
            if a_len != b_len {
                println!("0x{ct:04x}[{i}]: SIZE DIFFERS  a={} b={}", a_len, b_len);
                continue;
            }
            // Byte diff
            let mut diffs: Vec<(usize, u8, u8)> = Vec::new();
            for j in 0..a_len {
                let av = a_data.get(a_start + j).copied().unwrap_or(0);
                let bv = b_data.get(b_start + j).copied().unwrap_or(0);
                if av != bv {
                    diffs.push((j, av, bv));
                }
            }
            if diffs.is_empty() {
                continue;
            }
            // Skip noisy blocks that change in every probe (likely UIDs/
            // timestamps). Heuristic: > 30% of bytes differ.
            if diffs.len() * 3 > a_len {
                println!(
                    "0x{ct:04x}[{i}] (size {a_len}): NOISY ({} diffs >30% — likely UID/timestamp)",
                    diffs.len()
                );
                continue;
            }
            println!("0x{ct:04x}[{i}] (size {a_len}): {} diff bytes", diffs.len());
            for (j, av, bv) in &diffs {
                println!("    +{j:>4}: 0x{av:02x} → 0x{bv:02x}");
            }
        }
    }

    Ok(())
}
