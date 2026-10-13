//! Integration test for the DawFileService — exercises the migration of
//! the CLI-only `rpp_summary` op into a service callable by any client.
//!
//! Run with:
//!
//!   cargo xtask reaper-test -- reaper_dawfile_service

use daw::test::reaper_test;

#[reaper_test(isolated)]
async fn summarize_project_returns_track_count(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let dawfile = ctx.daw.dawfile();

    // Use a known fixture from dawfile-reaper's resources directory
    // (features/dawfile/dawfile-reaper in the monorepo layout).
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dawfile/dawfile-reaper/resources")
        .join("Basic Template (Tracks Only) .RPP")
        .canonicalize()
        .expect("dawfile-reaper fixture RPP should exist");

    let summary = dawfile
        .summarize_project(path.display().to_string())
        .await?;

    assert!(
        summary.error.is_empty(),
        "summarize_project should succeed: {}",
        summary.error
    );
    assert_eq!(summary.path, path.display().to_string());
    assert!(
        !summary.version_string.is_empty(),
        "version string must be parsed"
    );
    assert!(
        summary.track_count > 0,
        "Basic Template should report at least one track"
    );
    assert_eq!(
        summary.tracks.len() as u32,
        summary.track_count,
        "tracks vec must agree with track_count"
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn summarize_project_reports_io_error(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let summary = ctx
        .daw
        .dawfile()
        .summarize_project("/nonexistent/path/to/project.RPP")
        .await?;
    assert!(
        !summary.error.is_empty(),
        "missing-path summary must surface the read error"
    );
    assert_eq!(summary.track_count, 0);
    Ok(())
}
