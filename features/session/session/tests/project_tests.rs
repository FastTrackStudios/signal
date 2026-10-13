//! REAPER integration tests for project tab management.
//!
//! Verifies creating, switching, listing, and closing project tabs.
//!
//! Run with:
//!   cargo xtask reaper-test -- project_

use daw::test::reaper_test;
use std::time::Duration;

/// List all open projects.
#[reaper_test]
async fn project_list(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let projects = ctx.daw.projects().await?;
    println!("Open projects: {}", projects.len());
    for (i, p) in projects.iter().enumerate() {
        println!("  [{i}] GUID: {}", p.guid());
    }

    if projects.is_empty() {
        return Err(eyre::eyre!(
            "Should have at least 1 open project (the test project)"
        ));
    }

    // Our test project should be among them
    let test_guid = ctx.project.guid().to_string();
    let found = projects.iter().any(|p| p.guid() == test_guid);
    if !found {
        return Err(eyre::eyre!(
            "Test project GUID should be in the project list"
        ));
    }

    Ok(())
}

/// Get current project matches the test project.
#[reaper_test]
async fn project_current(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    // Select our test project to make it current
    ctx.daw.select_project(ctx.project.guid()).await?;
    tokio::time::sleep(Duration::from_millis(200)).await;

    let current = ctx.daw.current_project().await?;
    println!("Current project GUID: {}", current.guid());
    println!("Test project GUID:    {}", ctx.project.guid());

    if current.guid() != ctx.project.guid() {
        return Err(eyre::eyre!(
            "Current project should be the test project after select"
        ));
    }

    Ok(())
}

/// Create a new project tab, verify it exists, then close it.
#[reaper_test]
async fn project_create_and_close(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let before = ctx.daw.projects().await?;
    let count_before = before.len();
    println!("Projects before create: {count_before}");

    // Create a new project tab
    let new_project = ctx.daw.create_project().await?;
    let new_guid = new_project.guid().to_string();
    tokio::time::sleep(Duration::from_millis(300)).await;

    let after_create = ctx.daw.projects().await?;
    println!("Projects after create: {}", after_create.len());
    let expected_count = count_before.saturating_add(1);
    if after_create.len() != expected_count {
        return Err(eyre::eyre!(
            "Should have one more project after create, expected {}, got {}",
            expected_count,
            after_create.len()
        ));
    }

    let found = after_create.iter().any(|p| p.guid() == new_guid);
    if !found {
        return Err(eyre::eyre!("New project GUID should be in the list"));
    }

    // Close the new project
    // Remove all tracks first to avoid save dialog
    new_project.tracks().remove_all().await?;
    ctx.daw.close_project(&new_guid).await?;
    tokio::time::sleep(Duration::from_millis(300)).await;

    let after_close = ctx.daw.projects().await?;
    println!("Projects after close: {}", after_close.len());
    if after_close.len() != count_before {
        return Err(eyre::eyre!(
            "Should be back to original count after close, expected {}, got {}",
            count_before,
            after_close.len()
        ));
    }

    let still_exists = after_close.iter().any(|p| p.guid() == new_guid);
    if still_exists {
        return Err(eyre::eyre!("Closed project should no longer be in list"));
    }

    Ok(())
}

/// Switch between project tabs and verify correct project is active.
#[reaper_test]
async fn project_switch(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    // Create a second project
    let project_b = ctx.daw.create_project().await?;
    let guid_b = project_b.guid().to_string();
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Switch to project B
    ctx.daw.select_project(&guid_b).await?;
    tokio::time::sleep(Duration::from_millis(200)).await;

    let current = ctx.daw.current_project().await?;
    println!("After switching to B: current={}", current.guid());
    if current.guid() != guid_b {
        return Err(eyre::eyre!("Current should be project B"));
    }

    // Switch back to test project
    ctx.daw.select_project(ctx.project.guid()).await?;
    tokio::time::sleep(Duration::from_millis(200)).await;

    let current = ctx.daw.current_project().await?;
    println!("After switching back: current={}", current.guid());
    if current.guid() != ctx.project.guid() {
        return Err(eyre::eyre!("Current should be test project"));
    }

    // Cleanup: close project B
    project_b.tracks().remove_all().await?;
    ctx.daw.close_project(&guid_b).await?;

    Ok(())
}

/// Each project tab has independent transport state when switched to.
/// REAPER transport operations apply to the currently active project tab,
/// so we switch between tabs and verify each remembers its position.
#[reaper_test]
async fn project_independent_transport(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    // Create a second project
    let project_b = ctx.daw.create_project().await?;
    let guid_b = project_b.guid().to_string();
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Switch to project A, set position
    ctx.daw.select_project(ctx.project.guid()).await?;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let transport_a = ctx.project.transport();
    transport_a.stop().await?;
    transport_a.set_position(10.0).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Switch to project B, set a different position
    ctx.daw.select_project(&guid_b).await?;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let transport_b = project_b.transport();
    transport_b.stop().await?;
    transport_b.set_position(50.0).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Read project B position (currently active)
    let pos_b = transport_b.get_position().await?;
    println!("Project B position: {pos_b:.2}s (expected ~50s)");
    if (pos_b - 50.0).abs() >= 1.0 {
        return Err(eyre::eyre!("Project B should be at ~50s, got {pos_b:.2}"));
    }

    // Switch back to project A and verify it kept its position
    ctx.daw.select_project(ctx.project.guid()).await?;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let pos_a = transport_a.get_position().await?;
    println!("Project A position: {pos_a:.2}s (expected ~10s)");
    if (pos_a - 10.0).abs() >= 1.0 {
        return Err(eyre::eyre!("Project A should be at ~10s, got {pos_a:.2}"));
    }

    // Cleanup
    ctx.daw.select_project(&guid_b).await?;
    tokio::time::sleep(Duration::from_millis(200)).await;
    project_b.tracks().remove_all().await?;
    ctx.daw.close_project(&guid_b).await?;

    Ok(())
}

/// Get project by GUID.
#[reaper_test]
async fn project_get_by_guid(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let guid = ctx.project.guid().to_string();

    let project = ctx.daw.project(&guid).await?;
    println!("Got project by GUID: {}", project.guid());
    if project.guid() != guid {
        return Err(eyre::eyre!("GUID should match"));
    }

    Ok(())
}
