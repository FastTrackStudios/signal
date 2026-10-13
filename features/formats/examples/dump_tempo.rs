use formats::protools::block;

fn find_first(blocks: &[block::Block], raw: u16) -> Option<block::Block> {
    for b in blocks {
        if b.content_type_raw == raw {
            return Some(b.clone());
        }
        if let Some(found) = find_first(&b.children, raw) {
            return Some(found);
        }
    }
    None
}

fn dump_session(_path: &str, data: &[u8]) {
    let d = data.to_vec();
    let is_be = d.get(0x11).copied().unwrap_or(0) != 0;
    let blocks = block::parse_blocks(&d, is_be);

    if let Some(b) = find_first(&blocks, 0x2023) {
        let start = b.offset + 2;
        // Read ticks_per_beat at start+52, bpm*100 at start+60
        if start + 64 <= d.len() {
            let tpb = u32::from_le_bytes(d[start + 52..start + 56].try_into().unwrap());
            let bpm_x100 = u32::from_le_bytes(d[start + 60..start + 64].try_into().unwrap());
            println!(
                "  0x2023: ticks_per_beat={tpb}  bpm_x100={bpm_x100} → {:.2} BPM",
                bpm_x100 as f64 / 100.0
            );
        }
    }

    // Also check 0x2028 "Tempo" blocks
    let mut all_tempo = Vec::new();
    fn gather(blocks: &[block::Block], raw: u16, out: &mut Vec<block::Block>) {
        for b in blocks {
            if b.content_type_raw == raw {
                out.push(b.clone());
            }
            gather(&b.children, raw, out);
        }
    }
    gather(&blocks, 0x2028, &mut all_tempo);
    for b in &all_tempo {
        let start = b.offset + 2;
        let end = (start + b.block_size as usize).min(d.len());
        let hex: Vec<String> = d[start..end].iter().map(|x| format!("{:02x}", x)).collect();
        println!("  0x2028 sz={}: {}", b.block_size, hex.join(" "));
    }
}

fn main() {
    let sessions = [
        "features/formats/tests/fixtures/protools/studio-session-2.ptx",
        "features/formats/tests/fixtures/protools/wonder-session.ptx",
        "features/formats/tests/fixtures/protools/worship-session.ptx",
        "features/formats/tests/fixtures/protools/GodnessOfGod.ptx",
        "features/formats/tests/fixtures/protools/goodplaylists2.ptf",
    ];
    for path in &sessions {
        println!("{path}:");
        let mut data = std::fs::read(path).expect("read");
        let _ = formats::protools::decrypt::decrypt(&mut data).expect("decrypt");
        dump_session(path, &data);
        println!();
    }
}
