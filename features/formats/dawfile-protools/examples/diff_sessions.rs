//! Diff two decrypted .ptx buffers and report per-content-type where
//! they differ. Useful for snapshot pairs (the same session saved
//! at different times) to isolate which fields PT mutated.

use dawfile_protools::raw_block::RawBlock;

fn main() {
    let a_path = std::env::args().nth(1).expect("path A");
    let b_path = std::env::args().nth(2).expect("path B");

    let a_raw = std::fs::read(&a_path).unwrap();
    let b_raw = std::fs::read(&b_path).unwrap();

    let a = dawfile_protools::parse_raw(a_raw).unwrap();
    let b = dawfile_protools::parse_raw(b_raw).unwrap();

    println!("A: {} bytes, B: {} bytes", a.data.len(), b.data.len());

    // Group every block in each session by content_type and accumulate
    // a hash of the inner content. Compare aggregate bytes per CT.
    let a_by_ct = bytes_per_ct(&a.blocks, &a.data);
    let b_by_ct = bytes_per_ct(&b.blocks, &b.data);

    let mut all_cts: std::collections::BTreeSet<u16> = std::collections::BTreeSet::new();
    all_cts.extend(a_by_ct.keys());
    all_cts.extend(b_by_ct.keys());

    println!("\nct      A_count A_bytes  B_count B_bytes  changed");
    for ct in all_cts {
        let av = a_by_ct.get(&ct);
        let bv = b_by_ct.get(&ct);
        let (a_n, a_b, a_hash) = av.copied().unwrap_or((0, 0, 0));
        let (b_n, b_b, b_hash) = bv.copied().unwrap_or((0, 0, 0));
        let changed = a_hash != b_hash;
        if changed || a_n != b_n {
            println!(
                "0x{ct:04x}  {a_n:>5}  {a_b:>8}  {b_n:>5}  {b_b:>8}    {}",
                if changed { "Y" } else { "." }
            );
        }
    }
}

fn bytes_per_ct(
    blocks: &[RawBlock],
    data: &[u8],
) -> std::collections::HashMap<u16, (usize, usize, u64)> {
    let mut map: std::collections::HashMap<u16, (usize, usize, u64)> =
        std::collections::HashMap::new();
    fn rec(
        blocks: &[RawBlock],
        data: &[u8],
        map: &mut std::collections::HashMap<u16, (usize, usize, u64)>,
    ) {
        for b in blocks {
            let bytes = &data[b.start..b.end.min(data.len())];
            let h = fxhash(bytes);
            let e = map.entry(b.content_type_raw).or_insert((0, 0, 0));
            e.0 += 1;
            e.1 += bytes.len();
            // XOR hashes so order doesn't matter across same-CT siblings
            e.2 ^= h;
            rec(&b.children, data, map);
        }
    }
    rec(blocks, data, &mut map);
    map
}

fn fxhash(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}
