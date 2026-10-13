//! Pixel snapshot helpers for panel components.
//!
//! Drives a Dioxus root component through one off-screen render tick,
//! reads back BGRA8 bytes, and exposes them for hashing / golden
//! comparison. Designed for tests that want to verify a component's
//! visual output without REAPER, a window, or a desktop session.
//!
//! # Headless / no-GPU environments
//!
//! Some CI runners have no GPU adapter. The helpers return
//! `SnapshotError::Gpu` when wgpu can't acquire a device. Tests should
//! skip themselves on such envs:
//!
//! ```rust,ignore
//! #[test]
//! fn my_panel_snapshot() {
//!     if std::env::var("FTS_GPU_TESTS").is_err() {
//!         eprintln!("FTS_GPU_TESTS not set — skipping");
//!         return;
//!     }
//!     // ... call snapshot helpers
//! }
//! ```
//!
//! On Linux CI, lavapipe / mesa-software-rasterizer can provide a
//! software GPU adapter that satisfies wgpu without a real device.

use crate::EmbeddedView;
use daw_reaper_embed::GpuError;
use dioxus_native::prelude::Element;
use std::path::Path;

/// Errors from the snapshot pipeline.
#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("GPU init failed: {0}")]
    Gpu(#[from] GpuError),

    #[error("offscreen render produced no pixels (likely a GPU readback failure — see logs)")]
    NoPixels,

    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Render a Dioxus root component to an off-screen surface and return
/// the resulting BGRA8 pixel buffer (`width * height * 4` bytes).
///
/// The component runs through one full layout + paint tick. Long-running
/// effects (timers, async tasks awaiting external state) won't have time
/// to settle in a single tick — keep snapshot subjects deterministic.
pub fn render_panel_offscreen(
    app: fn() -> Element,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, SnapshotError> {
    let mut view = EmbeddedView::new_offscreen(app, width, height, Vec::new())?;
    // First tick: vdom build is already done in `new_offscreen`; this
    // resolves layout + paints the scene + reads back pixels.
    view.update();
    let pixels = view.bgra_pixels().ok_or(SnapshotError::NoPixels)?;
    Ok(pixels.to_vec())
}

/// Compare a freshly-rendered panel against a golden BGRA8 byte file.
///
/// On mismatch, writes `<golden_path>.actual.bin` next to the golden so
/// the test author can diff or convert to PNG manually. Returns
/// `Ok(true)` on match, `Ok(false)` on mismatch.
///
/// The first run (no golden file) writes the captured pixels to the
/// golden path and returns `Ok(true)` so the next run has a baseline.
/// Set `FTS_GOLDEN_REFRESH=1` in the env to force-overwrite an existing
/// golden.
pub fn compare_to_golden(
    actual: &[u8],
    width: u32,
    height: u32,
    golden_path: &Path,
) -> Result<bool, SnapshotError> {
    let expected_len = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(4);
    if actual.len() != expected_len {
        return Err(SnapshotError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "actual buffer is {} bytes, expected {} (w={width} h={height})",
                actual.len(),
                expected_len
            ),
        )));
    }

    let refresh = std::env::var("FTS_GOLDEN_REFRESH").is_ok_and(|v| v == "1");

    if !golden_path.exists() || refresh {
        if let Some(parent) = golden_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(golden_path, actual)?;
        return Ok(true);
    }

    let golden = std::fs::read(golden_path)?;
    if golden == actual {
        return Ok(true);
    }

    // Mismatch — dump actual next to golden for inspection.
    let actual_path = golden_path.with_extension("actual.bin");
    std::fs::write(&actual_path, actual)?;
    Ok(false)
}
