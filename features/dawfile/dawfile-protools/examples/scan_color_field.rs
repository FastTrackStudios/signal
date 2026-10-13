//! Find per-track fields that fit a color: 69 distinct values, NOT
//! monotonically increasing with block index (which would be a UID).
//!
//! Walk inner child blocks structurally so absolute-offset shifts don't
//! create false positives.

use dawfile_protools::raw_block::RawBlock;

fn main() {
    let path = std::env::args().nth(1).expect("path");
    let raw = std::fs::read(&path).unwrap();
    let session = dawfile_protools::parse_raw(raw).unwrap();
    let data = session.cursor().data();

    // For each content_type with N blocks (N close to 69 or 72), gather
    // every block's payload (relative to its own header). Then for each
    // byte position, count distinct values AND compute a "monotonicity
    // score" = how often value[i+1] - value[i] == 1.
    let mut by_ct: std::collections::HashMap<u16, Vec<&RawBlock>> =
        std::collections::HashMap::new();
    walk(&session.blocks, &mut by_ct);

    for (ct, blocks) in by_ct {
        let n = blocks.len();
        if !(60..=80).contains(&n) {
            continue;
        }
        let min_len = blocks
            .iter()
            .map(|b| b.block_size.saturating_sub(2) as usize)
            .min()
            .unwrap_or(0);
        if min_len == 0 {
            continue;
        }

        for i in 0..min_len {
            let vals: Vec<u8> = blocks
                .iter()
                .filter_map(|b| {
                    let p = b.start + 9 + i;
                    if p < data.len() { Some(data[p]) } else { None }
                })
                .collect();
            if vals.len() != n {
                continue;
            }
            let distinct: std::collections::BTreeSet<u8> = vals.iter().copied().collect();
            // Color: 23 to 70 distinct values
            if distinct.len() < 23 || distinct.len() > 70 {
                continue;
            }

            // Monotonicity score
            let mut mono_step1 = 0;
            for w in vals.windows(2) {
                if w[1].wrapping_sub(w[0]) == 1 {
                    mono_step1 += 1;
                }
            }
            // High monotonicity = UID-like; low = color-like
            let mono_pct = (mono_step1 * 100) / (n - 1);

            // Variance / entropy proxy: range
            let min_v = vals.iter().min().copied().unwrap_or(0);
            let max_v = vals.iter().max().copied().unwrap_or(0);

            if mono_pct < 50 {
                println!(
                    "0x{ct:04x} +{i:>4}  N={n}  distinct={}  mono={mono_pct}%  range=0x{min_v:02x}..0x{max_v:02x}",
                    distinct.len()
                );
            }
        }
    }
}

fn walk<'a>(blocks: &'a [RawBlock], by_ct: &mut std::collections::HashMap<u16, Vec<&'a RawBlock>>) {
    for b in blocks {
        by_ct.entry(b.content_type_raw).or_default().push(b);
        walk(&b.children, by_ct);
    }
}
