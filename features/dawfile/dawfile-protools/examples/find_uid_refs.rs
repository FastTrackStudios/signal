//! Search the decrypted PTX bytes for occurrences of specific track UIDs.
//! Goal: find a block that LISTS the UIDs of explicitly-muted tracks,
//! which would be the mute-marker block we're hunting.

use dawfile_protools::raw_block::RawBlock;
use std::collections::HashMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).expect("ptx");
    let raw = std::fs::read(&path)?;
    let session = dawfile_protools::parse_raw(raw)?;
    let data = session.cursor().data();

    // LotF UIDs for the 2 explicitly-muted tracks (from dump_251a):
    //   ClickPrint:              b0 d7 c4 fc
    //   02 LORD OF THE FIGHT.01: 03 93 cb 7e
    let click_uid = [0xb0u8, 0xd7, 0xc4, 0xfc];
    let lord_uid = [0x03u8, 0x93, 0xcb, 0x7e];
    // Also an NOT-muted track for control:
    //   Master 1: 54 a3 db e9
    let master_uid = [0x54u8, 0xa3, 0xdb, 0xe9];
    //   SYZ: 25 34 77 e8
    let syz_uid = [0x25u8, 0x34, 0x77, 0xe8];

    let queries = [
        ("ClickPrint", &click_uid[..]),
        ("02_LORD_OF_THE_FIGHT.01", &lord_uid[..]),
        ("Master 1 (NOT muted)", &master_uid[..]),
        ("SYZ (NOT muted)", &syz_uid[..]),
    ];

    // Build offset → containing block map.
    let mut all_blocks: Vec<&RawBlock> = Vec::new();
    fn collect_all<'a>(blocks: &'a [RawBlock], out: &mut Vec<&'a RawBlock>) {
        for b in blocks {
            out.push(b);
            collect_all(&b.children, out);
        }
    }
    collect_all(&session.blocks, &mut all_blocks);

    fn smallest_containing<'a>(off: usize, blocks: &[&'a RawBlock]) -> Option<&'a RawBlock> {
        let mut best: Option<&RawBlock> = None;
        let mut best_size = usize::MAX;
        for b in blocks {
            if off >= b.start + 9 && off < b.start + 9 + (b.block_size as usize).saturating_sub(2) {
                let sz = b.block_size as usize;
                if sz < best_size {
                    best_size = sz;
                    best = Some(b);
                }
            }
        }
        best
    }

    for (label, uid) in &queries {
        let mut positions: Vec<usize> = Vec::new();
        let mut i = 0;
        while i + uid.len() <= data.len() {
            if &data[i..i + uid.len()] == *uid {
                positions.push(i);
            }
            i += 1;
        }
        println!("\n=== {label} ({}× hits) ===", positions.len());
        // Group by smallest containing block CT
        let mut by_ct: HashMap<u16, usize> = HashMap::new();
        for &pos in &positions {
            if let Some(b) = smallest_containing(pos, &all_blocks) {
                *by_ct.entry(b.content_type_raw).or_insert(0) += 1;
            }
        }
        let mut sorted: Vec<(u16, usize)> = by_ct.into_iter().collect();
        sorted.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        for (ct, n) in &sorted {
            println!("  in 0x{ct:04x}: {n}×");
        }
    }
    Ok(())
}
