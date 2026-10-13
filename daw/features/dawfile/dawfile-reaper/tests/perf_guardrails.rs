use std::fs;
use std::path::Path;

use dawfile_reaper::{ProjectIndex, ReaperProject, SourceType, parse_rpp, parse_rpp_file};

const GOODNESS_FIXTURE_CANDIDATES: [&str; 2] = [
    "tests/fixtures/local/Goodness of God.RPP",
    "modules/dawfile/dawfile-reaper/tests/fixtures/local/Goodness of God.RPP",
];
const TEMPO_FIXTURE_CANDIDATES: [&str; 2] = [
    "tests/fixtures/tempo-map-advanced.RPP",
    "modules/dawfile/dawfile-reaper/tests/fixtures/tempo-map-advanced.RPP",
];

fn resolve_fixture(candidates: &[&str]) -> Option<String> {
    candidates
        .iter()
        .find(|p| Path::new(**p).exists())
        .map(|p| (*p).to_string())
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
fn guardrail_goodness_decoded_counts() {
    let Some(path) = resolve_fixture(&GOODNESS_FIXTURE_CANDIDATES) else {
        eprintln!("SKIP: missing Goodness fixture");
        return;
    };

    let content = fs::read_to_string(&path).expect("read Goodness fixture");
    let parsed = parse_rpp_file(&content).expect("parse_rpp_file");
    let typed = ReaperProject::from_rpp_project(&parsed).expect("typed decode");

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

    // Fixture-specific guardrails for optimization regressions.
    assert_eq!(typed.tracks.len(), 702);
    assert_eq!(typed.items.len(), 0);
    assert_eq!(typed.markers_regions.all.len(), 24);
    assert_eq!(
        typed
            .tempo_envelope
            .as_ref()
            .map(|t| t.points.len())
            .unwrap_or(0),
        3
    );
    assert_eq!(items.len(), 988);
    assert_eq!(take_count, 1592);
    assert_eq!(midi_sources.len(), 20);
    assert_eq!(midi_event_count, 69_854);
    assert_eq!(midi_x_event_count, 28);
    assert_eq!(envelope_count_total, 67);
}

#[test]
fn guardrail_fast_vs_nom_typed_parity_tempo_fixture() {
    let path = resolve_fixture(&TEMPO_FIXTURE_CANDIDATES).expect("tempo fixture path");
    let content = fs::read_to_string(&path).expect("read tempo fixture");

    let fast = parse_rpp_file(&content).expect("fast parse");
    let (remaining, nom) = parse_rpp(&content).expect("nom parse");
    assert!(
        remaining.trim().is_empty(),
        "nom parser had trailing input: {remaining:?}"
    );

    let fast_typed = ReaperProject::from_rpp_project(&fast).expect("fast typed");
    let nom_typed = ReaperProject::from_rpp_project(&nom).expect("nom typed");

    // Domain-level parity guardrails (avoid strict equality on known
    // property-surface differences between parser paths).
    assert_eq!(fast_typed.version, nom_typed.version);
    assert_eq!(fast_typed.version_string, nom_typed.version_string);
    assert_eq!(fast_typed.timestamp, nom_typed.timestamp);

    assert_eq!(fast_typed.tracks.len(), nom_typed.tracks.len());
    assert_eq!(fast_typed.items.len(), nom_typed.items.len());
    assert_eq!(fast_typed.envelopes.len(), nom_typed.envelopes.len());
    assert_eq!(fast_typed.fx_chains.len(), nom_typed.fx_chains.len());
    assert_eq!(fast_typed.markers_regions, nom_typed.markers_regions);
    assert_eq!(fast_typed.tempo_envelope, nom_typed.tempo_envelope);

    let fast_track_items: usize = fast_typed.tracks.iter().map(|t| t.items.len()).sum();
    let nom_track_items: usize = nom_typed.tracks.iter().map(|t| t.items.len()).sum();
    assert_eq!(fast_track_items, nom_track_items);

    let fast_track_fx: usize = fast_typed
        .tracks
        .iter()
        .filter(|t| t.fx_chain.is_some())
        .count();
    let nom_track_fx: usize = nom_typed
        .tracks
        .iter()
        .filter(|t| t.fx_chain.is_some())
        .count();
    assert_eq!(fast_track_fx, nom_track_fx);
}

// ── ProjectIndex guardrails ──────────────────────────────────────────────

#[test]
fn guardrail_index_matches_manual_traversal_goodness() {
    let Some(path) = resolve_fixture(&GOODNESS_FIXTURE_CANDIDATES) else {
        eprintln!("SKIP: missing Goodness fixture");
        return;
    };

    let content = fs::read_to_string(&path).expect("read Goodness fixture");
    let parsed = parse_rpp_file(&content).expect("parse_rpp_file");
    let typed = ReaperProject::from_rpp_project(&parsed).expect("typed decode");
    let index = ProjectIndex::build(&typed);

    // ── Match all existing guardrail counts ──
    assert_eq!(index.track_count, 702);
    assert_eq!(index.total_items(), 988);
    assert_eq!(index.total_takes(), 1592);
    assert_eq!(index.source_count("MIDI"), 20);
    assert_eq!(index.midi_event_count, 69_854);
    assert_eq!(index.midi_extended_event_count, 28);
    assert_eq!(index.envelope_count, 67);
    assert_eq!(index.marker_count + index.region_count, 24);
    assert_eq!(index.tempo_point_count, 3);

    // ── Verify against manual traversal (same logic as guardrail_goodness_decoded_counts) ──
    let manual_items = gather_items(&typed);
    assert_eq!(index.total_items(), manual_items.len() as u32);

    let manual_takes: u32 = manual_items.iter().map(|i| i.takes.len() as u32).sum();
    assert_eq!(index.total_takes(), manual_takes);

    let manual_envelopes: u32 = typed
        .tracks
        .iter()
        .map(|t| t.envelopes.len() as u32)
        .sum::<u32>()
        + typed.envelopes.len() as u32;
    assert_eq!(index.envelope_count, manual_envelopes);

    // ── Summary display doesn't panic ──
    let summary = index.summary();
    let display = summary.to_string();
    assert!(display.contains("702 tracks"));
    assert!(display.contains("988 items"));
}

#[test]
fn guardrail_index_tempo_fixture() {
    let path = resolve_fixture(&TEMPO_FIXTURE_CANDIDATES).expect("tempo fixture path");
    let content = fs::read_to_string(&path).expect("read tempo fixture");
    let parsed = parse_rpp_file(&content).expect("parse");
    let typed = ReaperProject::from_rpp_project(&parsed).expect("typed decode");
    let index = ProjectIndex::build(&typed);

    // Tempo fixture is small — validate basic structure
    assert!(index.track_count > 0, "should have tracks");
    assert!(index.tempo_point_count > 0, "should have tempo points");

    // Plugin inventory should be sorted and deduped
    let names = index.plugin_inventory();
    for w in names.windows(2) {
        assert!(w[0] <= w[1], "plugin names should be sorted: {:?}", names);
    }
}
