//! Compare chart-derived section timings against a legacy `manifest.json`,
//! to gauge the drift before retiring the manifest (issue #57).
//!
//! ```text
//! cargo run -p session --example compare_sections -- <songs-root> [slug ...]
//! ```
//! For each song folder that has both `chart.kf` and `manifest.json`, runs
//! [`session::setlist::chart_import::chart_to_layout`] on the chart and prints, per
//! section, the manifest's authored start/end vs the chart-derived ones,
//! plus the max absolute delta and duration difference.

use std::path::PathBuf;

use serde::Deserialize;
use session::keyflow::actions::SectionKind;
use session::setlist::chart_import::chart_to_layout;

#[derive(Deserialize)]
struct Manifest {
    #[serde(default)]
    duration_sec: f64,
    #[serde(default)]
    sections: Vec<Section>,
}

#[derive(Deserialize)]
struct Section {
    name: String,
    start_sec: f64,
    end_sec: f64,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(root) = args.first().map(PathBuf::from) else {
        eprintln!("usage: compare_sections <songs-root> [slug ...]");
        std::process::exit(2);
    };
    let only: Vec<String> = args.get(1..).unwrap_or(&[]).to_vec();

    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&root)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("chart.kf").is_file() && p.join("manifest.json").is_file())
        .collect();
    dirs.sort();

    for dir in dirs {
        let slug = dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if !only.is_empty() && !only.contains(&slug) {
            continue;
        }
        let chart = std::fs::read_to_string(dir.join("chart.kf")).unwrap_or_default();
        let manifest: Manifest = serde_json::from_str(
            &std::fs::read_to_string(dir.join("manifest.json")).unwrap_or_default(),
        )
        .unwrap_or(Manifest {
            duration_sec: 0.0,
            sections: vec![],
        });
        let layout = match chart_to_layout(&chart) {
            Ok(l) => l,
            Err(e) => {
                println!("{slug}: chart error: {e}\n");
                continue;
            }
        };
        // Drop the count-in from the chart's sections to align with the
        // manifest (which starts at the first musical section).
        let derived: Vec<_> = layout
            .sections
            .iter()
            .filter(|s| s.kind != SectionKind::CountIn)
            .collect();

        println!(
            "── {slug} ──  manifest {}s vs chart {:.1}s (Δ {:+.1}s)",
            manifest.duration_sec,
            layout.song_end_seconds,
            layout.song_end_seconds - manifest.duration_sec
        );
        println!(
            "   manifest sections: {}, chart sections: {}",
            manifest.sections.len(),
            derived.len()
        );
        let mut max_delta = 0.0f64;
        for (i, (ms, cs)) in manifest.sections.iter().zip(derived.iter()).enumerate() {
            let d = (ms.start_sec - cs.start_seconds)
                .abs()
                .max((ms.end_sec - cs.end_seconds).abs());
            max_delta = max_delta.max(d);
            if d > 1.0 {
                println!(
                    "   [{i}] {:<14} manifest {:>7.2}-{:>7.2}  chart {:>7.2}-{:>7.2}  Δ{:+.2}",
                    ms.name, ms.start_sec, ms.end_sec, cs.start_seconds, cs.end_seconds, d
                );
            }
        }
        println!("   max section delta: {max_delta:.2}s\n");
    }
}
