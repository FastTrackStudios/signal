//! Inspect each content_type that appears exactly twice in a PTX file.
//! For each, walk to find associated track name (via ancestor 0x2619)
//! and dump the first 64 bytes of payload.
//!
//! Goal: find which block type marks the 2 explicitly-muted tracks
//! in LotF (ClickPrint + "02 LORD OF THE FIGHT.01" per the Frida trace).

use dawfile_protools::raw_block::RawBlock;
use std::collections::HashMap;

fn build_parents<'a>(
    blocks: &'a [RawBlock],
    parent: Option<&'a RawBlock>,
    out: &mut HashMap<usize, Option<&'a RawBlock>>,
) {
    for b in blocks {
        out.insert(b.start, parent);
        build_parents(&b.children, Some(b), out);
    }
}

fn find_nearest_2619_name(
    parents: &HashMap<usize, Option<&RawBlock>>,
    start: &RawBlock,
    data: &[u8],
) -> Option<String> {
    // Walk ancestor chain looking for a 0x2619 child anywhere in the
    // ancestor's subtree.
    fn collect_2619_in_subtree(b: &RawBlock, data: &[u8]) -> Option<String> {
        for c in &b.children {
            if c.content_type_raw == 0x2619 {
                let p = c.start + 9;
                if p + 4 > data.len() {
                    continue;
                }
                let len = u32::from_le_bytes(data[p..p + 4].try_into().ok()?) as usize;
                if len == 0 || len > 64 || p + 4 + len > data.len() {
                    continue;
                }
                return Some(
                    String::from_utf8_lossy(&data[p + 4..p + 4 + len])
                        .trim_end_matches('\0')
                        .to_string(),
                );
            }
            if let Some(n) = collect_2619_in_subtree(c, data) {
                return Some(n);
            }
        }
        None
    }
    let mut anc = parents.get(&start.start).copied().flatten();
    let mut depth = 0;
    while let Some(a) = anc {
        if let Some(n) = collect_2619_in_subtree(a, data) {
            return Some(n);
        }
        anc = parents.get(&a.start).copied().flatten();
        depth += 1;
        if depth > 12 {
            break;
        }
    }
    None
}

fn collect<'a>(blocks: &'a [RawBlock], ct: u16, out: &mut Vec<&'a RawBlock>) {
    for b in blocks {
        if b.content_type_raw == ct {
            out.push(b);
        }
        collect(&b.children, ct, out);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).expect("ptx path");
    let raw = std::fs::read(&path)?;
    let session = dawfile_protools::parse_raw(raw)?;
    let data = session.cursor().data();

    let mut parents: HashMap<usize, Option<&RawBlock>> = HashMap::new();
    build_parents(&session.blocks, None, &mut parents);

    // Two-count CTs from LotF that could plausibly mark explicit mute:
    let targets: [u16; 10] = [
        0x2030, 0x2433, 0x2437, 0x258a, 0x258b, 0x2716, 0x4824, 0x0800, 0x1041, 0x1042,
    ];

    for ct in targets {
        let mut blks = Vec::new();
        collect(&session.blocks, ct, &mut blks);
        if blks.is_empty() {
            continue;
        }
        println!("\n=== 0x{ct:04x} × {} ===", blks.len());
        for (i, b) in blks.iter().enumerate() {
            let name = find_nearest_2619_name(&parents, b, data).unwrap_or_else(|| "?".to_string());
            let p = b.start + 9;
            let max = (b.block_size as usize).saturating_sub(2).min(48);
            let bytes: Vec<String> = (p..(p + max).min(data.len()))
                .map(|i| format!("{:02x}", data[i]))
                .collect();
            println!(
                "[{i}] @ 0x{:x} sz={} ancestor_name={:?}\n    bytes: {}",
                b.start,
                b.block_size,
                name,
                bytes.join(" ")
            );
        }
    }
    Ok(())
}
