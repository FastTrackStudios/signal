//! Dump raw bytes around sub-entries for tracks with bad positions.
//! Run with: cargo run --example debug_entry -- <path.ptx> [track-name-filter]

use dawfile_protools::{block, content_type::ContentType, cursor::Cursor, decrypt};

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        "crates/dawfile-protools/tests/fixtures/studio-session-2.ptx".to_string()
    });

    let mut data = std::fs::read(&path).expect("read");
    let _ = decrypt::decrypt(&mut data).expect("decrypt");
    let is_be = data.get(0x11).copied().unwrap_or(0) != 0;
    let cursor = Cursor::new(&data, is_be);
    let blocks = block::parse_blocks(&data, is_be);

    // Find top-level 0x1054
    let map_block = match blocks
        .iter()
        .find(|b| b.content_type == Some(ContentType::AudioRegionTrackMapNew))
    {
        Some(b) => b,
        None => {
            println!("No 0x1054 found");
            return;
        }
    };

    let target = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "Juno".to_string());

    for map_entry in map_block.find_all(ContentType::AudioRegionTrackMapEntriesNew) {
        let no = map_entry.offset + 2;
        if no + 4 >= data.len() {
            continue;
        }
        let (name, _) = cursor.length_prefixed_string(no);
        if !name.to_lowercase().contains(&target.to_lowercase()) {
            continue;
        }

        println!(
            "\n=== 0x1052: {:?} (offset=0x{:x}) ===",
            name, map_entry.offset
        );

        for track_entry in map_entry.find_all(ContentType::AudioRegionTrackEntryNew) {
            for sub_entry in track_entry.find_all(ContentType::AudioRegionTrackSubEntryNew) {
                let start = sub_entry.offset;
                let end = (start + 48).min(data.len());
                let bytes: Vec<String> = data[start..end]
                    .iter()
                    .map(|b| format!("{:02x}", b))
                    .collect();
                println!(
                    "  0x104f @ 0x{:x} block_size={}: {}",
                    start,
                    sub_entry.block_size,
                    bytes.join(" ")
                );

                let raw_idx = cursor.u32_at(start + 4);
                let rate = 48000u64;
                println!("    region_idx(+4) = {}", raw_idx);
                // Scan u32 at each byte offset looking for a plausible position (< 2 hrs)
                for off in 5..=16usize {
                    if start + off + 4 <= data.len() {
                        let v = cursor.u32_at(start + off) as u64;
                        let secs = v as f64 / rate as f64;
                        let marker = if secs < 7200.0 { " <-- plausible" } else { "" };
                        println!("    u32(+{off:02}) = {v:>12}  {secs:>10.3}s{marker}");
                    }
                }
            }
        }
    }
}
