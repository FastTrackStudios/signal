//! MVP: clone the single 0x261c block from a 1-track baseline once,
//! splice it in after the original, re-encrypt, and report whether
//! the result parses as 2 tracks.

use dawfile_protools::raw_block::RawBlock;
use dawfile_protools::write::splice::splice;

fn find_first(blocks: &[RawBlock], ct: u16) -> Option<&RawBlock> {
    for b in blocks {
        if b.content_type_raw == ct {
            return Some(b);
        }
        if let Some(found) = find_first(&b.children, ct) {
            return Some(found);
        }
    }
    None
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/one_aaa.ptx".into());
    let mut s = dawfile_protools::parse_raw(std::fs::read(&path)?)?;
    let (start, end) = {
        let b = find_first(&s.blocks, 0x261c).expect("0x261c not found");
        (b.start, b.end)
    };
    println!(
        "0x261c at 0x{start:06x}..0x{end:06x}, {} bytes",
        end - start
    );
    let clone: Vec<u8> = s.data[start..end].to_vec();
    println!("cloning {} bytes after original", clone.len());

    // Splice the clone bytes right after the original 0x261c.
    splice(&mut s, end, 0, &clone);

    println!("post-splice block count by CT (0x261c, 0x261b):");
    let mut n261c = 0;
    let mut n261b = 0;
    fn walk(blocks: &[RawBlock], n261c: &mut usize, n261b: &mut usize) {
        for b in blocks {
            if b.content_type_raw == 0x261c {
                *n261c += 1;
            }
            if b.content_type_raw == 0x261b {
                *n261b += 1;
            }
            walk(&b.children, n261c, n261b);
        }
    }
    walk(&s.blocks, &mut n261c, &mut n261b);
    println!("  0x261c × {n261c}");
    println!("  0x261b × {n261b}");

    let out_path = "/tmp/cloned_track.ptx";
    std::fs::write(out_path, s.encrypt())?;
    println!(
        "wrote {out_path} ({} bytes)",
        std::fs::metadata(out_path)?.len()
    );

    // Re-parse from disk via full read_session
    match dawfile_protools::read_session(out_path, 48000) {
        Ok(sess) => {
            let names: Vec<String> = sess.all_tracks().map(|t| t.name.clone()).collect();
            println!("parsed OK; tracks: {names:?}");
        }
        Err(e) => println!("parse FAILED: {e}"),
    }
    Ok(())
}
