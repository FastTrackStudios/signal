//! Where the rig's startup actually goes, and what parallelism buys.
//!
//!     cargo run --release -p signal-guitar --example startup_cost
//!
//! The gap between "audio device linked" and "profile loaded" was ten
//! seconds. The first guess was the NAM loads; it was wrong by a factor of
//! forty. This times every block of every patch, groups the cost by block
//! kind, and then builds all the chains again concurrently so the speedup is
//! a measurement rather than a hope.

use std::collections::BTreeMap;
use std::time::Instant;

use signal_sampler::rig::{RigBlock, prepare_chain};

const SAMPLE_RATE: u32 = 48_000;

fn chains() -> Vec<(String, Vec<RigBlock>)> {
    let def = signal_guitar::profiles::worship_def();
    let dps = signal_guitar::profiles::drive_presets();
    let profile = signal_guitar::profiles::build_profile(&def, &dps);
    profile
        .patches
        .iter()
        .map(|p| {
            let blocks: Vec<RigBlock> =
                p.chain.iter().filter(|b| b.has_backend()).cloned().collect();
            (p.name.clone(), blocks)
        })
        .filter(|(_, b)| !b.is_empty())
        .collect()
}

fn main() {
    let chains = chains();
    let blocks_total: usize = chains.iter().map(|(_, b)| b.len()).sum();
    println!(
        "patches: {}   blocks: {}   ({} per patch)",
        chains.len(),
        blocks_total,
        blocks_total / chains.len().max(1)
    );

    // Warm the page cache and the C++ loader so the breakdown below measures
    // steady-state construction, not first-touch I/O.
    for (_, blocks) in &chains {
        let _ = prepare_chain(blocks, &[], SAMPLE_RATE);
    }

    // Per-block cost, grouped by kind.
    let mut by_kind: BTreeMap<String, (usize, f64)> = BTreeMap::new();
    let serial_began = Instant::now();
    for (_, blocks) in &chains {
        for b in blocks {
            let one = std::slice::from_ref(b);
            let t = Instant::now();
            let _ = prepare_chain(one, &[], SAMPLE_RATE);
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            let kind = format!("{:?}", b.block_type);
            let e = by_kind.entry(kind).or_insert((0, 0.0));
            e.0 += 1;
            e.1 += ms;
        }
    }
    let serial_ms = serial_began.elapsed().as_secs_f64() * 1000.0;

    println!("\nper block kind (all {blocks_total} blocks):");
    let mut rows: Vec<_> = by_kind.into_iter().collect();
    rows.sort_by(|a, b| b.1.1.total_cmp(&a.1.1));
    for (kind, (n, ms)) in &rows {
        println!(
            "  {kind:<24} {n:>4} blocks  {ms:>9.1} ms  ({:>6.2} ms each, {:>4.1}%)",
            ms / *n as f64,
            ms / serial_ms * 100.0
        );
    }
    println!("  {:<24} {blocks_total:>4} blocks  {serial_ms:>9.1} ms", "TOTAL");

    // Whole chains, serial.
    let t = Instant::now();
    for (_, blocks) in &chains {
        let _ = prepare_chain(blocks, &[], SAMPLE_RATE);
    }
    let chains_serial = t.elapsed().as_secs_f64() * 1000.0;

    // Whole chains, chunked across N threads — the shape `load_profile`
    // uses. Swept, because the naive "one thread per chain" was *slower*
    // than serial and a number that moves the wrong way deserves a curve.
    println!("\nall {} chains:", chains.len());
    println!("  serial                  {chains_serial:>9.1} ms");
    for threads in [2usize, 3, 4, 6, 8, 12, 16] {
        if threads > chains.len() {
            break;
        }
        let chunk = chains.len().div_ceil(threads);
        let t = Instant::now();
        std::thread::scope(|scope| {
            let handles: Vec<_> = chains
                .chunks(chunk)
                .map(|c| {
                    scope.spawn(move || {
                        for (_, blocks) in c {
                            let _ = prepare_chain(blocks, &[], SAMPLE_RATE);
                        }
                    })
                })
                .collect();
            for h in handles {
                let _ = h.join();
            }
        });
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        println!(
            "  parallel, {threads:>2} threads   {ms:>9.1} ms   ({:.2}x)",
            chains_serial / ms.max(0.001)
        );
    }
}
