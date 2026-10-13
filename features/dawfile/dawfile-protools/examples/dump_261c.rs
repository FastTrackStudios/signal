//! Dump each 0x261c container with its child structure + the +163 color byte
//! at the inner 0x200b. Useful for debugging the color decoder on fixtures
//! where the standard `find_2619_track_name` heuristic misses tracks.

use dawfile_protools::content_type::ContentType;
use dawfile_protools::raw_block::RawBlock;

fn main() {
    let path = std::env::args().nth(1).expect("path");
    let raw = std::fs::read(&path).unwrap();
    let session = dawfile_protools::parse_raw(raw).unwrap();
    let data = session.cursor().data();

    fn walk(b: &RawBlock, depth: u32, data: &[u8]) {
        let pad = " ".repeat(depth as usize * 2);
        let ct = b.content_type_raw;
        let mut info = format!("{}0x{ct:04x} sz={}", pad, b.block_size);

        if ct == 0x2619 {
            // Try to read name (length-prefixed string at +2)
            let p = b.start + 9;
            if p + 4 <= data.len() {
                let len = u32::from_le_bytes(data[p..p + 4].try_into().unwrap()) as usize;
                if len < 256 && p + 4 + len <= data.len() {
                    let name = String::from_utf8_lossy(&data[p + 4..p + 4 + len]).into_owned();
                    info.push_str(&format!("  name='{name}'"));
                }
            }
            // also dump first 32 bytes of payload
            let bytes: Vec<String> = (b.start + 9..b.start + 9 + 32.min(b.block_size as usize))
                .filter(|p| *p < data.len())
                .map(|p| format!("{:02x}", data[p]))
                .collect();
            info.push_str(&format!("  bytes: {}", bytes.join(" ")));
        } else if ct == 0x200b {
            // dump +163 byte (the color byte) and a window
            let p = b.start + 9;
            let payload_len = b.block_size.saturating_sub(2) as usize;
            if payload_len > 163 && p + 163 < data.len() {
                info.push_str(&format!("  +163=0x{:02x}", data[p + 163]));
            }
        }
        println!("{info}");
        for c in &b.children {
            walk(c, depth + 1, data);
        }
    }

    let mut count = 0;
    fn find_261c<'a>(blocks: &'a [RawBlock], out: &mut Vec<&'a RawBlock>) {
        for b in blocks {
            if b.content_type == Some(ContentType::TrackContainer) {
                out.push(b);
            }
            find_261c(&b.children, out);
        }
    }
    let mut containers = Vec::new();
    find_261c(&session.blocks, &mut containers);
    println!("found {} containers", containers.len());

    for (i, c) in containers.iter().enumerate().take(5) {
        println!("\n=== container [{i}] @ 0x{:x} ===", c.start);
        walk(c, 0, data);
        count += 1;
    }
    let _ = count;
}
