//! How many instances of a plugin can this host actually run at once?
//!
//! The question behind it is whether Signal's host can carry a real session's
//! worth of plugins — a hundred compressors is an ordinary mix, and any DAW
//! does it without thinking. This loads `--count` instances, renders audio
//! through all of them across `--threads` worker threads, and reports the
//! realtime factor: how many seconds of audio came out per second of wall
//! clock. Below 1.0 the session would not play.
//!
//! ```sh
//! cargo run --release -p signal-plugin-host --example pool_stress -- \
//!     --plugin ~/.clap/"FTS Comp.clap" --count 100 --threads 8
//! ```
//!
//! `--load serial|parallel` selects how the instances are created, which is
//! the thing worth measuring separately: CLAP requires the plugin factory to
//! be entered on one thread at a time, and the bundle itself is cached
//! process-wide, so creation and processing have very different scaling.

use std::path::PathBuf;
use std::time::Instant;

use signal_plugin_host::PluginPool;

fn arg(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1).cloned())
}

fn num<T: std::str::FromStr>(name: &str, default: T) -> T {
    arg(name).and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn main() {
    let Some(path) = arg("--plugin") else {
        eprintln!(
            "usage: pool_stress --plugin <path> [--count 100] [--threads N] \
             [--load serial|parallel] [--blocks 2000] [--block 512]"
        );
        std::process::exit(2);
    };
    let count: usize = num("--count", 100);
    let threads: usize = num("--threads", std::thread::available_parallelism().map(|n| n.get()).unwrap_or(8));
    let blocks: usize = num("--blocks", 2000);
    let block: usize = num("--block", 512);
    let sample_rate: f64 = num("--sample-rate", 48_000.0);
    let parallel = arg("--load").as_deref() != Some("serial");

    let started = Instant::now();
    let pool = match PluginPool::open(PathBuf::from(&path), count, sample_rate, block as u32, parallel) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{path}: {e:?}");
            std::process::exit(1);
        }
    };
    let load_time = started.elapsed();

    println!("{}  ({})", pool.descriptor().name, if parallel { "parallel load" } else { "serial load" });
    println!(
        "{} instances in {:.2}s  ({:.1} ms each)",
        pool.len(),
        load_time.as_secs_f64(),
        load_time.as_secs_f64() * 1000.0 / pool.len().max(1) as f64
    );

    // A quarter-scale sine keeps every instance doing real work rather than
    // short-circuiting on silence — several compressors skip their detector
    // entirely on a zero block, which would flatter the numbers.
    let stimulus: Vec<f32> = (0..block * 2)
        .map(|i| (0.25 * ((i / 2) as f64 * 0.05).sin()) as f32)
        .collect();

    let mut instances = pool.into_instances();
    let per_thread = instances.len().div_ceil(threads.max(1));

    let started = Instant::now();
    std::thread::scope(|s| {
        for group in instances.chunks_mut(per_thread.max(1)) {
            let stimulus = stimulus.clone();
            s.spawn(move || {
                let mut buf = vec![0.0f32; block * 2];
                for _ in 0..blocks {
                    for inst in group.iter_mut() {
                        buf.copy_from_slice(&stimulus);
                        let _ = inst.process_interleaved(&mut buf, &[], &[]);
                    }
                }
            });
        }
    });
    let elapsed = started.elapsed();

    let audio_seconds = (blocks * block) as f64 / sample_rate;
    let total_audio = audio_seconds * instances.len() as f64;
    println!(
        "rendered {:.1}s of audio through {} instances on {} threads in {:.2}s",
        audio_seconds,
        instances.len(),
        threads,
        elapsed.as_secs_f64()
    );
    println!(
        "realtime factor {:.1}x  ({:.1}x total across instances)",
        audio_seconds / elapsed.as_secs_f64(),
        total_audio / elapsed.as_secs_f64()
    );
    let headroom = audio_seconds / elapsed.as_secs_f64();
    println!(
        "{}",
        if headroom >= 1.0 {
            format!("this session would play, with {headroom:.1}x headroom")
        } else {
            format!("this session would NOT play — {:.0}% of realtime", headroom * 100.0)
        }
    );
    // Keep instances alive to here so teardown is not timed above.
    drop(instances);
}
