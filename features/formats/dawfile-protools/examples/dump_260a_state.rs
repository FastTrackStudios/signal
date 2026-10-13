//! For each track, dump its 0x260a[0] (master-send) bytes including
//! the suspicious +8 flag and the UID-like +19..+22 region.

use dawfile_protools::content_type::ContentType;
use dawfile_protools::raw_block::RawBlock;
use std::collections::HashSet;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).expect("ptx");
    let raw = std::fs::read(&path)?;
    let session = dawfile_protools::parse_raw(raw)?;
    let data = session.cursor().data();

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
    let list = first(&session.blocks, ContentType::MidiTrackList).expect("0x2519");

    let mut wrappers: Vec<&RawBlock> = Vec::new();
    fn walk<'a>(bs: &'a [RawBlock], out: &mut Vec<&'a RawBlock>) {
        for b in bs {
            if b.content_type == Some(ContentType::TrackMixWrapper) {
                out.push(b);
            }
            walk(&b.children, out);
        }
    }
    walk(&session.blocks, &mut wrappers);

    let mut idx = 0usize;
    let mut seen: HashSet<String> = HashSet::new();
    println!("Name                              0x1029+5  0x260a[0]+8  0x260a[0]+19..22");
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
        let Some(w) = wrappers.get(idx) else {
            println!("{name:<33} (no wrapper)");
            continue;
        };
        idx += 1;

        // Find 0x1029 mute bit
        let mute_5 = w
            .children
            .iter()
            .find(|c| c.content_type == Some(ContentType::TrackMixSettings))
            .map(|b| data.get(b.start + 9 + 5).copied().unwrap_or(0))
            .unwrap_or(0xFF);

        // Find first 0x260a child
        let send = w.children.iter().find(|c| c.content_type_raw == 0x260a);
        let (b8, uid) = if let Some(s) = send {
            let v8 = data.get(s.start + 9 + 8).copied().unwrap_or(0);
            let uid: Vec<String> = (19..23)
                .map(|i| {
                    data.get(s.start + 9 + i)
                        .copied()
                        .map(|b| format!("{b:02x}"))
                        .unwrap_or_else(|| "??".to_string())
                })
                .collect();
            (v8, uid.join(" "))
        } else {
            (0xFF, "(no 0x260a)".to_string())
        };

        println!("{name:<33}  0x{mute_5:02x}      0x{b8:02x}         {uid}");
    }
    Ok(())
}
