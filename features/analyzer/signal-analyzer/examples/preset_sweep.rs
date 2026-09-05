//! Run `reverb_match --tune` across a whole preset library, in parallel.
//!
//! ```text
//! cargo build -p signal-analyzer --example reverb_match --example preset_sweep
//! ./target/debug/examples/preset_sweep \
//!     --plugin "$HOME/.vst3/yabridge/ValhallaVintageVerb.vst3" \
//!     --presets "$HOME/.wine-audiohaven/drive_c/ProgramData/Valhalla DSP, LLC/ValhallaVintageVerb" \
//!     --jobs 8
//! ```
//!
//! Each preset runs as its own `reverb_match` process rather than a thread.
//! That is deliberate: every run hosts a live plugin instance, and a bridged
//! VST3 makes no promises about being driven from several threads of one
//! process at once. Processes also mean one preset that hangs or crashes the
//! bridge costs its own result and nothing else.
//!
//! `--save-dir` writes each run's tuned parameters and measurements as JSON —
//! that output is the translated preset, so it is worth keeping.
//! `--reference-cache` stores each reference render as a WAV and reuses it on
//! later runs, which makes a re-sweep reproducible and possible without the
//! plugin at all.
//!
//! Add `--tsv` for machine-readable output, and `--limit`/`--stride` to sweep
//! a sample rather than the whole library.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

fn arg(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn flag(name: &str) -> bool {
    std::env::args().any(|a| a == name)
}

/// One preset's outcome.
#[derive(Debug)]
struct Outcome {
    name: String,
    passed: bool,
    /// Worst per-band decay ratio error, if the comparison could measure one.
    error: Option<f64>,
    seconds: f64,
    /// Set when the run failed to produce a verdict at all.
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
            .is_some_and(|e| e.eq_ignore_ascii_case("vpreset"))
        {
            out.push(path);
        }
    }
}

/// Run one preset and parse the verdict out of `reverb_match`'s report.
fn run_one(
    binary: &Path,
    plugin: Option<&str>,
    preset: &Path,
    save_dir: Option<&str>,
    reference_cache: Option<&str>,
    target_error: Option<&str>,
) -> Outcome {
    let name = preset
        .file_stem()
        .map_or_else(|| "<unnamed>".into(), |s| s.to_string_lossy().into_owned());
    let started = std::time::Instant::now();

    let mut cmd = Command::new(binary);
    if let Some(plugin) = plugin {
        cmd.arg("--plugin").arg(plugin);
    }
    cmd.arg("--preset").arg(preset).arg("--tune");
    if let Some(dir) = save_dir {
        cmd.arg("--save-dir").arg(dir);
    }
    if let Some(dir) = reference_cache {
        cmd.arg("--reference-cache").arg(dir);
    }
    if let Some(t) = target_error {
        cmd.arg("--target-error").arg(t);
    }
    let output = cmd.output();

    let seconds = started.elapsed().as_secs_f64();

    let Ok(output) = output else {
        return Outcome {
            name,
            passed: false,
            error: None,
            seconds,
            note: Some("could not start reverb_match".into()),
        };
    };

    let text = String::from_utf8_lossy(&output.stdout);
    let Some(line) = text.lines().find(|l| l.trim_start().starts_with("Decay:")) else {
        // No verdict: the plugin failed to load, the reference was silent, or
        // the run died. Report it rather than counting it as a pass.
        return Outcome {
            name,
            passed: false,
            error: None,
            seconds,
            note: Some("no decay verdict".into()),
        };
    };

    let passed = line.contains(" pass ");
    let error = line
        .split("Some(")
        .nth(1)
        .and_then(|rest| rest.split(')').next())
        .and_then(|v| v.parse::<f64>().ok());

    Outcome {
        name,
        passed,
        error,
        seconds,
        note: None,
    }
}

fn main() {
    let Some(presets) = arg("--presets") else {
        eprintln!(
            "usage: preset_sweep --presets <dir> [--plugin <path>] \\\n         \
             [--jobs N] [--limit N] [--stride N] [--target-error E] \\\n         \
             [--save-dir <dir>] [--reference-cache <dir>] [--tsv] [--binary <path>]"
        );
        std::process::exit(2);
    };
    // The plugin is only needed to capture references. Re-tuning from a cache
    // is pure arithmetic — no wine, no bridge, no realtime deadline — which is
    // why it parallelizes to core count where a live sweep does not.
    let plugin = arg("--plugin");
    if plugin.is_none() && arg("--reference-cache").is_none() {
        eprintln!(
            "need either --plugin (to render references) or --reference-cache (to reuse them)"
        );
        std::process::exit(2);
    }

    let binary = arg("--binary").map_or_else(
        || {
            // Sibling of this example in the same target directory.
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("reverb_match")))
                .unwrap_or_else(|| PathBuf::from("reverb_match"))
        },
        PathBuf::from,
    );
    if !binary.exists() {
        eprintln!("reverb_match not found at {}", binary.display());
        eprintln!("build it first: cargo build -p signal-analyzer --example reverb_match");
        std::process::exit(1);
    }

    let mut all = Vec::new();
    collect_presets(Path::new(&presets), &mut all);
    let stride: usize = arg("--stride")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
        .max(1);
    let mut selected: Vec<PathBuf> = all.into_iter().step_by(stride).collect();
    if let Some(limit) = arg("--limit").and_then(|v| v.parse::<usize>().ok()) {
        selected.truncate(limit);
    }
    if selected.is_empty() {
        eprintln!("no .vpreset files under {presets}");
        std::process::exit(1);
    }

    let jobs: usize = arg("--jobs")
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(std::num::NonZero::get)
                .unwrap_or(4)
        })
        .clamp(1, 64);
    let tsv = flag("--tsv");
    // Passed through to every run: the tuned parameters are the deliverable,
    // and a cached reference makes a re-run reproducible (and possible at all
    // without the plugin).
    let save_dir = arg("--save-dir");
    let reference_cache = arg("--reference-cache");
    let target_error = arg("--target-error");

    if tsv {
        println!("preset\tverdict\terror\tseconds");
    } else {
        println!("sweeping {} presets, {jobs} at a time", selected.len());
    }

    let queue = Arc::new(Mutex::new(selected.into_iter()));
    let results = Arc::new(Mutex::new(Vec::<Outcome>::new()));
    let done = Arc::new(AtomicUsize::new(0));

    std::thread::scope(|scope| {
        for _ in 0..jobs {
            let queue = Arc::clone(&queue);
            let results = Arc::clone(&results);
            let done = Arc::clone(&done);
            let binary = binary.clone();
            let plugin = plugin.clone();
            let save_dir = save_dir.clone();
            let reference_cache = reference_cache.clone();
            let target_error = target_error.clone();
            scope.spawn(move || {
                loop {
                    let next = { queue.lock().unwrap().next() };
                    let Some(preset) = next else { break };
                    let outcome = run_one(
                        &binary,
                        plugin.as_deref(),
                        &preset,
                        save_dir.as_deref(),
                        reference_cache.as_deref(),
                        target_error.as_deref(),
                    );
                    done.fetch_add(1, Ordering::Relaxed);
                    // Print as they land so a long sweep shows progress.
                    if tsv {
                        println!(
                            "{}\t{}\t{}\t{:.1}",
                            outcome.name,
                            if outcome.passed { "pass" } else { "fail" },
                            outcome.error.map_or_else(
                                || outcome.note.clone().unwrap_or_default(),
                                |e| format!("{e:.4}")
                            ),
                            outcome.seconds
                        );
                    } else {
                        println!(
                            "  {:<44} {:<5} {:<9} {:>5.1}s",
                            outcome.name.chars().take(44).collect::<String>(),
                            if outcome.passed { "pass" } else { "FAIL" },
                            outcome.error.map_or_else(
                                || outcome.note.clone().unwrap_or_default(),
                                |e| format!("{e:.3}")
                            ),
                            outcome.seconds
                        );
                    }
                    results.lock().unwrap().push(outcome);
                }
            });
        }
    });

    let mut results = Arc::try_unwrap(results).unwrap().into_inner().unwrap();
    results.sort_by(|a, b| {
        b.error
            .unwrap_or(f64::INFINITY)
            .partial_cmp(&a.error.unwrap_or(f64::INFINITY))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let total = results.len();
    let passed = results.iter().filter(|r| r.passed).count();
    let unmeasured = results.iter().filter(|r| r.error.is_none()).count();
    let wall: f64 = results.iter().map(|r| r.seconds).sum();

    if tsv {
        return;
    }

    println!();
    println!("{passed}/{total} pass  ({unmeasured} without a verdict)");
    if passed < total {
        println!("\nworst:");
        for r in results.iter().filter(|r| !r.passed).take(15) {
            println!(
                "  {:<44} {}",
                r.name.chars().take(44).collect::<String>(),
                r.error
                    .map_or_else(|| r.note.clone().unwrap_or_default(), |e| format!("{e:.3}"))
            );
        }
    }
    println!("\n{wall:.0}s of work across {jobs} jobs");
}
