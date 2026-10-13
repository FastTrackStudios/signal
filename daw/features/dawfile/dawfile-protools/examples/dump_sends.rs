//! Walk each 0x260d wrapper and dump its 0x260a children (suspected sends).

use dawfile_protools::content_type::ContentType;
use dawfile_protools::raw_block::RawBlock;

fn main() {
    let path = std::env::args().nth(1).expect("path");
    let raw = std::fs::read(&path).unwrap();
    let session = dawfile_protools::parse_raw(raw).unwrap();
    let data = session.cursor().data();

    // Walk 0x251a list to recover track names in document order
    let track_list = find(&session.blocks, ContentType::MidiTrackList).expect("no 0x2519");
    let mut names: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for c in &track_list.children {
        if c.content_type != Some(ContentType::MidiTrackInfo) {
            continue;
        }
        let off = c.start + 11;
        let len = u32::from_le_bytes(data[off..off + 4].try_into().unwrap()) as usize;
        let name = String::from_utf8_lossy(&data[off + 4..off + 4 + len.min(64)]).to_string();
        if !seen.insert(name.clone()) {
            break;
        }
        names.push(name);
    }

    let wrappers = collect(&session.blocks, ContentType::TrackMixWrapper);
    println!("{} wrappers, {} names", wrappers.len(), names.len());

    for (i, w) in wrappers.iter().enumerate() {
        let name = names.get(i).map(|s| s.as_str()).unwrap_or("?");
        let sends: Vec<_> = w
            .children
            .iter()
            .filter(|c| c.content_type_raw == 0x260a)
            .collect();
        let routing = w
            .children
            .iter()
            .find(|c| c.content_type == Some(ContentType::TrackRouting));
        let dest = match routing {
            Some(r) => decode_routing_dest(r, data),
            None => String::new(),
        };
        println!("\n[{i:02}] {name} → {dest}  ({} 0x260a)", sends.len());
        for s in sends {
            let p = s.start + 9;
            if p + 30 > data.len() {
                continue;
            }
            let flag_u16 = u16::from_le_bytes(data[p + 8..p + 10].try_into().unwrap());
            let pos = u64::from_le_bytes({
                let mut b = [0u8; 8];
                b[..6].copy_from_slice(&data[p + 14..p + 20]);
                b
            });
            let level = i32::from_le_bytes(data[p + 26..p + 30].try_into().unwrap());
            println!("    flag={flag_u16} pos={pos} level={level}");
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

fn collect(blocks: &[RawBlock], ct: ContentType) -> Vec<&RawBlock> {
    let mut out = Vec::new();
    fn rec<'a>(blocks: &'a [RawBlock], ct: ContentType, out: &mut Vec<&'a RawBlock>) {
        for b in blocks {
            if b.content_type == Some(ct) {
                out.push(b);
            }
            rec(&b.children, ct, out);
        }
    }
    rec(blocks, ct, &mut out);
    out
}

fn decode_routing_dest(r: &RawBlock, data: &[u8]) -> String {
    let payload = r.start + 9;
    let str_off = payload + 0x24;
    if str_off + 4 > data.len() {
        return String::new();
    }
    let str_len = u32::from_le_bytes(data[str_off..str_off + 4].try_into().unwrap()) as usize;
    if str_len == 0 || str_len > 128 || str_off + 4 + str_len > data.len() {
        return String::new();
    }
    String::from_utf8_lossy(&data[str_off + 4..str_off + 4 + str_len])
        .trim_end_matches('\0')
        .to_string()
}
