//! `pack_memory` — what a pack actually costs to play.
//!
//! Preloads N entries from a pack and reports the two numbers that matter:
//! the sampler's own charge (decoded bytes it is holding) and the process's
//! anonymous RSS. A raw-PCM pack should show ~0 for both however many samples
//! are "loaded" — those reads land in page cache, which the OS owns and can
//! evict — while a FLAC/Ogg pack pays for every one.
//!
//! ```bash
//! cargo run -p signal-sampler --release --example pack_memory -- <pack> [count]
//! ```

use signal_sampler::engine::budget;
use signal_sampler::engine::cache::{SampleCache, SignalPcmPack};

/// Anonymous (heap/stack) resident set — the part the process actually owns.
fn rss_anon_mb() -> f64 {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else { return f64::NAN };
    status
        .lines()
        .find(|l| l.starts_with("RssAnon:"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|kb| kb.parse::<f64>().ok())
        .map(|kb| kb / 1024.0)
        .unwrap_or(f64::NAN)
}

fn main() -> eyre::Result<()> {
    let path = std::env::args().nth(1).ok_or_else(|| eyre::eyre!("usage: pack_memory <pack> [count]"))?;
    let count: usize = std::env::args().nth(2).and_then(|c| c.parse().ok()).unwrap_or(64);

    let pack = SignalPcmPack::open(std::path::Path::new(&path))?;
    println!("pack   {} ({})", path, pack.kind_label());

    // Sorted, so two packs of the same library measure the SAME samples.
    let mut paths: Vec<std::path::PathBuf> = pack.entries_iter().map(|(p, _)| p.clone()).collect();
    paths.sort();
    paths.truncate(count);
    println!("samples {}", paths.len());

    let before_anon = rss_anon_mb();
    let before_charge = budget::used_bytes();

    let cache = SampleCache::with_pack(pack);
    let t0 = std::time::Instant::now();
    let stats = cache.preload(paths.iter().map(|p| p.as_path()));
    let load_ms = t0.elapsed().as_secs_f64() * 1000.0;

    // Touch every frame the way a voice would, so mapped pages are actually
    // faulted in — otherwise "free" would just mean "not read yet".
    let t1 = std::time::Instant::now();
    let mut acc = 0.0f64;
    for p in &paths {
        if let Some(data) = cache.get_loaded(p) {
            for f in 0..data.num_frames {
                let (l, r) = data.frame(f);
                acc += (l + r) as f64;
            }
        }
    }
    let read_ms = t1.elapsed().as_secs_f64() * 1000.0;

    // What the samples themselves say they are holding, right now.
    let live = |cache: &SampleCache| -> (usize, usize, usize) {
        let (mut streamed, mut other, mut bytes) = (0, 0, 0);
        for p in &paths {
            if let Some(d) = cache.get_loaded(p) {
                bytes += d.decoded_bytes();
                if d.is_streamed() { streamed += 1 } else { other += 1 }
            }
        }
        (streamed, other, bytes)
    };
    let (streamed, decoded, live_bytes) = live(&cache);
    println!("loaded  {} failed {} skipped {}", stats.loaded, stats.failed, stats.skipped);
    println!("mode    {streamed} streamed · {decoded} decoded whole");
    println!("live    {:.1} MB held by the samples", live_bytes as f64 / 1048576.0);
    println!("charged {:.1} MB (sampler's own accounting)", (budget::used_bytes() - before_charge) as f64 / 1048576.0);
    println!("anon    {:.1} MB → {:.1} MB  (Δ {:+.1} MB)", before_anon, rss_anon_mb(), rss_anon_mb() - before_anon);
    println!("preload {load_ms:.0} ms · full read {read_ms:.0} ms · checksum {acc:.3}");
    // Streamed samples shed their chunks once nobody is reading them: what is
    // left after a few seconds idle is the steady-state cost of having the
    // library open.
    std::thread::sleep(std::time::Duration::from_secs(6));
    let (_, _, idle_bytes) = live(&cache);
    println!(
        "idle    {:.1} MB held by the samples · anon {:.1} MB",
        idle_bytes as f64 / 1048576.0,
        rss_anon_mb(),
    );
    Ok(())
}
