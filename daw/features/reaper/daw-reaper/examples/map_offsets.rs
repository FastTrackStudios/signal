use dawfile_protools::raw_block::RawBlock;
use std::io::BufRead;
fn find(blocks: &[RawBlock], target: usize) -> Option<(u16, usize, usize)> {
    // Returns innermost block containing target
    let mut best: Option<(u16, usize, usize)> = None;
    fn walk(blocks: &[RawBlock], target: usize, best: &mut Option<(u16, usize, usize)>) {
        for b in blocks {
            if target >= b.start && target < b.end {
                *best = Some((b.content_type_raw, b.start, target - b.start));
                walk(&b.children, target, best);
            }
        }
    }
    walk(blocks, target, &mut best);
    best
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).unwrap();
    let s = dawfile_protools::parse_raw(std::fs::read(&path)?)?;
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = line?;
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }
        let off: usize = match parts[0].parse() {
            Ok(o) => o,
            _ => continue,
        };
        let val: u8 = match parts[1].parse() {
            Ok(v) => v,
            _ => continue,
        };
        if let Some((ct, start, rel)) = find(&s.blocks, off) {
            println!("off={} val={} 0x{:04x}@0x{:x}+{}", off, val, ct, start, rel);
        } else {
            println!("off={} val={} OUT_OF_RANGE", off, val);
        }
    }
    Ok(())
}
