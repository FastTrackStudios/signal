//! For the per-track 1:1 content types, dump payload hex per-track alongside
//! ground-truth mute flag. Manual visual scan for the byte that differs.

use dawfile_protools::content_type::ContentType;
use dawfile_protools::raw_block::RawBlock;
use std::collections::{BTreeMap, HashSet};

fn main() {
    let path = std::env::args().nth(1).expect("path");
    let raw = std::fs::read(&path).unwrap();
    let session = dawfile_protools::parse_raw(raw).unwrap();
    let data = session.cursor().data();

    let list = find(&session.blocks, ContentType::MidiTrackList).unwrap();
    let mut names: Vec<String> = Vec::new();
    let mut seen = HashSet::new();
    for c in &list.children {
        if c.content_type != Some(ContentType::MidiTrackInfo) {
            continue;
        }
        let off = c.start + 11;
        let len = u32::from_le_bytes(data[off..off + 4].try_into().unwrap()) as usize;
        let n = String::from_utf8_lossy(&data[off + 4..off + 4 + len.min(64)]).to_string();
        if !seen.insert(n.clone()) {
            break;
        }
        names.push(n);
    }

    let expected_muted: HashSet<&str> = [
        "ClickPrint",
        "02 LORD OF THE FIGHT.01",
        "02 LORD OF THE FIGHT_Vocals",
        "02 LORD OF THE FIGHT_Bass",
        "02 LORD OF THE FIGHT_Drums",
        "02 LORD OF THE FIGHT_Guitar",
        "02 LORD OF THE FIGHT_Other",
        "02 LORD OF THE FIGHT_Piano",
        "MIDI 1",
        "Inst 1",
        "Inst 1.dup1",
        "Inst 1.dup2",
        "Inst 1.dup1.02",
        "Inst 1.dup2.02",
        "Inst 1.dup2.04",
        "Inst 1.dup3.02",
        "Inst 1.dup4.02",
    ]
    .into_iter()
    .collect();

    let mut by_ct: BTreeMap<u16, Vec<&RawBlock>> = BTreeMap::new();
    walk(&session.blocks, &mut by_ct);

    let targets: [u16; 6] = [0x102d, 0x200a, 0x2011, 0x2015, 0x210b, 0x2589];
    for ct in targets {
        let Some(blocks) = by_ct.get(&ct) else {
            continue;
        };
        if blocks.len() != names.len() {
            continue;
        }
        println!("\n=== CT 0x{ct:04x} ({} blocks) ===", blocks.len());
        let max_show = 24;
        for (i, b) in blocks.iter().enumerate() {
            let muted = expected_muted.contains(names[i].as_str());
            let payload_start = b.start + 9;
            let payload_len = b.block_size.saturating_sub(2) as usize;
            let end = (payload_start + payload_len.min(max_show)).min(data.len());
            let hex: Vec<String> = (payload_start..end)
                .map(|p| format!("{:02x}", data[p]))
                .collect();
            println!(
                "  [{:02}] M={} sz={:>5} {}  {}",
                i,
                muted as u8,
                b.block_size,
                hex.join(" "),
                names[i]
            );
        }
    }
}

fn find(blocks: &[RawBlock], ct: ContentType) -> Option<&RawBlock> {
    for b in blocks {
        if b.content_type == Some(ct) {
            return Some(b);
        }
        if let Some(x) = find(&b.children, ct) {
            return Some(x);
        }
    }
    None
}

fn walk<'a>(blocks: &'a [RawBlock], by_ct: &mut BTreeMap<u16, Vec<&'a RawBlock>>) {
    for b in blocks {
        by_ct.entry(b.content_type_raw).or_default().push(b);
        walk(&b.children, by_ct);
    }
}
