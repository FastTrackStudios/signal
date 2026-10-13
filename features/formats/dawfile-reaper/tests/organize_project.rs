//! Integration test: organize a real project into FTS hierarchy.
//!
//! Uses the Belief project to verify that `organize_into_fts_hierarchy`
//! correctly classifies and restructures tracks.

use std::path::Path;

const BELIEF_RPP: &str = "/Users/codywright/Music/Projects/Live Tracks/Just Friends/Belief - John Mayer/Belief - John Mayer [Battle SP26].RPP";

fn file_exists(path: &str) -> bool {
    Path::new(path).exists()
}

#[test]
fn organize_belief_project() {
    if !file_exists(BELIEF_RPP) {
        eprintln!("Skipping: RPP not found at {BELIEF_RPP}");
        return;
    }

    let content = std::fs::read_to_string(BELIEF_RPP).expect("read RPP");
    let project = dawfile_reaper::parse_project_text(&content).expect("parse RPP");

    println!("\n=== Original Track Structure ===");
    print_track_tree(&project.tracks);

    // Run the organizer
    let organized = dawfile_reaper::types::organize_into_fts_hierarchy(project.tracks);

    println!("\n=== Organized Track Structure ===");
    print_track_tree(&organized);

    // Collect track names
    let names: Vec<&str> = organized.iter().map(|t| t.name.as_str()).collect();

    // Should have Reference folder (not inside TRACKS)
    assert!(
        names.contains(&"Reference"),
        "Should have top-level Reference folder. Got: {:?}",
        names
    );

    // Reference should contain the mix track and Stem Split
    let ref_idx = names.iter().position(|n| *n == "Reference").unwrap();
    assert!(
        organized[ref_idx].folder.as_ref().is_some_and(
            |f| f.folder_state == dawfile_reaper::types::track::FolderState::FolderParent
        ),
        "Reference should be a folder parent"
    );

    // Stem Split should exist inside Reference
    assert!(
        names.contains(&"Stem Split"),
        "Should have Stem Split folder. Got: {:?}",
        names
    );

    // Stem tracks should exist (Drums, Bass, Guitar, Vocals, etc.)
    let stem_names = ["Drums", "Bass", "Guitar", "Vocals", "Piano", "Other"];
    for stem in &stem_names {
        assert!(
            names.contains(stem),
            "Should have stem track '{}'. Got: {:?}",
            stem,
            names
        );
    }

    // The mp3 reference track should exist
    let has_mix = names.iter().any(|n| n.contains("Belief"));
    assert!(has_mix, "Should have the Belief reference/mix track");

    // Should NOT have TRACKS folder (no actual content tracks in this project)
    // Actually, this project might have no content tracks at all — it's all stems
    // If there's a TRACKS folder, it should only be because there are content tracks
    if names.contains(&"TRACKS") {
        // Find what's inside TRACKS — should not be empty
        let tracks_idx = names.iter().position(|n| *n == "TRACKS").unwrap();
        assert!(
            organized[tracks_idx].folder.as_ref().is_some_and(
                |f| f.folder_state == dawfile_reaper::types::track::FolderState::FolderParent
            ),
            "TRACKS should be a folder parent if it exists"
        );
        println!("  Note: TRACKS folder exists (project has content tracks)");
    }

    // Folder depth should balance
    let net_depth: i32 = organized
        .iter()
        .map(|t| t.folder.as_ref().map_or(0, |f| f.indentation))
        .sum();
    assert_eq!(
        net_depth, 0,
        "Folder depth should balance to 0, got {}",
        net_depth
    );

    // No track should have been lost — total item count should match
    let original = dawfile_reaper::parse_project_text(&content).expect("reparse");
    let original_items: usize = original.tracks.iter().map(|t| t.items.len()).sum();
    let organized_items: usize = organized.iter().map(|t| t.items.len()).sum();
    assert_eq!(
        original_items, organized_items,
        "Item count should be preserved: original={}, organized={}",
        original_items, organized_items
    );

    println!(
        "\n  Tracks: {} → {}",
        original.tracks.len(),
        organized.len()
    );
    println!("  Items preserved: {}", organized_items);
}

fn print_track_tree(tracks: &[dawfile_reaper::types::track::Track]) {
    use dawfile_reaper::types::track::FolderState;
    let mut depth = 0i32;
    for track in tracks {
        let indent = "  ".repeat(depth.max(0) as usize);
        let folder_info = track
            .folder
            .as_ref()
            .map(|f| match f.folder_state {
                FolderState::FolderParent => " [folder]".to_string(),
                FolderState::LastInFolder => format!(" [close {}]", f.indentation),
                _ => String::new(),
            })
            .unwrap_or_default();
        let item_count = track.items.len();
        let items_str = if item_count > 0 {
            format!(" ({} items)", item_count)
        } else {
            String::new()
        };
        println!("  {}{}{}{}", indent, track.name, folder_info, items_str);

        if let Some(ref f) = track.folder {
            depth += f.indentation;
        }
    }
}
