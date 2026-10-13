//! Dump each track's 0x260d +14 byte (the proposed "effective mute"
//! discriminator) alongside 0x1029 +5 ("storage mute").
//!
//! Goal: find a flag that separates LotF effective-muted tracks
//! (ClickPrint, 02 LORD.01, 6 stems) from over-muted tracks
//! (SYZ, AC GTR, etc.) given both groups have 0x1029 +5 = 1.

use dawfile_protools::content_type::ContentType;
use dawfile_protools::raw_block::{RawBlock, RawSession};
use std::collections::{HashMap, HashSet};

fn name_to_blocks(
    session: &RawSession,
) -> (HashMap<String, &RawBlock>, HashMap<String, &RawBlock>) {
    let data = session.cursor().data();
    let mut name_to_1029 = HashMap::new();
    let mut name_to_260d = HashMap::new();

    let track_list = {
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
        first(&session.blocks, ContentType::MidiTrackList).expect("0x2519")
    };

    let mut mix: Vec<&RawBlock> = Vec::new();
    let mut wrap: Vec<&RawBlock> = Vec::new();
    fn walk<'a>(bs: &'a [RawBlock], mix: &mut Vec<&'a RawBlock>, wrap: &mut Vec<&'a RawBlock>) {
        for b in bs {
            if b.content_type == Some(ContentType::TrackMixSettings) {
                mix.push(b);
            }
            if b.content_type == Some(ContentType::TrackMixWrapper) {
                wrap.push(b);
            }
            walk(&b.children, mix, wrap);
        }
    }
    walk(&session.blocks, &mut mix, &mut wrap);

    let mut mix_idx = 0usize;
    let mut wrap_idx = 0usize;
    let mut seen: HashSet<String> = HashSet::new();
    for c in &track_list.children {
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
        if let Some(b) = mix.get(mix_idx) {
            name_to_1029.insert(name.clone(), *b);
            mix_idx += 1;
        }
        if let Some(b) = wrap.get(wrap_idx) {
            name_to_260d.insert(name.clone(), *b);
            wrap_idx += 1;
        }
    }
    (name_to_1029, name_to_260d)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).expect("ptx");
    let raw = std::fs::read(&path)?;
    let session = dawfile_protools::parse_raw(raw)?;
    let data = session.cursor().data();
    let (m1029, m260d) = name_to_blocks(&session);

    // Build the union of names, preserving 0x251a order.
    let track_list = {
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
        first(&session.blocks, ContentType::MidiTrackList).expect("0x2519")
    };

    let mut order: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for c in &track_list.children {
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
        order.push(name);
    }

    println!("Name                              0x1029+5  0x260d+14  0x260d+447");
    for name in &order {
        let m_5 = m1029
            .get(name)
            .map(|b| data.get(b.start + 9 + 5).copied().unwrap_or(0))
            .unwrap_or(0xFF);
        let w_14 = m260d
            .get(name)
            .map(|b| data.get(b.start + 9 + 14).copied().unwrap_or(0))
            .unwrap_or(0xFF);
        let w_447 = m260d
            .get(name)
            .map(|b| data.get(b.start + 9 + 447).copied().unwrap_or(0))
            .unwrap_or(0xFF);
        println!("{name:<33}  0x{m_5:02x}      0x{w_14:02x}       0x{w_447:02x}");
    }
    Ok(())
}
