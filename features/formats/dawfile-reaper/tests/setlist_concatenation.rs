//! Integration tests for setlist RPP concatenation.
//!
//! Uses the builder API to construct song projects programmatically,
//! then validates concatenation, shell copies, and role setlists.

use dawfile_reaper::ReaperProject;
use dawfile_reaper::builder::{MarkerBuilder, ReaperProjectBuilder};
use dawfile_reaper::setlist_rpp::{
    self, build_song_infos_from_projects, concatenate_projects, measures_to_seconds,
    resolve_song_bounds,
};
use std::path::PathBuf;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/setlist")
        .join(name)
}

// ─── Builder Helpers ─────────────────────────────────────────────────────────

/// Song A: 120 BPM, 4/4, 5 tracks, bounds 0→22
fn build_song_a() -> ReaperProject {
    ReaperProjectBuilder::new()
        .tempo(120.0)
        .sample_rate(48000)
        .tempo_envelope(|e| e.point(0.0, 120.0))
        // Markers
        .add_marker(
            MarkerBuilder::marker(1, 0.0, "PREROLL")
                .guid("{00000000-0000-0000-0000-00000000A001}")
                .build(),
        )
        .add_marker(
            MarkerBuilder::marker(2, 2.0, "Count-In")
                .guid("{00000000-0000-0000-0000-00000000A002}")
                .build(),
        )
        .add_marker(
            MarkerBuilder::marker(3, 4.0, "=START")
                .guid("{00000000-0000-0000-0000-00000000A003}")
                .build(),
        )
        .add_marker(
            MarkerBuilder::marker(4, 4.0, "SONGSTART")
                .guid("{00000000-0000-0000-0000-00000000A004}")
                .build(),
        )
        .add_marker(
            MarkerBuilder::region(5, 4.0, 12.0, "Verse")
                .guid("{00000000-0000-0000-0000-00000000A005}")
                .build(),
        )
        .add_marker(
            MarkerBuilder::region(6, 12.0, 20.0, "Chorus")
                .guid("{00000000-0000-0000-0000-00000000A006}")
                .build(),
        )
        .add_marker(
            MarkerBuilder::marker(7, 20.0, "SONGEND")
                .guid("{00000000-0000-0000-0000-00000000A007}")
                .build(),
        )
        .add_marker(
            MarkerBuilder::marker(8, 20.0, "=END")
                .guid("{00000000-0000-0000-0000-00000000A008}")
                .build(),
        )
        .add_marker(
            MarkerBuilder::marker(9, 22.0, "POSTROLL")
                .guid("{00000000-0000-0000-0000-00000000A009}")
                .build(),
        )
        // Tracks
        .track("Click/Guide", |t| {
            t.guid("{AAAAAAAA-0001-0000-0000-000000000000}")
                .folder_start()
        })
        .track("Click", |t| {
            t.guid("{AAAAAAAA-0002-0000-0000-000000000000}")
                .folder_end(1)
                .item(0.0, 22.0, |i| {
                    i.name("Click Pattern A")
                        .guid("{AAAAAAAA-I001-0000-0000-000000000000}")
                })
        })
        .track("TRACKS", |t| {
            t.guid("{AAAAAAAA-0003-0000-0000-000000000000}")
                .folder_start()
        })
        .track("Guitar", |t| {
            t.guid("{AAAAAAAA-0004-0000-0000-000000000000}")
                .volume(0.8)
                .item(4.0, 16.0, |i| {
                    i.name("Guitar Recording A")
                        .guid("{AAAAAAAA-I002-0000-0000-000000000000}")
                })
        })
        .track("Bass", |t| {
            t.guid("{AAAAAAAA-0005-0000-0000-000000000000}")
                .volume(0.6)
                .folder_end(1)
                .item(4.0, 16.0, |i| {
                    i.name("Bass Recording A")
                        .guid("{AAAAAAAA-I003-0000-0000-000000000000}")
                })
        })
        .build()
}

/// Song B: 90 BPM, 3/4, 3 tracks, bounds 0→18
fn build_song_b() -> ReaperProject {
    ReaperProjectBuilder::new()
        .tempo_with_time_sig(90.0, 3, 4)
        .sample_rate(48000)
        .tempo_envelope(|e| e.point(0.0, 90.0).point(10.666667, 110.0))
        // Markers
        .add_marker(
            MarkerBuilder::marker(1, 0.0, "Count-In")
                .guid("{BBBBBBBB-0000-0000-0000-000000000001}")
                .build(),
        )
        .add_marker(
            MarkerBuilder::marker(2, 2.0, "=START")
                .guid("{BBBBBBBB-0000-0000-0000-000000000002}")
                .build(),
        )
        .add_marker(
            MarkerBuilder::marker(3, 2.0, "SONGSTART")
                .guid("{BBBBBBBB-0000-0000-0000-000000000003}")
                .build(),
        )
        .add_marker(
            MarkerBuilder::region(4, 2.0, 10.0, "Bridge")
                .guid("{BBBBBBBB-0000-0000-0000-000000000004}")
                .build(),
        )
        .add_marker(
            MarkerBuilder::region(10, 4.0, 14.0, "Solo Section")
                .guid("{BBBBBBBB-R000-0000-0000-000000000001}")
                .build(),
        )
        .add_marker(
            MarkerBuilder::region(5, 10.0, 16.0, "Outro")
                .guid("{BBBBBBBB-0000-0000-0000-000000000005}")
                .build(),
        )
        .add_marker(
            MarkerBuilder::marker(6, 16.0, "SONGEND")
                .guid("{BBBBBBBB-0000-0000-0000-000000000006}")
                .build(),
        )
        .add_marker(
            MarkerBuilder::marker(7, 16.0, "=END")
                .guid("{BBBBBBBB-0000-0000-0000-000000000007}")
                .build(),
        )
        .add_marker(
            MarkerBuilder::marker(8, 18.0, "POSTROLL")
                .guid("{BBBBBBBB-0000-0000-0000-000000000008}")
                .build(),
        )
        // Tracks
        .track("Click", |t| {
            t.guid("{BBBBBBBB-0001-0000-0000-000000000000}")
                .item(0.0, 18.0, |i| {
                    i.name("Click Pattern B")
                        .guid("{BBBBBBBB-I001-0000-0000-000000000000}")
                })
        })
        .track("Keys", |t| {
            t.guid("{BBBBBBBB-0002-0000-0000-000000000000}")
                .volume(0.7)
                .pan(0.3)
                .item(2.0, 14.0, |i| {
                    i.name("Keys Recording B")
                        .guid("{BBBBBBBB-I002-0000-0000-000000000000}")
                })
        })
        .track("Drums", |t| {
            t.guid("{BBBBBBBB-0003-0000-0000-000000000000}")
                .volume(0.9)
                .item(2.0, 14.0, |i| {
                    i.name("Drums Recording B")
                        .guid("{BBBBBBBB-I003-0000-0000-000000000000}")
                })
        })
        .build()
}

/// Song C: 140 BPM, 4/4, 3 tracks, bounds 0→17.142857
fn build_song_c() -> ReaperProject {
    ReaperProjectBuilder::new()
        .tempo(140.0)
        .sample_rate(48000)
        .tempo_envelope(|e| e.point(0.0, 140.0))
        // Markers
        .add_marker(
            MarkerBuilder::marker(1, 0.0, "PREROLL")
                .guid("{CCCCCCCC-0000-0000-0000-000000000001}")
                .build(),
        )
        .add_marker(
            MarkerBuilder::marker(2, 1.714286, "=START")
                .guid("{CCCCCCCC-0000-0000-0000-000000000002}")
                .build(),
        )
        .add_marker(
            MarkerBuilder::marker(3, 1.714286, "SONGSTART")
                .guid("{CCCCCCCC-0000-0000-0000-000000000003}")
                .build(),
        )
        .add_marker(
            MarkerBuilder::region(4, 1.714286, 6.857143, "Intro")
                .guid("{CCCCCCCC-0000-0000-0000-000000000004}")
                .build(),
        )
        .add_marker(
            MarkerBuilder::region(5, 6.857143, 15.428571, "Breakdown")
                .guid("{CCCCCCCC-0000-0000-0000-000000000005}")
                .build(),
        )
        .add_marker(
            MarkerBuilder::marker(6, 15.428571, "SONGEND")
                .guid("{CCCCCCCC-0000-0000-0000-000000000006}")
                .build(),
        )
        .add_marker(
            MarkerBuilder::marker(7, 15.428571, "=END")
                .guid("{CCCCCCCC-0000-0000-0000-000000000007}")
                .build(),
        )
        .add_marker(
            MarkerBuilder::marker(8, 17.142857, "POSTROLL")
                .guid("{CCCCCCCC-0000-0000-0000-000000000008}")
                .build(),
        )
        // Tracks
        .track("Click", |t| {
            t.guid("{CCCCCCCC-0001-0000-0000-000000000000}")
                .item(0.0, 17.142857, |i| {
                    i.name("Click Pattern C")
                        .guid("{CCCCCCCC-I001-0000-0000-000000000000}")
                })
        })
        .track("Guide", |t| {
            t.guid("{CCCCCCCC-0002-0000-0000-000000000000}")
                .item(0.0, 17.142857, |i| {
                    i.name("Guide Cues C")
                        .guid("{CCCCCCCC-I002-0000-0000-000000000000}")
                })
        })
        .track("Synth Lead", |t| {
            t.guid("{CCCCCCCC-0003-0000-0000-000000000000}")
                .volume(0.75)
                .pan(-0.5)
                .item(1.714286, 13.714286, |i| {
                    i.name("Synth Lead Recording C")
                        .guid("{CCCCCCCC-I003-0000-0000-000000000000}")
                })
        })
        .build()
}

// ─── Builder Verification ────────────────────────────────────────────────────

#[test]
fn parse_song_a() {
    let project = build_song_a();

    assert_eq!(
        project.tracks.len(),
        5,
        "Song A: Click/Guide, Click, TRACKS, Guitar, Bass"
    );

    // Click item: 0→22s
    assert_eq!(project.tracks[1].items.len(), 1);
    assert_eq!(project.tracks[1].items[0].length, 22.0);

    // FTS markers present
    let marker_names: Vec<&str> = project
        .markers_regions
        .all
        .iter()
        .map(|m| m.name.as_str())
        .collect();
    assert!(marker_names.contains(&"PREROLL"));
    assert!(marker_names.contains(&"Count-In"));
    assert!(marker_names.contains(&"=START"));
    assert!(marker_names.contains(&"SONGSTART"));
    assert!(marker_names.contains(&"SONGEND"));
    assert!(marker_names.contains(&"=END"));
    assert!(marker_names.contains(&"POSTROLL"));

    // Bounds resolution
    let bounds = resolve_song_bounds(&project);
    assert_eq!(bounds.start, 0.0, "PREROLL is at 0");
    assert_eq!(bounds.end, 22.0, "POSTROLL is at 22");

    println!(
        "Song A: {} tracks, bounds {:.1}→{:.1}s, {} markers/regions",
        project.tracks.len(),
        bounds.start,
        bounds.end,
        project.markers_regions.all.len()
    );
}

#[test]
fn parse_song_b() {
    let project = build_song_b();

    assert_eq!(project.tracks.len(), 3, "Song B: Click, Keys, Drums");

    // Tempo: 90→110 BPM
    let env = project.tempo_envelope.as_ref().unwrap();
    assert_eq!(env.points.len(), 2);
    assert_eq!(env.points[0].tempo, 90.0);
    assert_eq!(env.points[1].tempo, 110.0);

    // Bounds: COUNT-IN at 0 → POSTROLL at 18
    let bounds = resolve_song_bounds(&project);
    assert_eq!(bounds.start, 0.0);
    assert_eq!(bounds.end, 18.0);

    // Has Solo Section region
    assert!(
        project
            .markers_regions
            .all
            .iter()
            .any(|m| m.name == "Solo Section" && m.end_position.is_some())
    );

    println!(
        "Song B: {} tracks, bounds {:.1}→{:.1}s",
        project.tracks.len(),
        bounds.start,
        bounds.end
    );
}

#[test]
fn parse_song_c() {
    let project = build_song_c();

    assert_eq!(project.tracks.len(), 3, "Song C: Click, Guide, Synth Lead");

    // Bounds: PREROLL at 0 → POSTROLL at ~17.14
    let bounds = resolve_song_bounds(&project);
    assert_eq!(bounds.start, 0.0);
    assert!((bounds.end - 17.142857).abs() < 0.01);

    println!(
        "Song C: {} tracks, bounds {:.1}→{:.1}s",
        project.tracks.len(),
        bounds.start,
        bounds.end
    );
}

// ─── RPL Parsing ─────────────────────────────────────────────────────────────

#[test]
fn parse_rpl_file() {
    let rpl_path = fixture_path("test_setlist.RPL");
    let paths = setlist_rpp::parse_rpl(&rpl_path).unwrap();
    assert_eq!(paths.len(), 3);
    for p in &paths {
        assert!(p.exists(), "RPP should exist: {}", p.display());
    }
}

#[test]
fn song_name_extraction() {
    assert_eq!(
        setlist_rpp::song_name_from_path(&PathBuf::from("Belief - John Mayer [Battle SP26].RPP")),
        "Belief - John Mayer"
    );
    assert_eq!(
        setlist_rpp::song_name_from_path(&PathBuf::from("song_a.RPP")),
        "song_a"
    );
}

// ─── Bounds-Based Concatenation ──────────────────────────────────────────────

#[test]
fn concatenate_three_songs_with_bounds() {
    let projects = vec![build_song_a(), build_song_b(), build_song_c()];

    let gap = measures_to_seconds(2, 120.0, 4); // 4s gap

    // Build song infos from resolved bounds
    let names: Vec<&str> = vec!["Song A", "Song B", "Song C"];
    let songs = build_song_infos_from_projects(&projects, &names, gap);

    println!("\nSong layout with bounds:");
    for s in &songs {
        println!(
            "  {} @ {:.1}s, duration {:.1}s",
            s.name, s.global_start_seconds, s.duration_seconds
        );
    }

    // Song A: PREROLL(0) → POSTROLL(22) = 22s
    assert_eq!(songs[0].global_start_seconds, 0.0);
    assert_eq!(songs[0].duration_seconds, 22.0);

    // Song B: starts at 22 + 4 gap = 26s, COUNT-IN(0) → POSTROLL(18) = 18s
    assert_eq!(songs[1].global_start_seconds, 26.0);
    assert_eq!(songs[1].duration_seconds, 18.0);

    // Song C: starts at 26 + 18 + 4 gap = 48s
    assert_eq!(songs[2].global_start_seconds, 48.0);

    // Concatenate
    let combined = concatenate_projects(&projects, &songs);

    println!("\n═══ COMBINED PROJECT ═══\n");

    // Tracks
    println!("Tracks ({}):", combined.tracks.len());
    for (i, t) in combined.tracks.iter().enumerate() {
        let items = if t.items.is_empty() {
            String::new()
        } else {
            format!(" ({} items)", t.items.len())
        };
        println!("  {:>2}. {}{}", i, t.name, items);
    }

    // Tempo envelope — should have trailing tempo markers
    println!("\nTempo:");
    let env = combined.tempo_envelope.as_ref().unwrap();
    for pt in &env.points {
        println!(
            "  {:.2}s → {:.0} BPM (shape={})",
            pt.position, pt.tempo, pt.shape
        );
    }
    // All shapes should be 1 (square)
    for pt in &env.points {
        assert_eq!(
            pt.shape, 1,
            "All tempo points should be square, got shape={} at {:.2}s",
            pt.shape, pt.position
        );
    }

    // Markers/regions
    println!(
        "\nMarkers/Regions ({}):",
        combined.markers_regions.all.len()
    );
    for mr in &combined.markers_regions.all {
        let kind = if mr.end_position.is_some() {
            "RGN"
        } else {
            "MKR"
        };
        let end = mr
            .end_position
            .map(|e| format!("→{:.1}s", e))
            .unwrap_or_default();
        let lane = mr
            .lane
            .map(|l| format!(" [lane {}]", l))
            .unwrap_or_default();
        println!(
            "  [{:>2}] {} {:.1}s{} {:?}{}",
            mr.id, kind, mr.position, end, mr.name, lane
        );
    }

    // SONG regions should be in lane 3
    let song_regions: Vec<_> = combined
        .markers_regions
        .all
        .iter()
        .filter(|m| m.lane == Some(3) && m.is_region())
        .collect();
    assert_eq!(song_regions.len(), 3, "Should have 3 SONG lane regions");
    assert_eq!(song_regions[0].name, "Song A");
    assert_eq!(song_regions[1].name, "Song B");
    assert_eq!(song_regions[2].name, "Song C");

    // All IDs unique
    let ids: Vec<i32> = combined.markers_regions.all.iter().map(|m| m.id).collect();
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(unique.len(), ids.len(), "All IDs must be unique");

    println!("\n═══ ALL ASSERTIONS PASSED ═══\n");
}

// ─── Shell Copy Generation ───────────────────────────────────────────────────

#[test]
fn shell_copy_preserves_structure_strips_content() {
    let projects = vec![build_song_a(), build_song_b(), build_song_c()];

    let gap = measures_to_seconds(2, 120.0, 4);
    let names: Vec<&str> = vec!["Song A", "Song B", "Song C"];
    let songs = build_song_infos_from_projects(&projects, &names, gap);
    let master = concatenate_projects(&projects, &songs);

    // Generate a Vocals shell copy
    let shell = setlist_rpp::generate_shell_copy(&master, "Vocals");

    println!("\n═══ SHELL COPY: Vocals ═══\n");
    println!("Tracks ({}):", shell.tracks.len());
    for (i, t) in shell.tracks.iter().enumerate() {
        let folder = t
            .folder
            .as_ref()
            .map(|f| format!(" [{:?} indent={}]", f.folder_state, f.indentation))
            .unwrap_or_default();
        let items = if t.items.is_empty() {
            String::new()
        } else {
            format!(" ({} items)", t.items.len())
        };
        println!("  {:>2}. {}{}{}", i, t.name, folder, items);
    }

    // Should have Click/Guide tracks WITH items (performer needs the click)
    let click = shell.tracks.iter().find(|t| t.name == "Click");
    assert!(click.is_some(), "Shell should keep Click track");
    assert!(
        !click.unwrap().items.is_empty(),
        "Click track should keep its items"
    );

    // Should NOT have content tracks (Guitar, Bass, Keys, Drums, Synth Lead)
    let content_names = ["Guitar", "Bass", "Keys", "Drums", "Synth Lead"];
    for name in &content_names {
        assert!(
            !shell.tracks.iter().any(|t| t.name == *name),
            "Shell should NOT have {} track",
            name
        );
    }

    // Should NOT have Song A/B/C folders or TRACKS folder
    assert!(
        !shell.tracks.iter().any(|t| t.name == "TRACKS"),
        "No TRACKS folder"
    );
    assert!(
        !shell.tracks.iter().any(|t| t.name == "Song A"),
        "No Song A folder"
    );

    // Should have a Vocals role folder
    let vocals_folder = shell.tracks.iter().find(|t| t.name == "Vocals");
    assert!(vocals_folder.is_some(), "Should have Vocals folder");

    // Should preserve tempo envelope
    assert!(shell.tempo_envelope.is_some(), "Should keep tempo envelope");
    let env = shell.tempo_envelope.as_ref().unwrap();
    assert!(env.points.len() >= 3, "Should keep all tempo points");

    // Should preserve markers/regions
    assert!(
        !shell.markers_regions.all.is_empty(),
        "Should keep markers/regions"
    );

    println!("\nTempo points: {}", env.points.len());
    println!("Markers/regions: {}", shell.markers_regions.all.len());
    println!("\n═══ SHELL COPY VERIFIED ═══\n");
}

#[test]
fn generate_all_role_setlists() {
    let projects = vec![build_song_a(), build_song_b(), build_song_c()];

    let gap = measures_to_seconds(2, 120.0, 4);
    let names: Vec<&str> = vec!["Song A", "Song B", "Song C"];
    let songs = build_song_infos_from_projects(&projects, &names, gap);
    let master = concatenate_projects(&projects, &songs);

    // Generate all standard roles
    let roles = setlist_rpp::STANDARD_ROLES;
    let role_projects = setlist_rpp::generate_role_setlists(&master, roles);

    println!("\n═══ ROLE SETLISTS ═══\n");
    for (role, project) in &role_projects {
        let has_click = project
            .tracks
            .iter()
            .any(|t| t.name == "Click" && !t.items.is_empty());
        let has_role_folder = project.tracks.iter().any(|t| t.name == *role);
        // Check no content tracks remain (excluding the role folder itself and its placeholder)
        let content_names = ["TRACKS", "Song A", "Song B", "Song C", "Synth Lead"];
        let no_content = !project
            .tracks
            .iter()
            .any(|t| content_names.contains(&t.name.as_str()));

        println!(
            "  {} — {} tracks, click={}, role_folder={}, no_content={}",
            role,
            project.tracks.len(),
            has_click,
            has_role_folder,
            no_content
        );

        assert!(has_click, "{} should have Click with items", role);
        assert!(has_role_folder, "{} should have role folder", role);
        assert!(no_content, "{} should not have content tracks", role);
    }

    assert_eq!(role_projects.len(), roles.len());
    println!("\n═══ ALL {} ROLES VERIFIED ═══\n", roles.len());
}
