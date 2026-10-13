//! Integration test: combine an RPL (REAPER Project List) into a single RPP.
//!
//! Uses the real Battle SP26 JF Tracks RPL with 7 songs to verify the full
//! combine pipeline produces a valid, reparseable RPP with all songs on
//! a shared timeline.

use std::path::Path;

const RPL_PATH: &str = "/Users/codywright/Downloads/Battle SP26 JF Tracks.RPL";

fn rpl_exists() -> bool {
    Path::new(RPL_PATH).exists()
}

// r[verify combine.rpl-to-rpp]
// r[verify combine.sequential-layout]
// r[verify combine.file-paths]
// r[verify combine.song-folders]
#[test]
fn combine_rpl_produces_valid_rpp() {
    if !rpl_exists() {
        eprintln!("Skipping: RPL not found at {RPL_PATH}");
        return;
    }

    let options = dawfile_reaper::setlist_rpp::CombineOptions::default();
    let (combined, song_infos) =
        dawfile_reaper::setlist_rpp::combine_rpl(Path::new(RPL_PATH), &options)
            .expect("combine_rpl failed");

    // ── Song infos ──
    assert_eq!(song_infos.len(), 7, "Should have 7 songs");

    let song_names: Vec<&str> = song_infos.iter().map(|s| s.name.as_str()).collect();
    println!("Songs:");
    for (i, info) in song_infos.iter().enumerate() {
        println!(
            "  {i}: {:?} start={:.2}s dur={:.2}s local_start={:.2}s",
            info.name, info.global_start_seconds, info.duration_seconds, info.local_start
        );
    }

    // Verify song names are extracted correctly (no brackets)
    assert_eq!(song_names[0], "Belief - John Mayer");
    assert_eq!(song_names[1], "Vienna - Couch");
    assert!(song_names.iter().all(|n| !n.contains('[')));

    // Songs should be sequential: each starts after the previous ends
    for i in 1..song_infos.len() {
        let prev_end = song_infos[i - 1].global_start_seconds + song_infos[i - 1].duration_seconds;
        assert!(
            song_infos[i].global_start_seconds >= prev_end - 0.01,
            "Song {} ({}) starts at {:.2}s but song {} ends at {:.2}s",
            i,
            song_infos[i].name,
            song_infos[i].global_start_seconds,
            i - 1,
            prev_end
        );
    }

    // ── Combined RPP validity ──
    assert!(!combined.is_empty());
    assert!(combined.starts_with("<REAPER_PROJECT"));
    assert!(combined.trim_end().ends_with('>'));

    // Parse as RChunk tree (structural validity)
    let chunk = dawfile_reaper::read_rpp_chunk(&combined)
        .expect("Combined RPP should parse as valid RChunk tree");
    assert_eq!(chunk.name().as_deref(), Some("REAPER_PROJECT"));

    // Parse as ReaperProject (semantic validity)
    let project = dawfile_reaper::parse_project_text(&combined)
        .expect("Combined RPP should parse as ReaperProject");

    println!("\nCombined project:");
    println!("  Tracks: {}", project.tracks.len());
    println!(
        "  Items: {}",
        project.tracks.iter().map(|t| t.items.len()).sum::<usize>()
    );
    println!("  Markers: {}", project.markers_regions.all.len());

    // Should have tracks from all 7 songs plus 7 song folder tracks
    // Each original RPP has some tracks — the combined should have more than any single one
    assert!(
        project.tracks.len() >= 14,
        "Combined should have at least 14 tracks (7 folders + 7 content minimum), got {}",
        project.tracks.len()
    );

    // Verify all source tracks have items (the stem tracks should have items)
    let total_items: usize = project.tracks.iter().map(|t| t.items.len()).sum();
    assert!(total_items > 0, "Combined project should have items");

    // Verify song folder tracks exist
    let track_names: Vec<&str> = project.tracks.iter().map(|t| t.name.as_str()).collect();
    for name in &song_names {
        assert!(
            track_names.contains(name),
            "Combined project should have folder track for song '{name}'"
        );
    }

    // ── Structural counts ──
    fn count(text: &str, pattern: &str) -> usize {
        text.matches(pattern).count()
    }

    let track_count = count(&combined, "<TRACK");
    let item_count = count(&combined, "<ITEM");
    let source_count = count(&combined, "<SOURCE");
    let marker_count = count(&combined, "MARKER ");

    println!("\nStructural counts:");
    println!("  <TRACK: {track_count}");
    println!("  <ITEM: {item_count}");
    println!("  <SOURCE: {source_count}");
    println!("  MARKER: {marker_count}");

    assert!(track_count >= 14, "Should have many tracks");
    assert!(item_count > 0, "Should have items");
    assert!(source_count > 0, "Should have sources");
    assert!(marker_count > 0, "Should have markers");

    // ── Write to file for manual verification ──
    let output_path = std::env::temp_dir().join("Battle_SP26_Combined.RPP");
    std::fs::write(&output_path, &combined).expect("write combined RPP");
    println!("\nWrote combined RPP to: {}", output_path.display());

    // Verify file can be re-read
    let reread = std::fs::read_to_string(&output_path).expect("re-read");
    let _reparsed =
        dawfile_reaper::read_rpp_chunk(&reread).expect("re-read combined RPP should parse");

    // Total combined duration
    let last_song = song_infos.last().unwrap();
    let total_duration = last_song.global_start_seconds + last_song.duration_seconds;
    println!(
        "Total setlist duration: {:.1}s ({:.1} minutes)",
        total_duration,
        total_duration / 60.0
    );
}

// r[verify combine.gap-measures]
#[test]
fn combine_rpl_with_gap() {
    if !rpl_exists() {
        eprintln!("Skipping: RPL not found at {RPL_PATH}");
        return;
    }

    let options = dawfile_reaper::setlist_rpp::CombineOptions {
        gap_measures: 2,
        trim_to_bounds: false,
    };
    let (_, song_infos) = dawfile_reaper::setlist_rpp::combine_rpl(Path::new(RPL_PATH), &options)
        .expect("combine_rpl with gap failed");

    // With gap_measures=2, each gap should be 2 measures at the NEXT song's tempo.
    // Gaps should be > 0 between all adjacent songs.
    for i in 1..song_infos.len() {
        let prev_end = song_infos[i - 1].global_start_seconds + song_infos[i - 1].duration_seconds;
        let gap = song_infos[i].global_start_seconds - prev_end;
        assert!(
            gap > 0.5,
            "Gap between song {} and {} should be > 0.5s (2 measures), got {:.2}s",
            i - 1,
            i,
            gap
        );
        println!(
            "  Gap {}->{}: {:.2}s ({})",
            i - 1,
            i,
            gap,
            song_infos[i].name
        );
    }
}
