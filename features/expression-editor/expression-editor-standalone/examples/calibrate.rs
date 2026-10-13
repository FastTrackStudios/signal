//! Compute the detector's default settings instead of guessing them.
//!
//! ```sh
//! cargo run -p expression-editor-standalone --example calibrate -- \
//!     "/path/to/album"
//! ```
//!
//! Every default in the detect chain was, until this existed, a number
//! somebody picked. That is how the shipped sensitivity of 0.5 came to
//! find about a fifth of what a drummer played without anyone noticing:
//! nothing measured it, so nothing contradicted it.
//!
//! # The two references
//!
//! **Drum MIDI**, where a project has it. It is not an exact
//! transcription of the audio — takes differ, and the drummer played
//! different fills — so the absolute F1 it reports is pessimistic and
//! should not be read as "the detector is 60% right". What it *is* good
//! for is comparing settings, because the same mismatch applies equally
//! to every setting on the sweep. A ranking survives a biased reference;
//! an absolute score does not.
//!
//! **Section placement**, for the fill threshold, which has no ground
//! truth at all. A drummer fills into a section, so a threshold is
//! scored on how much more often its fills land at a section boundary
//! than a bar picked at random does. The "than random" half is the
//! whole metric: roughly a third of each of these songs is already
//! within a bar of some marker, so a detector firing blindly scores 30%
//! and looks fine.
//!
//! # Reading the output
//!
//! The tool prints the sweep and names the argmax. It does not write
//! anything: a default is a decision, and the point is to make it an
//! informed one, not an automatic one.

use std::path::{Path, PathBuf};

use expression_editor_core::Viewport;
use expression_editor_core::fills::FillConfig;
use expression_editor_standalone::{Loaded, Runner, Source, Target};
use expression_editor_ui::quantize_panel::QuantizePanel;

/// How close a detected hit must be to a reference onset to count.
///
/// ±25 ms is the standard onset-detection evaluation window (Böck &
/// Widmer, DAFx-13, following the MIREX convention), so the numbers
/// here can be read against published results rather than only against
/// each other.
const MATCH_TOL: f64 = 0.025;

/// Reference onsets closer together than this are one event.
///
/// 30 ms, the same convention's `combination_width`: a kick and a crash
/// struck together are one moment, and counting them twice would make a
/// detector that finds the moment look like it missed one.
const COMBINE: f64 = 0.030;

struct Project {
    name: String,
    host: std::sync::Arc<expression_editor_standalone::drum_host::DrumHost>,
    /// Drum MIDI onsets in seconds, when the project has usable ones.
    reference: Vec<f64>,
    /// Section boundaries in seconds, for scoring the fill threshold.
    sections: Vec<f64>,
    bars: Vec<f64>,
}

fn main() {
    let base = std::env::args().nth(1).unwrap_or_else(|| {
        "/run/media/AudioHaven/Project/Crescendum-Rockstars-SESSION-BACKUP-2026-09-06/Crescendum"
            .to_string()
    });
    let base = PathBuf::from(base);
    if !base.exists() {
        eprintln!("no such directory: {}", base.display());
        std::process::exit(1);
    }

    // Load once; the sweep re-detects in memory, which is cheap next to
    // opening a project and reading its audio.
    println!("loading projects from {}…", base.display());
    let mut projects = Vec::new();
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&base)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    for dir in dirs {
        // A project that cannot be opened must not take the sweep down
        // with it. One of these panics inside the daw's `Duration` on
        // load, and a calibration run that dies partway is worse than
        // one that reports which project it could not read.
        let opened = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| load(&dir)));
        let opened = match opened {
            Ok(v) => v,
            Err(_) => {
                println!(
                    "  {:<24} FAILED TO OPEN (panicked)",
                    dir.file_name().unwrap_or_default().to_string_lossy()
                );
                continue;
            }
        };
        if let Some(p) = opened {
            println!(
                "  {:<24} bars {:<4} midi-reference {}",
                p.name,
                p.bars.len().saturating_sub(1),
                if p.reference.is_empty() {
                    "none".to_string()
                } else {
                    format!("{} onsets", p.reference.len())
                }
            );
            projects.push(p);
        }
    }
    if projects.is_empty() {
        eprintln!("no drum projects loaded");
        std::process::exit(1);
    }

    sweep_detect(&projects);
    sweep_fill_threshold(&projects);
}

// ── loading ──────────────────────────────────────────────────────────

fn load(dir: &Path) -> Option<Project> {
    let name = dir.file_name()?.to_string_lossy().trim().to_string();
    let rpp = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| {
            p.extension().is_some_and(|e| e.eq_ignore_ascii_case("rpp"))
                && p.to_string_lossy().contains(".organized.")
        })?;

    let runner = Runner::open(
        &Source::Rpp(rpp.clone()),
        &Target {
            drums: Some(None),
            ..Target::default()
        },
        Viewport {
            w: 1600.0,
            h: 900.0,
        },
        None,
    )
    .ok()?;
    let Loaded::DrumWorkspace(ed) = &runner.loaded else {
        return None;
    };
    let host = runner.host.clone()?;
    let bars = host.bar_grid_secs();
    if bars.len() < 2 {
        return None;
    }

    let ups = ed.doc.time_base.units_per_second(120.0);
    let mut sections: Vec<f64> = ed.doc.markers.iter().map(|m| m.t).collect();
    sections.extend(ed.doc.regions.iter().map(|r| r.start));
    if ups > 0.0 {
        for s in &mut sections {
            *s /= ups;
        }
    }

    Some(Project {
        reference: midi_reference(&rpp),
        name,
        host,
        sections,
        bars,
    })
}

/// Drum-MIDI note onsets, in seconds.
///
/// Read straight from the project file rather than through the daw: the
/// MIDI is a *reference*, not something being edited, and parsing the
/// events is less machinery than a take location and a PPQ range.
///
/// Returns nothing unless the project is at a single constant tempo in
/// 4/4. Converting ticks to seconds under a tempo map means walking it,
/// and a reference that is quietly wrong about *when* is worse than no
/// reference: it would score every setting badly and equally, which
/// looks exactly like a detector that cannot be tuned.
fn midi_reference(rpp: &Path) -> Vec<f64> {
    let Ok(text) = std::fs::read_to_string(rpp) else {
        return Vec::new();
    };
    let mut bpm = None;
    let mut tempo_points = 0usize;
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("TEMPO ") {
            let f: Vec<&str> = rest.split_whitespace().collect();
            if f.len() >= 3 {
                if f[1] != "4" || f[2] != "4" {
                    return Vec::new();
                }
                bpm = f[0].parse::<f64>().ok();
            }
        }
        // A point in the tempo envelope means the tempo moves.
        if t.starts_with("PT ") {
            tempo_points += 1;
        }
    }
    let (Some(bpm), 0) = (bpm, tempo_points) else {
        return Vec::new();
    };
    let spq = 60.0 / bpm;

    // Which track holds the MIDI, and how many items it has.
    let mut best: Vec<f64> = Vec::new();
    for take in 1..=64 {
        let notes = midi_take(&text, take, spq);
        if notes.len() > best.len() {
            best = notes;
        }
    }
    best
}

/// Note-on times of the `take`-th MIDI item on a drum-MIDI track.
///
/// One item, not all of them: these tracks carry thirty-odd items
/// stacked at position zero, which are alternate takes of one part
/// rather than a performance. Summing them reports every hit as many
/// times as it was tracked, at 40 events a second, which reads as a
/// detector missing 90% of the take.
fn midi_take(text: &str, take: usize, spq: f64) -> Vec<f64> {
    let mut out = Vec::new();
    let (mut track, mut pos, mut ppq, mut ticks) = (String::new(), 0.0f64, 960.0f64, 0i64);
    let (mut in_midi, mut item) = (false, 0usize);
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("<TRACK") {
            track.clear();
        }
        if line.starts_with("    NAME \"") {
            track = t[6..].trim_matches('"').to_string();
        }
        let drum_midi = {
            let n = track.to_ascii_lowercase();
            n.contains("midi") && (n.contains("drum") || n.contains("kit"))
        };
        if t == "<ITEM" {
            in_midi = false;
            ticks = 0;
        }
        if let Some(rest) = t.strip_prefix("POSITION ") {
            pos = rest
                .split_whitespace()
                .next()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.0);
        }
        if let Some(rest) = t.strip_prefix("HASDATA ") {
            ppq = rest
                .split_whitespace()
                .nth(1)
                .and_then(|v| v.parse().ok())
                .unwrap_or(960.0);
            in_midi = true;
            ticks = 0;
            if drum_midi {
                item += 1;
            }
        }
        if t == "<SOURCE WAVE" {
            in_midi = false;
        }
        if in_midi && drum_midi && item == take && t.starts_with("E ") {
            let f: Vec<&str> = t.split_whitespace().collect();
            if f.len() >= 5 {
                ticks += f[1].parse::<i64>().unwrap_or(0);
                let status = u8::from_str_radix(f[2], 16).unwrap_or(0);
                let vel = u8::from_str_radix(f[4], 16).unwrap_or(0);
                if status & 0xF0 == 0x90 && vel > 0 {
                    out.push(pos + ticks as f64 / ppq * spq);
                }
            }
        }
    }
    out.sort_by(f64::total_cmp);
    // Distinct moments: a kick and a crash on the same beat are one
    // event as far as "was something struck here" goes.
    out.dedup_by(|a, b| (*a - *b).abs() < COMBINE);
    out
}

// ── scoring ──────────────────────────────────────────────────────────

fn near(sorted: &[f64], t: f64, tol: f64) -> bool {
    let i = sorted.partition_point(|&x| x < t - tol);
    sorted.get(i).is_some_and(|&x| (x - t).abs() <= tol)
}

/// True positives under **one-to-one** matching: each reference onset
/// may be claimed by at most one detection, and vice versa.
///
/// The standard rule, and not a detail. Counting every detection that
/// merely sits near *some* reference onset lets one reference absolve a
/// whole burst of false positives, so a detector firing continuously
/// scores well — which is exactly the failure that made F1 rank
/// sensitivity 1.0 top while it fired thirty times a second. Both lists
/// are sorted, so a two-pointer walk is enough.
fn matched_pairs(detected: &[f64], reference: &[f64], tol: f64) -> usize {
    let (mut i, mut j, mut tp) = (0usize, 0usize, 0usize);
    while i < detected.len() && j < reference.len() {
        let d = detected[i] - reference[j];
        if d.abs() <= tol {
            tp += 1;
            i += 1;
            j += 1;
        } else if d < 0.0 {
            i += 1;
        } else {
            j += 1;
        }
    }
    tp
}

/// Precision, recall and F1 of `detected` against `reference`, over the
/// span where both have something to say.
fn score(detected: &[f64], reference: &[f64]) -> (f64, f64, f64) {
    if detected.is_empty() || reference.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    // Only judge where the audio take and the reference overlap; the
    // MIDI often runs past the end of the recorded performance.
    let lo = detected[0].max(reference[0]);
    let hi = detected[detected.len() - 1].min(reference[reference.len() - 1]);
    let d: Vec<f64> = detected
        .iter()
        .copied()
        .filter(|t| *t >= lo && *t <= hi)
        .collect();
    let r: Vec<f64> = reference
        .iter()
        .copied()
        .filter(|t| *t >= lo && *t <= hi)
        .collect();
    if d.is_empty() || r.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let tp = matched_pairs(&d, &r, MATCH_TOL);
    let precision = tp as f64 / d.len() as f64;
    let recall = tp as f64 / r.len() as f64;
    let f1 = if precision + recall > 0.0 {
        2.0 * precision * recall / (precision + recall)
    } else {
        0.0
    };
    (precision, recall, f1)
}

// ── the sweeps ───────────────────────────────────────────────────────

fn sweep_detect(projects: &[Project]) {
    let with_midi: Vec<&Project> = projects
        .iter()
        .filter(|p| !p.reference.is_empty())
        .collect();
    println!("\n── detect settings, against drum MIDI ──────────────────");
    if with_midi.is_empty() {
        println!("no project has a usable MIDI reference; skipping");
        return;
    }
    println!(
        "reference projects: {}",
        with_midi
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!("\n  sens crest retrig |  prec  recall     F1 | hits/s vs ref");

    // How often the drummer actually struck something, from the
    // reference. F1 alone cannot police over-detection here: the
    // reference is a different take, so precision is capped around 0.55
    // however good the detector is, and piling on spurious hits costs
    // almost nothing in F1 while recall keeps climbing. At sensitivity
    // 1.0 that yields thirty hits a second against a drummer playing
    // eleven, with an F1 indistinguishable from the sane settings.
    let ref_rate: f64 = with_midi
        .iter()
        .map(|p| {
            let span = p.bars.last().copied().unwrap_or(1.0).max(1.0);
            p.reference.len() as f64 / span
        })
        .sum::<f64>()
        / with_midi.len() as f64;
    println!("reference hit rate: {ref_rate:.1}/s");

    let mut rows: Vec<(f64, f64, String)> = Vec::new();
    for sens in [0.5, 0.8, 0.9, 0.95, 0.98, 1.0] {
        for crest in [0.0, 6.0] {
            for retrig in [0.010, 0.020] {
                let mut panel = QuantizePanel::default();
                panel.detect.sensitivity = sens;
                panel.detect.crest_db = crest;
                panel.detect.retrigger_secs = retrig;

                let (mut p, mut r, mut f, mut rate) = (0.0, 0.0, 0.0, 0.0);
                for proj in &with_midi {
                    let hits: Vec<f64> = proj
                        .host
                        .role_hits(&panel)
                        .into_iter()
                        .map(|(t, _)| t)
                        .collect();
                    let (pp, rr, ff) = score(&hits, &proj.reference);
                    p += pp;
                    r += rr;
                    f += ff;
                    let span = proj.bars.last().copied().unwrap_or(1.0).max(1.0);
                    rate += hits.len() as f64 / span;
                }
                let n = with_midi.len() as f64;
                let ratio = (rate / n) / ref_rate.max(0.001);
                let line = format!(
                    "  {sens:>4.2} {crest:>5.0} {:>6.0} | {:>5.2} {:>7.2} {:>6.3} | {:>6.1} {:>6.2}x",
                    retrig * 1000.0,
                    p / n,
                    r / n,
                    f / n,
                    rate / n,
                    ratio
                );
                println!("{line}");
                rows.push((f / n, ratio, line));
            }
        }
    }
    // The hybrid detector. It has no *sensitivity* — the flux stage
    // scores each frame against its own neighbourhood — but the flux
    // stage has settings of its own, and leaving those at a guess while
    // sweeping the gate's would be comparing a tuned detector against
    // an untuned one.
    println!("\n  flux: thresh median  hop |  prec  recall     F1 | hits/s vs ref");
    let mut hyb: Vec<(f64, String)> = Vec::new();
    for threshold in [0.06, 0.09, 0.12, 0.18, 0.25] {
        for median_frames in [8usize, 16, 24] {
            for hop in [128usize, 256] {
                let cfg = expression_editor_audio::hybrid::HybridConfig {
                    onsets: expression_editor_audio::onsets::OnsetConfig {
                        hop,
                        median_frames,
                        threshold,
                        ..expression_editor_audio::onsets::OnsetConfig::default()
                    },
                    ..expression_editor_audio::hybrid::HybridConfig::default()
                };
                let (mut p, mut r, mut f, mut rate) = (0.0, 0.0, 0.0, 0.0);
                for proj in &with_midi {
                    let hits: Vec<f64> = proj
                        .host
                        .role_hits_hybrid_with(&cfg)
                        .into_iter()
                        .map(|(t, _)| t)
                        .collect();
                    let (pp, rr, ff) = score(&hits, &proj.reference);
                    p += pp;
                    r += rr;
                    f += ff;
                    let span = proj.bars.last().copied().unwrap_or(1.0).max(1.0);
                    rate += hits.len() as f64 / span;
                }
                let n = with_midi.len() as f64;
                let line = format!(
                    "  {threshold:>11.2} {median_frames:>6} {hop:>4} | {:>5.2} {:>7.2} {:>6.3} | {:>6.1} {:>6.2}x",
                    p / n,
                    r / n,
                    f / n,
                    rate / n,
                    (rate / n) / ref_rate.max(0.001)
                );
                println!("{line}");
                hyb.push((f / n, line));
            }
        }
    }
    hyb.sort_by(|a, b| b.0.total_cmp(&a.0));
    println!("\n  best hybrid:\n{}", hyb[0].1);
    {
        let (mut p, mut r, mut f, mut rate) = (0.0, 0.0, 0.0, 0.0);
        for proj in &with_midi {
            let hits: Vec<f64> = proj
                .host
                .role_hits_hybrid()
                .into_iter()
                .map(|(t, _)| t)
                .collect();
            let (pp, rr, ff) = score(&hits, &proj.reference);
            p += pp;
            r += rr;
            f += ff;
            let span = proj.bars.last().copied().unwrap_or(1.0).max(1.0);
            rate += hits.len() as f64 / span;
        }
        let n = with_midi.len() as f64;
        println!(
            "  hybrid at shipped defaults      | {:>5.2} {:>7.2} {:>6.3} | {:>6.1} {:>6.2}x",
            p / n,
            r / n,
            f / n,
            rate / n,
            (rate / n) / ref_rate.max(0.001)
        );
    }

    rows.sort_by(|a, b| b.0.total_cmp(&a.0));
    println!("\n  best gate setting by F1 alone:\n{}", rows[0].2);
    // The one to take: best F1 among settings that do not invent hits.
    const MAX_RATE: f64 = 1.5;
    match rows.iter().find(|(_, ratio, _)| *ratio <= MAX_RATE) {
        Some((_, _, line)) => {
            println!("\n  best with hit rate within {MAX_RATE}x of the drummer:\n{line}")
        }
        None => println!("\n  every setting over-detects; widen the sweep downwards"),
    }
    let d = QuantizePanel::default().detect;
    println!(
        "  shipped default: sens {:.2} crest {:.0} retrig {:.0}ms",
        d.sensitivity,
        d.crest_db,
        d.retrigger_secs * 1000.0
    );
}

fn sweep_fill_threshold(projects: &[Project]) {
    println!("\n── fill threshold, against section placement ───────────");
    // Swept as a pair: detection sensitivity and the threshold are
    // coupled, since more hits raise every bar's score.
    println!("   sens thresh | fills |  lift over chance");
    let mut rows: Vec<(f64, f64, usize)> = Vec::new();
    for (hybrid, sens, threshold) in [(false, 0.9), (true, 0.9)]
        .into_iter()
        .flat_map(|(h, s)| [4.0, 5.0, 6.0, 7.0, 8.0, 10.0].map(|t| (h, s, t)))
    {
        let cfg = FillConfig {
            threshold,
            detect_sensitivity: sens,
            hybrid_detect: hybrid,
            ..FillConfig::default()
        };
        let (mut lift, mut total, mut n) = (0.0, 0usize, 0.0);
        for proj in projects {
            if proj.sections.len() < 3 {
                continue;
            }
            let fills = proj.host.fills(&cfg);
            if fills.is_empty() {
                continue;
            }
            // A bar of tolerance either side, as a fill leads *into* the
            // section it announces.
            let tol = 2.5;
            let near_sec = |a: f64, b: f64| {
                proj.sections
                    .iter()
                    .any(|s| (a - s).abs() <= tol || (b - s).abs() <= tol)
            };
            let hit = fills.iter().filter(|f| near_sec(f.start, f.end)).count();
            let bars_near = proj
                .bars
                .windows(2)
                .filter(|w| near_sec(w[0], w[1]))
                .count();
            let chance = bars_near as f64 / (proj.bars.len() - 1).max(1) as f64;
            if chance <= 0.0 {
                continue;
            }
            lift += (hit as f64 / fills.len() as f64) / chance;
            total += fills.len();
            n += 1.0;
        }
        if n > 0.0 {
            println!(
                "  {:<6} t{threshold:>4.1} | {total:>5} | {:>6.2}x   ({:.1} per song)",
                if hybrid { "hybrid" } else { "gate" },
                lift / n,
                total as f64 / n
            );
            rows.push((lift / n, threshold, total));
        }
    }
    rows.sort_by(|a, b| b.0.total_cmp(&a.0));
    if let Some((lift, threshold, total)) = rows.first() {
        println!("\n  best lift: threshold {threshold:.1} at {lift:.2}x ({total} fills)");
        println!(
            "  NOTE: lift is a precision proxy with no recall term, so it\n\
             \x20       rewards reporting fewer, safer fills. Read it next to\n\
             \x20       the per-song count before taking the argmax."
        );
        println!(
            "  shipped default: threshold {:.1}",
            FillConfig::default().threshold
        );
    }
}
