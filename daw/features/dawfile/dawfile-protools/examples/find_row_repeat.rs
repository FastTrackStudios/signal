//! Find bytes inside 0x200b where the value follows ROW position (1..23)
//! regardless of folder (x1/x2/x3). Confirms "hue byte" hypothesis.

use dawfile_protools::raw_block::RawBlock;

fn main() {
    let path = std::env::args().nth(1).expect("path");
    let raw = std::fs::read(&path).unwrap();
    let session = dawfile_protools::parse_raw(raw).unwrap();
    let data = session.cursor().data();

    // 0x261c blocks are in track order (folders excluded — 69 entries).
    // Track ordering in the doc: 1x1..23x1, 1x2..23x2, 1x3..23x3.
    let containers = collect(&session.blocks, 0x261c);
    if containers.len() != 69 {
        println!("expected 69 containers, got {}", containers.len());
        return;
    }

    // Build (folder_idx, row_idx) per container
    let mut tracks: Vec<(usize, usize)> = Vec::new(); // (row 0..22, folder 0..2)
    for i in 0..69 {
        let folder = i / 23;
        let row = i % 23;
        tracks.push((row, folder));
    }

    // Find 0x200b inside each 0x261c
    let b200bs: Vec<&RawBlock> = containers
        .iter()
        .filter_map(|c| c.children.iter().find(|x| x.content_type_raw == 0x200b))
        .collect();
    if b200bs.len() != 69 {
        println!("missing 0x200b children: {}", b200bs.len());
        return;
    }
    let min_len = b200bs
        .iter()
        .map(|b| b.block_size.saturating_sub(2) as usize)
        .min()
        .unwrap_or(0);

    println!(
        "checking {} 0x200b payloads, min len {}",
        b200bs.len(),
        min_len
    );

    // For each byte position, check:
    //   "row-consistent": value depends only on row (1x1==1x2==1x3, etc.)
    //   "folder-consistent": value depends only on folder
    //   "unique-per-cell": all 69 distinct
    println!("\nposition | category | cardinality | example");
    for i in 0..min_len {
        let vals: Vec<u8> = b200bs.iter().map(|b| data[b.start + 9 + i]).collect();

        // Row-consistent: rows in different folders should have same value
        let mut row_consistent = true;
        for row in 0..23 {
            let v0 = vals[row]; // folder 0
            let v1 = vals[row + 23]; // folder 1
            let v2 = vals[row + 46]; // folder 2
            if v0 != v1 || v0 != v2 {
                row_consistent = false;
                break;
            }
        }

        // Folder-consistent: all tracks in same folder should have same value
        let mut folder_consistent = true;
        for folder in 0..3 {
            let v0 = vals[folder * 23];
            for k in 1..23 {
                if vals[folder * 23 + k] != v0 {
                    folder_consistent = false;
                    break;
                }
            }
            if !folder_consistent {
                break;
            }
        }

        let distinct: std::collections::BTreeSet<u8> = vals.iter().copied().collect();
        if distinct.len() < 2 {
            continue;
        }

        let mut tag = String::new();
        if row_consistent {
            tag.push_str("ROW-CONSISTENT (hue?) ");
        }
        if folder_consistent {
            tag.push_str("FOLDER-CONSISTENT (shade?) ");
        }
        if distinct.len() == 69 {
            tag.push_str("UNIQUE-PER-CELL ");
        }
        if tag.is_empty() {
            continue;
        }

        let mut example = String::new();
        for k in [0, 1, 2, 22, 23, 24, 45, 46, 47, 68] {
            example.push_str(&format!("{:02x} ", vals[k]));
        }
        println!(
            "+{i:>4}  {tag:<60}  card={:3}  vals[0,1,2,22,23,24,45,46,47,68]: {example}",
            distinct.len()
        );
    }
}

fn collect(blocks: &[RawBlock], ct: u16) -> Vec<&RawBlock> {
    let mut out = Vec::new();
    fn rec<'a>(blocks: &'a [RawBlock], ct: u16, out: &mut Vec<&'a RawBlock>) {
        for b in blocks {
            if b.content_type_raw == ct {
                out.push(b);
            }
            rec(&b.children, ct, out);
        }
    }
    rec(blocks, ct, &mut out);
    out
}
