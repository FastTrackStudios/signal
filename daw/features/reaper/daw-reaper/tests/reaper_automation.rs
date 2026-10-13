//! Integration tests for the Automation service (#22 / #24).
//!
//! Round-trip coverage for the Phase 1 envelope read + point CRUD path:
//! - List envelopes on a track.
//! - Add a point, read back, assert presence and value.
//! - Update the point, re-read, assert the change took.
//! - Delete the point, re-read, assert it's gone.
//!
//! Run with: `cargo xtask reaper-test -- reaper_automation`

use daw::test::reaper_test;
use daw_proto::primitives::PositionInSeconds;
use daw_proto::{EnvelopeShape, EnvelopeType};

const POSITION_TOLERANCE_SECS: f64 = 0.001;
const VALUE_TOLERANCE: f64 = 1e-6;

#[reaper_test(isolated)]
async fn list_envelopes_returns_volume_for_new_track(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let track = project.tracks().add("Envelope Test", None).await?;
    let envelopes = track.envelopes().all().await?;

    // A fresh track always has at least the post-FX Volume envelope —
    // REAPER materialises it lazily but our chunk-name lookup finds it.
    assert!(
        envelopes
            .iter()
            .any(|e| matches!(e.envelope_type, EnvelopeType::Volume)),
        "Volume envelope should be discoverable on a new track, got: {:?}",
        envelopes
            .iter()
            .map(|e| e.envelope_type)
            .collect::<Vec<_>>()
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn add_then_read_point_round_trip(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let track = project.tracks().add("Envelope Round Trip", None).await?;
    let env = track.envelopes().volume();

    let target_time = 2.5_f64;
    let target_value = 0.75_f64;
    let inserted_index = env
        .add_point_linear(PositionInSeconds::from_seconds(target_time), target_value)
        .await?;
    ctx.log(&format!("inserted point at index {inserted_index}"));

    let points = env.points().await?;
    assert!(
        !points.is_empty(),
        "envelope should have at least the point we just added"
    );
    let found = points
        .iter()
        .find(|p| (p.time.as_seconds() - target_time).abs() < POSITION_TOLERANCE_SECS)
        .expect("inserted point should be discoverable in the read-back");
    assert!(
        (found.value - target_value).abs() < VALUE_TOLERANCE,
        "value mismatch: expected {target_value}, got {}",
        found.value
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn update_point_changes_value(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let track = project.tracks().add("Envelope Update", None).await?;
    let env = track.envelopes().volume();

    let idx = env
        .add_point_linear(PositionInSeconds::from_seconds(1.0), 0.25)
        .await?;

    env.set_point(
        idx,
        PositionInSeconds::from_seconds(1.0),
        0.9,
        EnvelopeShape::Linear,
    )
    .await?;

    let points = env.points().await?;
    let updated = points
        .iter()
        .find(|p| (p.time.as_seconds() - 1.0).abs() < POSITION_TOLERANCE_SECS)
        .expect("updated point should still be present at t=1.0");
    assert!(
        (updated.value - 0.9).abs() < VALUE_TOLERANCE,
        "set_point did not take: expected 0.9, got {}",
        updated.value
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn delete_point_removes_it(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let track = project.tracks().add("Envelope Delete", None).await?;
    let env = track.envelopes().volume();

    let _ = env
        .add_point_linear(PositionInSeconds::from_seconds(1.0), 0.25)
        .await?;
    let idx_2 = env
        .add_point_linear(PositionInSeconds::from_seconds(2.0), 0.5)
        .await?;

    let before = env.points().await?;
    let count_before = before
        .iter()
        .filter(|p| {
            let t = p.time.as_seconds();
            (t - 1.0).abs() < POSITION_TOLERANCE_SECS || (t - 2.0).abs() < POSITION_TOLERANCE_SECS
        })
        .count();
    assert_eq!(count_before, 2, "both inserted points should be present");

    env.delete_point(idx_2).await?;

    let after = env.points().await?;
    let still_t1 = after
        .iter()
        .any(|p| (p.time.as_seconds() - 1.0).abs() < POSITION_TOLERANCE_SECS);
    let still_t2 = after
        .iter()
        .any(|p| (p.time.as_seconds() - 2.0).abs() < POSITION_TOLERANCE_SECS);
    assert!(still_t1, "point at t=1.0 should still be present");
    assert!(
        !still_t2,
        "point at t=2.0 should have been deleted, but it's still there"
    );

    Ok(())
}
