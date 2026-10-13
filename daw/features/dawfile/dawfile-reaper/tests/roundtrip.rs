//! RPP roundtrip tests — parse an RPP, write it back, and verify no data is lost.
//!
//! These tests validate multiple levels of round-trip fidelity:
//!
//! 1. **RChunk tree idempotence**: RPP → RChunk → stringify → re-parse → stringify again.
//!    The second stringify must be identical to the first (stable after quote normalization).
//!
//! 2. **ReaperProject semantic validation**: Verify all typed fields are correctly extracted.
//!
//! 3. **Semantic equivalence**: Parse original and round-tripped RPP into ReaperProject,
//!    verify they contain the same semantic data.
//!
//! 4. **File I/O**: Write to disk, read back, verify structural integrity.
//!
//! Note: The RChunk tree normalizes quoting (e.g. `"Media"` → `Media` when no spaces).
//! This is semantically identical — REAPER treats both forms the same.

const BELIEF_RPP: &str = "/Users/codywright/Music/Projects/Live Tracks/Just Friends/Belief - John Mayer/Belief - John Mayer [Battle SP26].RPP";

fn belief_rpp() -> Option<String> {
    std::fs::read_to_string(BELIEF_RPP).ok()
}

fn count(text: &str, pattern: &str) -> usize {
    text.matches(pattern).count()
}

// ──────────────────────────────────────────────────────────────
// Level 1: RChunk tree idempotent round-trip
// r[verify roundtrip.idempotent]
// r[verify roundtrip.structural]
// ──────────────────────────────────────────────────────────────

#[test]
fn roundtrip_rchunk_tree_idempotent() {
    let Some(original) = belief_rpp() else {
        eprintln!("Skipping: Belief RPP not found at {BELIEF_RPP}");
        return;
    };

    // First pass: parse → stringify (normalizes quoting)
    let chunk1 = dawfile_reaper::read_rpp_chunk(&original).expect("first parse failed");
    let text1 = dawfile_reaper::stringify_rpp_node(&dawfile_reaper::RNodeTree::Chunk(chunk1));

    // Second pass: re-parse → stringify again
    let chunk2 = dawfile_reaper::read_rpp_chunk(&text1).expect("second parse failed");
    let text2 = dawfile_reaper::stringify_rpp_node(&dawfile_reaper::RNodeTree::Chunk(chunk2));

    // The second stringify must be byte-identical to the first (idempotent)
    assert_eq!(
        text1, text2,
        "RChunk round-trip is not idempotent — output changed on second pass"
    );

    // Verify structural counts are preserved from the original
    let structural_checks = [
        "<TRACK",
        "<ITEM",
        "<SOURCE",
        "SOURCE WAVE",
        "FADEIN",
        "FADEOUT",
        "SOFFS",
        "VOLPAN",
        "PLAYRATE",
        "MARKER",
        "ISBUS",
        "<TEMPOENVEX",
        "PT ",
        "NAME ",
        "TRACKID",
        "GUID",
    ];

    println!("Structural preservation (original → normalized):");
    let mut all_ok = true;
    for label in &structural_checks {
        let orig_count = count(&original, label);
        let norm_count = count(&text1, label);
        let ok = orig_count == norm_count;
        let status = if ok { "OK" } else { "MISMATCH" };
        println!("  {label:<15} orig={orig_count:>4}  norm={norm_count:>4}  {status}");
        if !ok {
            all_ok = false;
        }
    }
    assert!(all_ok, "Structural counts changed during round-trip");

    // Verify line count preserved
    let orig_lines = original.lines().count();
    let norm_lines = text1.lines().count();
    assert_eq!(
        orig_lines, norm_lines,
        "Line count changed: original={orig_lines}, normalized={norm_lines}"
    );
}

// ──────────────────────────────────────────────────────────────
// Level 2: ReaperProject semantic validation
// r[verify rpp.parse.project]
// r[verify rpp.parse.track]
// r[verify rpp.parse.item]
// r[verify rpp.parse.tempo]
// r[verify rpp.parse.markers]
// ──────────────────────────────────────────────────────────────

#[test]
fn roundtrip_reaper_project_semantic() {
    let Some(original) = belief_rpp() else {
        eprintln!("Skipping: Belief RPP not found at {BELIEF_RPP}");
        return;
    };

    let project =
        dawfile_reaper::parse_project_text(&original).expect("ReaperProject parse failed");

    // ── Project metadata ──
    assert!(
        (project.version - 0.1).abs() < f64::EPSILON,
        "version should be 0.1, got {}",
        project.version
    );
    assert!(
        project.version_string.contains("7.63"),
        "version string should contain 7.63, got '{}'",
        project.version_string
    );
    assert_eq!(project.timestamp, 1773690584);

    // ── Project properties ──
    let props = &project.properties;
    assert_eq!(props.sample_rate, Some((48000, 0, 0)));
    assert_eq!(props.tempo, Some((101.0, 4, 4, 0)));

    // ── Tracks ──
    assert_eq!(project.tracks.len(), 10);

    let track_names: Vec<&str> = project.tracks.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(
        track_names,
        vec![
            "Belief (Live) - John Mayer",
            "TRACKS",
            "REFERENCE",
            "STEM SPLIT",
            "Drums",
            "Other",
            "Guitar",
            "Vocals",
            "Bass",
            "Piano",
        ]
    );

    // Folder structure
    let tracks_folder = &project.tracks[1];
    assert_eq!(tracks_folder.name, "TRACKS");
    assert_eq!(
        tracks_folder.folder.as_ref().unwrap().folder_state,
        dawfile_reaper::types::track::FolderState::FolderParent,
    );

    let piano = &project.tracks[9];
    assert_eq!(piano.name, "Piano");
    let piano_folder = piano.folder.as_ref().unwrap();
    assert_eq!(
        piano_folder.folder_state,
        dawfile_reaper::types::track::FolderState::LastInFolder,
    );
    assert_eq!(piano_folder.indentation, -3);

    // ── Items ──
    let total_items: usize = project.tracks.iter().map(|t| t.items.len()).sum();
    assert_eq!(total_items, 23, "Total items across all tracks");

    // Drums track items
    let drums = &project.tracks[4];
    assert_eq!(drums.name, "Drums");
    assert_eq!(drums.items.len(), 4);

    let first_item = &drums.items[0];
    assert!(
        (first_item.position - 1.69066749472487).abs() < 1e-10,
        "First drums item position"
    );
    assert!(
        (first_item.length - 131.20822586816902).abs() < 1e-10,
        "First drums item length"
    );
    assert_eq!(first_item.name, "Belief (Live) - John Mayer (Drums)_1.wav");

    // SOURCE WAVE via takes
    assert!(!first_item.takes.is_empty());
    let source = first_item.takes[0]
        .source
        .as_ref()
        .expect("First drums take should have a source");
    assert!(source.file_path.contains("Drums"));
    assert_eq!(source.source_type, dawfile_reaper::SourceType::Wave);

    // ── Markers ──
    let markers = &project.markers_regions;
    assert!(!markers.all.is_empty());

    let marker_names: Vec<&str> = markers.all.iter().map(|m| m.name.as_str()).collect();
    for expected in ["SONGSTART", "SONGEND", "=START", "=END"] {
        assert!(
            marker_names.contains(&expected),
            "Missing marker: {expected}"
        );
    }
    assert!(
        marker_names.iter().any(|n| n.contains("Intro")),
        "Should have Intro markers"
    );

    // ── Tempo envelope ──
    let tempo = project.tempo_envelope.as_ref().expect("tempo envelope");
    assert!(!tempo.points.is_empty());
    assert!((tempo.points[0].position).abs() < 1e-6);
    assert!((tempo.points[0].tempo - 101.0).abs() < 0.01);

    println!("ReaperProject semantic validation: PASSED");
    println!(
        "  tracks={}, items={total_items}, markers={}, tempo_pts={}",
        project.tracks.len(),
        markers.all.len(),
        tempo.points.len()
    );
}

// ──────────────────────────────────────────────────────────────
// Level 3: Semantic equivalence across round-trip
// r[verify roundtrip.semantic]
// ──────────────────────────────────────────────────────────────

#[test]
fn roundtrip_serialize_reparse_semantic_equivalence() {
    let Some(original) = belief_rpp() else {
        eprintln!("Skipping: Belief RPP not found at {BELIEF_RPP}");
        return;
    };

    let project_before = dawfile_reaper::parse_project_text(&original).expect("first parse failed");

    // Round-trip through RChunk tree
    let chunk = dawfile_reaper::read_rpp_chunk(&original).expect("RChunk parse failed");
    let serialized = dawfile_reaper::stringify_rpp_node(&dawfile_reaper::RNodeTree::Chunk(chunk));

    let project_after =
        dawfile_reaper::parse_project_text(&serialized).expect("re-parse of serialized RPP failed");

    // ── Metadata ──
    assert_eq!(project_before.version, project_after.version);
    assert_eq!(project_before.version_string, project_after.version_string);
    assert_eq!(project_before.timestamp, project_after.timestamp);

    // ── Properties ──
    assert_eq!(
        project_before.properties.sample_rate,
        project_after.properties.sample_rate
    );
    assert_eq!(
        project_before.properties.tempo,
        project_after.properties.tempo
    );

    // ── Tracks ──
    assert_eq!(project_before.tracks.len(), project_after.tracks.len());
    for (i, (before, after)) in project_before
        .tracks
        .iter()
        .zip(project_after.tracks.iter())
        .enumerate()
    {
        assert_eq!(before.name, after.name, "track[{i}] name");
        assert_eq!(before.track_id, after.track_id, "track[{i}] track_id");
        assert_eq!(before.peak_color, after.peak_color, "track[{i}] peak_color");
        assert_eq!(before.selected, after.selected, "track[{i}] selected");
        assert_eq!(before.folder, after.folder, "track[{i}] folder");
        assert_eq!(before.volpan, after.volpan, "track[{i}] volpan");
        assert_eq!(before.mutesolo, after.mutesolo, "track[{i}] mutesolo");
        assert_eq!(
            before.channel_count, after.channel_count,
            "track[{i}] nchan"
        );

        // Items
        assert_eq!(
            before.items.len(),
            after.items.len(),
            "track[{i}] ({}) item count",
            before.name
        );
        for (j, (ib, ia)) in before.items.iter().zip(after.items.iter()).enumerate() {
            assert!(
                (ib.position - ia.position).abs() < 1e-12,
                "track[{i}] item[{j}] position"
            );
            assert!(
                (ib.length - ia.length).abs() < 1e-12,
                "track[{i}] item[{j}] length"
            );
            assert_eq!(ib.name, ia.name, "track[{i}] item[{j}] name");
            assert_eq!(ib.item_guid, ia.item_guid, "track[{i}] item[{j}] iguid");
            assert_eq!(ib.fade_in, ia.fade_in, "track[{i}] item[{j}] fade_in");
            assert_eq!(ib.fade_out, ia.fade_out, "track[{i}] item[{j}] fade_out");

            // Takes
            assert_eq!(
                ib.takes.len(),
                ia.takes.len(),
                "track[{i}] item[{j}] take count"
            );
            for (k, (tb, ta)) in ib.takes.iter().zip(ia.takes.iter()).enumerate() {
                assert_eq!(
                    tb.source.is_some(),
                    ta.source.is_some(),
                    "track[{i}] item[{j}] take[{k}] source"
                );
                if let (Some(sb), Some(sa)) = (&tb.source, &ta.source) {
                    assert_eq!(sb.source_type, sa.source_type);
                    assert_eq!(sb.file_path, sa.file_path);
                }
            }
        }
    }

    // ── Markers (compare by content, not order — parser may sort differently) ──
    assert_eq!(
        project_before.markers_regions.all.len(),
        project_after.markers_regions.all.len(),
        "marker count"
    );

    // Build comparable sets: (name, position_rounded)
    let marker_set = |markers: &[dawfile_reaper::MarkerRegion]| -> Vec<(String, i64)> {
        let mut v: Vec<_> = markers
            .iter()
            .map(|m| (m.name.clone(), (m.position * 1e6) as i64))
            .collect();
        v.sort();
        v
    };
    assert_eq!(
        marker_set(&project_before.markers_regions.all),
        marker_set(&project_after.markers_regions.all),
        "marker content mismatch"
    );

    // ── Tempo envelope ──
    match (
        &project_before.tempo_envelope,
        &project_after.tempo_envelope,
    ) {
        (Some(te_b), Some(te_a)) => {
            assert_eq!(te_b.points.len(), te_a.points.len());
            for (i, (pb, pa)) in te_b.points.iter().zip(te_a.points.iter()).enumerate() {
                assert!(
                    (pb.position - pa.position).abs() < 1e-10,
                    "tempo[{i}] position"
                );
                assert!(
                    (pb.tempo - pa.tempo).abs() < 0.001,
                    "tempo[{i}] bpm: {} vs {}",
                    pb.tempo,
                    pa.tempo
                );
            }
        }
        (None, None) => {}
        _ => panic!("tempo_envelope presence mismatch"),
    }

    println!("Semantic equivalence: PASSED");
    println!(
        "  {} tracks, {} items, {} markers, {} tempo points",
        project_before.tracks.len(),
        project_before
            .tracks
            .iter()
            .map(|t| t.items.len())
            .sum::<usize>(),
        project_before.markers_regions.all.len(),
        project_before
            .tempo_envelope
            .as_ref()
            .map_or(0, |t| t.points.len())
    );
}

// ──────────────────────────────────────────────────────────────
// Level 4: File I/O round-trip
// ──────────────────────────────────────────────────────────────

#[test]
fn roundtrip_write_to_file_and_reparse() {
    let Some(original) = belief_rpp() else {
        eprintln!("Skipping: Belief RPP not found at {BELIEF_RPP}");
        return;
    };

    // Parse → write to temp file → read back
    let chunk = dawfile_reaper::read_rpp_chunk(&original).expect("parse failed");
    let output_path = std::env::temp_dir().join("dawfile_roundtrip_test.RPP");
    dawfile_reaper::write_rpp(&output_path, &chunk).expect("write_rpp failed");

    let written = std::fs::read_to_string(&output_path).expect("read back failed");

    // Parse the written file at both levels
    let _reparsed_chunk = dawfile_reaper::read_rpp_chunk(&written).expect("re-parse failed");
    let reparsed_project = dawfile_reaper::parse_project_text(&written)
        .expect("ReaperProject parse of written file failed");

    // Verify idempotence: write → read → write should be identical
    let serialized_original =
        dawfile_reaper::stringify_rpp_node(&dawfile_reaper::RNodeTree::Chunk(chunk));
    // write_rpp adds trailing newline, stringify doesn't always — normalize
    assert_eq!(
        serialized_original.trim(),
        written.trim(),
        "File content should match direct stringify"
    );

    // Structural count verification
    let checks = [
        (
            "<TRACK",
            count(&original, "<TRACK"),
            count(&written, "<TRACK"),
        ),
        ("<ITEM", count(&original, "<ITEM"), count(&written, "<ITEM")),
        (
            "<SOURCE",
            count(&original, "<SOURCE"),
            count(&written, "<SOURCE"),
        ),
        (
            "SOURCE WAVE",
            count(&original, "SOURCE WAVE"),
            count(&written, "SOURCE WAVE"),
        ),
        (
            "FADEIN",
            count(&original, "FADEIN"),
            count(&written, "FADEIN"),
        ),
        (
            "FADEOUT",
            count(&original, "FADEOUT"),
            count(&written, "FADEOUT"),
        ),
        ("SOFFS", count(&original, "SOFFS"), count(&written, "SOFFS")),
        (
            "VOLPAN",
            count(&original, "VOLPAN"),
            count(&written, "VOLPAN"),
        ),
        (
            "PLAYRATE",
            count(&original, "PLAYRATE"),
            count(&written, "PLAYRATE"),
        ),
        (
            "MARKER",
            count(&original, "MARKER"),
            count(&written, "MARKER"),
        ),
        ("ISBUS", count(&original, "ISBUS"), count(&written, "ISBUS")),
        (
            "<TEMPOENVEX",
            count(&original, "<TEMPOENVEX"),
            count(&written, "<TEMPOENVEX"),
        ),
        ("PT ", count(&original, "PT "), count(&written, "PT ")),
    ];

    println!("Structural counts (file round-trip):");
    let mut all_ok = true;
    for (label, orig, writ) in &checks {
        let ok = orig == writ;
        let status = if ok { "OK" } else { "MISMATCH" };
        println!("  {label:<15} orig={orig:>4}  written={writ:>4}  {status}");
        if !ok {
            all_ok = false;
        }
    }
    assert!(all_ok, "Structural count mismatch");

    // Semantic data survived
    assert_eq!(reparsed_project.tracks.len(), 10);
    let total_items: usize = reparsed_project.tracks.iter().map(|t| t.items.len()).sum();
    assert_eq!(total_items, 23);

    let _ = std::fs::remove_file(&output_path);
    println!("File round-trip: PASSED");
}
