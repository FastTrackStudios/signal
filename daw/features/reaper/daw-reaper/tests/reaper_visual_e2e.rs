//! Full visual REAPER e2e — render a real Dioxus panel inside a real
//! REAPER, capture pixels, drive synthetic input, assert on visual
//! state changes.
//!
//! These tests require:
//!
//! 1. `FTS_VISUAL_TESTS=1` so the xtask rig builds + installs the
//!    `daw-reaper-dioxus-ext-test` cdylib alongside daw-bridge.
//! 2. A real GPU (or a fully-featured software rasterizer that
//!    supports vello's compute pipelines — lavapipe is NOT enough).
//!
//! Both prerequisites are skipped at test entry so default
//! `cargo xtask reaper-test` runs stay green.
//!
//! Run with:
//!
//!   FTS_VISUAL_TESTS=1 cargo xtask reaper-test -- reaper_visual_e2e

use std::path::Path;
use std::time::{Duration, Instant};

use daw::test::reaper_test;
use daw_proto::dock_host::{DockKind, UiEventDto};

const TEST_PANEL_ID: &str = "FTS_TEST_PANEL";
const TEST_PANEL_WIDTH: u32 = 320;
const TEST_PANEL_HEIGHT: u32 = 200;

fn visual_tests_enabled() -> bool {
    std::env::var("FTS_VISUAL_TESTS").is_ok_and(|v| !v.is_empty() && v != "0")
}

/// Poll `capture_pixels` for up to `timeout`, returning the first
/// non-empty buffer. Renders inside REAPER tick at ~30 Hz so 1s is
/// plenty for the initial paint.
async fn wait_for_pixels(
    dock: &daw_control::DockHost,
    handle: daw_proto::dock_host::DockHandle,
    timeout: Duration,
) -> eyre::Result<daw_proto::dock_host::PanelPixels> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Some(pixels) = dock.capture_pixels(handle).await?
            && !pixels.bgra.is_empty()
        {
            return Ok(pixels);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    eyre::bail!("test panel never rendered pixels within {timeout:?}");
}

#[reaper_test(isolated)]
async fn test_panel_renders_initial_frame(ctx: &ReaperTestContext) -> eyre::Result<()> {
    if !visual_tests_enabled() {
        ctx.log("FTS_VISUAL_TESTS not set — skipping visual e2e");
        return Ok(());
    }

    let dock = ctx.daw.dock_host();
    let handle = dock
        .register_dock(TEST_PANEL_ID, "Test Panel", DockKind::Floating)
        .await?;
    dock.show(handle).await?;

    let pixels = wait_for_pixels(&dock, handle, Duration::from_secs(3)).await?;

    assert_eq!(
        pixels.width, TEST_PANEL_WIDTH,
        "test panel width should match the cdylib's declared default"
    );
    assert_eq!(pixels.height, TEST_PANEL_HEIGHT);
    assert_eq!(
        pixels.bgra.len(),
        (pixels.width as usize) * (pixels.height as usize) * 4,
        "BGRA buffer must be width * height * 4"
    );

    // Lock visual regression once the test is real on the dev box. Kept
    // in tests/golden/ next to other reaper-asset fixtures.
    let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/test_panel_initial.bin");
    if std::env::var("FTS_GOLDEN_REFRESH").is_ok_and(|v| v == "1") {
        if let Some(parent) = golden.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&golden, &pixels.bgra)?;
        ctx.log(&format!("Refreshed golden -> {}", golden.display()));
    } else if golden.exists() {
        let expected = std::fs::read(&golden)?;
        if expected != pixels.bgra {
            let actual = golden.with_extension("actual.bin");
            std::fs::write(&actual, &pixels.bgra)?;
            eyre::bail!(
                "test panel pixel mismatch — expected {} bytes vs actual; dumped to {}",
                expected.len(),
                actual.display()
            );
        }
    } else {
        ctx.log(&format!(
            "No golden at {}; first run — set FTS_GOLDEN_REFRESH=1 to capture",
            golden.display()
        ));
    }

    Ok(())
}

#[reaper_test(isolated)]
async fn test_panel_click_changes_pixels(ctx: &ReaperTestContext) -> eyre::Result<()> {
    if !visual_tests_enabled() {
        ctx.log("FTS_VISUAL_TESTS not set — skipping visual interaction e2e");
        return Ok(());
    }

    let dock = ctx.daw.dock_host();
    let handle = dock
        .register_dock(TEST_PANEL_ID, "Test Panel", DockKind::Floating)
        .await?;
    dock.show(handle).await?;

    let before = wait_for_pixels(&dock, handle, Duration::from_secs(3)).await?;

    // Click the centered button. The component lays it out at panel
    // centre, so (w/2, h/2) hits it.
    let cx = (before.width as f32) / 2.0;
    let cy = (before.height as f32) / 2.0;
    dock.inject_event(handle, UiEventDto::PointerMove { x: cx, y: cy })
        .await?;
    dock.inject_event(
        handle,
        UiEventDto::PointerDown {
            x: cx,
            y: cy,
            button: 0,
        },
    )
    .await?;
    dock.inject_event(
        handle,
        UiEventDto::PointerUp {
            x: cx,
            y: cy,
            button: 0,
        },
    )
    .await?;

    // Poll until the pixels change. Component must re-render after the
    // click increments the signal.
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut after = before.clone();
    while Instant::now() < deadline {
        let snapshot = dock
            .capture_pixels(handle)
            .await?
            .ok_or_else(|| eyre::eyre!("post-click capture returned None"))?;
        if snapshot.bgra != before.bgra {
            after = snapshot;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    assert_ne!(
        after.bgra, before.bgra,
        "click should have re-rendered the counter, but pixels are identical"
    );

    Ok(())
}
