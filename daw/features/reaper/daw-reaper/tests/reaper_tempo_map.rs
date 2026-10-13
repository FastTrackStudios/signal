//! Integration tests for the TempoMap service (#24).
//!
//! Round-trip coverage:
//! - add tempo point → query points → assert presence
//! - query tempo at the point's time matches the requested BPM
//! - time → musical → time round-trip is identity (within tolerance)
//! - add → remove → query: point disappears from list
//!
//! Run with: `cargo xtask reaper-test -- reaper_tempo_map`

use daw::test::reaper_test;

const BPM_TOLERANCE: f64 = 0.01;
const TIME_TOLERANCE: f64 = 0.001;

#[reaper_test(isolated)]
async fn add_then_query_tempo_point(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let tempo_map = project.tempo_map();

    let initial_count = tempo_map.count().await?;
    let _idx = tempo_map.add_point(8.0, 140.0).await?;
    let after_count = tempo_map.count().await?;
    assert_eq!(
        after_count,
        initial_count + 1,
        "add_point should grow the count by exactly one"
    );

    // The tempo at the inserted point's time should match.
    let queried = tempo_map.tempo_at(8.0).await?;
    assert!(
        (queried - 140.0).abs() < BPM_TOLERANCE,
        "tempo_at({}) returned {} BPM, expected 140.0",
        8.0,
        queried
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn time_to_musical_round_trip(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let tempo_map = project.tempo_map();

    // Use a time far enough into the project that any default 4/4 120 BPM
    // grid lands cleanly on a measure boundary.
    let original_seconds = 12.0_f64;
    let (measure, beat, fraction) = tempo_map.time_to_musical(original_seconds).await?;
    let round_tripped = tempo_map.musical_to_time(measure, beat, fraction).await?;

    assert!(
        (round_tripped - original_seconds).abs() < TIME_TOLERANCE,
        "time→musical→time should be identity: {} → ({}m {}b {}) → {}",
        original_seconds,
        measure,
        beat,
        fraction,
        round_tripped,
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn remove_point_drops_from_list(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let tempo_map = project.tempo_map();

    let initial_count = tempo_map.count().await?;
    let idx = tempo_map.add_point(4.0, 100.0).await?;
    let after_add = tempo_map.count().await?;
    assert_eq!(after_add, initial_count + 1, "add did not take");

    tempo_map.remove_point(idx).await?;
    let after_remove = tempo_map.count().await?;
    assert_eq!(
        after_remove, initial_count,
        "remove_point should restore the count"
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn time_signature_at_returns_default(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let tempo_map = project.tempo_map();

    let (num, denom) = tempo_map.time_signature_at(0.0).await?;
    assert!(
        num > 0 && denom > 0,
        "time_signature_at should return a valid (num, denom), got ({num}, {denom})"
    );
    // REAPER's default project is 4/4; allow any sensible default by
    // checking ranges rather than asserting the exact value, since the
    // default could change with REAPER preferences.
    assert!(num <= 16 && denom <= 32, "implausible time signature");

    Ok(())
}
