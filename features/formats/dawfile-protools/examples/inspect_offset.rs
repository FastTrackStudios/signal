//! For each 0x260d wrapper, print the track name + the byte values at given
//! offsets — aligned via 0x251a order.
//!
//! Usage: inspect_offset <ptx> <ct_hex> <off1> [off2] [off3] ...

use dawfile_protools::content_type::ContentType;
use dawfile_protools::raw_block::RawBlock;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = &args[1];
    let ct = u16::from_str_radix(args[2].trim_start_matches("0x"), 16).expect("bad ct");
    let offsets: Vec<usize> = args[3..]
        .iter()
        .map(|s| s.parse().expect("bad offset"))
        .collect();

    let raw = std::fs::read(path).unwrap();
    let session = dawfile_protools::parse_raw(raw).unwrap();
    let data = session.cursor().data();

    // Names in 0x251a order, doubled-list-aware
    let track_list = find(&session.blocks, ContentType::MidiTrackList).unwrap();
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

    // Collect blocks of the target CT in document order
    let blocks = collect_ct(&session.blocks, ct);
    println!(
        "{} blocks of ct=0x{ct:04x}, {} names",
        blocks.len(),
        names.len()
    );

    let off_list: String = offsets
        .iter()
        .map(|o| format!("+{o:>3}"))
        .collect::<Vec<_>>()
        .join(" ");
    println!("idx  {off_list}  track");
    for (i, b) in blocks.iter().enumerate() {
        let payload = b.start + 9;
        let vals: Vec<String> = offsets
            .iter()
            .map(|&o| {
                let p = payload + o;
                if p < data.len() {
                    format!("0x{:02x}", data[p])
                } else {
                    "..".into()
                }
            })
            .collect();
        let name = names.get(i).map(|s| s.as_str()).unwrap_or("?");
        println!("[{i:02}]  {}  {name}", vals.join(" "));
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

fn collect_ct(blocks: &[RawBlock], ct: u16) -> Vec<&RawBlock> {
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
