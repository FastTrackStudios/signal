use dawfile_protools::{block, cursor::Cursor, decrypt};
use std::env;

fn show_tree(b: &block::Block, cursor: &Cursor, depth: usize) {
    let indent = "  ".repeat(depth);
    let data = cursor.data();
    let name_offset = b.offset + 2;
    let name_hint = if name_offset + 5 < data.len() {
        let len = cursor.u32_at(name_offset) as usize;
        if len > 0 && len < 64 && name_offset + 4 + len <= data.len() {
            if let Ok(s) = std::str::from_utf8(&data[name_offset + 4..name_offset + 4 + len]) {
                if s.chars().all(|c| c.is_ascii_graphic() || c == ' ') {
                    format!(" name={s:?}")
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    } else {
        String::new()
    };
    println!(
        "{indent}[0x{:04x}]{name_hint} sz={} children={}",
        b.content_type_raw,
        b.block_size,
        b.children.len()
    );
    for child in &b.children {
        show_tree(child, cursor, depth + 1);
    }
}

fn main() {
    let path = env::args().nth(1).expect("usage: inspect_playlists <file>");
    let mut data = std::fs::read(&path).expect("read");
    let _ = decrypt::decrypt(&mut data).expect("decrypt");
    let is_be = data.get(0x11).copied().unwrap_or(0) != 0;
    let cursor = Cursor::new(&data, is_be);
    let blocks = block::parse_blocks(&data, is_be);

    // Find 0x2428 and 0x2429 blocks (alternate playlist containers)
    for b in &blocks {
        if b.content_type_raw == 0x2428 || b.content_type_raw == 0x2429 {
            println!(
                "=== Found 0x{:04x} at offset 0x{:x} ===",
                b.content_type_raw, b.offset
            );
            show_tree(b, &cursor, 0);
        }
        // Also check children
        for child in &b.children {
            if child.content_type_raw == 0x2428 || child.content_type_raw == 0x2429 {
                println!(
                    "=== Found 0x{:04x} (child) at offset 0x{:x} ===",
                    child.content_type_raw, child.offset
                );
                show_tree(child, &cursor, 0);
            }
        }
    }

    // Show main 0x1054 playlist name structure
    println!("\n=== Main 0x1054 (active playlist) 0x1052 names ===");
    for b in &blocks {
        if b.content_type_raw == 0x1054 {
            for entry in &b.children {
                if entry.content_type_raw == 0x1052 {
                    let no = entry.offset + 2;
                    let len = cursor.u32_at(no) as usize;
                    let name = if len < 128 && no + 4 + len <= data.len() {
                        std::str::from_utf8(&data[no + 4..no + 4 + len])
                            .unwrap_or("?")
                            .to_string()
                    } else {
                        format!("len={len}?")
                    };
                    println!("  playlist={name:?} regions={}", entry.children.len());
                }
            }
            break; // Only main
        }
    }
}
