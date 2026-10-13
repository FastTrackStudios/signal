//! Dump the structure of every block matching a given content type.
//!
//! Usage: dump_ct <session.ptx> 0x260d [max_entries]

use dawfile_protools::raw_block::RawBlock;

fn main() {
    let path = std::env::args().nth(1).expect("path");
    let ct_str = std::env::args().nth(2).expect("ct (e.g. 0x260d)");
    let max: usize = std::env::args()
        .nth(3)
        .and_then(|s| s.parse().ok())
        .unwrap_or(3);
    let target_ct = u16::from_str_radix(ct_str.trim_start_matches("0x"), 16).expect("bad ct");

    let raw = std::fs::read(&path).unwrap();
    let session = dawfile_protools::parse_raw(raw).unwrap();
    let data = session.cursor().data();

    let mut blocks: Vec<&RawBlock> = Vec::new();
    collect(&session.blocks, target_ct, &mut blocks);

    println!("found {} blocks with ct=0x{target_ct:04x}", blocks.len());
    for (i, b) in blocks.iter().take(max).enumerate() {
        let payload_start = b.start + 9;
        let payload_end = b.end.min(data.len());
        let payload_len = payload_end.saturating_sub(payload_start);
        println!(
            "\n[{i:02}] @0x{:06x}  size={}  children={}  payload={}B",
            b.start,
            b.block_size,
            b.children.len(),
            payload_len
        );
        if !b.children.is_empty() {
            let ct_summary: Vec<String> = b
                .children
                .iter()
                .map(|c| format!("0x{:04x}({}B)", c.content_type_raw, c.block_size))
                .collect();
            println!("    children: {}", ct_summary.join(", "));
        }
        if payload_len > 0 {
            let head = payload_len.min(256);
            let bytes = &data[payload_start..payload_start + head];
            hex_dump(bytes, payload_start);
        }
    }
}

fn collect<'a>(blocks: &'a [RawBlock], ct: u16, out: &mut Vec<&'a RawBlock>) {
    for b in blocks {
        if b.content_type_raw == ct {
            out.push(b);
        }
        collect(&b.children, ct, out);
    }
}

fn hex_dump(bytes: &[u8], base_off: usize) {
    for (i, chunk) in bytes.chunks(16).enumerate() {
        let off = i * 16;
        let hex: Vec<String> = chunk.iter().map(|b| format!("{:02x}", b)).collect();
        let ascii: String = chunk
            .iter()
            .map(|&b| {
                if (0x20..=0x7e).contains(&b) {
                    b as char
                } else {
                    '.'
                }
            })
            .collect();
        println!(
            "    +{off:03x} ({:06x}): {:<48}  |{ascii}|",
            base_off + off,
            hex.join(" ")
        );
    }
}
