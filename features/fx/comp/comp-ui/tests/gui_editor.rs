//! Behavioral tests for the REAL FTS Comp plugin editor UI.
//!
//! Mounts `comp_ui::control_view::App` — the exact Dioxus surface the
//! CLAP/VST3 plugin embeds — on the vendored dioxus-test harness (headless
//! Blitz DOM, no GPU, no window) and drives it with real hit-tested pointer
//! events, including a knob drag that must change a parameter through
//! recorded host automation gestures.
//!
//! Requires the `native` feature (declared via `[[test]] required-features`):
//!
//! ```sh
//! cargo test -p comp-ui --features native --test gui_editor
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
//! - Knob dragging goes through `fts_ui_audio`'s `DragProvider`: mousedown on
//!   the knob's overlay starts the gesture, mousemoves anywhere in the window
//!   move the value (vertical, up = increase), mouseup ends it.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use dioxus_test::{
    by_testid,
    matchers::{contains_substring, inner_html},
    render, DocumentTester,
};

use comp_ui::control_view::App;
use comp_ui::params::{CompParams, CompUiState};

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
        pub params: Arc<CompParams>,
        pub log: Arc<Mutex<Vec<Gesture>>>,
    }

    /// Mounts the real editor with every context `AppShell` consumes:
    ///
    /// - `ParamContext` over the [`RecordingGuiContext`] stub (what
    ///   `use_param_context()` / all `ctx.begin_set_raw(..)` call sites hit),
    /// - `SharedState` wrapping `Arc<CompUiState>` (params + meters).
    pub fn mount() -> Fixture {
        let params = Arc::new(CompParams::default());
        let ui_state = Arc::new(CompUiState::new(params.clone()));
        let log = Arc::new(Mutex::new(Vec::new()));

        let gui: Arc<dyn GuiContext> = Arc::new(RecordingGuiContext { log: log.clone() });
        let param_ctx = ParamContext::new(gui, Arc::new(AtomicBool::new(true)));

        let tester = render(Harness)
            .with_window_size(1200, 700)
            .with_root_context(param_ctx)
            .with_root_context(SharedState::new(ui_state))
            .build();

        Fixture { tester, params, log }
    }

    impl Fixture {
        /// Document-space centre of the knob wrapper with the given testid.
        pub fn knob_center(&self, testid: &str) -> (f64, f64) {
            let el = self
                .tester
                .query(by_testid(testid))
                .immediately()
                .unwrap_or_else(|e| panic!("knob {testid} not in DOM: {e:?}"));
            let (ox, oy) = el.document_origin();
            let (w, h) = el.size();
            (ox + w as f64 / 2.0, oy + h as f64 / 2.0)
        }
    }
}

use support::{mount, ptr_key, Gesture};

// ─────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────

/// The editor mounts headless: the header, all eight classic-comp knobs, and
/// the GR meter render with real (non-collapsed) layout.
#[tokio::test]
async fn editor_mounts_headless_with_knobs_and_meters() -> dioxus_test::Result<()> {
    let fx = mount();

    let html = fx.tester.query(":root").immediately()?.inner_html();
    assert!(html.contains("FTS Comp"), "header title missing");
    for name in [
        "Threshold",
        "Ratio",
        "Attack",
        "Release",
        "Knee",
        "Makeup",
        "Mix",
        "Stereo Link",
    ] {
        assert!(html.contains(name), "knob label {name:?} missing from DOM");
    }
    // GR meter (labelled "GR") + I/O peak meters.
    for label in ["GR", "IN", "OUT"] {
        assert!(html.contains(label), "meter label {label:?} missing from DOM");
    }

    // Every knob got a real (non-collapsed) layout box.
    for id in [
        "knob-threshold",
        "knob-ratio",
        "knob-attack",
        "knob-release",
        "knob-knee",
        "knob-makeup",
        "knob-mix",
        "knob-link",
    ] {
        let el = fx.tester.query(by_testid(id)).immediately()?;
        let (w, h) = el.size();
        assert!(
            w > 20.0 && h > 20.0,
            "knob {id} collapsed to {w}x{h}px — Tailwind/layout broken"
        );
    }

    // Default threshold value is rendered by the knob (nice_plug formatter).
    fx.tester
        .query(":root")
        .expect(inner_html(contains_substring("-20.0")))
        .immediately()?;
    Ok(())
}

/// Press-and-release on a knob without moving is not a value change: the
/// drag begins and ends but records no Set, and the param keeps its default.
#[tokio::test]
async fn clicking_a_knob_without_dragging_changes_nothing() -> dioxus_test::Result<()> {
    let fx = mount();
    let key = ptr_key(fx.params.ratio.as_ptr());
    let before = fx.params.ratio.value();

    let (x, y) = fx.knob_center("knob-ratio");
    fx.tester.pointer_down(x, y);
    let _ = fx.tester.pump().await;
    fx.tester.pointer_up(x, y);
    let _ = fx.tester.pump().await;

    assert_eq!(fx.params.ratio.value(), before, "click alone moved the ratio");
    let log = fx.log.lock().unwrap();
    let sets = log.iter().filter(|g| matches!(g, Gesture::Set(k, _) if *k == key)).count();
    assert_eq!(sets, 0, "click without drag recorded value sets: {log:?}");
    Ok(())
}

/// THE drag test: press on the Threshold knob, drag 30 px up in steps,
/// release. Vertical knob drags map up → increase (DragProvider, sensitivity
/// 150 px per full sweep), so the threshold must rise from its −20 dB default
/// by 30/150 of the −60..0 range = +12 dB, through recorded host gestures.
#[tokio::test]
async fn dragging_threshold_knob_up_raises_threshold() -> dioxus_test::Result<()> {
    let fx = mount();
    let tp = &fx.params.threshold_db;
    let key = ptr_key(tp.as_ptr());

    let before = tp.value();
    assert!((before - (-20.0)).abs() < 1e-4, "default threshold: {before}");

    let (sx, sy) = fx.knob_center("knob-threshold");
    let dy = -30.0; // up

    fx.tester.pointer_down(sx, sy);
    let _ = fx.tester.pump().await;
    for step in 1..=3 {
        let t = step as f64 / 3.0;
        fx.tester.pointer_move(sx, sy + dy * t, true);
        let _ = fx.tester.pump().await;
    }
    fx.tester.pointer_up(sx, sy + dy);
    let _ = fx.tester.pump().await;

    let after = tp.value();
    assert!(
        after > before,
        "drag up did not raise threshold: {before} → {after}"
    );
    // 30 px at 150 px-per-sweep over the linear −60..0 dB range = +12 dB.
    let expected = before + (30.0 / 150.0) * 60.0;
    assert!(
        (after - expected).abs() < 0.5,
        "threshold landed at {after} dB, expected ~{expected} dB"
    );

    // The host saw a real automation gesture: begin, one set per drag step
    // (monotonically nondecreasing — every step moved up), then end.
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
        sets.windows(2).all(|w| w[1] >= w[0]),
        "threshold sets not monotonic: {sets:?}"
    );
    Ok(())
}
