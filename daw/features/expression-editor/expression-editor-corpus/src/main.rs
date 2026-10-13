//! `drum-corpus` — the build-time tool `fetch-corpus.sh` drives.
//!
//! Everything here operates on material outside the tree. The script
//! handles the network and the GPL `drumgizmo` renderer; this handles
//! the parts that need to know what a flam is.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use expression_editor_audio::onsets::OnsetConfig;
use expression_editor_corpus::enst::{self, FLAM_WINDOW_MS, FlamEvidence};
use expression_editor_corpus::flam::{FlamSweep, Side, parse_truth_csv};
use expression_editor_corpus::recall::{self, Tolerance, flam_config};
use expression_editor_corpus::{DRUMGIZMO_ATTRIBUTION, wav};

#[derive(Parser)]
#[command(
    name = "drum-corpus",
    about = "Build the drum corpus. Ship none of it."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Report the WAV headers under a path — the kit pages state
    /// neither sample rate nor bit depth.
    Probe {
        path: PathBuf,
        /// List every file rather than the distinct headers.
        #[arg(long)]
        all: bool,
    },
    /// Write the flam sweep as MIDI, for rendering through a real kit.
    SweepMidi {
        #[arg(long, default_value = "flam-sweep.mid")]
        out: PathBuf,
        #[arg(long, value_delimiter = ',')]
        spacings_ms: Option<Vec<f64>>,
    },
    /// Write the flam sweep's ground truth as CSV.
    SweepTruth {
        #[arg(long, default_value = "flam-sweep.csv")]
        out: PathBuf,
        #[arg(long, value_delimiter = ',')]
        spacings_ms: Option<Vec<f64>>,
    },
    /// Render the flam sweep with the synthetic snare.
    SweepWav {
        #[arg(long, default_value = "flam-sweep.wav")]
        out: PathBuf,
        #[arg(long, value_delimiter = ',')]
        spacings_ms: Option<Vec<f64>>,
    },
    /// Measure the flam-recall curve.
    ///
    /// With no `--wav`, measures the synthetic sweep, which is what the
    /// committed baseline records. With one, measures a real render and
    /// needs the matching `--truth`.
    Recall {
        #[arg(long)]
        wav: Option<PathBuf>,
        #[arg(long)]
        truth: Option<PathBuf>,
        /// Which mic. Omit to sum, which is usually wrong — see
        /// `wav::read_channel`.
        #[arg(long)]
        channel: Option<usize>,
        /// Detector's minimum spacing, in ms. The default 50 forbids
        /// every flam before the audio is looked at.
        #[arg(long, default_value_t = 3.0)]
        min_spacing_ms: f64,
        #[arg(long)]
        threshold: Option<f64>,
        /// How far before its strike a detection may land, in ms.
        #[arg(long, default_value_t = Tolerance::default().early_secs * 1000.0)]
        tolerance_early_ms: f64,
        /// And how far after. Wider, because spectral flux lags.
        #[arg(long, default_value_t = Tolerance::default().late_secs * 1000.0)]
        tolerance_late_ms: f64,
        /// Write the curve here as CSV.
        #[arg(long)]
        csv: Option<PathBuf>,
        /// Override the spacing axis, in ms. The ghost-after-accent
        /// series never resolves inside the sweep's 5–60 ms range, so
        /// finding where it does means asking for wider spacings.
        #[arg(long, value_delimiter = ',')]
        spacings_ms: Option<Vec<f64>>,
    },
    /// Histogram inter-onset intervals in an ENST annotation tree, to
    /// test whether flams are visible in it at all.
    Enst {
        /// Directory of annotation `.txt` files, or one file.
        path: PathBuf,
        #[arg(long, default_value = "sd")]
        label: String,
        /// Tempo of the material, for the subdivision confound check.
        #[arg(long, default_value_t = 120.0)]
        bpm: f64,
        /// Shortest subdivision the material plausibly plays, as a
        /// divisor of the quarter note. 8 is 32nd notes.
        #[arg(long, default_value_t = 4.0)]
        subdivision: f64,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("drum-corpus: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Command::Probe { path, all } => probe(&path, all),
        Command::SweepMidi { out, spacings_ms } => {
            let cases = sweep(spacings_ms).cases();
            let file = std::fs::File::create(&out).map_err(|e| e.to_string())?;
            expression_editor_corpus::smf::write_sweep(&cases, file).map_err(|e| e.to_string())?;
            println!("{} cases → {}", cases.len(), out.display());
            Ok(())
        }
        Command::SweepTruth { out, spacings_ms } => {
            let rendered = sweep(spacings_ms).render();
            std::fs::write(&out, rendered.truth_csv()).map_err(|e| e.to_string())?;
            println!("{} cases → {}", rendered.cases.len(), out.display());
            Ok(())
        }
        Command::SweepWav { out, spacings_ms } => {
            let rendered = sweep(spacings_ms).render();
            wav::write_mono(&out, &rendered.samples, rendered.sample_rate)
                .map_err(|e| e.to_string())?;
            println!(
                "{:.1}s, {} cases → {}",
                rendered.samples.len() as f64 / rendered.sample_rate,
                rendered.cases.len(),
                out.display()
            );
            Ok(())
        }
        Command::Recall {
            wav: wav_path,
            truth,
            channel,
            min_spacing_ms,
            threshold,
            tolerance_early_ms,
            tolerance_late_ms,
            csv,
            spacings_ms,
        } => {
            let mut cfg = OnsetConfig {
                min_spacing_secs: min_spacing_ms / 1000.0,
                ..flam_config()
            };
            if let Some(t) = threshold {
                cfg.threshold = t;
            }

            let (samples, rate, cases) = match wav_path {
                Some(path) => {
                    let truth =
                        truth.ok_or("--wav needs --truth: the ground truth is not in the audio")?;
                    let text = std::fs::read_to_string(&truth).map_err(|e| e.to_string())?;
                    let cases = parse_truth_csv(&text)?;
                    let (samples, rate) =
                        wav::read_channel(&path, channel).map_err(|e| e.to_string())?;
                    println!(
                        "{} — {:.1}s at {rate} Hz, {} cases",
                        path.display(),
                        samples.len() as f64 / rate,
                        cases.len()
                    );
                    println!("{DRUMGIZMO_ATTRIBUTION}");
                    (samples, rate, cases)
                }
                None => {
                    let r = sweep(spacings_ms).render();
                    println!(
                        "synthetic sweep — {:.1}s, {} cases",
                        r.samples.len() as f64 / r.sample_rate,
                        r.cases.len()
                    );
                    (r.samples, r.sample_rate, r.cases)
                }
            };

            let tolerance = Tolerance {
                early_secs: tolerance_early_ms / 1000.0,
                late_secs: tolerance_late_ms / 1000.0,
            };
            let results = recall::measure(&samples, rate, &cases, cfg, tolerance);
            let curve = recall::recall_curve(&results);
            print_curve(&curve);
            let lag = recall::accent_lag(&results);
            println!(
                "  accent lag: median {:.1} ms, worst {:.1} ms over {} strikes",
                lag.median_ms, lag.worst_ms, lag.matched
            );
            if let Some(path) = csv {
                std::fs::write(&path, curve.to_csv()).map_err(|e| e.to_string())?;
                println!("curve → {}", path.display());
            }
            Ok(())
        }
        Command::Enst {
            path,
            label,
            bpm,
            subdivision,
        } => enst_report(&path, &label, bpm, subdivision),
    }
}

/// The sweep, with the spacing axis optionally overridden.
fn sweep(spacings_ms: Option<Vec<f64>>) -> FlamSweep {
    match spacings_ms {
        Some(spacings_ms) if !spacings_ms.is_empty() => FlamSweep {
            spacings_ms,
            ..FlamSweep::default()
        },
        _ => FlamSweep::default(),
    }
}

fn probe(path: &std::path::Path, all: bool) -> Result<(), String> {
    let probed = wav::probe_tree(path).map_err(|e| e.to_string())?;
    if probed.is_empty() {
        return Err(format!("no .wav files under {}", path.display()));
    }
    if all {
        for (p, h) in &probed {
            println!("{h:?}  {}", p.display());
        }
    }
    println!("{} files under {}", probed.len(), path.display());
    for (h, n) in wav::summarize(&probed) {
        println!(
            "  {n:>5} × {} ch, {} Hz, {}-bit {:?}",
            h.channels, h.sample_rate, h.bits_per_sample, h.format
        );
    }
    println!("{DRUMGIZMO_ATTRIBUTION}");
    Ok(())
}

fn print_curve(curve: &recall::Curve) {
    println!();
    println!("  side    spacing    flam    ghost   accent");
    let mut last: Option<Side> = None;
    for p in &curve.0 {
        if last != Some(p.side) {
            println!();
            last = Some(p.side);
        }
        println!(
            "  {:<7} {:>5.0} ms   {:>4.0}%    {:>4.0}%    {:>4.0}%   ({}/{})",
            p.side.as_str(),
            p.spacing_ms,
            p.flam_recall() * 100.0,
            p.ghost_recall() * 100.0,
            p.accent_recall() * 100.0,
            p.both_found,
            p.cases
        );
    }
    println!();
    for side in [Side::Before, Side::After] {
        match curve.knee_ms(side, 1.0) {
            Some(ms) => println!("  {}: both strikes resolved from {ms} ms up", side.as_str()),
            None => println!(
                "  {}: never resolves both strikes in 5–60 ms",
                side.as_str()
            ),
        }
    }
}

fn enst_report(
    path: &std::path::Path,
    label: &str,
    bpm: f64,
    subdivision: f64,
) -> Result<(), String> {
    eprintln!(
        "ENST-Drums is CC BY-NC-ND 4.0: internal evaluation only. \
         Nothing read here may be vendored, shipped, or derived into a release asset."
    );
    let mut files = Vec::new();
    if path.is_dir() {
        collect_txt(path, &mut files)?;
    } else {
        files.push(path.to_path_buf());
    }
    files.sort();
    if files.is_empty() {
        return Err(format!("no .txt annotations under {}", path.display()));
    }

    let mut all = Vec::new();
    for f in &files {
        let text = std::fs::read_to_string(f).map_err(|e| format!("{}: {e}", f.display()))?;
        let onsets = enst::parse(&text).map_err(|e| format!("{}: {e}", f.display()))?;
        all.extend(enst::intervals_ms(&enst::times_for(&onsets, label)));
    }

    let hist = enst::histogram(&all, 5.0, 100.0);
    println!("{} files, {} `{label}` intervals", files.len(), all.len());
    print!("{}", hist.render(40));
    let evidence = FlamEvidence::measure(&all, FLAM_WINDOW_MS);
    println!(
        "\n{} of {} intervals in {:.0}–{:.0} ms ({:.2}%) — verdict at {bpm} bpm, 1/{}: {:?}",
        evidence.candidates,
        evidence.total,
        FLAM_WINDOW_MS.0,
        FLAM_WINDOW_MS.1,
        evidence.fraction() * 100.0,
        (subdivision * 4.0) as u32,
        evidence.verdict(bpm, subdivision)
    );
    Ok(())
}

fn collect_txt(dir: &std::path::Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            collect_txt(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "txt") {
            out.push(path);
        }
    }
    Ok(())
}
