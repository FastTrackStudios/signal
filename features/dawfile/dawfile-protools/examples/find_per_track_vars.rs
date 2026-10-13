#![allow(clippy::all, dead_code)]
//! For every content type that has ~track-count instances, print all payload
//! byte positions where values vary AND the distinct values present.
//!
//! Useful for finding track color (and any other per-track attribute that's
//! not yet decoded). Color should show as a low-cardinality field
//! (palette = 0..16 distinct values, or 3-byte RGB clusters).

use dawfile_protools::raw_block::RawBlock;
use std::collections::{BTreeMap, BTreeSet, HashMap};

fn main() {
    let path = std::env::args().nth(1).expect("path");
    let target_count: usize = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);
    let raw = std::fs::read(&path).unwrap();
    let session = dawfile_protools::parse_raw(raw).unwrap();
    let data = session.cursor().data();

    // Bucket blocks by content type
    let mut by_ct: HashMap<u16, Vec<&RawBlock>> = HashMap::new();
    walk(&session.blocks, &mut by_ct);

    // For each CT whose count matches target (±2), look at varying bytes
    let mut keys: Vec<u16> = by_ct
        .iter()
        .filter(|(_, v)| {
            let c = v.len();
            (c as i64 - target_count as i64).abs() <= 2
        })
        .map(|(k, _)| *k)
        .collect();
    keys.sort();

    for ct in keys {
        let blocks = &by_ct[&ct];
        let mut min_len = usize::MAX;
        for b in blocks {
            let payload_len = (b.block_size as usize).saturating_sub(2);
            if payload_len < min_len {
                min_len = payload_len;
            }
        }
        if min_len == 0 {
            continue;
        }

        // distinct[i] = set of byte values at position i across blocks
        let mut distinct: Vec<BTreeSet<u8>> = vec![BTreeSet::new(); min_len];
        for b in blocks {
            let payload_start = b.start + 9;
            for i in 0..min_len {
                let off = payload_start + i;
                if off < data.len() {
                    distinct[i].insert(data[off]);
                }
            }
        }

        let varying: Vec<usize> = (0..min_len).filter(|&i| distinct[i].len() >= 2).collect();
        if varying.is_empty() {
            continue;
        }
        println!(
            "\n0x{ct:04x}  ({} blocks, min_payload={min_len}B)  — {} varying positions",
            blocks.len(),
            varying.len()
        );

        // Group consecutive varying positions into RUNS (multi-byte fields)
        let mut runs: Vec<Vec<usize>> = Vec::new();
        let mut cur: Vec<usize> = Vec::new();
        for &i in &varying {
            if let Some(&last) = cur.last() {
                if i == last + 1 {
                    cur.push(i);
                    continue;
                }
                runs.push(std::mem::take(&mut cur));
            }
            cur.push(i);
        }
        if !cur.is_empty() {
            runs.push(cur);
        }

        for run in &runs {
            // Collapse the run into per-block values
            let mut counts: BTreeMap<Vec<u8>, usize> = BTreeMap::new();
            for b in blocks {
                let payload_start = b.start + 9;
                let mut v = Vec::with_capacity(run.len());
                for &i in run {
                    let off = payload_start + i;
                    v.push(if off < data.len() { data[off] } else { 0 });
                }
                *counts.entry(v).or_insert(0) += 1;
            }
            // Sort by frequency (most common first) so the default value
            // bubbles to the top and outliers stand out.
            let mut sorted: Vec<(Vec<u8>, usize)> = counts.into_iter().collect();
            sorted.sort_by(|a, b| b.1.cmp(&a.1));
            let preview: Vec<String> = sorted
                .iter()
                .take(8)
                .map(|(v, c)| {
                    let hex: Vec<String> = v.iter().map(|b| format!("{:02x}", b)).collect();
                    format!("{}×{}", hex.join(""), c)
                })
                .collect();
            println!(
                "  +{:>3}..+{:<3} ({}B, {} distinct) | {}",
                run[0],
                run[run.len() - 1],
                run.len(),
                sorted.len(),
                preview.join(" ")
            );
        }
    }
}

fn walk<'a>(blocks: &'a [RawBlock], by_ct: &mut HashMap<u16, Vec<&'a RawBlock>>) {
    for b in blocks {
        by_ct.entry(b.content_type_raw).or_default().push(b);
        walk(&b.children, by_ct);
    }
}
