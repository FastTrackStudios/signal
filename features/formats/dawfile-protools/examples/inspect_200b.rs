#![allow(clippy::all, dead_code)]
//! Align 0x200b blocks with their parent track. 0x200b is per-colored-track,
//! living inside 0x261c (which is also per-colored-track, missing folders).

use dawfile_protools::raw_block::RawBlock;

fn main() {
    let path = std::env::args().nth(1).expect("path");
    let raw = std::fs::read(&path).unwrap();
    let session = dawfile_protools::parse_raw(raw).unwrap();
    let data = session.cursor().data();

    // Find every 0x261c (parent), grab its track name from inner 0x102d,
    // then read its 0x200b sibling/child's byte at +163.
    let containers = collect(&session.blocks, 0x261c);
    println!("{} 0x261c containers", containers.len());

    for (i, c) in containers.iter().enumerate() {
        // Track name lives in 0x102d → +0..+4 length-prefixed string. The path:
        // 0x261c → 0x261b → 0x102d → name at payload + 0
        let name = find_track_name(c, data).unwrap_or_else(|| format!("?#{i}"));

        // 0x200b is a direct child of 0x261c
        let b200b = c.children.iter().find(|x| x.content_type_raw == 0x200b);
        let val_163 = b200b.and_then(|b| {
            let p = b.start + 9 + 163;
            if p < data.len() { Some(data[p]) } else { None }
        });
        let val_106 = b200b.and_then(|b| {
            let p = b.start + 9 + 106;
            if p < data.len() { Some(data[p]) } else { None }
        });

        println!(
            "  [{i:02}]  +106=0x{:02x}  +163=0x{:02x}  {}",
            val_106.unwrap_or(0xff),
            val_163.unwrap_or(0xff),
            name
        );
    }
}

fn collect(blocks: &[RawBlock], ct: u16) -> Vec<&RawBlock> {
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

fn find_track_name(b: &RawBlock, data: &[u8]) -> Option<String> {
    // descend looking for 0x102d
    for c in &b.children {
        if c.content_type_raw == 0x2619 {
            // 0x2619 payload: u32 length + chars
            let p = c.start + 9;
            if p + 4 > data.len() {
                return None;
            }
            let len = u32::from_le_bytes(data[p..p + 4].try_into().unwrap()) as usize;
            if len > 64 || p + 4 + len > data.len() {
                return None;
            }
            return Some(String::from_utf8_lossy(&data[p + 4..p + 4 + len]).to_string());
        }
        if let Some(n) = find_track_name(c, data) {
            return Some(n);
        }
        // legacy duplicate branch (will be reached only if 0x102d wasn't found above)
        if false && c.content_type_raw == 0x102d {
            // payload + 0 = u32 length, then chars
            let p = c.start + 9;
            if p + 4 > data.len() {
                return None;
            }
            let len = u32::from_le_bytes(data[p..p + 4].try_into().unwrap()) as usize;
            if len > 64 || p + 4 + len > data.len() {
                return None;
            }
            return Some(String::from_utf8_lossy(&data[p + 4..p + 4 + len]).to_string());
        }
    }
    None
}
