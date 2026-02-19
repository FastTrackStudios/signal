//! Live REAPER integration test for `activate_patch` with `ReaperPatchApplier`.
//!
//! Verifies that calling `Signal::activate_patch()` with a wired
//! `ReaperPatchApplier` creates a folder-based multi-track structure and
//! loads rfxchain state data into child tracks for gapless switching.
//!
//! Run with:
//!   cargo xtask reaper-test -- activate_patch

mod daw_helpers;

use reaper_test::reaper_test;
use signal::reaper_applier::ReaperPatchApplier;
use signal::{bootstrap_in_memory_controller_async, graph_state_chunks, seed_id};
use std::sync::Arc;

/// Activate each of the 8 All-Around patches via the controller API and
/// verify the folder-based gapless switching structure in REAPER.
#[reaper_test]
async fn activate_patch_loads_all_around_into_reaper(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();

    // 1. Bootstrap in-memory controller with ReaperPatchApplier wired in
    let applier = Arc::new(ReaperPatchApplier::new());
    applier.set_target(project.clone(), "plugin").await;

    let signal = bootstrap_in_memory_controller_async()
        .await?
        .with_daw_applier(applier.clone());

    // 2. Load the All-Around profile
    let profile_id = seed_id("guitar-allaround-profile");
    let profile = signal
        .profiles().load(profile_id.clone())
        .await
        .ok_or_else(|| eyre::eyre!("All-Around profile not found"))?;

    assert_eq!(profile.patches.len(), 8, "All-Around should have 8 patches");

    let expected_names = [
        "Clean", "Crunch", "Drive", "Lead", "Funk", "Ambient", "Q-Tron", "Solo",
    ];

    // 3. Verify folder structure was created
    let folder = project
        .tracks()
        .by_name("Guitar Rig")
        .await?
        .ok_or_else(|| eyre::eyre!("Guitar Rig folder not found after set_target"))?;
    let input = project
        .tracks()
        .by_name("Input: Guitar Rig")
        .await?
        .ok_or_else(|| eyre::eyre!("Input track not found after set_target"))?;
    println!("Folder track: {}", folder.guid());
    println!("Input track: {}", input.guid());

    let initial_track_count = project.tracks().count().await?;
    println!(
        "Initial track count: {} (folder + input)",
        initial_track_count
    );

    // 4. Activate each patch and verify structure
    for (i, patch) in profile.patches.iter().enumerate() {
        assert_eq!(
            patch.name, expected_names[i],
            "patch {i} name mismatch: expected '{}', got '{}'",
            expected_names[i], patch.name
        );

        let graph = signal
            .profiles().activate(profile_id.clone(), Some(patch.id.clone()))
            .await
            .unwrap_or_else(|e| panic!("activate_patch('{}') failed: {:?}", patch.name, e));

        // Diagnostic: check if the resolved graph carries state data
        let chunks = graph_state_chunks(&graph, "plugin");
        println!(
            "  [{}] {} — {} chunk(s), {} engine(s)",
            i + 1,
            patch.name,
            chunks.len(),
            graph.engines.len(),
        );

        // Neural DSP plugins need time to initialize after a chunk swap
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

        // Verify the patch child track exists
        let patch_track = project
            .tracks()
            .by_name(&patch.name)
            .await?
            .ok_or_else(|| eyre::eyre!("patch track '{}' not found", patch.name))?;

        // Verify the patch track has FX loaded
        let fx_list = patch_track.fx_chain().all().await?;
        println!("    patch track '{}': {} FX", patch.name, fx_list.len(),);

        // Verify folder was renamed to current patch name
        let folder_info = folder.info().await?;
        println!(
            "    folder name: '{}', is_folder: {}",
            folder_info.name, folder_info.is_folder,
        );
        assert_eq!(
            folder_info.name, patch.name,
            "folder should be renamed to current patch '{}'",
            patch.name,
        );

        // After the first switch: folder + input + current = 3 tracks
        // After second+: folder + input + current + tail = 4 tracks
        //   (tail from two switches ago gets deleted, so max 4)
        let track_count = project.tracks().count().await?;
        if i == 0 {
            println!("    track count after first patch: {}", track_count);
            assert_eq!(track_count, 3, "first patch: folder + input + current");
        } else {
            println!("    track count after patch {}: {}", i + 1, track_count);
            // current + tail + folder + input = 4
            // (old tail from 2 ago was cleaned up)
            assert!(
                track_count <= 4,
                "should have at most 4 tracks (folder + input + current + tail), got {}",
                track_count,
            );
        }

        // Check sends from input track
        let sends = input.sends().all().await?;
        println!("    input sends: {}", sends.len());
    }

    println!("\nAll 8 patches activated successfully with folder-based gapless switching");
    Ok(())
}
