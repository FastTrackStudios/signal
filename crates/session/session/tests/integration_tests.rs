//! Integration tests for Session cell
//!
//! These tests demonstrate how the session cell uses daw-control

/// Test that demonstrates the session using daw-control to call transport commands
///
/// This test shows the pattern:
/// 1. Get a Daw handle from the test host
/// 2. Get the current project
/// 3. Call transport commands (play, stop)
#[tokio::test]
async fn test_session_transport_controls() -> eyre::Result<()> {
    // In a real test, we would:
    // 1. Start the DAW test host
    // 2. Get a Daw handle
    // 3. Call transport commands

    // For now, this is a placeholder showing the intended API usage:
    //
    // let project = Daw::current_project().await?;
    // project.transport().play().await?;
    // tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    // project.transport().stop().await?;

    Ok(())
}

/// Test that demonstrates session getting project info
#[tokio::test]
async fn test_session_project_access() -> eyre::Result<()> {
    // Intended API:
    // let project = Daw::current_project().await?;
    // println!("Project GUID: {}", project.guid());
    //
    // let transport = project.transport();
    // transport.play().await?;

    Ok(())
}
