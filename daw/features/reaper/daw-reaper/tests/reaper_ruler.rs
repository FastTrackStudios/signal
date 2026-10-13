//! Markers and regions inside a real REAPER.
//!
//! There were no REAPER tests for either, which is how the identity
//! problem stayed a design note rather than a known bug. Markers and
//! regions are ONE list in REAPER wearing one set of numbers, and the
//! renumber action reassigns them all at once.
//!
//! The renumbering rule itself is tested in `crate::renumber`, not
//! here. Driving REAPER's own renumber action from a test was tried
//! and abandoned: it never returned, and the run sat through its whole
//! timeout. A rule that is arithmetic on two maps should be asked
//! directly rather than through a DAW.
//!
//! Run with: `just reaper integration-test reaper_ruler`

use daw::test::reaper_test;
use daw_proto::marker::MarkerEvent;
use daw_proto::region::RegionEvent;

/// How long to wait for the 30 Hz poller.
const PATIENCE: std::time::Duration = std::time::Duration::from_secs(5);

async fn wait_for_marker(
    stream: &mut daw::rpc::EventStream<daw_proto::marker::MarkerStreamEvent>,
    mut wanted: impl FnMut(&MarkerEvent) -> bool,
) -> eyre::Result<MarkerEvent> {
    let deadline = tokio::time::Instant::now() + PATIENCE;
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            eyre::bail!("no matching marker event within {PATIENCE:?}");
        }
        match tokio::time::timeout(left, stream.recv()).await {
            Err(_) => eyre::bail!("no matching marker event within {PATIENCE:?}"),
            Ok(Ok(Some(event))) => {
                let event = event.get().event.clone();
                if wanted(&event) {
                    return Ok(event);
                }
            }
            Ok(Ok(None)) => eyre::bail!("the marker stream ended"),
            Ok(Err(error)) => eyre::bail!("the marker stream failed: {error}"),
        }
    }
}

async fn wait_for_region(
    stream: &mut daw::rpc::EventStream<daw_proto::region::RegionStreamEvent>,
    mut wanted: impl FnMut(&RegionEvent) -> bool,
) -> eyre::Result<RegionEvent> {
    let deadline = tokio::time::Instant::now() + PATIENCE;
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            eyre::bail!("no matching region event within {PATIENCE:?}");
        }
        match tokio::time::timeout(left, stream.recv()).await {
            Err(_) => eyre::bail!("no matching region event within {PATIENCE:?}"),
            Ok(Ok(Some(event))) => {
                let event = event.get().event.clone();
                if wanted(&event) {
                    return Ok(event);
                }
            }
            Ok(Ok(None)) => eyre::bail!("the region stream ended"),
            Ok(Err(error)) => eyre::bail!("the region stream failed: {error}"),
        }
    }
}

/// A marker's life, as a subscriber sees it.
#[reaper_test(isolated)]
async fn a_markers_whole_life_reaches_a_subscriber(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let markers = project.markers();
    let mut stream = markers.subscribe().await?;

    let id = markers.add(12.0, "SONGSTART").await?;
    wait_for_marker(
        &mut stream,
        |event| matches!(event, MarkerEvent::Added(m) if m.id == Some(id) && m.name == "SONGSTART"),
    )
    .await?;

    markers.move_to(id, 16.0).await?;
    wait_for_marker(&mut stream, |event| {
        matches!(event, MarkerEvent::Changed(m)
            if m.id == Some(id) && (m.position_seconds() - 16.0).abs() < 1e-6)
    })
    .await?;

    // Renaming reads the position back first, because REAPER's setter
    // takes no "leave this alone" sentinel — passing -1.0 for position
    // moves the marker to -1 seconds rather than leaving it.
    markers.rename(id, "SONGEND").await?;
    wait_for_marker(
        &mut stream,
        |event| matches!(event, MarkerEvent::Changed(m) if m.name == "SONGEND"),
    )
    .await?;
    let after = markers.get(id).await?.ok_or_else(|| eyre::eyre!("gone"))?;
    assert!(
        (after.position_seconds() - 16.0).abs() < 1e-6,
        "renaming moved the marker to {}",
        after.position_seconds()
    );

    markers.remove(id).await?;
    wait_for_marker(
        &mut stream,
        |event| matches!(event, MarkerEvent::Removed(gone) if *gone == id),
    )
    .await?;

    Ok(())
}

/// A region's life, including the lane that makes it a song section.
#[reaper_test(isolated)]
async fn a_regions_whole_life_reaches_a_subscriber(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let regions = project.regions();
    let mut stream = regions.subscribe().await?;

    let id = regions.add(0.0, 8.0, "Intro").await?;
    wait_for_region(
        &mut stream,
        |event| matches!(event, RegionEvent::Added(r) if r.id == Some(id) && r.name == "Intro"),
    )
    .await?;

    regions.set_bounds(id, 0.0, 16.0).await?;
    wait_for_region(&mut stream, |event| {
        matches!(event, RegionEvent::Changed(r)
            if r.id == Some(id) && (r.end_seconds() - 16.0).abs() < 1e-6)
    })
    .await?;

    // The lane is what makes a region a section rather than just a
    // region, so it has to survive a read — and REAPER's own getter
    // does not round-trip it, which is why the backend shadows it.
    regions.set_lane(id, Some(1)).await?;
    let back = regions.get(id).await?.ok_or_else(|| eyre::eyre!("gone"))?;
    assert_eq!(back.lane, Some(1), "the SECTIONS lane did not stick");

    regions.remove(id).await?;
    wait_for_region(
        &mut stream,
        |event| matches!(event, RegionEvent::Removed(gone) if *gone == id),
    )
    .await?;

    Ok(())
}
