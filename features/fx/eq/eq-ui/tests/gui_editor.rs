//! Behavioral tests for the REAL FTS-EQ plugin editor UI.
//!
//! Unlike tests/gui_headless.rs (a synthetic component around the curve
//! generators), these mount `eq_ui::control_view::App` — the exact Dioxus
//! surface the CLAP/VST3 plugin embeds — on the vendored dioxus-test harness
//! (headless Blitz DOM, no GPU, no window) and drive it with real hit-tested
//! pointer events, including a graph drag that must change band parameters.
//!
//! Requires the `native` feature (declared via `[[test]] required-features`):
//!
//! ```sh
//! cargo test -p eq-ui --features native --test gui_editor
//! ```
//!
//! ## Harness notes / workarounds
//!
//! - `App` injects its CSS through `document::Style` head elements. The
//!   headless harness has no head-element provider (dioxus falls back to
//!   `NoOpDocument`, which drops them), so the [`support::Harness`] wrapper
//!   re-injects the same stylesheets as ordinary body `<style>` elements —
//!   blitz-dom processes `<style>` anywhere in the tree. Without them every
//!   Tailwind class (`flex-1`, `relative`, …) is undefined and the graph
//!   area collapses to zero height.
//! - The EQ graph itself is painted by a blitz *custom widget* attached to
//!   an `<object>` node (no SVG, no wgpu canvas in this path). Headless, the
//!   widget's `paint()` never runs — which is exactly why this works without
//!   a GPU — and `EqGraph` falls back to its fixed 800×350 viewBox for
//!   hit-testing (`graph_width`/`graph_height` fallback in eq_graph.rs). The
//!   tests reuse the same `GraphMapper` with those dimensions to compute
//!   where band nodes live on screen.
//! - The spectrum analyzer engine (`EqUiState::analyzer`) is pure CPU math
//!   and runs fine headless; no stubbing was needed.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use dioxus_test::{matchers::inner_html, render, DocumentTester};
use test_that::prelude::*;

use eq_ui::control_view::App;
use eq_ui::eq_graph_interaction::GraphMapper;
use eq_ui::params::{EqUiState, FtsEqParams};

use audiocore_core::prelude::Param;
use nice_plug_dioxus::{ParamContext, SharedState};

// ─────────────────────────────────────────────────────────────────────────
// Fixture
// ─────────────────────────────────────────────────────────────────────────

mod support {
    use super::*;
    use dioxus::prelude::*;
    use nice_plug::context::gui::{GuiContext, GuiContextInner};
    use nice_plug::prelude::*;
    use std::collections::BTreeMap;

    /// One recorded host automation call.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub enum Gesture {
        Begin(usize),
        Set(usize, f32),
        End(usize),
    }

    /// Stable identity for a parameter, usable across threads (raw `ParamPtr`
    /// is not `Send`, so the log stores the pointer address instead).
    pub fn ptr_key(p: ParamPtr) -> usize {
        match p {
            ParamPtr::FloatParam(p) => p as usize,
            ParamPtr::IntParam(p) => p as usize,
            ParamPtr::BoolParam(p) => p as usize,
            ParamPtr::EnumParam(p) => p as usize,
        }
    }

    /// Host stub: records every begin/set/end gesture into a shared log AND
    /// applies the value to the parameter (like the standalone's
    /// `StandaloneGuiContext`), so the UI re-renders against the new values —
    /// required for multi-step drags to accumulate correctly.
    pub struct RecordingGuiContext {
        pub log: Arc<Mutex<Vec<Gesture>>>,
    }

    impl GuiContextInner for RecordingGuiContext {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        unsafe fn raw_begin_set_parameter(&self, param: ParamPtr) {
            self.log.lock().unwrap().push(Gesture::Begin(ptr_key(param)));
        }
        unsafe fn raw_set_parameter_normalized(&self, param: ParamPtr, normalized: f32) {
            self.log
                .lock()
                .unwrap()
                .push(Gesture::Set(ptr_key(param), normalized));
            unsafe { param._internal_set_normalized_value(normalized) };
        }
        unsafe fn raw_end_set_parameter(&self, param: ParamPtr) {
            self.log.lock().unwrap().push(Gesture::End(ptr_key(param)));
        }
        fn get_state(&self) -> PluginState {
            PluginState {
                version: String::new(),
                params: BTreeMap::new(),
                fields: BTreeMap::new(),
            }
        }
        fn set_state(&self, _state: PluginState) {}
    }

    /// Root component: the real editor `App`, plus its stylesheets re-hosted
    /// as body `<style>` elements (see module docs — `document::Style` head
    /// elements are dropped by the headless harness's NoOpDocument fallback).
    #[component]
    pub fn Harness() -> Element {
        rsx! {
            style {
                "html, body {{ width:100%; height:100%; margin:0; padding:0; overflow:hidden; }}"
            }
            style { {nice_plug_dioxus::TAILWIND_CSS} }
            style { {include_str!("../assets/tailwind.css")} }
            App {}
        }
    }

    pub struct Fixture {
        pub tester: DocumentTester,
        pub params: Arc<FtsEqParams>,
        pub log: Arc<Mutex<Vec<Gesture>>>,
    }

    /// Mounts the real editor with every context `AppShell` consumes:
    ///
    /// - `ParamContext` over the [`RecordingGuiContext`] stub (what
    ///   `use_param_context()` / all `ctx.begin_set_raw(..)` call sites hit),
    /// - `SharedState` wrapping `Arc<EqUiState>` (params + meters + analyzer),
    /// - `Arc<dyn TrackInfoProvider>` (`StaticTrackProvider::none()`, so the
    ///   cheat-sheet overlay's Auto mode resolves to off).
    ///
    /// The window-size signal `nice-plug-dioxus`'s windowed shell provides is
    /// only consumed by `ResizeHandle` via `try_use_context`, which tolerates
    /// its absence — so it is deliberately not stubbed.
    pub fn mount() -> Fixture {
        let params = Arc::new(FtsEqParams::default());
        let ui_state = Arc::new(EqUiState::new(params.clone()));
        let log = Arc::new(Mutex::new(Vec::new()));

        let gui = GuiContext::new(Arc::new(RecordingGuiContext { log: log.clone() }));
        let param_ctx = ParamContext::new(gui, Arc::new(AtomicBool::new(true)));
        let track: Arc<dyn eq_ui::cheatsheet::TrackInfoProvider> =
            Arc::new(eq_ui::cheatsheet::StaticTrackProvider::none());

        let tester = render(Harness)
            .with_window_size(1600, 1000)
            .with_root_context(param_ctx)
            .with_root_context(SharedState::new(ui_state))
            .with_root_context(track)
            .build();

        Fixture { tester, params, log }
    }

    impl Fixture {
        /// The `GraphMapper` matching what `EqGraph` uses headless: fixed
        /// 800×350 viewBox fallback (the custom-widget painter that would
        /// publish the live canvas size never runs without a renderer),
        /// default 20 Hz–20 kHz range, and the default ±3 dB display range
        /// (`db_range` param index 0).
        pub fn mapper(&self) -> GraphMapper {
            GraphMapper::new(20.0, 20_000.0, 3.0, 800.0, 350.0, 0.0)
        }

        /// Document-space origin of the EQ graph interaction surface. The
        /// graph `<object>` (custom-widget node) fills the wrapper div that
        /// owns the mouse handlers, so its origin is the wrapper's origin.
        pub fn graph_origin(&self) -> (f64, f64) {
            let obj = self
                .tester
                .query("object")
                .immediately()
                .expect("EQ graph <object> node not in DOM");
            obj.document_origin()
        }

        /// Document-space position of band `idx`'s node.
        pub fn band_point(&self, idx: usize) -> (f64, f64) {
            let mapper = self.mapper();
            let bp = &self.params.bands[idx];
            let (ox, oy) = self.graph_origin();
            (
                ox + mapper.freq_to_x(bp.freq_hz.value() as f64),
                oy + mapper.db_to_y(bp.gain_db.value() as f64),
            )
        }
    }
}

use support::{mount, ptr_key, Gesture};

// ─────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────

/// The graph-only editor mounts headless: the graph widget node has real
/// (non-collapsed) layout and the EqGraph subtree rendered its band name
/// labels. The header / inspector / bottom bar are parked behind
/// `SHOW_CHROME = false` in control_view.rs and must NOT render.
#[tokio::test]
async fn editor_mounts_headless_graph_only() -> dioxus_test::Result<()> {
    let fx = mount();

    // The graph custom-widget node exists and got a real (non-collapsed)
    // layout box — i.e. the re-injected Tailwind actually applied and the
    // fixed 800×350 hit-test area fits inside it.
    let obj = fx.tester.query("object").immediately()?;
    let (w, h) = obj.size();
    assert!(w > 800.0, "graph surface too narrow for hit-testing: {w}px");
    assert!(h > 350.0, "graph surface too short for hit-testing: {h}px");

    // EqGraph subtree rendered: the default Gregory-Scott bands are named and
    // their DOM name labels are present.
    fx.tester
        .query(":root")
        .expect(inner_html(contains_substring("Low Shelf")))
        .immediately()?;
    fx.tester
        .query(":root")
        .expect(inner_html(contains_substring("High Shelf")))
        .immediately()?;

    // Chrome is parked: no header title, no inspector.
    let html = fx.tester.query(":root").immediately()?.inner_html();
    assert!(!html.contains("Inspector"), "inspector chrome leaked into the graph-only editor");
    Ok(())
}

/// A hit-tested click on band 2's node (High Shelf @ 2.5 kHz) focuses it:
/// the graph's band info popup appears with the band's frequency label
/// ("2.5k"). A pure selection click must NOT touch any parameter.
#[tokio::test]
async fn clicking_a_band_node_focuses_it_and_shows_its_popup() -> dioxus_test::Result<()> {
    let fx = mount();
    let (x, y) = fx.band_point(1);

    fx.tester.pointer_down(x, y);
    let _ = fx.tester.pump().await;
    fx.tester.pointer_up(x, y);
    let _ = fx.tester.pump().await;

    fx.tester
        .query(":root")
        .expect(inner_html(contains_substring("2.5k")))
        .await?;

    // Selection alone is not an automation gesture.
    let log = fx.log.lock().unwrap();
    assert!(log.is_empty(), "selection click recorded gestures: {log:?}");
    Ok(())
}

/// THE drag test: press on band 1's node (Low Shelf @ 400 Hz, 0 dB), drag
/// +40 px right and −30 px up in steps, release. The band's frequency and
/// gain params must change through recorded host gestures, in the right
/// directions (right → higher frequency, up → higher gain), landing where
/// the graph mapper says the pointer is.
#[tokio::test]
async fn dragging_a_band_node_changes_frequency_and_gain() -> dioxus_test::Result<()> {
    let fx = mount();
    let mapper = fx.mapper();
    let bp = &fx.params.bands[0];
    let freq_key = ptr_key(bp.freq_hz.as_ptr());
    let gain_key = ptr_key(bp.gain_db.as_ptr());

    let freq_before = bp.freq_hz.value();
    let gain_before = bp.gain_db.value();
    assert!((freq_before - 400.0).abs() < 1.0, "band 1 default freq: {freq_before}");
    assert!(gain_before.abs() < 1e-6, "band 1 default gain: {gain_before}");

    let (sx, sy) = fx.band_point(0);
    let (dx, dy) = (40.0, -30.0);

    fx.tester.pointer_down(sx, sy);
    let _ = fx.tester.pump().await;
    for step in 1..=4 {
        let t = step as f64 / 4.0;
        fx.tester.pointer_move(sx + dx * t, sy + dy * t, true);
        let _ = fx.tester.pump().await;
    }
    fx.tester.pointer_up(sx + dx, sy + dy);
    let _ = fx.tester.pump().await;

    // Where the mapper says the final pointer position lands. `band_point`
    // used the graph origin, so subtract it back out for element coords.
    let (ox, oy) = fx.graph_origin();
    let expected_freq = mapper.x_to_freq(sx + dx - ox) as f32;
    let expected_gain = mapper.y_to_db(sy + dy - oy) as f32;

    let freq_after = bp.freq_hz.value();
    let gain_after = bp.gain_db.value();

    // Direction sanity: dragged right → frequency up; dragged up → gain up.
    assert!(
        freq_after > freq_before,
        "drag right did not raise frequency: {freq_before} → {freq_after}"
    );
    assert!(
        gain_after > gain_before,
        "drag up did not raise gain: {gain_before} → {gain_after}"
    );

    // Magnitude: the params landed where the graph mapper puts the pointer
    // (tolerances cover the normalized round-trip through the skewed range).
    assert!(
        (freq_after - expected_freq).abs() < expected_freq * 0.02,
        "freq landed at {freq_after} Hz, mapper expected {expected_freq} Hz"
    );
    assert!(
        (gain_after - expected_gain).abs() < 0.05,
        "gain landed at {gain_after} dB, mapper expected {expected_gain} dB"
    );

    // The host saw real automation gestures for both params: begin, at
    // least one set per drag step, and end.
    let log = fx.log.lock().unwrap();
    for (name, key) in [("freq", freq_key), ("gain", gain_key)] {
        let begins = log.iter().filter(|g| matches!(g, Gesture::Begin(k) if *k == key)).count();
        let sets = log.iter().filter(|g| matches!(g, Gesture::Set(k, _) if *k == key)).count();
        let ends = log.iter().filter(|g| matches!(g, Gesture::End(k) if *k == key)).count();
        assert!(begins >= 1, "no begin gesture for {name}");
        assert!(sets >= 4, "expected ≥4 set gestures for {name}, got {sets}");
        assert!(ends >= 1, "no end gesture for {name}");
    }
    // The recorded set values for frequency are monotonically nondecreasing —
    // each drag step moved the band right, never back.
    let freq_sets: Vec<f32> = log
        .iter()
        .filter_map(|g| match g {
            Gesture::Set(k, v) if *k == freq_key => Some(*v),
            _ => None,
        })
        .collect();
    assert!(
        freq_sets.windows(2).all(|w| w[1] >= w[0]),
        "frequency sets not monotonic: {freq_sets:?}"
    );

    Ok(())
}
