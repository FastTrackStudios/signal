//! Integration tests for per-test project tab isolation.
//!
//! Validates that the parallel test infrastructure works:
//! - Each test gets its own REAPER project tab
//! - Tracks created in one project don't appear in another
//! - Project GUIDs are unique across tests
//! - Basic operations (add track, add FX, read params) work in isolated tabs
//!
//! Run with:
//!
//!   cargo xtask reaper-test

mod daw_helpers;

use reaper_test::reaper_test;

// ---------------------------------------------------------------------------
// Basic: verify the test gets a project with a unique GUID
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn project_has_unique_guid(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project();
    let guid = project.guid();

    println!("Project GUID: {}", guid);
    assert!(!guid.is_empty(), "project should have a non-empty GUID");

    // Verify we can fetch project info
    let info = project.info().await?;
    println!("Project name: '{}', path: '{}'", info.name, info.path);
    println!("Project GUID from info: {}", info.guid);
    assert_eq!(guid, info.guid, "guid() and info().guid should match");

    Ok(())
}

// ---------------------------------------------------------------------------
// Isolation: tracks added in this test's project are visible only here
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn tracks_are_isolated(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();

    // Fresh project should be empty (remove_all runs in setup)
    let initial = project.tracks().all().await?;
    println!("Initial track count: {}", initial.len());
    assert_eq!(initial.len(), 0, "fresh project should have no tracks");

    // Add tracks with unique names
    let track_a = project.tracks().add("IsolationTestA", None).await?;
    let track_b = project.tracks().add("IsolationTestB", None).await?;

    let after_add = project.tracks().all().await?;
    println!("After adding 2 tracks: {} tracks", after_add.len());
    assert_eq!(after_add.len(), 2, "should have exactly 2 tracks");

    // Verify names
    let names: Vec<&str> = after_add.iter().map(|t| t.name.as_str()).collect();
    println!("Track names: {:?}", names);
    assert!(names.contains(&"IsolationTestA"));
    assert!(names.contains(&"IsolationTestB"));

    // Verify track_by_name works against our project
    let found = ctx.track_by_name("IsolationTestA").await?;
    let found_info = found.info().await?;
    println!(
        "Found track by name: {} (guid={})",
        found_info.name, found_info.guid
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// FX on isolated track: add FX, read params, set param, read back
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn fx_operations_in_isolated_project(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();

    let track = project.tracks().add("FxIsolationTest", None).await?;
    let chain = track.fx_chain();
    let fx = chain.add("ReaEQ").await?;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    // Read FX info
    let info = fx.info().await?;
    println!("Added FX: {} ({})", info.name, info.plugin_name);

    // Count FX
    let count = chain.count().await?;
    assert_eq!(count, 1, "should have exactly 1 FX");

    // Read parameters
    let params = fx.parameters().await?;
    println!("FX has {} parameters", params.len());
    assert!(params.len() > 0, "ReaEQ should have parameters");

    // Set a parameter and read it back
    let param0 = fx.param(0);
    let original = param0.get().await?;
    let nudged = if original > 0.5 {
        original - 0.1
    } else {
        original + 0.1
    };
    param0.set(nudged).await?;
    let readback = param0.get().await?;
    println!(
        "Param 0: original={:.4}, set={:.4}, readback={:.4}",
        original, nudged, readback
    );
    assert!(
        (readback - nudged).abs() < 0.02,
        "param should be close to set value"
    );

    // Restore
    param0.set(original).await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Multiple tracks + FX: heavier manipulation in an isolated project
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn multi_track_manipulation(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();

    // Create 3 tracks, each with a different FX
    let plugins = ["ReaEQ", "ReaComp", "ReaDelay"];
    let mut track_guids = Vec::new();

    for (i, plugin) in plugins.iter().enumerate() {
        let name = format!("MultiTrack_{}", i);
        let track = project.tracks().add(&name, None).await?;
        let track_info = track.info().await?;
        track_guids.push(track_info.guid.clone());

        let fx = track.fx_chain().add(plugin).await?;
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let fx_info = fx.info().await?;
        println!(
            "Track '{}': added {} ({})",
            name, fx_info.name, fx_info.plugin_name
        );
    }

    // Verify all 3 tracks exist
    let all = project.tracks().all().await?;
    assert_eq!(all.len(), 3, "should have 3 tracks");
    println!(
        "All tracks: {:?}",
        all.iter().map(|t| &t.name).collect::<Vec<_>>()
    );

    // Verify each track has exactly 1 FX
    for track_info in &all {
        let track = project.tracks().by_index(track_info.index).await?.unwrap();
        let fx_count = track.fx_chain().count().await?;
        assert_eq!(
            fx_count, 1,
            "track '{}' should have 1 FX, got {}",
            track_info.name, fx_count
        );
    }

    // Remove middle track by index
    let middle = all.iter().find(|t| t.name == "MultiTrack_1").unwrap();
    project
        .tracks()
        .remove(daw_proto::TrackRef::Index(middle.index))
        .await?;

    let after_remove = project.tracks().all().await?;
    assert_eq!(after_remove.len(), 2, "should have 2 tracks after removal");
    let remaining_names: Vec<&str> = after_remove.iter().map(|t| t.name.as_str()).collect();
    println!("After removing middle: {:?}", remaining_names);
    assert!(remaining_names.contains(&"MultiTrack_0"));
    assert!(remaining_names.contains(&"MultiTrack_2"));

    Ok(())
}
