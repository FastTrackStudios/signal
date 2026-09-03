//! Find reference material where the kick actually owns some low end.
//!
//! ```text
//! cargo run -p signal-separator --example scan_kick_bass -- /…/cambridge-mt
//! ```
//!
//! Written because the first song reached for had a subordinate kick —
//! bass won every band from 25 Hz to 570 Hz, so there was no handover to
//! measure and nothing to learn about how the two are tuned together.
//! Picking by genre is guesswork; this measures.
//!
//! Reads a window from the middle of each song rather than the whole
//! thing: intros and outros are unrepresentative, and thirty full
//! multitracks is a lot of I/O to answer a ranking question.
//!
//! # Point this at SEPARATED STEMS, not raw multitracks
//!
//! Run across the Cambridge library this reports 0% for all 54 songs —
//! not a bug, and not a property of the records. Those are *unmixed*
//! multitracks, where the level of a kick mic relative to a bass DI is
//! set by preamp gain and mic placement, and no one has balanced
//! anything yet. Measured raw, the kick sits 7-12 dB below the bass on
//! average while their PEAKS are within half a decibel — the signature
//! of a transient, not of a quiet kick.
//!
//! Summing a multitrack at unity is not a mix either, so ground-truth
//! stems built that way answer the same wrong question.
//!
//! Ownership is a property of a *mix*. Ask it of separated stems from
//! released records. Doing that on a mixed record shows what the raw
//! tracks cannot: kick and bass sharing 40-63 Hz to within a decibel,
//! then bass taking everything above roughly 80 Hz.
//!
//! The raw-multitrack corpus remains the right thing for *validating*
//! separation, because it is the only place ground truth exists. It is
//! simply the wrong place to ask what a mix does.

use std::path::{Path, PathBuf};

use signal_analyzer::elements;

/// Seconds analysed, taken from the middle of the song.
const WINDOW_S: f64 = 60.0;

/// The region where a kick and a bass actually compete.
const LOW_HZ: (f64, f64) = (35.0, 160.0);

fn main() {
    let root = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: scan_kick_bass <cambridge-mt dir>");
        std::process::exit(2);
    });

    let mut rows = Vec::new();
    let Ok(entries) = std::fs::read_dir(&root) else {
        eprintln!("cannot read {root}");
        std::process::exit(1);
    };

    for entry in entries.filter_map(Result::ok) {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let (Some(kick), Some(bass)) = (find(&dir, &["kick"], &[]), find(&dir, &["bass"], &["kick", "drum"]))
        else {
            continue;
        };

        let (Some((k, sr)), Some((b, _))) = (read_window(&kick), read_window(&bass)) else {
            continue;
        };
        if k.len() < 4096 || b.len() < 4096 {
            continue;
        }

        let d = elements::dominance(&k, &b, sr);
        let low: Vec<_> = d
            .iter()
            .filter(|x| (LOW_HZ.0..=LOW_HZ.1).contains(&x.centre_hz) && x.first_wins.is_finite())
            .collect();
        if low.is_empty() {
            continue;
        }

        // Share of the low region the kick owns, and how decisively.
        let owned = low.iter().filter(|x| x.first_wins > 0.5).count() as f64 / low.len() as f64;
        let median_margin =
            low.iter().map(|x| x.median_margin_db).sum::<f64>() / low.len() as f64;

        rows.push((
            dir.file_name().unwrap().to_string_lossy().into_owned(),
            owned,
            median_margin,
        ));
    }

    rows.sort_by(|a, b| b.1.total_cmp(&a.1).then(b.2.total_cmp(&a.2)));

    println!("{:<38}{:>12}{:>14}", "song", "kick owns", "mean margin");
    println!("{}", "-".repeat(64));
    for (name, owned, margin) in &rows {
        println!("{:<38}{:>11.0}%{:>+14.1}", trunc(name, 37), owned * 100.0, margin);
    }
    println!("{}", "-".repeat(64));
    println!(
        "  {} songs with both a kick mic and a bass track, {}–{} Hz,",
        rows.len(),
        LOW_HZ.0 as i32,
        LOW_HZ.1 as i32
    );
    println!("  measured over {WINDOW_S:.0}s from the middle of each.");
    println!("  'kick owns' is the share of those bands where the kick leads;");
    println!("  a song near 0% has nothing to say about kick/bass handover.");
}

/// First WAV in the directory whose name contains every term in `want`
/// and none in `avoid`.
fn find(dir: &Path, want: &[&str], avoid: &[&str]) -> Option<PathBuf> {
    let mut best: Option<PathBuf> = None;
    for sub in [dir.to_path_buf()]
        .into_iter()
        .chain(std::fs::read_dir(dir).ok()?.filter_map(|e| {
            let p = e.ok()?.path();
            p.is_dir().then_some(p)
        }))
    {
        let Ok(files) = std::fs::read_dir(&sub) else {
            continue;
        };
        for f in files.filter_map(Result::ok) {
            let p = f.path();
            if p.extension().and_then(|e| e.to_str()).map(str::to_lowercase) != Some("wav".into()) {
                continue;
            }
            let name = p.file_name()?.to_string_lossy().to_lowercase();
            if want.iter().all(|w| name.contains(w)) && !avoid.iter().any(|a| name.contains(a)) {
                // Prefer the earliest-numbered match, which in this
                // dataset is the close mic rather than a room or a sub.
                if best.as_ref().is_none_or(|b| {
                    p.file_name().unwrap() < b.file_name().unwrap()
                }) {
                    best = Some(p);
                }
            }
        }
    }
    best
}

/// Read a mono window from the middle of a WAV.
fn read_window(path: &Path) -> Option<(Vec<f32>, f64)> {
    let mut r = hound::WavReader::open(path).ok()?;
    let spec = r.spec();
    let ch = spec.channels.max(1) as usize;
    let sr = spec.sample_rate as f64;

    let total_frames = r.len() as usize / ch;
    let want = (sr * WINDOW_S) as usize;
    let start = total_frames.saturating_sub(want) / 2;
    r.seek(start as u32).ok()?;

    let take = want.min(total_frames.saturating_sub(start)) * ch;
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().take(take).filter_map(Result::ok).collect(),
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1i64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>()
                .take(take)
                .filter_map(Result::ok)
                .map(|s| s as f32 * scale)
                .collect()
        }
    };
    let mono: Vec<f32> = samples
        .chunks(ch)
        .map(|f| f.iter().sum::<f32>() / ch as f32)
        .collect();
    Some((mono, sr))
}

fn trunc(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n - 1).collect::<String>() + "…"
    }
}
