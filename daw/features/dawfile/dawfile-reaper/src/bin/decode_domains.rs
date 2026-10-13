use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use dawfile_reaper::{DecodeOptions, ReaperProject, TrackParseOptions, parse_rpp_file};

#[derive(Clone, Copy)]
struct Scenario {
    name: &'static str,
    options: DecodeOptions,
}

fn default_fixture() -> PathBuf {
    PathBuf::from(
        "/Users/codywright/Music/Projects/Client work/TOM BROOKS/Goodness of God/Goodness of God/Goodness of God.RPP",
    )
}

fn parse_args() -> Result<(PathBuf, usize), String> {
    let mut fixture: Option<PathBuf> = None;
    let mut repeat = 5usize;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--fixture" => {
                let value = args
                    .next()
                    .ok_or("--fixture expects a file path".to_string())?;
                fixture = Some(PathBuf::from(value));
            }
            "--repeat" => {
                let value = args
                    .next()
                    .ok_or("--repeat expects an integer".to_string())?;
                repeat = value
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --repeat value: {value}"))?;
            }
            "--help" | "-h" => {
                println!("decode_domains - typed decode domain profiler");
                println!("Usage:");
                println!(
                    "  cargo run -p dawfile-reaper --release --bin decode_domains -- [options]"
                );
                println!("Options:");
                println!("  --fixture <path>  RPP fixture path");
                println!("  --repeat <n>      Decode repeats per scenario (default: 5)");
                std::process::exit(0);
            }
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }

    Ok((fixture.unwrap_or_else(default_fixture), repeat))
}

fn count_nested(project: &ReaperProject) -> (usize, usize, usize) {
    let mut items = 0usize;
    let mut envs = 0usize;
    let mut fx = 0usize;
    for t in &project.tracks {
        items += t.items.len();
        envs += t.envelopes.len();
        if t.fx_chain.is_some() {
            fx += 1;
        }
        if t.input_fx.is_some() {
            fx += 1;
        }
    }
    (items, envs, fx)
}

fn mean(v: &[f64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.iter().sum::<f64>() / v.len() as f64
}

fn build_scenarios() -> Vec<Scenario> {
    let mut tracks_meta = DecodeOptions::summary();
    tracks_meta.parse_markers_regions = false;
    tracks_meta.parse_tempo_envelope = false;
    tracks_meta.parse_project_items = false;
    tracks_meta.parse_project_envelopes = false;
    tracks_meta.parse_project_fxchains = false;
    tracks_meta.track_options = TrackParseOptions::summary();

    let mut tracks_items = tracks_meta;
    tracks_items.track_options = TrackParseOptions {
        parse_items: true,
        parse_envelopes: false,
        parse_fx_chain: false,
    };

    let mut tracks_envelopes = tracks_meta;
    tracks_envelopes.track_options = TrackParseOptions {
        parse_items: false,
        parse_envelopes: true,
        parse_fx_chain: false,
    };

    let mut tracks_plugins = tracks_meta;
    tracks_plugins.track_options = TrackParseOptions {
        parse_items: false,
        parse_envelopes: false,
        parse_fx_chain: true,
    };

    let mut tracks_full = tracks_meta;
    tracks_full.track_options = TrackParseOptions::full();

    let mut top_items = tracks_meta;
    top_items.parse_tracks = false;
    top_items.parse_project_items = true;

    let mut top_envs = tracks_meta;
    top_envs.parse_tracks = false;
    top_envs.parse_project_envelopes = true;

    let mut top_fx = tracks_meta;
    top_fx.parse_tracks = false;
    top_fx.parse_project_fxchains = true;

    let mut marker_tempo = tracks_meta;
    marker_tempo.parse_tracks = false;
    marker_tempo.parse_markers_regions = true;
    marker_tempo.parse_tempo_envelope = true;

    vec![
        Scenario {
            name: "tracks_meta",
            options: tracks_meta,
        },
        Scenario {
            name: "tracks_items",
            options: tracks_items,
        },
        Scenario {
            name: "tracks_envelopes",
            options: tracks_envelopes,
        },
        Scenario {
            name: "tracks_plugins",
            options: tracks_plugins,
        },
        Scenario {
            name: "tracks_full",
            options: tracks_full,
        },
        Scenario {
            name: "top_items",
            options: top_items,
        },
        Scenario {
            name: "top_envelopes",
            options: top_envs,
        },
        Scenario {
            name: "top_fxchains",
            options: top_fx,
        },
        Scenario {
            name: "markers_tempo",
            options: marker_tempo,
        },
    ]
}

fn main() -> Result<(), String> {
    let (fixture, repeat) = parse_args()?;
    if !Path::new(&fixture).exists() {
        return Err(format!("fixture not found: {}", fixture.display()));
    }
    let content = fs::read_to_string(&fixture)
        .map_err(|e| format!("failed to read fixture {}: {e}", fixture.display()))?;
    let size_mb = content.len() as f64 / (1024.0 * 1024.0);

    println!("fixture: {}", fixture.display());
    println!("size_mb: {:.2}", size_mb);
    println!("repeat: {}", repeat);

    let mut parse_times = Vec::with_capacity(repeat);
    let mut parsed = None;
    for _ in 0..repeat {
        let t0 = Instant::now();
        let r = parse_rpp_file(&content).map_err(|e| format!("parse failed: {e}"))?;
        parse_times.push(t0.elapsed().as_secs_f64());
        parsed = Some(r);
    }
    let parsed = parsed.ok_or("no parse result".to_string())?;
    let parse_avg = mean(&parse_times);
    println!(
        "parse_avg_s: {:.4} (throughput {:.2} MB/s)",
        parse_avg,
        size_mb / parse_avg
    );
    println!();

    println!(
        "{:<18} {:>9} {:>8} {:>8} {:>8} {:>10} {:>10} {:>10} {:>10}",
        "scenario",
        "avg_s",
        "tracks",
        "items",
        "envs",
        "fx_track",
        "fx_top",
        "markers",
        "tempo_pts"
    );

    for scenario in build_scenarios() {
        let mut times = Vec::with_capacity(repeat);
        let mut sample: Option<ReaperProject> = None;
        for _ in 0..repeat {
            let t0 = Instant::now();
            let project = ReaperProject::from_rpp_project_with_options(&parsed, scenario.options)?;
            times.push(t0.elapsed().as_secs_f64());
            sample = Some(project);
        }
        let project = sample.ok_or("missing sample decode".to_string())?;
        let (nested_items, nested_envs, nested_fx_tracks) = count_nested(&project);
        println!(
            "{:<18} {:>9.4} {:>8} {:>8} {:>8} {:>10} {:>10} {:>10} {:>10}",
            scenario.name,
            mean(&times),
            project.tracks.len(),
            nested_items + project.items.len(),
            nested_envs + project.envelopes.len(),
            nested_fx_tracks,
            project.fx_chains.len(),
            project.markers_regions.all.len(),
            project
                .tempo_envelope
                .as_ref()
                .map(|t| t.points.len())
                .unwrap_or(0),
        );
    }

    Ok(())
}
