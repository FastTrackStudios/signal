//! Behavioral tests for the REAL FTS Trigger plugin editor UI.
//!
//! Mounts `trigger_ui::control_view::App` — the exact Dioxus surface the
//! CLAP/VST3 plugin embeds — on the vendored dioxus-test harness (headless
//! Blitz DOM, no GPU, no window) and drives it with real hit-tested pointer
//! events, including a threshold-line drag that must change the parameter
//! through recorded host automation gestures.
//!
//! Requires the `native` feature (declared via `[[test]] required-features`):
//!
//! ```sh
//! cargo test -p trigger-ui --features native --test gui_editor
//! ```
//!
//! ## Harness notes
//!
//! - `App` injects its CSS through `document::Style` head elements. The
//!   headless harness has no head-element provider (dioxus falls back to
//!   `NoOpDocument`, which drops them), so the [`support::Harness`] wrapper
//!   re-injects the same stylesheets as ordinary body `<style>` elements —
//!   blitz-dom processes `<style>` anywhere in the tree. Without them every
//!   Tailwind class (`flex-1`, `grid`, …) is undefined and the layout
//!   collapses.
//! - The analysis-waveform container is pinned to `GRAPH_H` CSS px, so
//!   element-relative pointer y IS viewBox y — threshold-drag targets are
//!   computed with `db_to_y(.., GRAPH_H)` plus the container origin.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use dioxus_test::{
    by_testid,
    matchers::{contains_substring, inner_html},
    render, DocumentTester,
};

use trigger_ui::control_view::App;
use trigger_ui::params::{TriggerParams, TriggerUiState};
use trigger_ui::trigger_waveform::GRAPH_H;
use trigger_ui::trigger_waveform_svg::db_to_y;

use audiocore_core::prelude::Param;
use nice_plug_dioxus::{ParamContext, SharedState};

// ─────────────────────────────────────────────────────────────────────────
// Fixture
// ─────────────────────────────────────────────────────────────────────────

mod support {
    use super::*;
    use dioxus::prelude::*;
    use nice_plug::context::gui::GuiContext;
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

    impl GuiContext for RecordingGuiContext {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        fn request_resize(&self) -> bool {
            true
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
        pub params: Arc<TriggerParams>,
        /// Kept alive for the mounted editor; tests read the params instead.
        pub _ui_state: Arc<TriggerUiState>,
        pub log: Arc<Mutex<Vec<Gesture>>>,
    }

    /// Mounts the real editor with every context `AppShell` consumes:
    ///
    /// - `ParamContext` over the [`RecordingGuiContext`] stub (what
    ///   `use_param_context()` / all `ctx.begin_set_raw(..)` call sites hit),
    /// - `SharedState` wrapping `Arc<TriggerUiState>` (params + rings).
    ///
    /// The waveform + hit rings are pre-seeded so the peak bars and hit
    /// markers actually render (an all-silent ring is an empty path).
    pub fn mount() -> Fixture {
        let params = Arc::new(TriggerParams::default());
        let ui_state = Arc::new(TriggerUiState::new(params.clone()));
        let log = Arc::new(Mutex::new(Vec::new()));

        // Seed the display: a ramp of audible peaks + two hits with distinct
        // velocities inside the visible window.
        for k in 0..256u32 {
            ui_state.input_wave.push(0.05 + 0.9 * (k as f32 / 255.0));
        }
        ui_state.hits.push(200, 0.9);
        ui_state.hits.push(250, 0.4);

        let gui: Arc<dyn GuiContext> = Arc::new(RecordingGuiContext { log: log.clone() });
        let param_ctx = ParamContext::new(gui, Arc::new(AtomicBool::new(true)));

        let tester = render(Harness)
            .with_window_size(1200, 700)
            .with_root_context(param_ctx)
            .with_root_context(SharedState::new(ui_state.clone()))
            .build();

        Fixture { tester, params, _ui_state: ui_state, log }
    }

    impl Fixture {
        /// Document-space origin of the analysis-waveform interaction
        /// surface. The graph container is pinned to GRAPH_H CSS px, so
        /// element y IS viewBox y — points computed with
        /// `db_to_y(.., GRAPH_H)` plus this origin land exactly where the
        /// component hit-tests them.
        pub fn graph_origin(&self) -> (f64, f64) {
            self.tester
                .query(by_testid("trigger-graph"))
                .immediately()
                .expect("trigger-graph container not in DOM")
                .document_origin()
        }

        /// The graph container's inner HTML (bars path, markers, threshold
        /// line).
        pub fn graph_html(&self) -> String {
            self.tester
                .query(by_testid("trigger-graph"))
                .immediately()
                .expect("trigger-graph container not in DOM")
                .inner_html()
        }
    }
}

use support::{mount, ptr_key, Gesture};

// ─────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────

/// The editor mounts headless: the header, the waveform (peak bars, dB
/// scale, threshold line), all eight knobs, both segmented selects, the note
/// stepper, and the Listen toggle render with real (non-collapsed) layout.
#[tokio::test]
async fn editor_mounts_headless_with_waveform_and_controls() -> dioxus_test::Result<()> {
    let fx = mount();

    let html = fx.tester.query(":root").immediately()?.inner_html();
    assert!(html.contains("FTS Trigger"), "header title missing");
    for name in [
        "Threshold",
        "Sensitivity",
        "Retrigger",
        "Dynamics",
        "SC HPF",
        "SC LPF",
        "Vel Min",
        "Vel Max",
        "Listen",
    ] {
        assert!(html.contains(name), "control label {name:?} missing from DOM");
    }
    // Segmented select options.
    for opt in ["Peak Env", "SuperFlux", "Mod KL", "Linear", "Log", "Exp", "Fixed"] {
        assert!(html.contains(opt), "select option {opt:?} missing from DOM");
    }
    // Note stepper shows the default note number.
    assert!(html.contains("(36)"), "note stepper value missing from DOM");

    // Every knob + the selects/steppers got a real (non-collapsed) box.
    for id in [
        "knob-threshold",
        "knob-sensitivity",
        "knob-retrigger",
        "knob-dynamics",
        "knob-sc-hpf",
        "knob-sc-lpf",
        "knob-vel-min",
        "knob-vel-max",
        "select-algorithm",
        "select-curve",
        "note-dec",
        "note-inc",
        "toggle-listen",
    ] {
        let el = fx.tester.query(by_testid(id)).immediately()?;
        let (w, h) = el.size();
        assert!(
            w > 10.0 && h > 10.0,
            "control {id} collapsed to {w}x{h}px — Tailwind/layout broken"
        );
    }

    // The graph container is pinned to GRAPH_H (pointer↔viewBox mapping).
    let el = fx.tester.query(by_testid("trigger-graph")).immediately()?;
    let (w, h) = el.size();
    assert!(w > 300.0, "graph too narrow: {w}px");
    assert!(
        (h as f64 - GRAPH_H).abs() < 1.0,
        "graph height {h}px != pinned {GRAPH_H}px — pointer↔viewBox mapping broken"
    );

    // Waveform chrome: seeded peak bars, threshold line, dB scale, readout.
    let graph = fx.graph_html();
    assert!(graph.contains("wave-bars"), "peak-bar path missing from graph");
    assert!(graph.contains("threshold-line"), "threshold line missing from graph");
    for db_label in ["-6", "-12", "-24", "-48"] {
        assert!(graph.contains(db_label), "dB scale label {db_label:?} missing");
    }
    assert!(graph.contains("Threshold"), "threshold readout missing from graph");
    // Default threshold value is rendered (readout + grab chip).
    fx.tester
        .query(by_testid("trigger-graph"))
        .expect(inner_html(contains_substring("-30.0")))
        .immediately()?;
    Ok(())
}

/// THE drag test: grab the threshold line (drawn at db_to_y(−30 dB)) and
/// drag it 45 px down. The threshold must fall to the dB the pointer lands
/// on (60 dB over GRAPH_H px), through recorded host gestures — begin, one
/// monotonically-falling set per move, end.
#[tokio::test]
async fn dragging_threshold_line_lowers_threshold_to_pointer_db() -> dioxus_test::Result<()> {
    let fx = mount();
    let tp = &fx.params.threshold_db;
    let key = ptr_key(tp.as_ptr());
    let before = tp.value();
    assert!((before - (-30.0)).abs() < 1e-4, "default threshold: {before}");

    let (gx, gy) = fx.graph_origin();
    let ty = db_to_y(before as f64, GRAPH_H); // 130 px for −30 dB
    let (sx, sy) = (gx + 180.0, gy + ty);

    fx.tester.pointer_down(sx, sy);
    let _ = fx.tester.pump().await;
    for step in 1..=3 {
        fx.tester.pointer_move(sx, sy + 15.0 * step as f64, true);
        let _ = fx.tester.pump().await;
    }
    fx.tester.pointer_up(sx, sy + 45.0);
    let _ = fx.tester.pump().await;

    // 45 px down on the 60 dB / GRAPH_H px scale.
    let after = tp.value();
    assert!(after < before, "drag down did not lower threshold: {before} → {after}");
    let expected = -(((ty + 45.0) / GRAPH_H) * 60.0) as f32;
    assert!(
        (after - expected).abs() < 0.5,
        "threshold landed at {after} dB, expected ~{expected} dB"
    );

    // Real host gestures: begin, one set per move (monotonically falling —
    // every step moved down), end.
    let log = fx.log.lock().unwrap();
    let begins = log.iter().filter(|g| matches!(g, Gesture::Begin(k) if *k == key)).count();
    let ends = log.iter().filter(|g| matches!(g, Gesture::End(k) if *k == key)).count();
    let sets: Vec<f32> = log
        .iter()
        .filter_map(|g| match g {
            Gesture::Set(k, v) if *k == key => Some(*v),
            _ => None,
        })
        .collect();
    assert!(begins >= 1, "no begin gesture for threshold: {log:?}");
    assert!(ends >= 1, "no end gesture for threshold: {log:?}");
    assert!(sets.len() >= 3, "expected ≥3 set gestures, got {}", sets.len());
    assert!(
        sets.windows(2).all(|w| w[1] <= w[0]),
        "threshold sets not monotonically falling: {sets:?}"
    );

    // The rendered threshold line moved with the param.
    let graph = fx.graph_html();
    assert!(
        graph.contains(&format!("{after:.1} dB")),
        "threshold readout did not track the drag"
    );
    Ok(())
}

/// Press-and-release on the threshold line without moving is not a value
/// change: no Set gesture is recorded and the param keeps its default. A
/// click away from the line (outside the ±16 px grab zone) records nothing
/// at all.
#[tokio::test]
async fn clicking_without_dragging_changes_nothing() -> dioxus_test::Result<()> {
    let fx = mount();
    let tp = &fx.params.threshold_db;
    let key = ptr_key(tp.as_ptr());
    let before = tp.value();

    let (gx, gy) = fx.graph_origin();
    let ty = db_to_y(before as f64, GRAPH_H);

    // Click ON the line: a grab, but no movement → begin/end only, no Set.
    fx.tester.pointer_down(gx + 180.0, gy + ty);
    let _ = fx.tester.pump().await;
    fx.tester.pointer_up(gx + 180.0, gy + ty);
    let _ = fx.tester.pump().await;

    // Click far from the line (top of the graph): not a grab at all.
    fx.tester.pointer_down(gx + 180.0, gy + 8.0);
    let _ = fx.tester.pump().await;
    fx.tester.pointer_up(gx + 180.0, gy + 8.0);
    let _ = fx.tester.pump().await;

    assert_eq!(tp.value(), before, "click alone moved the threshold");
    let log = fx.log.lock().unwrap();
    let sets = log.iter().filter(|g| matches!(g, Gesture::Set(k, _) if *k == key)).count();
    assert_eq!(sets, 0, "click without drag recorded value sets: {log:?}");
    Ok(())
}
