//! Dump per-track property blocks (0x1029, 0x102d, 0x251a) for RE.
//!
//! Goal: find which bytes in these blocks encode volume, pan and mute.
use dawfile_protools::{block, content_type::ContentType, cursor::Cursor, decrypt};
use std::env;

fn find_recursive(blocks: &[block::Block], ct: ContentType) -> Option<&block::Block> {
    for b in blocks {
        if b.content_type == Some(ct) {
            return Some(b);
        }
        if let Some(f) = find_recursive(&b.children, ct) {
            return Some(f);
        }
    }
    None
}

fn collect_raw<'a>(blocks: &'a [block::Block], raw: u16, out: &mut Vec<&'a block::Block>) {
    for b in blocks {
        if b.content_type_raw == raw {
            out.push(b);
        }
        collect_raw(&b.children, raw, out);
    }
}

fn hex(slice: &[u8]) -> String {
    slice
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn ascii(slice: &[u8]) -> String {
    slice
        .iter()
        .map(|&b| {
            if (0x20..0x7f).contains(&b) {
                b as char
            } else {
                '.'
            }
        })
        .collect()
}

fn main() {
    let path = env::args().nth(1).expect("usage: dump_track_props <file>");
    let want_raw = env::args()
        .nth(2)
        .and_then(|s| u16::from_str_radix(s.trim_start_matches("0x"), 16).ok());
    let mut data = std::fs::read(&path).expect("read");
    let _ = decrypt::decrypt(&mut data).expect("decrypt");

    let is_be = data.get(0x11).copied().unwrap_or(0) != 0;
    let cursor = Cursor::new(&data, is_be);
    let blocks = block::parse_blocks(&data, is_be);

    println!("is_be: {is_be}");

    // Print track names from 0x1015
    if let Some(tl) = find_recursive(&blocks, ContentType::AudioTrackList) {
        let kids = tl.find_children(ContentType::AudioTrackInfo);
        println!("\n== Tracks ({} entries in 0x1015) ==", kids.len());
        for (i, c) in kids.iter().enumerate() {
            let (n, _) = cursor.length_prefixed_string(c.offset + 2);
            println!("  [{i:02}] {:?}", n);
        }
    }

    // For a target raw type, dump all instances.
    let targets: Vec<u16> = if let Some(r) = want_raw {
        vec![r]
    } else {
        vec![0x1029, 0x102d, 0x251a, 0x2580, 0x2589]
    };

    // Walk top-level blocks and report the first parent that contains 0x1029 / 0x251a children
    {
        fn parents_of(
            blocks: &[block::Block],
            raw: u16,
            depth: usize,
            path: &mut Vec<(u16, usize)>,
        ) {
            for (i, b) in blocks.iter().enumerate() {
                let has_kid = b.children.iter().any(|c| c.content_type_raw == raw);
                if has_kid {
                    path.push((
                        b.content_type_raw,
                        b.children
                            .iter()
                            .filter(|c| c.content_type_raw == raw)
                            .count(),
                    ));
                    println!(
                        "  parent ct=0x{:04x} (depth {depth} #{i}) holds {} children of 0x{raw:04x}",
                        b.content_type_raw,
                        b.children
                            .iter()
                            .filter(|c| c.content_type_raw == raw)
                            .count()
                    );
                }
                parents_of(&b.children, raw, depth + 1, path);
            }
        }
        println!("\n== Parents of 0x1029 ==");
        let mut path = Vec::new();
        parents_of(&blocks, 0x1029, 0, &mut path);
        println!("== Parents of 0x251a ==");
        path.clear();
        parents_of(&blocks, 0x251a, 0, &mut path);
        println!("== Parents of 0x102d ==");
        path.clear();
        parents_of(&blocks, 0x102d, 0, &mut path);
    }

    // Concise per-track summary for 0x1029 if present
    {
        let mut found = Vec::new();
        collect_raw(&blocks, 0x1029, &mut found);
        println!(
            "\n== 0x1029 fields (val_a@+1 i32, flag@+5, val_b@+13 i32, dup_a@+87, dup_b@+103) =="
        );
        for (i, b) in found.iter().enumerate() {
            let p = b.offset + 2;
            let i32at = |o: usize| -> i32 {
                i32::from_le_bytes([
                    data[p + o],
                    data[p + o + 1],
                    data[p + o + 2],
                    data[p + o + 3],
                ])
            };
            let u8at = |o: usize| -> u8 { data[p + o] };
            println!(
                "  [{i:02}] a={:>6} flag={} b={:>6} | dup_a={:>6} dup_flag={} dup_b={:>6} | byte171={} byte175={}",
                i32at(1),
                u8at(5),
                i32at(13),
                i32at(87),
                u8at(91),
                i32at(103),
                u8at(171),
                u8at(175)
            );
        }
    }

    for &t in &targets {
        let mut found = Vec::new();
        collect_raw(&blocks, t, &mut found);
        println!("\n== content_type 0x{:04x} ({} blocks) ==", t, found.len());
        for (i, b) in found.iter().enumerate() {
            // payload starts at offset+2 (offset already points past header)
            // block_size is total size including content_type field;
            // payload bytes: offset+2 .. offset + block_size? Actually block_size
            // measures from the size field; offset = pos+7. End = pos + 7 + block_size = offset + block_size.
            let payload_start = b.offset + 2;
            let payload_end = (b.offset + b.block_size as usize).min(data.len());
            let len = payload_end.saturating_sub(payload_start);
            let take = len.min(290);
            let bytes = &data[payload_start..payload_start + take];
            println!(
                "  [{i:03}] off=0x{:x} size={} children={} payload[0..{take}]:",
                b.offset,
                b.block_size,
                b.children.len()
            );
            // 16 bytes per row with offset header
            for row in 0..take.div_ceil(16) {
                let s = row * 16;
                let e = (s + 16).min(take);
                println!(
                    "        +{:03}: {:<48}  |{}|",
                    s,
                    hex(&bytes[s..e]),
                    ascii(&bytes[s..e])
                );
            }
            // Print direct children content types
            if !b.children.is_empty() {
                let cts: Vec<String> = b
                    .children
                    .iter()
                    .map(|c| format!("0x{:04x}({}B)", c.content_type_raw, c.block_size))
                    .collect();
                println!("        children: {}", cts.join(", "));
            }
        }
    }
}
