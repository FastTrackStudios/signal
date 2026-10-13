//! Pixel snapshot smoke tests for the panel-component testing layer.
//!
//! These tests render trivial Dioxus components to an off-screen GPU
//! surface and assert basic invariants on the readback buffer. They're
//! gated behind the `FTS_GPU_TESTS` env var because they require a
//! functioning GPU adapter (real or via lavapipe / mesa-llvmpipe) and
//! some CI runners don't have one.
//!
//! When implementing fts-extensions panels, mirror these tests for each
//! component to lock in visual regressions.

use daw_reaper_dioxus::prelude::*;
use daw_reaper_dioxus::snapshot::{compare_to_golden, render_panel_offscreen};

fn gpu_tests_enabled() -> bool {
    std::env::var("FTS_GPU_TESTS").is_ok_and(|v| v != "0" && !v.is_empty())
}

fn empty_panel() -> Element {
    rsx! {
        div { style: "width: 64px; height: 64px; background: black;" }
    }
}

#[test]
fn render_returns_correct_buffer_size() {
    if !gpu_tests_enabled() {
        eprintln!("FTS_GPU_TESTS not set — skipping GPU-backed snapshot test");
        return;
    }
    let pixels = render_panel_offscreen(empty_panel, 64, 64).expect("offscreen render");
    assert_eq!(
        pixels.len(),
        64 * 64 * 4,
        "BGRA8 buffer must be width * height * 4 bytes"
    );
}

#[test]
fn first_run_creates_golden() {
    if !gpu_tests_enabled() {
        eprintln!("FTS_GPU_TESTS not set — skipping GPU-backed snapshot test");
        return;
    }
    let pixels = render_panel_offscreen(empty_panel, 32, 32).expect("offscreen render");
    let tmp = tempfile::tempdir().expect("tmpdir");
    let path = tmp.path().join("first_run.bin");
    assert!(!path.exists());
    let matched = compare_to_golden(&pixels, 32, 32, &path).expect("compare");
    assert!(matched, "first run must succeed and write the golden");
    assert!(path.exists(), "golden must have been written");
    let on_disk = std::fs::read(&path).expect("read golden");
    assert_eq!(on_disk, pixels);
}
