//! Scene-graph snapshot tests for the EQ graph painter.
//!
//! These run without a GPU — they only call [`paint_eq_graph_scene`] against
//! a fresh anyrender `Scene` and inspect the recorded `commands` (count,
//! emptiness). Goal: catch regressions in the scene-builder logic (grid,
//! curves, nodes) before they ship, independent of wgpu availability.

use eq_ui::eq_graph::{EqBand, EqBandShape, StereoMode};
use eq_ui::eq_graph_model::{EqGraphRenderState, GraphConfig};
use eq_ui::eq_graph_painter::paint_eq_graph_scene;
// The painter records into an anyrender `Scene` (we are moving off vello's
// scene type directly). Its `commands` vec is what we assert on.
use nice_plug_dioxus::prelude::vello::kurbo::Affine;
use nice_plug_dioxus::widget::Scene;

const W: u32 = 800;
const H: u32 = 350;

fn make_state(bands: Vec<EqBand>, spectrum: Vec<f32>) -> EqGraphRenderState {
    // EqGraphRenderState::new() returns Arc<Self>; we want owned for tests.
    EqGraphRenderState {
        bands: parking_lot::RwLock::new(bands),
        spectrum_db: parking_lot::RwLock::new(spectrum),
        model_response_db: parking_lot::RwLock::new(Vec::new()),
        analyzer: parking_lot::RwLock::new(Default::default()),
        config: parking_lot::RwLock::new(GraphConfig {
            rect_w: W as f64,
            rect_h: H as f64,
            ..GraphConfig::default()
        }),
        interaction: parking_lot::RwLock::new(Default::default()),
        overlay: parking_lot::RwLock::new(None),
    }
}

#[test]
fn model_response_curve_adds_paths_without_bands() {
    let state = EqGraphRenderState {
        bands: parking_lot::RwLock::new(Vec::new()),
        spectrum_db: parking_lot::RwLock::new(Vec::new()),
        model_response_db: parking_lot::RwLock::new(
            (0..256)
                .map(|i| ((i as f32 / 255.0) * std::f32::consts::TAU).sin() * 6.0)
                .collect(),
        ),
        analyzer: parking_lot::RwLock::new(Default::default()),
        config: parking_lot::RwLock::new(GraphConfig {
            rect_w: W as f64,
            rect_h: H as f64,
            ..GraphConfig::default()
        }),
        interaction: parking_lot::RwLock::new(Default::default()),
        overlay: parking_lot::RwLock::new(None),
    };
    let scene = paint(&state);
    assert!(scene.commands.len() > 4);
}

fn band(idx: usize, freq: f32, gain: f32, q: f32, shape: EqBandShape) -> EqBand {
    EqBand {
        index: idx,
        used: true,
        enabled: true,
        frequency: freq,
        gain,
        q,
        shape,
        solo: false,
        stereo_mode: StereoMode::default(),
        name: String::new(),
    }
}

fn paint(state: &EqGraphRenderState) -> Scene {
    let mut scene = Scene::new();
    paint_eq_graph_scene(&mut scene, state, Affine::IDENTITY, W, H);
    scene
}

#[test]
fn empty_state_still_paints_background_and_grid() {
    // Spectrum needs >=2 entries to be drawn; bands list is empty.
    let state = make_state(vec![], vec![-60.0; 256]);
    let scene = paint(&state);
    assert!(
        !scene.commands.is_empty(),
        "scene must not be empty even with no bands (background + grid + spectrum)"
    );
    assert!(
        scene.commands.len() > 4,
        "expected at least background + several grid lines (got {})",
        scene.commands.len()
    );
}

#[test]
fn zero_size_canvas_renders_nothing() {
    let state = make_state(
        vec![band(0, 1000.0, 6.0, 1.0, EqBandShape::Bell)],
        vec![-60.0; 256],
    );
    let mut scene = Scene::new();
    // Zero size — painter should early-return without writing anything.
    paint_eq_graph_scene(&mut scene, &state, Affine::IDENTITY, 0, H);
    assert!(
        scene.commands.is_empty(),
        "zero-width paint must short-circuit"
    );

    let mut scene = Scene::new();
    paint_eq_graph_scene(&mut scene, &state, Affine::IDENTITY, W, 0);
    assert!(
        scene.commands.is_empty(),
        "zero-height paint must short-circuit"
    );
}

#[test]
fn each_active_band_adds_paths() {
    let baseline = make_state(vec![], vec![-60.0; 256]);
    let baseline_paths = paint(&baseline).commands.len();

    let with_one = make_state(
        vec![band(0, 1000.0, 6.0, 1.0, EqBandShape::Bell)],
        vec![-60.0; 256],
    );
    let one_paths = paint(&with_one).commands.len();
    assert!(
        one_paths > baseline_paths,
        "adding one band should add paths to the scene (baseline={baseline_paths}, with_one={one_paths})"
    );

    let with_three = make_state(
        vec![
            band(0, 80.0, -3.0, 0.7, EqBandShape::LowShelf),
            band(1, 1000.0, 6.0, 1.0, EqBandShape::Bell),
            band(2, 8000.0, -4.0, 0.9, EqBandShape::HighShelf),
        ],
        vec![-60.0; 256],
    );
    let three_paths = paint(&with_three).commands.len();
    assert!(
        three_paths > one_paths,
        "three bands should produce more paths than one (one={one_paths}, three={three_paths})"
    );
}

#[test]
fn disabled_band_paints_node_but_skips_curve() {
    let mut b = band(0, 1000.0, 6.0, 1.0, EqBandShape::Bell);
    b.enabled = true;
    let enabled_paths = paint(&make_state(vec![b.clone()], vec![-60.0; 256]))
        .commands
        .len();

    b.enabled = false;
    let disabled_paths = paint(&make_state(vec![b], vec![-60.0; 256])).commands.len();

    assert!(
        disabled_paths < enabled_paths,
        "disabled band should skip its curve (enabled={enabled_paths}, disabled={disabled_paths})"
    );
}

#[test]
fn unused_band_is_fully_skipped() {
    let mut b = band(0, 1000.0, 6.0, 1.0, EqBandShape::Bell);
    b.used = false;
    let unused_paths = paint(&make_state(vec![b], vec![-60.0; 256])).commands.len();
    let empty_paths = paint(&make_state(vec![], vec![-60.0; 256])).commands.len();
    assert_eq!(
        unused_paths, empty_paths,
        "unused bands must not contribute paths or nodes"
    );
}

#[test]
fn spectrum_without_data_is_skipped() {
    let no_spec = paint(&make_state(vec![], vec![])).commands.len();
    let one_spec = paint(&make_state(vec![], vec![-60.0])).commands.len();
    assert_eq!(
        no_spec, one_spec,
        "spectrum needs >=2 bins to be drawn — len 0 and 1 must produce same scene"
    );

    let many_spec = paint(&make_state(vec![], vec![-60.0; 256])).commands.len();
    assert!(
        many_spec > no_spec,
        "spectrum with 256 bins must add paths beyond the empty-spectrum baseline"
    );
}
