use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use dawfile_reaper::{ReaperProject, SourceType, parse_rpp_file};

fn audit_path() -> PathBuf {
    if let Ok(path) = std::env::var("RPP_AUDIT_PATH") {
        return PathBuf::from(path);
    }
    PathBuf::from(
        "/Users/codywright/Documents/Development/Rust/roam-test/modules/dawfile/dawfile-reaper/tests/fixtures/local/Goodness of God.RPP",
    )
}

fn chunk_tag(line: &str) -> Option<String> {
    let l = line.trim_start();
    if !l.starts_with('<') {
        return None;
    }
    let rest = &l[1..];
    let mut out = String::new();
    for ch in rest.chars() {
        if ch.is_whitespace() || ch == '>' {
            break;
        }
        out.push(ch);
    }
    if out.is_empty() { None } else { Some(out) }
}

fn line_tag(line: &str) -> Option<String> {
    let l = line.trim();
    if l.is_empty() || l == ">" || l.starts_with('<') || l.starts_with("//") {
        return None;
    }
    l.split_whitespace().next().map(|s| s.to_string())
}

fn is_interesting_line_tag(tag: &str) -> bool {
    matches!(tag, "PT" | "E" | "e" | "POOLEDENVINST" | "PARMENV")
        || tag
            .chars()
            .next()
            .map(|c| c.is_ascii_uppercase())
            .unwrap_or(false)
}

fn gather_items(project: &ReaperProject) -> Vec<&dawfile_reaper::Item> {
    let mut out = Vec::new();
    for t in &project.tracks {
        for item in &t.items {
            out.push(item);
        }
    }
    for item in &project.items {
        out.push(item);
    }
    out
}

#[test]
fn audit_large_rpp_fixture() {
    let path = audit_path();
    if !Path::new(&path).exists() {
        eprintln!("SKIP: audit fixture missing at {}", path.display());
        return;
    }

    let content = fs::read_to_string(&path).expect("read large rpp fixture");
    let file_size_mb = content.len() as f64 / (1024.0 * 1024.0);

    let t0 = Instant::now();
    let parsed = parse_rpp_file(&content).expect("parse_rpp_file");
    let parse_elapsed = t0.elapsed();

    let t1 = Instant::now();
    let typed = ReaperProject::from_rpp_project(&parsed).expect("ReaperProject::from_rpp_project");
    let typed_elapsed = t1.elapsed();

    let mut chunk_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut line_counts: BTreeMap<String, usize> = BTreeMap::new();

    for line in content.lines() {
        if let Some(tag) = chunk_tag(line) {
            *chunk_counts.entry(tag).or_insert(0) += 1;
        }
        if let Some(tag) = line_tag(line)
            && is_interesting_line_tag(&tag)
        {
            *line_counts.entry(tag).or_insert(0) += 1;
        }
    }

    let known_chunk_tags: BTreeSet<&str> = BTreeSet::from([
        "REAPER_PROJECT",
        "TRACK",
        "ITEM",
        "VOLENV",
        "VOLENV2",
        "PANENV",
        "PANENV2",
        "PARMENV",
        "FXCHAIN",
        "FXCHAIN_REC",
        "SOURCE",
        "TAKE",
        "TEMPOENVEX",
    ]);

    let unknown_chunk_tags: Vec<(String, usize)> = chunk_counts
        .iter()
        .filter(|(k, _)| !known_chunk_tags.contains(k.as_str()))
        .map(|(k, v)| (k.clone(), *v))
        .collect();

    let items = gather_items(&typed);
    let take_count: usize = items.iter().map(|i| i.takes.len()).sum();
    let midi_sources: Vec<&dawfile_reaper::MidiSource> = items
        .iter()
        .flat_map(|item| item.takes.iter())
        .filter_map(|take| take.source.as_ref())
        .filter(|src| src.source_type == SourceType::Midi)
        .filter_map(|src| src.midi_data.as_ref())
        .collect();

    let midi_event_count: usize = midi_sources.iter().map(|m| m.events.len()).sum();
    let midi_x_event_count: usize = midi_sources.iter().map(|m| m.extended_events.len()).sum();

    let envelope_count_from_tracks: usize = typed.tracks.iter().map(|t| t.envelopes.len()).sum();
    let envelope_count_total = typed.envelopes.len() + envelope_count_from_tracks;
    let parsed_envelope_points: usize = typed
        .tracks
        .iter()
        .flat_map(|t| t.envelopes.iter())
        .map(|e| e.points.len())
        .sum::<usize>()
        + typed
            .envelopes
            .iter()
            .map(|e| e.points.len())
            .sum::<usize>();

    eprintln!("\n=== LARGE RPP AUDIT ===");
    eprintln!("Path: {}", path.display());
    eprintln!("Size: {:.2} MB", file_size_mb);
    eprintln!(
        "parse_rpp_file: {:.3}s (throughput {:.2} MB/s)",
        parse_elapsed.as_secs_f64(),
        file_size_mb / parse_elapsed.as_secs_f64().max(0.000_001),
    );
    eprintln!("typed conversion: {:.3}s", typed_elapsed.as_secs_f64());
    eprintln!();
    eprintln!("Top-level blocks parsed: {}", parsed.blocks.len());
    eprintln!(
        "Typed project: tracks={}, items(root)={}, markers/regions={}, tempo_points={}",
        typed.tracks.len(),
        typed.items.len(),
        typed.markers_regions.all.len(),
        typed
            .tempo_envelope
            .as_ref()
            .map(|t| t.points.len())
            .unwrap_or(0),
    );
    eprintln!(
        "Track item coverage: items_in_tracks={}, takes={}, midi_sources={}, midi_events={}, midi_x_blocks={}",
        items.len(),
        take_count,
        midi_sources.len(),
        midi_event_count,
        midi_x_event_count
    );
    eprintln!(
        "Envelope coverage: envelopes_total={}, parsed_points={}, raw_PT_lines={}, raw_POOLEDENVINST_lines={}",
        envelope_count_total,
        parsed_envelope_points,
        line_counts.get("PT").copied().unwrap_or(0),
        line_counts.get("POOLEDENVINST").copied().unwrap_or(0),
    );
    eprintln!();

    let mut chunk_by_count: Vec<(String, usize)> = chunk_counts.into_iter().collect();
    chunk_by_count.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    eprintln!("Top chunk tags:");
    for (name, count) in chunk_by_count.iter().take(20) {
        eprintln!("  {name:<16} {count}");
    }

    eprintln!("Unknown chunk tags vs typed parser block model:");
    for (name, count) in unknown_chunk_tags.iter().take(20) {
        eprintln!("  {name:<16} {count}");
    }

    assert!(!typed.tracks.is_empty(), "expected at least one track");
}
