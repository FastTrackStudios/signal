//! Run `eq_match` across a whole EQ preset library, in parallel.
//!
//! The single-preset probes answer "is this band right"; this answers the only
//! question the work is actually for — **can a mix engineer reach for a Pro-Q
//! preset and not be able to tell which engine rendered it**. It is also the
//! guard against the failure mode this project keeps hitting: a constant
//! fitted to one measurement that improves the band it was fitted to and makes
//! the library worse.
//!
//! ```sh
//! cargo build --release -p signal-analyzer --example eq_match --example eq_sweep
//! ./target/release/examples/eq_sweep \
//!     --plugin ~/.vst3/yabridge/"FabFilter Pro-Q 4.vst3" \
//!     --presets ~/Documents/FabFilter/Presets/"Pro-Q 4" \
//!     --jobs 8 --json sweep.json
//! ```
//!
//! One process per preset, like `preset_sweep`: each run hosts a live bridged
//! VST3, which makes no promises about being driven from several threads of
//! one process, and a preset that hangs the bridge then costs its own result
//! and nothing else.
//!
//! `--baseline <sweep.json>` diffs against an earlier run and lists what moved,
//! which is the only honest way to accept or reject a DSP change: the median
//! and the tail have gone in opposite directions more than once here.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

fn arg(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter()
        .position(|x| x == name)
        .and_then(|i| a.get(i + 1).cloned())
}

/// One preset's measurement.
#[derive(Debug, Clone)]
struct Outcome {
    name: String,
    mean: Option<f64>,
    worst: Option<f64>,
    worst_hz: Option<f64>,
    seconds: f64,
    note: Option<String>,
}

fn collect_presets(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    let mut items: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    items.sort();
    for path in items {
        if path.is_dir() {
            collect_presets(&path, out);
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("ffp"))
        {
            out.push(path);
        }
    }
}

/// Run one preset and parse the summary line out of `eq_match`'s report.
fn run_one(binary: &Path, plugin: &str, preset: &Path, extra: &[String]) -> Outcome {
    let name = preset
        .file_stem()
        .map_or_else(|| "<unnamed>".into(), |s| s.to_string_lossy().into_owned());
    let started = std::time::Instant::now();

    let mut cmd = Command::new(binary);
    cmd.arg("--plugin").arg(plugin).arg("--preset").arg(preset);
    for e in extra {
        cmd.arg(e);
    }
    let output = cmd.output();
    let seconds = started.elapsed().as_secs_f64();

    let Ok(output) = output else {
        return Outcome {
            name,
            mean: None,
            worst: None,
            worst_hz: None,
            seconds,
            note: Some("could not start eq_match".into()),
        };
    };

    let text = String::from_utf8_lossy(&output.stdout);
    // "worst 12.34 dB at 5000 Hz   mean 1.23 dB"
    let Some(line) = text.lines().find(|l| l.starts_with("worst ")) else {
        return Outcome {
            name,
            mean: None,
            worst: None,
            worst_hz: None,
            seconds,
            note: Some("no verdict".into()),
        };
    };
    let nums: Vec<f64> = line
        .split_whitespace()
        .filter_map(|t| t.parse::<f64>().ok())
        .collect();
    let note = if text.contains("is not processing") {
        Some("plugin passed the signal through".into())
    } else {
        None
    };
    Outcome {
        name,
        worst: nums.first().copied(),
        worst_hz: nums.get(1).copied(),
        mean: nums.get(2).copied(),
        seconds,
        note,
    }
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let i = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[i]
}

fn main() {
    let (Some(plugin), Some(presets)) = (arg("--plugin"), arg("--presets")) else {
        eprintln!(
            "usage: eq_sweep --plugin <path> --presets <dir> [--jobs n] [--limit n] \
             [--json out] [--baseline in] [--tonal]"
        );
        std::process::exit(2);
    };
    let jobs: usize = arg("--jobs").and_then(|v| v.parse().ok()).unwrap_or(8);
    let limit: usize = arg("--limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(usize::MAX);
    // Anything after `--` is handed to every `eq_match` run, so a whole-library
    // sweep can be taken at a different level or on a different stimulus
    // without teaching this program about either.
    let argv: Vec<String> = std::env::args().collect();
    let mut extra: Vec<String> = argv
        .iter()
        .position(|a| a == "--")
        .map(|i| argv[i + 1..].to_vec())
        .unwrap_or_default();
    if argv.iter().any(|a| a == "--tonal") {
        extra.push("--tonal".into());
    }

    let binary = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("eq_match")))
        .unwrap_or_else(|| PathBuf::from("eq_match"));
    if !binary.exists() {
        eprintln!("eq_match not found at {}", binary.display());
        eprintln!("build it first: cargo build --release -p signal-analyzer --example eq_match");
        std::process::exit(1);
    }

    let mut files = Vec::new();
    collect_presets(Path::new(&presets), &mut files);
    files.truncate(limit);
    if files.is_empty() {
        eprintln!("no .ffp presets under {presets}");
        std::process::exit(1);
    }
    println!("{} presets, {jobs} at a time", files.len());

    let files = Arc::new(files);
    let next = Arc::new(AtomicUsize::new(0));
    let done = Arc::new(AtomicUsize::new(0));
    let results: Arc<Mutex<Vec<Outcome>>> = Arc::new(Mutex::new(Vec::new()));

    std::thread::scope(|scope| {
        for _ in 0..jobs.max(1) {
            let (files, next, done, results) =
                (files.clone(), next.clone(), done.clone(), results.clone());
            let (plugin, binary, extra) = (plugin.clone(), binary.clone(), extra.clone());
            scope.spawn(move || {
                loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some(preset) = files.get(i) else { return };
                    let outcome = run_one(&binary, &plugin, preset, &extra);
                    let n = done.fetch_add(1, Ordering::SeqCst) + 1;
                    println!(
                        "  [{n:>3}/{}] {:<44} {}",
                        files.len(),
                        outcome.name,
                        match (outcome.mean, &outcome.note) {
                            (_, Some(note)) => note.clone(),
                            (Some(m), _) => format!("mean {m:>6.2} dB"),
                            _ => "—".into(),
                        }
                    );
                    results.lock().unwrap().push(outcome);
                }
            });
        }
    });

    let mut all = results.lock().unwrap().clone();
    all.sort_by(|a, b| {
        b.mean
            .unwrap_or(f64::INFINITY)
            .total_cmp(&a.mean.unwrap_or(f64::INFINITY))
    });

    let mut means: Vec<f64> = all.iter().filter_map(|o| o.mean).collect();
    means.sort_by(f64::total_cmp);
    let n = means.len();
    let under = |t: f64| means.iter().filter(|m| **m < t).count();

    println!("\n── library ──────────────────────────────────────────");
    println!("  measured           {n} of {}", all.len());
    println!("  median             {:.2} dB", percentile(&means, 0.5));
    println!(
        "  mean               {:.2} dB",
        means.iter().sum::<f64>() / n.max(1) as f64
    );
    println!("  90th percentile    {:.2} dB", percentile(&means, 0.9));
    println!(
        "  under 1 dB         {} ({:.0}%)",
        under(1.0),
        100.0 * under(1.0) as f64 / n.max(1) as f64
    );
    println!("  under 2 dB         {}", under(2.0));
    println!("  2 dB or worse      {}", n - under(2.0));
    println!("  above 3 dB         {}", n - under(3.0));

    println!("\n── worst twelve ─────────────────────────────────────");
    println!(
        "  {:<44} {:>8} {:>8} {:>9}",
        "preset", "mean", "worst", "at"
    );
    for o in all.iter().take(12) {
        println!(
            "  {:<44} {:>8} {:>8} {:>9}",
            o.name,
            o.mean.map_or_else(|| "—".into(), |v| format!("{v:.2}")),
            o.worst.map_or_else(|| "—".into(), |v| format!("{v:.2}")),
            o.worst_hz.map(|v| format!("{v:.0} Hz")).unwrap_or_default(),
        );
    }

    let failures: Vec<&Outcome> = all.iter().filter(|o| o.note.is_some()).collect();
    if !failures.is_empty() {
        println!("\n── no measurement ───────────────────────────────────");
        for o in &failures {
            println!("  {:<44} {}", o.name, o.note.clone().unwrap_or_default());
        }
    }

    if let Some(base) = arg("--baseline") {
        match std::fs::read(&base)
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        {
            Some(doc) => {
                let mut moved: Vec<(String, f64, f64)> = Vec::new();
                for o in &all {
                    let Some(mean) = o.mean else { continue };
                    let was = doc["presets"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .find(|p| p["preset"].as_str() == Some(o.name.as_str()))
                        .and_then(|p| p["mean_db"].as_f64());
                    if let Some(was) = was {
                        if (mean - was).abs() >= 0.05 {
                            moved.push((o.name.clone(), was, mean));
                        }
                    }
                }
                moved.sort_by(|a, b| (b.2 - b.1).total_cmp(&(a.2 - a.1)));
                println!("\n── against {base} ───────────────────────────");
                println!("  {} presets moved by 0.05 dB or more", moved.len());
                println!(
                    "  {:<44} {:>8} {:>8} {:>8}",
                    "preset", "was", "now", "delta"
                );
                for (name, was, now) in moved.iter().take(10).chain(moved.iter().rev().take(10)) {
                    println!("  {name:<44} {was:>8.2} {now:>8.2} {:>8.2}", now - was);
                }
                let better = moved.iter().filter(|(_, w, n)| n < w).count();
                println!("  better {better}, worse {}", moved.len() - better);
            }
            None => eprintln!("could not read baseline {base}"),
        }
    }

    if let Some(out) = arg("--json") {
        let doc = serde_json::json!({
            "count": n,
            "median_db": percentile(&means, 0.5),
            "under_1db": under(1.0),
            "under_2db": under(2.0),
            "above_3db": n - under(3.0),
            "presets": all.iter().map(|o| serde_json::json!({
                "preset": o.name,
                "mean_db": o.mean,
                "worst_db": o.worst,
                "worst_hz": o.worst_hz,
                "seconds": o.seconds,
                "note": o.note,
            })).collect::<Vec<_>>(),
        });
        let _ = std::fs::write(&out, serde_json::to_string_pretty(&doc).unwrap());
        println!("\nwrote {out}");
    }
}
