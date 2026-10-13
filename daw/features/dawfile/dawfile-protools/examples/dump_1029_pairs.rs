//! Dump the 0x1029 block content for ClickPrint (truly muted) and SYZ
//! (NOT truly muted, but +5=1) side-by-side. Look for a byte that
//! distinguishes them.

use dawfile_protools::content_type::ContentType;
use dawfile_protools::raw_block::RawBlock;
use std::collections::HashSet;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).expect("ptx");
    let raw = std::fs::read(&path)?;
    let session = dawfile_protools::parse_raw(raw)?;
    let data = session.cursor().data();

    let truly_muted: HashSet<&str> = [
        "ClickPrint",
        "02 LORD OF THE FIGHT.01",
        "02 LORD OF THE FIGHT_Vocals",
        "02 LORD OF THE FIGHT_Bass",
        "02 LORD OF THE FIGHT_Drums",
        "02 LORD OF THE FIGHT_Guitar",
        "02 LORD OF THE FIGHT_Other",
        "02 LORD OF THE FIGHT_Piano",
    ]
    .into_iter()
    .collect();

    // Get track names from 0x251a in document order
    let track_list: Option<&RawBlock> = {
        fn first(bs: &[RawBlock], ct: ContentType) -> Option<&RawBlock> {
            for b in bs {
                if b.content_type == Some(ct) {
                    return Some(b);
                }
                if let Some(x) = first(&b.children, ct) {
                    return Some(x);
                }
            }
            None
        }
        first(&session.blocks, ContentType::MidiTrackList)
    };
    let list = track_list.expect("0x2519");
    let mut names: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for c in &list.children {
        if c.content_type != Some(ContentType::MidiTrackInfo) {
            continue;
        }
        let p = c.start + 11;
        if p + 4 > data.len() {
            continue;
        }
        let len = u32::from_le_bytes(data[p..p + 4].try_into().unwrap()) as usize;
        if len == 0 || len > 64 || p + 4 + len > data.len() {
            continue;
        }
        let name = String::from_utf8_lossy(&data[p + 4..p + 4 + len]).to_string();
        if !seen.insert(name.clone()) {
            break;
        }
        names.push(name);
    }

    // Collect all 0x1029 in document order
    let mut blocks_1029: Vec<&RawBlock> = Vec::new();
    fn walk<'a>(bs: &'a [RawBlock], out: &mut Vec<&'a RawBlock>) {
        for b in bs {
            if b.content_type == Some(ContentType::TrackMixSettings) {
                out.push(b);
            }
            walk(&b.children, out);
        }
    }
    walk(&session.blocks, &mut blocks_1029);

    // The mix-block:track alignment: 1:1 with names list (0x1029 count == 29 == names - 1
    // because Master has no 0x1029). So names[1..] aligns with blocks_1029.
    println!("names: {}  blocks_1029: {}", names.len(), blocks_1029.len());

    // For each track that has +5=1, dump its 0x1029 payload and flag whether
    // it's truly muted per the converter.
    let dump_len = 40usize;
    println!("\nTrack name                       muted   payload bytes (first {dump_len})");
    for (i, b) in blocks_1029.iter().enumerate() {
        let name = names.get(i + 1).cloned().unwrap_or_else(|| format!("?{i}"));
        let p = b.start + 9;
        let v5 = data[p + 5];
        if v5 == 0 {
            continue;
        }
        let real_muted = truly_muted.contains(name.as_str());
        let bytes: Vec<String> = (p..(p + dump_len).min(data.len()))
            .map(|j| format!("{:02x}", data[j]))
            .collect();
        let flag = if real_muted { "MUTED" } else { "NOT  " };
        println!("{name:<32} {flag}   {}", bytes.join(" "));
    }
    Ok(())
}
