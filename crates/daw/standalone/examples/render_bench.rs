//! Real-time render benchmark: load an RPP and render it in the block size
//! the audio callback asks for, timing every block against its deadline.
//!
//! An audio callback fails on its slowest block, not its average, so this
//! reports the spread — p50, p99, the worst — next to the budget.
//!
//! ```sh
//! cargo run --release -p daw-standalone --features rpp-loader --example render_bench -- \
//!     song.RPP [start_seconds=30] [seconds=20] [block_frames=128]
//! ```
use daw_standalone::audio_engine::render::ProjectRenderer;
use daw_standalone::media_bay::ProjectRelativeResolver;
use daw_standalone::project_loader::load_rpp_via_bay;
use daw_standalone::sync::Standalone;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn arg<T: std::str::FromStr>(n: usize, default: T) -> T {
    std::env::args()
        .nth(n)
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

fn main() {
    let rpp = std::env::args()
        .nth(1)
        .expect("usage: render_bench <song.RPP> [start] [seconds] [block]");
    let at: f64 = arg(2, 30.0);
    let seconds: f64 = arg(3, 20.0);
    let block: usize = arg(4, 128);
    let sr = 48_000u32;

    let text = std::fs::read_to_string(&rpp).expect("read");
    let dir = PathBuf::from(&rpp)
        .parent()
        .map(PathBuf::from)
        .expect("a folder");
    let daw = Standalone::new();
    daw.media_bay()
        .set_file_resolver(Box::new(ProjectRelativeResolver::new(dir)));
    let t0 = Instant::now();
    let (proj, audio) = load_rpp_via_bay(&daw, "bench", &rpp, &text).expect("load");
    println!(
        "loaded {} tracks / {} sources in {:?} ({} failed)",
        proj.track_count,
        audio.loaded,
        t0.elapsed(),
        audio.failed.len()
    );

    let renderer = ProjectRenderer::new(&daw, &proj.project_guid, sr);
    let start = (at * f64::from(sr)) as u64;
    let count = ((seconds * f64::from(sr)) / block as f64) as u64;
    // One block first: the snapshot is built on the first render.
    let warm = Instant::now();
    let _ = renderer.render_block(start, block);
    println!("first block (snapshot build): {:?}", warm.elapsed());

    let mut times: Vec<Duration> = Vec::with_capacity(count as usize);
    for b in 0..count {
        let t = Instant::now();
        let _ = renderer.render_block(start + b * block as u64, block);
        times.push(t.elapsed());
    }
    let budget = Duration::from_secs_f64(block as f64 / f64::from(sr));
    let mut sorted = times.clone();
    sorted.sort();
    let pct = |p: f64| sorted[((sorted.len() as f64 - 1.0) * p) as usize];
    let total: Duration = times.iter().sum();
    let over = times.iter().filter(|t| **t > budget).count();
    println!(
        "{count} blocks of {block} @ {sr} Hz from {at}s: budget {budget:?}\n  \
         mean {:?}  p50 {:?}  p99 {:?}  max {:?}\n  \
         over budget: {over} ({:.2}%)  load: {:.0}% of real time",
        total / count as u32,
        pct(0.5),
        pct(0.99),
        sorted[sorted.len() - 1],
        100.0 * over as f64 / count as f64,
        100.0 * total.as_secs_f64() / (count as f64 * budget.as_secs_f64()),
    );
}
