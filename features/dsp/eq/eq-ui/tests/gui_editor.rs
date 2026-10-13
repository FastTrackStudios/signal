//! Behavioral tests for the REAL FTS-EQ plugin editor UI.
//!
//! Unlike `tests/gui_headless.rs` (a synthetic component around the curve
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
//!   hit-testing (`graph_width`/`graph_height` fallback in `eq_graph.rs`). The
//!   tests reuse the same `GraphMapper` with those dimensions to compute
//!   where band nodes live on screen.
//! - The spectrum analyzer engine (`EqUiState::analyzer`) is pure CPU math
//!   and runs fine headless; no stubbing was needed.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use dioxus_test::{DocumentTester, matchers::inner_html, render};
use test_that::prelude::*;

use eq_ui::control_view::App;
use eq_ui::eq_graph_interaction::GraphMapper;
use eq_ui::params::{EqUiState, FtsEqParams};

use audiocore_core::prelude::Param;
use fts_audio_ui::hardware::rack::FilterGlyph;
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

    /// `data-testid` on the EQ graph's band detail panel.
    pub const PANEL_TESTID: &str = "eq-band-popup";

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
        /// Upstream nice-plug added this to `GuiContextInner`. Nothing in a test
        /// harness or the standalone shell has a host to restart, so it is a no-op.
        fn request_restart(&self) {}
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        unsafe fn raw_begin_set_parameter(&self, param: ParamPtr) {
            self.log
                .lock()
                .unwrap()
                .push(Gesture::Begin(ptr_key(param)));
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
    /// elements are dropped by the headless harness's `NoOpDocument` fallback).
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

        Fixture {
            tester,
            params,
            log,
        }
    }

    impl Fixture {
        /// The `GraphMapper` matching what `EqGraph` uses headless: fixed
        /// 800×350 viewBox fallback (the custom-widget painter that would
        /// publish the live canvas size never runs without a renderer),
        /// default 20 Hz–20 kHz range, and whatever the default display range
        /// currently is.
        ///
        /// Reads the graph's own constants rather than repeating their
        /// values. The dB range was hardcoded to 3.0 once, and changing the
        /// default to ±6 dB made every pointer test miss its band node by half
        /// the graph — the fixture was computing where nodes *used* to be. The
        /// frequency axis then did exactly the same thing when its top moved
        /// from 20 kHz to Nyquist.
        pub fn mapper(&self) -> GraphMapper {
            GraphMapper::new(
                eq_ui::eq_graph_model::DEFAULT_MIN_FREQ,
                eq_ui::eq_graph_model::DEFAULT_MAX_FREQ,
                eq_ui::eq_graph_model::DEFAULT_DB_RANGE,
                800.0,
                350.0,
                0.0,
            )
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

        /// Run pending event handlers, then lay the document out again.
        ///
        /// `pump()` alone applies DOM mutations but leaves layout stale, so a
        /// panel that a pointer event just opened would have no box and would
        /// not be hit-testable. Anything that measures or clicks event-created
        /// UI must settle instead of bare-pumping.
        pub async fn settle(&self) {
            let _ = self.tester.pump().await;
            self.tester.relayout();
        }

        /// The band detail panel, if it is currently mounted.
        pub fn panel(&self) -> Option<dioxus_test::ResolvedElement> {
            self.tester
                .query(dioxus_test::by_testid(PANEL_TESTID))
                .immediately()
                .ok()
        }

        /// Document-space center of the band detail panel.
        ///
        /// Panics if the panel is not showing — every caller has just done
        /// something that must have opened it.
        pub fn panel_center(&self) -> (f64, f64) {
            let panel = self.panel().expect("band detail panel is not mounted");
            let (x, y) = panel.document_origin();
            let (w, h) = panel.size();
            (x + w as f64 / 2.0, y + h as f64 / 2.0)
        }

        /// A control inside the detail panel, found by its exact label.
        ///
        /// `architect_ui`'s `Button` has no attribute passthrough, so a
        /// captioned control is addressed the way a user sees it. The ones
        /// that carry an icon instead have a `data-testid` on a wrapper —
        /// see [`Self::panel_icon`].
        pub fn panel_control(&self, label: &str) -> dioxus_test::ResolvedElement {
            let sel = format!("[data-testid='{PANEL_TESTID}'] button");
            let found = self
                .tester
                .query_all(sel.as_str())
                .immediately()
                .into_iter()
                .find(|e| e.inner_html().trim() == label);
            if let Some(e) = found {
                e
            } else {
                let labels: Vec<String> = self
                    .tester
                    .query_all(sel.as_str())
                    .immediately()
                    .into_iter()
                    .map(|e| e.inner_html().trim().to_string())
                    .collect();
                panic!("no {label:?} control in the band panel; found {labels:?}");
            }
        }

        /// An icon control inside the detail panel, by the testid on its
        /// wrapper. Icons have no caption to match on, and adding a wrapper is
        /// safe where removing a node is not — see the panel's own notes.
        pub fn panel_icon(&self, testid: &str) -> dioxus_test::ResolvedElement {
            let sel = format!("[data-testid='{testid}'] button");
            self.tester
                .query(sel.as_str())
                .immediately()
                .unwrap_or_else(|e| panic!("no {testid} control in the band panel: {e:?}"))
        }

        /// Walks the pointer from `from` to `to` in `steps` hit-tested moves,
        /// the way a hand actually crosses the gap between the band node and
        /// its detail panel. `held` reports the primary button state.
        pub async fn glide(&self, from: (f64, f64), to: (f64, f64), steps: usize, held: bool) {
            for step in 1..=steps {
                let t = step as f64 / steps as f64;
                self.tester.pointer_move(
                    from.0 + (to.0 - from.0) * t,
                    from.1 + (to.1 - from.1) * t,
                    held,
                );
                self.settle().await;
            }
        }

        /// A hit-tested press-and-release at a document point.
        pub async fn tap(&self, x: f64, y: f64) {
            self.tester.pointer_down(x, y);
            self.settle().await;
            self.tester.pointer_up(x, y);
            self.settle().await;
        }
    }
}

use support::{Gesture, mount, ptr_key};

// ─────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────

/// The graph-only editor mounts headless: the graph widget node has real
/// (non-collapsed) layout and the `EqGraph` subtree rendered its band name
/// labels. The header / inspector / bottom bar are parked behind
/// `SHOW_CHROME = false` in `control_view.rs` and must NOT render.
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
    assert!(
        !html.contains("Inspector"),
        "inspector chrome leaked into the graph-only editor"
    );
    Ok(())
}

/// A hit-tested click on band 2's node (High Shelf @ 2.5 kHz) focuses it:
/// the graph's band info popup appears with the band's frequency label
/// ("2.5 kHz"). A pure selection click must NOT touch any parameter.
#[tokio::test]
async fn clicking_a_band_node_focuses_it_and_shows_its_popup() -> dioxus_test::Result<()> {
    let fx = mount();
    let (x, y) = fx.band_point(1);

    fx.tester.pointer_down(x, y);
    fx.settle().await;
    fx.tester.pointer_up(x, y);
    fx.settle().await;

    fx.tester
        .query(":root")
        .expect(inner_html(contains_substring("2.5 kHz")))
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
    assert!(
        (freq_before - 400.0).abs() < 1.0,
        "band 1 default freq: {freq_before}"
    );
    assert!(
        gain_before.abs() < 1e-6,
        "band 1 default gain: {gain_before}"
    );

    let (sx, sy) = fx.band_point(0);
    let (dx, dy) = (40.0, -30.0);

    fx.tester.pointer_down(sx, sy);
    fx.settle().await;
    for step in 1..=4 {
        let t = step as f64 / 4.0;
        fx.tester.pointer_move(sx + dx * t, sy + dy * t, true);
        fx.settle().await;
    }
    fx.tester.pointer_up(sx + dx, sy + dy);
    fx.settle().await;

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
        let begins = log
            .iter()
            .filter(|g| matches!(g, Gesture::Begin(k) if *k == key))
            .count();
        let sets = log
            .iter()
            .filter(|g| matches!(g, Gesture::Set(k, _) if *k == key))
            .count();
        let ends = log
            .iter()
            .filter(|g| matches!(g, Gesture::End(k) if *k == key))
            .count();
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

// ─────────────────────────────────────────────────────────────────────────
// Band detail panel
//
// The panel that floats above a hovered/selected band is a real, clickable
// surface, and these tests hold it to that. Every one of them drives
// hit-tested pointer events at coordinates resolved from the live layout —
// none of them poke a handler directly — so a panel that is unreachable in
// the plugin window is unreachable here too.
//
// The bug they lock down: pointer events landing on the panel bubbled to the
// graph carrying panel-relative coordinates, which the graph's hit-test read
// as "nowhere near the band". Focus then faded on a 300 ms timer and the
// panel vanished out from under the cursor before anything in it could be
// clicked.
// ─────────────────────────────────────────────────────────────────────────

/// Hovering a band node opens its detail panel, and the panel is laid out
/// with a real box — not collapsed to nothing.
#[tokio::test]
async fn hovering_a_band_opens_a_panel_with_real_layout() -> dioxus_test::Result<()> {
    let fx = mount();
    let (x, y) = fx.band_point(1);

    assert!(
        fx.panel().is_none(),
        "panel showing before any pointer input"
    );

    fx.tester.pointer_move(x, y, false);
    fx.settle().await;

    let panel = fx
        .panel()
        .expect("hovering a band node did not open its panel");
    let (w, h) = panel.size();
    assert!(w > 100.0 && h > 20.0, "panel collapsed to {w}x{h}");
    Ok(())
}

/// THE regression: the pointer must be able to travel from the band node onto
/// the panel without the panel disappearing. Every intermediate step is
/// checked, so a fade anywhere along the path fails here.
#[tokio::test]
async fn panel_survives_the_pointer_trip_from_node_to_panel() -> dioxus_test::Result<()> {
    let fx = mount();
    let node = fx.band_point(1);

    fx.tester.pointer_move(node.0, node.1, false);
    fx.settle().await;
    let target = fx.panel_center();

    for step in 1..=10 {
        let t = step as f64 / 10.0;
        let px = node.0 + (target.0 - node.0) * t;
        let py = node.1 + (target.1 - node.1) * t;
        fx.tester.pointer_move(px, py, false);
        fx.settle().await;
        assert!(
            fx.panel().is_some(),
            "panel disappeared {}% of the way from the band node to the panel",
            (t * 100.0) as i32,
        );
    }

    // And it stays up while the pointer moves around inside it.
    let (cx, cy) = fx.panel_center();
    for (dx, dy) in [(-40.0, -8.0), (40.0, -8.0), (40.0, 8.0), (-40.0, 8.0)] {
        fx.tester.pointer_move(cx + dx, cy + dy, false);
        fx.settle().await;
        assert!(
            fx.panel().is_some(),
            "panel closed while the pointer moved inside it"
        );
    }
    Ok(())
}

/// Pointer events on the panel must not reach the graph's band hit-test. If
/// they do, their panel-relative coordinates get read as a drag and the band
/// jumps across the graph.
#[tokio::test]
async fn interacting_with_the_panel_does_not_move_the_band() -> dioxus_test::Result<()> {
    let fx = mount();
    let bp = &fx.params.bands[1];
    let node = fx.band_point(1);

    fx.tester.pointer_move(node.0, node.1, false);
    fx.settle().await;

    let freq_before = bp.freq_hz.value();
    let gain_before = bp.gain_db.value();

    // A point inside the panel that is not a control. The panel's centre used
    // to be dead space between two button columns; now the dials have that
    // width, so pressing there works a dial — which is the dial doing its job,
    // not the press leaking to the graph, and a poor probe for this question.
    let (cx, cy) = {
        let panel = fx.panel().expect("band detail panel is not mounted");
        let (px, py) = panel.document_origin();
        let (pw, ph) = panel.size();
        (px + f64::from(pw) - 10.0, py + f64::from(ph) / 2.0)
    };
    fx.glide(node, (cx, cy), 6, false).await;
    // Press, drag a little, release — all inside the panel.
    fx.tester.pointer_down(cx, cy);
    fx.settle().await;
    fx.glide((cx, cy), (cx + 30.0, cy + 10.0), 3, true).await;

    // Checked while the button is still down: the graph clears its selection
    // rectangle on release, so after the release there would be nothing to
    // see even if the press had leaked through.
    assert!(
        fx.tester
            .query(dioxus_test::by_testid("eq-selection-rect"))
            .immediately()
            .is_err(),
        "dragging inside the panel started a selection rectangle on the graph",
    );

    fx.tester.pointer_up(cx + 30.0, cy + 10.0);
    fx.settle().await;

    assert!(
        (bp.freq_hz.value() - freq_before).abs() < 1e-3,
        "panel interaction moved the band's frequency: {freq_before} → {}",
        bp.freq_hz.value(),
    );
    assert!(
        (bp.gain_db.value() - gain_before).abs() < 1e-3,
        "panel interaction moved the band's gain: {gain_before} → {}",
        bp.gain_db.value(),
    );
    Ok(())
}

/// The panel's focus control is reachable by a hit-tested click and actually
/// drives the parameter through a host automation gesture.
#[tokio::test]
async fn panel_focus_control_is_clickable() -> dioxus_test::Result<()> {
    let fx = mount();
    let bp = &fx.params.bands[1];
    let focus_key = ptr_key(bp.focus.as_ptr());
    let node = fx.band_point(1);

    fx.tester.pointer_move(node.0, node.1, false);
    fx.settle().await;
    assert!(bp.focus.value() < 0.5, "band 2 starts focused");

    let target = fx.panel_center();
    fx.glide(node, target, 6, false).await;

    let focus = fx.panel_icon("eq-band-focus");
    let (bx, by) = focus.document_origin();
    let (bw, bh) = focus.size();
    assert!(bw > 0.0 && bh > 0.0, "focus control has no layout box");
    fx.tap(bx + bw as f64 / 2.0, by + bh as f64 / 2.0).await;

    assert!(
        bp.focus.value() > 0.5,
        "clicking the panel's focus control did not focus the band",
    );
    let log = fx.log.lock().unwrap();
    assert!(
        log.iter()
            .any(|g| matches!(g, Gesture::Set(k, v) if *k == focus_key && *v > 0.5)),
        "no host automation gesture for focus: {log:?}",
    );
    Ok(())
}

/// Same for the bypass control at the other end of the panel's control row —
/// proving the whole row is reachable, not just the middle of the panel.
#[tokio::test]
async fn panel_bypass_control_is_clickable() -> dioxus_test::Result<()> {
    let fx = mount();
    let bp = &fx.params.bands[1];
    let node = fx.band_point(1);

    fx.tester.pointer_move(node.0, node.1, false);
    fx.settle().await;
    assert!(bp.enabled.value() > 0.5, "band 2 starts bypassed");

    let target = fx.panel_center();
    fx.glide(node, target, 6, false).await;

    let bypass = fx.panel_icon("eq-band-bypass");
    let (bx, by) = bypass.document_origin();
    let (bw, bh) = bypass.size();
    fx.tap(bx + bw as f64 / 2.0, by + bh as f64 / 2.0).await;

    assert!(
        bp.enabled.value() < 0.5,
        "clicking the panel's bypass control did not bypass the band",
    );
    Ok(())
}

/// A band the user has *selected* (clicked) keeps its panel up even when the
/// pointer wanders off, so the panel can be aimed at deliberately.
#[tokio::test]
async fn selected_band_keeps_its_panel_when_the_pointer_leaves() -> dioxus_test::Result<()> {
    let fx = mount();
    let node = fx.band_point(1);
    let (ox, oy) = fx.graph_origin();

    fx.tap(node.0, node.1).await;
    assert!(
        fx.panel().is_some(),
        "clicking a band did not open its panel"
    );

    // Far corner of the graph, well outside any band's focus radius, with
    // enough wall-clock time for the fade timer to have fired several times.
    let far = (ox + 760.0, oy + 330.0);
    for _ in 0..3 {
        fx.glide(node, far, 4, false).await;
        std::thread::sleep(std::time::Duration::from_millis(200));
        fx.tester.pointer_move(far.0, far.1, false);
        fx.settle().await;
    }

    assert!(
        fx.panel().is_some(),
        "the selected band's panel faded while the pointer was elsewhere",
    );
    Ok(())
}

/// …and clicking empty graph is how that sticky panel is dismissed.
#[tokio::test]
async fn clicking_empty_graph_dismisses_the_selected_panel() -> dioxus_test::Result<()> {
    let fx = mount();
    let node = fx.band_point(1);
    let (ox, oy) = fx.graph_origin();

    fx.tap(node.0, node.1).await;
    assert!(
        fx.panel().is_some(),
        "clicking a band did not open its panel"
    );

    fx.tap(ox + 760.0, oy + 330.0).await;
    // The fade path still needs a move event to run.
    fx.tester.pointer_move(ox + 755.0, oy + 325.0, false);
    fx.settle().await;

    assert!(
        fx.panel().is_none(),
        "clicking empty graph left the panel up",
    );
    Ok(())
}

/// The stickiness above must not become permanence: a merely *hovered* band
/// (never clicked, never selected) still closes its panel once the pointer
/// leaves and the fade timer expires.
#[tokio::test]
async fn hovered_only_panel_still_fades_after_the_pointer_leaves() -> dioxus_test::Result<()> {
    let fx = mount();
    let node = fx.band_point(1);
    let (ox, oy) = fx.graph_origin();

    fx.tester.pointer_move(node.0, node.1, false);
    fx.settle().await;
    assert!(fx.panel().is_some(), "hover did not open the panel");

    // Leave, wait past both the fade timeout and the panel-contact grace
    // period (real wall clock — the graph timestamps with SystemTime), then
    // move again so the timer actually runs.
    let far = (ox + 760.0, oy + 330.0);
    fx.tester.pointer_move(far.0, far.1, false);
    fx.settle().await;
    std::thread::sleep(std::time::Duration::from_millis(700));
    fx.tester.pointer_move(far.0 - 5.0, far.1 - 5.0, false);
    fx.settle().await;

    assert!(
        fx.panel().is_none(),
        "a hover-only panel never closed after the pointer left",
    );
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────
// Hardware faces
//
// The faceplates are drawn, so most of what is wrong with one is a picture
// (tests/screenshots.rs). These cover the two things a picture cannot show:
// whether a control can be operated, and whether a piece of the drawing that
// must stay still actually does.
// ─────────────────────────────────────────────────────────────────────────

/// Mount the editor already on a hardware model, the way a host opens it.
async fn mount_model(model: i32) -> support::Fixture {
    let fx = mount();
    unsafe {
        fx.params
            .model
            .as_ptr()
            ._internal_set_normalized_value(model as f32 / 5.0)
    };
    fx.settle().await;
    fx
}

/// The specular highlight on a Pultec knob is the panel's lamp, not part of
/// the knob: it must not sit inside the group that rotates with the value.
///
/// A reflection that turns with the control is the first thing that reads as
/// wrong on a faceplate, and it is invisible in a code review — hence a
/// structural assertion rather than an eyeball.
#[tokio::test]
async fn a_pultec_knobs_highlight_is_not_inside_the_rotating_group() -> dioxus_test::Result<()> {
    let fx = mount_model(1).await;

    // The light exists…
    fx.tester
        .query(dioxus_test::by_testid("hw-knob-low-boost-light"))
        .immediately()?;

    // …and no rotating ancestor owns it.
    assert!(
        fx.tester
            .query("g[transform] [data-testid='hw-knob-low-boost-light']")
            .immediately()
            .is_err(),
        "the knob's highlight turns with the knob",
    );
    Ok(())
}

/// The Pultec's frequency levers are draggable, like every other control on
/// the panel. They used to respond to clicks alone, which is why a sweep
/// across one did nothing at all.
#[tokio::test]
async fn a_pultec_frequency_lever_can_be_dragged() -> dioxus_test::Result<()> {
    let fx = mount_model(1).await;
    let bp = &fx.params.pultec_low_freq;

    let paddle = fx
        .tester
        .query(dioxus_test::by_testid("hw-lever-low-freq-paddle"))
        .immediately()?;
    let (px, py) = paddle.document_origin();
    let (pw, ph) = paddle.size();
    assert!(pw > 0.0 && ph > 0.0, "the lever paddle has no layout box");
    let (cx, cy) = (px + pw as f64 / 2.0, py + ph as f64 / 2.0);

    let before = bp.value();

    // Push the paddle to the right: a lever's travel is horizontal.
    fx.tester.pointer_down(cx, cy);
    fx.settle().await;
    for step in 1..=6 {
        fx.tester.pointer_move(cx + step as f64 * 12.0, cy, true);
        fx.settle().await;
    }
    fx.tester.pointer_up(cx + 72.0, cy);
    fx.settle().await;

    assert!(
        bp.value() > before,
        "dragging the lever right did not raise its position: {before} → {}",
        bp.value(),
    );
    Ok(())
}

/// A press that never travelled is still a click, and a click advances the
/// lever one position — the way you use one without aiming. The drag support
/// above must not have eaten that.
#[tokio::test]
async fn clicking_a_pultec_lever_still_advances_one_position() -> dioxus_test::Result<()> {
    let fx = mount_model(1).await;

    let index = |fx: &support::Fixture| -> usize {
        fx.tester
            .query(dioxus_test::by_testid("hw-lever-low-freq"))
            .immediately()
            .expect("lever missing")
            .attribute("data-index")
            .and_then(|v| v.parse().ok())
            .expect("lever has no data-index")
    };

    let before = index(&fx);
    let paddle = fx
        .tester
        .query(dioxus_test::by_testid("hw-lever-low-freq-paddle"))
        .immediately()?;
    let (px, py) = paddle.document_origin();
    let (pw, ph) = paddle.size();
    fx.tap(px + pw as f64 / 2.0, py + ph as f64 / 2.0).await;

    assert_eq!(
        index(&fx),
        (before + 1) % 4,
        "a click on the paddle did not advance the lever",
    );
    Ok(())
}

/// The 1073's rings are dots, and its bands are labelled by the filter's
/// shape rather than by a word. Both are the panel's identity; a refactor
/// that drops either leaves a face that no longer reads as a 1073.
#[tokio::test]
async fn the_1073_prints_dot_rings_and_filter_glyphs() -> dioxus_test::Result<()> {
    let fx = mount_model(2).await;

    // Every control is on the panel. The swept bands are concentric, so the
    // pair is addressed by its collar's id plus an `-inner` cap.
    for id in [
        "hw-knob-drive",
        "hw-knob-high-gain",
        "hw-knob-mid-freq",
        "hw-knob-mid-freq-inner",
        "hw-knob-low-freq",
        "hw-knob-low-freq-inner",
        "hw-knob-hpf",
        "hw-knob-trim",
    ] {
        fx.tester
            .query(dioxus_test::by_testid(id))
            .immediately()
            .unwrap_or_else(|e| panic!("1073 is missing {id}: {e:?}"));
    }

    // The printed ring is dots, not tick lines.
    let dots = fx
        .tester
        .query_all("[data-testid='hw-knob-hpf'] circle")
        .immediately()
        .len();
    assert!(dots >= 21, "1073 knob prints only {dots} ring dots");

    // And the band's shape is drawn above it.
    let html = fx.tester.query(":root").immediately()?.inner_html();
    for glyph in [
        FilterGlyph::HighShelf,
        FilterGlyph::LowShelf,
        FilterGlyph::Bell,
        FilterGlyph::HighPass,
    ] {
        assert!(
            html.contains(glyph.path()),
            "the 1073 panel does not draw the {glyph:?} symbol",
        );
    }
    Ok(())
}

/// A 1073 band is two controls in one place, and which one you get is decided
/// by *where* you press: the grey cap is the band's gain, the bright collar
/// around it is the band's frequency. Dragging one must leave the other alone
/// — a concentric knob that moves both is worse than two separate knobs.
#[tokio::test]
async fn the_1073s_collar_and_cap_are_separate_controls() -> dioxus_test::Result<()> {
    let fx = mount_model(2).await;
    let freq = &fx.params.neve_mid_freq;
    let gain = &fx.params.neve_mid_gain_db;

    // Drag the cap: the gain moves, the frequency does not.
    let (freq_before, gain_before) = (freq.value(), gain.value());
    let cap = fx
        .tester
        .query(dioxus_test::by_testid("hw-knob-mid-freq-inner"))
        .immediately()?;
    let (cx, cy) = cap.document_origin();
    let (cw, ch) = cap.size();
    assert!(cw > 0.0 && ch > 0.0, "the cap has no drag region");
    let (cx, cy) = (cx + cw as f64 / 2.0, cy + ch as f64 / 2.0);

    fx.tester.pointer_down(cx, cy);
    fx.settle().await;
    for step in 1..=5 {
        fx.tester.pointer_move(cx, cy - step as f64 * 10.0, true);
        fx.settle().await;
    }
    fx.tester.pointer_up(cx, cy - 50.0);
    fx.settle().await;

    assert!(
        gain.value() > gain_before,
        "dragging the cap up did not raise the band's gain: {gain_before} → {}",
        gain.value(),
    );
    // The frequency is a stepped selector, so "unchanged" is exact.
    assert_eq!(
        freq.value(),
        freq_before,
        "dragging the cap moved the band's frequency too",
    );

    // Now the collar. Press outside the cap but inside the knob — the ring
    // between them — and the frequency moves instead.
    let (freq_before, gain_before) = (freq.value(), gain.value());
    let knob = fx
        .tester
        .query(dioxus_test::by_testid("hw-knob-mid-freq"))
        .immediately()?;
    let (kx, ky) = knob.document_origin();
    let (kw, kh) = knob.size();
    // Just inside the knob's left edge, level with its centre: collar, not cap.
    let (px, py) = (kx + kw as f64 * 0.30, ky + kh as f64 / 2.0);

    fx.tester.pointer_down(px, py);
    fx.settle().await;
    for step in 1..=5 {
        fx.tester.pointer_move(px, py - step as f64 * 10.0, true);
        fx.settle().await;
    }
    fx.tester.pointer_up(px, py - 50.0);
    fx.settle().await;

    assert!(
        freq.value() > freq_before,
        "dragging the collar up did not raise the band's frequency: {freq_before} → {}",
        freq.value(),
    );
    assert!(
        (gain.value() - gain_before).abs() < 1e-3,
        "dragging the collar moved the band's gain too: {gain_before} → {}",
        gain.value(),
    );
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────
// Display range (docs/spec/fx/eq-display.md)
// ─────────────────────────────────────────────────────────────────────────

/// Dragging a band above the display's top expands the dB range to the next
/// step that shows the pointer — the curve is never clipped mid-edit.
// r[verify fx.eq.display.auto-range]
#[tokio::test]
async fn dragging_a_band_past_the_top_expands_the_range() -> dioxus_test::Result<()> {
    let fx = mount();
    // Whatever the default is, auto-range has to take it one step further.
    // Pinned against the constant rather than a literal: the default moved
    // from ±3 dB to ±6 dB and a hardcoded 0 here would have failed for a
    // reason that had nothing to do with auto-range.
    let start_index = fx.params.db_range.value();
    assert!(
        start_index < 5,
        "the default range must leave somewhere to expand to (index {start_index})"
    );

    let (ox, oy) = fx.graph_origin();
    let (sx, sy) = fx.band_point(0);
    // Drag the band into the top edge of the display: mouse handlers are
    // element-scoped, so "outside the range" is expressed by reaching the
    // edge, which must expand the range one step.
    let target_y = oy + 1.0;

    fx.tester.pointer_down(sx, sy);
    fx.settle().await;
    for step in 1..=4 {
        let t = step as f64 / 4.0;
        fx.tester.pointer_move(sx, sy + (target_y - sy) * t, true);
        fx.settle().await;
    }
    fx.tester.pointer_up(sx, target_y);
    fx.settle().await;

    // One step wider, never narrower.
    assert!(
        fx.params.db_range.value() > start_index,
        "range did not expand past index {start_index} (still index {})",
        fx.params.db_range.value()
    );
    let _ = ox;
    Ok(())
}

/// The range selector lives on the graph surface and writes the `db_range`
/// param as an ordinary gesture.
// r[verify fx.eq.display.range]
#[tokio::test]
async fn the_range_selector_is_on_the_graph_and_sets_the_param() -> dioxus_test::Result<()> {
    let fx = mount();
    let sel = fx
        .tester
        .query(dioxus_test::by_testid("eq-db-range"))
        .immediately()?;
    assert!(
        sel.inner_html().contains("3 dB") && sel.inner_html().contains("30 dB"),
        "selector does not offer the range steps: {}",
        sel.inner_html()
    );
    Ok(())
}

/// The preset strip is mounted in the top rail, and browsing opens the list.
///
/// The library lives on disk and is usually empty in a test run, so this
/// asserts the surfaces exist and respond, not what is in them — the browsing
/// rules are covered in `preset-browser` and the panel in `preset-browser-ui`.
/// What only this mount can answer is whether the EQ editor carries them, and
/// whether the Browse control is actually reachable: a z-indexed overlay
/// renders on top in blitz but takes no clicks headlessly.
#[tokio::test]
async fn the_top_rail_carries_the_preset_strip() -> dioxus_test::Result<()> {
    let fx = mount();
    let _ = fx.tester.pump().await;

    fx.tester
        .query(dioxus_test::by_testid("preset-bar-name"))
        .immediately()?;
    assert!(
        fx.tester
            .query(dioxus_test::by_testid("eq-presets"))
            .immediately()
            .is_err(),
        "the browser stays shut until asked for",
    );

    let browse = fx
        .tester
        .query(dioxus_test::by_testid("preset-bar-browse"))
        .immediately()?;
    let (ox, oy) = browse.document_origin();
    let (w, h) = browse.size();
    fx.tap(ox + w as f64 / 2.0, oy + h as f64 / 2.0).await;

    fx.tester
        .query(dioxus_test::by_testid("eq-presets"))
        .immediately()?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────
// Modifier gestures
// ─────────────────────────────────────────────────────────────────────────
//
// The Pro-Q modifier chords, driven through the real editor rather than
// reasoned about. `eq_graph_interaction::gesture_tests` pins what a chord
// *means*; these prove the meaning survives the trip through Blitz's event
// system and lands on the parameter — which is a different question, and one
// that OS-level synthetic input cannot answer: the automation API applies
// modifiers to clicks and scrolls but not to the pointer moves between them,
// so a real Alt+drag or Alt+scroll is unreproducible from outside the process.
// `pointer_*_mods` and `wheel_mods` go straight into the document with the
// modifiers attached, so the whole set is testable here.

use dioxus_test::keyboard_types::Modifiers;

/// Nudge the pointer onto a band so the graph considers it hovered — the
/// precondition for a scroll to have a target at all.
async fn hover_band(fx: &support::Fixture, idx: usize) -> (f64, f64) {
    let (x, y) = fx.band_point(idx);
    fx.tester.pointer_move(x, y, false);
    fx.settle().await;
    (x, y)
}

#[tokio::test]
async fn plain_scroll_over_a_band_moves_q() -> dioxus_test::Result<()> {
    let fx = mount();
    let bp = &fx.params.bands[0];
    let (x, y) = hover_band(&fx, 0).await;

    let q_before = bp.q.value();
    let range_before = bp.dyn_range_db.value();
    let gain_before = bp.gain_db.value();

    // Negative delta is wheel-up, the direction that increases a control.
    fx.tester.wheel_mods(x, y, -3.0, Modifiers::empty());
    fx.settle().await;

    assert!(
        bp.q.value() > q_before,
        "unmodified scroll up should raise Q: {q_before} -> {}",
        bp.q.value()
    );
    assert!(
        (bp.dyn_range_db.value() - range_before).abs() < 1e-6,
        "unmodified scroll must not touch the dynamic range"
    );
    assert!(
        (bp.gain_db.value() - gain_before).abs() < 1e-6,
        "unmodified scroll must not touch gain"
    );
    Ok(())
}

#[tokio::test]
async fn alt_scroll_moves_the_dynamic_range_and_leaves_q_alone() -> dioxus_test::Result<()> {
    let fx = mount();
    let bp = &fx.params.bands[0];
    let (x, y) = hover_band(&fx, 0).await;

    let q_before = bp.q.value();
    let range_before = bp.dyn_range_db.value();

    fx.tester.wheel_mods(x, y, -3.0, Modifiers::ALT);
    fx.settle().await;

    assert!(
        bp.dyn_range_db.value() > range_before,
        "Alt+scroll up should widen the dynamic range: {range_before} -> {}",
        bp.dyn_range_db.value()
    );
    assert!(
        (bp.q.value() - q_before).abs() < 1e-6,
        "Alt+scroll must not fall through to Q: {q_before} -> {}",
        bp.q.value()
    );
    Ok(())
}

#[tokio::test]
async fn cmd_scroll_moves_gain() -> dioxus_test::Result<()> {
    let fx = mount();
    let bp = &fx.params.bands[0];
    let (x, y) = hover_band(&fx, 0).await;

    let gain_before = bp.gain_db.value();
    let q_before = bp.q.value();

    // Ctrl and Command are the same gesture; the handler ORs them.
    fx.tester.wheel_mods(x, y, -2.0, Modifiers::META);
    fx.settle().await;

    assert!(
        bp.gain_db.value() > gain_before,
        "Cmd+scroll up should raise gain: {gain_before} -> {}",
        bp.gain_db.value()
    );
    assert!(
        (bp.q.value() - q_before).abs() < 1e-6,
        "Cmd+scroll must not also move Q"
    );
    Ok(())
}

#[tokio::test]
async fn alt_cmd_scroll_moves_gain_and_range_together() -> dioxus_test::Result<()> {
    let fx = mount();
    let bp = &fx.params.bands[0];
    let (x, y) = hover_band(&fx, 0).await;

    let gain_before = bp.gain_db.value();
    let range_before = bp.dyn_range_db.value();

    fx.tester
        .wheel_mods(x, y, -2.0, Modifiers::ALT | Modifiers::META);
    fx.settle().await;

    assert!(
        bp.gain_db.value() > gain_before,
        "Alt+Cmd scroll should move gain"
    );
    assert!(
        bp.dyn_range_db.value() > range_before,
        "Alt+Cmd scroll should move the range in the same gesture"
    );
    Ok(())
}

#[tokio::test]
async fn alt_click_on_a_band_toggles_its_bypass() -> dioxus_test::Result<()> {
    let fx = mount();
    let bp = &fx.params.bands[0];
    let (x, y) = fx.band_point(0);

    let enabled_before = bp.enabled.value() > 0.5;
    let freq_before = bp.freq_hz.value();

    fx.tester.pointer_down_mods(x, y, Modifiers::ALT);
    fx.settle().await;
    fx.tester.pointer_up_mods(x, y, Modifiers::ALT);
    fx.settle().await;

    assert_ne!(
        bp.enabled.value() > 0.5,
        enabled_before,
        "Alt+click should flip the band's bypass"
    );
    // Not bit-exact: every `on_band_change` rewrites the whole band, and the
    // normalize/denormalize round-trip lands 400 Hz on 400.00006. What matters
    // is that the band did not *move* — a drag of the same span would be tens
    // of hertz.
    assert!(
        (bp.freq_hz.value() - freq_before).abs() < 0.5,
        "Alt+click is a button press, not the start of a drag — frequency must \
         not move: {freq_before} -> {}",
        bp.freq_hz.value()
    );
    Ok(())
}

#[tokio::test]
async fn cmd_alt_click_cycles_the_band_shape() -> dioxus_test::Result<()> {
    let fx = mount();
    let bp = &fx.params.bands[0];
    let (x, y) = fx.band_point(0);

    let shape_before = bp.filter_type.value();
    let enabled_before = bp.enabled.value() > 0.5;

    let mods = Modifiers::ALT | Modifiers::META;
    fx.tester.pointer_down_mods(x, y, mods);
    fx.settle().await;
    fx.tester.pointer_up_mods(x, y, mods);
    fx.settle().await;

    assert_ne!(
        bp.filter_type.value(),
        shape_before,
        "Cmd+Alt+click should step the shape"
    );
    // The regression the ordering test guards, seen from the other end: if
    // the two-modifier chord fell through to the bare-Alt arm this would have
    // toggled bypass instead.
    assert_eq!(
        bp.enabled.value() > 0.5,
        enabled_before,
        "Cmd+Alt+click must not fall through to the bypass toggle"
    );
    Ok(())
}

#[tokio::test]
async fn alt_shift_click_cycles_the_slope() -> dioxus_test::Result<()> {
    let fx = mount();
    let bp = &fx.params.bands[0];
    let (x, y) = fx.band_point(0);

    let slope_before = bp.slope.value();
    let enabled_before = bp.enabled.value() > 0.5;

    let mods = Modifiers::ALT | Modifiers::SHIFT;
    fx.tester.pointer_down_mods(x, y, mods);
    fx.settle().await;
    fx.tester.pointer_up_mods(x, y, mods);
    fx.settle().await;

    assert_ne!(
        bp.slope.value(),
        slope_before,
        "Alt+Shift+click should step the slope"
    );
    assert_eq!(
        bp.enabled.value() > 0.5,
        enabled_before,
        "Alt+Shift+click must not fall through to the bypass toggle"
    );
    Ok(())
}

#[tokio::test]
async fn alt_drag_pins_the_gain_and_moves_only_the_frequency() -> dioxus_test::Result<()> {
    let fx = mount();
    let bp = &fx.params.bands[0];
    let (sx, sy) = fx.band_point(0);

    let freq_before = bp.freq_hz.value();
    let gain_before = bp.gain_db.value();

    // Mostly *vertical* travel, which is the case that matters: the old rule
    // locked to whichever axis the pointer committed to, so a grab like this
    // pinned the frequency and let the gain run — the opposite of the gesture.
    let (dx, dy) = (24.0, -60.0);
    fx.tester.pointer_down_mods(sx, sy, Modifiers::ALT);
    fx.settle().await;
    for step in 1..=4 {
        let t = f64::from(step) / 4.0;
        fx.tester
            .pointer_move_mods(sx + dx * t, sy + dy * t, true, Modifiers::ALT);
        fx.settle().await;
    }
    fx.tester.pointer_up_mods(sx + dx, sy + dy, Modifiers::ALT);
    fx.settle().await;

    assert!(
        (bp.freq_hz.value() - freq_before).abs() > 1.0,
        "frequency should follow the pointer under Alt: {freq_before} -> {}",
        bp.freq_hz.value()
    );
    assert!(
        (bp.gain_db.value() - gain_before).abs() < 0.01,
        "Alt must pin the gain the band was already at: {gain_before} -> {}",
        bp.gain_db.value()
    );
    Ok(())
}

#[tokio::test]
async fn cmd_drag_adjusts_resonance_instead_of_gain() -> dioxus_test::Result<()> {
    let fx = mount();
    let bp = &fx.params.bands[0];
    let (sx, sy) = fx.band_point(0);

    let q_before = bp.q.value();
    let gain_before = bp.gain_db.value();

    // Straight up: gain would rise a lot if this were an ordinary drag.
    let dy = -50.0;
    fx.tester.pointer_down_mods(sx, sy, Modifiers::META);
    fx.settle().await;
    for step in 1..=4 {
        let t = f64::from(step) / 4.0;
        fx.tester
            .pointer_move_mods(sx, sy + dy * t, true, Modifiers::META);
        fx.settle().await;
    }
    fx.tester.pointer_up_mods(sx, sy + dy, Modifiers::META);
    fx.settle().await;

    assert!(
        bp.q.value() > q_before,
        "Cmd+drag up should raise Q: {q_before} -> {}",
        bp.q.value()
    );
    assert!(
        (bp.gain_db.value() - gain_before).abs() < 0.01,
        "Cmd+drag must not move gain: {gain_before} -> {}",
        bp.gain_db.value()
    );
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────
// Multi-band selection and group drag
// ─────────────────────────────────────────────────────────────────────────

/// Drag a band by `(dx, dy)` in document pixels, no modifiers.
async fn drag_band(fx: &support::Fixture, idx: usize, dx: f64, dy: f64) {
    let (sx, sy) = fx.band_point(idx);
    fx.tester.pointer_down(sx, sy);
    fx.settle().await;
    for step in 1..=4 {
        let t = f64::from(step) / 4.0;
        fx.tester.pointer_move(sx + dx * t, sy + dy * t, true);
        fx.settle().await;
    }
    fx.tester.pointer_up(sx + dx, sy + dy);
    fx.settle().await;
}

/// Rubber-band across the whole graph, which encloses every used band.
///
/// Starts in the top-left corner rather than on a node: a press that lands on
/// a band starts a drag instead of a selection.
async fn select_all_by_rectangle(fx: &support::Fixture) {
    let (ox, oy) = fx.graph_origin();
    let (x0, y0) = (ox + 4.0, oy + 4.0);
    let (x1, y1) = (ox + 780.0, oy + 340.0);
    fx.tester.pointer_down(x0, y0);
    fx.settle().await;
    for step in 1..=4 {
        let t = f64::from(step) / 4.0;
        fx.tester
            .pointer_move(x0 + (x1 - x0) * t, y0 + (y1 - y0) * t, true);
        fx.settle().await;
    }
    fx.tester.pointer_up(x1, y1);
    fx.settle().await;
}

#[tokio::test]
async fn a_rectangle_selects_bands_and_a_later_drag_moves_all_of_them() -> dioxus_test::Result<()> {
    let fx = mount();
    // Give the two default bands opposite gains so the group drag has both a
    // boost and a cut to act on.
    drag_band(&fx, 0, 0.0, -70.0).await;
    drag_band(&fx, 1, 0.0, 70.0).await;

    let b0 = &fx.params.bands[0];
    let b1 = &fx.params.bands[1];
    assert!(b0.gain_db.value() > 0.2, "band 0 should be boosting");
    assert!(b1.gain_db.value() < -0.2, "band 1 should be cutting");

    let f0_before = b0.freq_hz.value();
    let f1_before = b1.freq_hz.value();

    select_all_by_rectangle(&fx).await;

    // Drag one selected band sideways; the whole selection should follow.
    drag_band(&fx, 0, 60.0, 0.0).await;

    assert!(
        (b0.freq_hz.value() - f0_before).abs() > 1.0,
        "the dragged band should move: {f0_before} -> {}",
        b0.freq_hz.value()
    );
    assert!(
        (b1.freq_hz.value() - f1_before).abs() > 1.0,
        "the OTHER selected band should move too, but it stayed at {f1_before}"
    );
    Ok(())
}

#[tokio::test]
async fn a_group_drag_moves_frequency_in_parallel() -> dioxus_test::Result<()> {
    let fx = mount();
    drag_band(&fx, 0, 0.0, -70.0).await;
    drag_band(&fx, 1, 0.0, 70.0).await;

    let b0 = &fx.params.bands[0];
    let b1 = &fx.params.bands[1];
    let f0_before = f64::from(b0.freq_hz.value());
    let f1_before = f64::from(b1.freq_hz.value());

    select_all_by_rectangle(&fx).await;
    drag_band(&fx, 0, 60.0, 0.0).await;

    let r0 = f64::from(b0.freq_hz.value()) / f0_before;
    let r1 = f64::from(b1.freq_hz.value()) / f1_before;

    // The axis is logarithmic, so "parallel on screen" means "same frequency
    // RATIO" — every selected band shifts by the same number of octaves and
    // therefore the same number of pixels, holding the shape of the selection.
    assert!(
        (r0 - r1).abs() / r0 < 0.02,
        "bands should shift by the same ratio (parallel on a log axis): \
         {r0:.4} vs {r1:.4}"
    );
    assert!(r0 > 1.0, "a rightward drag should raise frequency: {r0:.4}");
    Ok(())
}

#[tokio::test]
async fn a_group_drag_deepens_cuts_while_it_lifts_boosts() -> dioxus_test::Result<()> {
    let fx = mount();
    drag_band(&fx, 0, 0.0, -70.0).await;
    drag_band(&fx, 1, 0.0, 70.0).await;

    let b0 = &fx.params.bands[0];
    let b1 = &fx.params.bands[1];
    let g0_before = b0.gain_db.value();
    let g1_before = b1.gain_db.value();
    assert!(
        g0_before > 0.2 && g1_before < -0.2,
        "need a boost and a cut"
    );

    select_all_by_rectangle(&fx).await;
    // Drag the BOOSTING band further up; the cut should go further down.
    drag_band(&fx, 0, 0.0, -40.0).await;

    let g0_after = b0.gain_db.value();
    let g1_after = b1.gain_db.value();

    assert!(
        g0_after > g0_before,
        "the boost should grow: {g0_before} -> {g0_after}"
    );
    assert!(
        g1_after < g1_before,
        "the cut should DEEPEN as the boost grows — gain scales about zero, so \
         a negative band moves opposite to a positive one: {g1_before} -> {g1_after}"
    );
    Ok(())
}

/// A dynamic band's envelope counts toward auto-range.
///
/// The node can sit comfortably inside the display while the range extreme is
/// already off the bottom — and the extreme is the part that carries the
/// information. Before this, dragging a dynamic band expanded the view only
/// when the *node* reached the edge, so the envelope silently ran off.
// r[verify fx.eq.display.auto-range]
#[tokio::test]
async fn a_dynamic_bands_envelope_expands_the_range_before_its_node_does() -> dioxus_test::Result<()>
{
    let fx = mount();
    let bp = &fx.params.bands[0];
    let (x, y) = hover_band(&fx, 0).await;

    // Give the band a wide dynamic range without moving its node: Alt+scroll
    // is the range gesture, and it leaves gain alone.
    for _ in 0..8 {
        fx.tester.wheel_mods(x, y, -3.0, Modifiers::ALT);
        fx.settle().await;
    }
    let range_db = bp.dyn_range_db.value();
    assert!(
        range_db.abs() > 3.0,
        "needed a range wide enough to leave a ±6 dB view: {range_db}"
    );

    let start_index = fx.params.db_range.value();
    let gain_before = bp.gain_db.value();

    // A small drag — nowhere near the top of the display for the node itself.
    drag_band(&fx, 0, 0.0, -20.0).await;

    assert!(
        bp.gain_db.value().abs() < 3.0,
        "the node itself should still be well inside a ±6 dB view: {}",
        bp.gain_db.value()
    );
    assert!(
        fx.params.db_range.value() > start_index,
        "the envelope reaches {:.1} dB, past the ±{} dB view, so the range \
         should have expanded (still index {})",
        gain_before + range_db,
        eq_ui::eq_graph_model::db_range_for_index(start_index),
        fx.params.db_range.value()
    );
    Ok(())
}

/// The graph carries a readable frequency and gain scale.
///
/// It had neither — bare gridlines and nothing to read them against. These
/// assert presence and count rather than pixel positions, which is the part
/// that regresses: a refactor that drops the overlay leaves the curves looking
/// fine and the graph unreadable.
#[tokio::test]
async fn the_graph_labels_both_of_its_axes() -> dioxus_test::Result<()> {
    let fx = mount();

    let freq_labels = fx
        .tester
        .query_all(dioxus_test::by_testid("eq-freq-label"))
        .immediately();
    assert!(
        freq_labels.len() >= 8,
        "expected a frequency scale along the bottom, found {} labels",
        freq_labels.len()
    );

    let db_labels = fx
        .tester
        .query_all(dioxus_test::by_testid("eq-db-label"))
        .immediately();
    assert!(
        db_labels.len() >= 5,
        "expected a gain scale up the side, found {} labels",
        db_labels.len()
    );
    Ok(())
}

/// The gain scale follows the display range.
///
/// The labels are computed from `db_range`, so an auto-range expansion has to
/// relabel — a scale that still reads ±6 after the view opened to ±12 is worse
/// than no scale at all.
#[tokio::test]
async fn the_gain_scale_relabels_when_the_range_expands() -> dioxus_test::Result<()> {
    /// The gain scale as currently rendered, top to bottom.
    fn scale(fx: &support::Fixture) -> Vec<String> {
        fx.tester
            .query_all(dioxus_test::by_testid("eq-db-label"))
            .immediately()
            .iter()
            .map(dioxus_test::ResolvedElement::inner_html)
            .collect()
    }

    let fx = mount();
    let before = scale(&fx);
    assert_eq!(
        before,
        vec!["+6", "+3", "0", "-3", "-6"],
        "the default ±6 dB view should be labelled in 3 dB steps"
    );

    // Push a band into the top edge, which expands the range one step.
    let (_ox, oy) = fx.graph_origin();
    let (sx, sy) = fx.band_point(0);
    fx.tester.pointer_down(sx, sy);
    fx.settle().await;
    for step in 1..=4 {
        let t = f64::from(step) / 4.0;
        fx.tester.pointer_move(sx, sy + (oy + 1.0 - sy) * t, true);
        fx.settle().await;
    }
    fx.tester.pointer_up(sx, oy + 1.0);
    fx.settle().await;

    assert!(
        fx.params.db_range.value() > 1,
        "precondition: the drag should have expanded the range (index {})",
        fx.params.db_range.value()
    );

    // Wait for the relabel rather than snapshotting once. The param changes on
    // the drag, but the re-render that carries the new range into the overlay
    // lands a frame or more later — reading immediately made this pass alone
    // and fail two runs in three under the parallel suite.
    let mut after = before.clone();
    for _ in 0..20 {
        fx.settle().await;
        after = scale(&fx);
        if after != before {
            break;
        }
    }

    assert_ne!(
        before,
        after,
        "the gain scale still reads {before:?} after the range expanded to ±{} dB",
        eq_ui::eq_graph_model::db_range_for_index(fx.params.db_range.value())
    );
    Ok(())
}

// ── Naming a band ───────────────────────────────────────────────────────
//
// Double-clicking a band node turns its name label into a text field, in
// place, above the node. The label itself is display-only: it renders on top
// of the node it names, so giving it pointer events would shadow the node and
// break dragging.

use dioxus_test::keyboard_types::Key;

const LABEL_INPUT: &str = "eq-band-label-input";

impl support::Fixture {
    async fn click_at(&self, x: f64, y: f64) {
        self.tester.pointer_down(x, y);
        self.settle().await;
        self.tester.pointer_up(x, y);
        self.settle().await;
    }

    async fn double_click_at(&self, x: f64, y: f64) {
        self.click_at(x, y).await;
        self.click_at(x, y).await;
    }

    fn label_editor_is_open(&self) -> bool {
        self.tester.root().inner_html().contains(LABEL_INPUT)
    }
}

/// A single click selects the band. Only the second one opens the field —
/// otherwise every band you touched would drop you into text entry.
#[tokio::test]
async fn one_click_on_a_band_does_not_open_the_name_field() -> dioxus_test::Result<()> {
    let fx = mount();
    let (x, y) = fx.band_point(1);
    fx.click_at(x, y).await;

    assert!(
        !fx.label_editor_is_open(),
        "a single click on a band must not open the name field"
    );
    Ok(())
}

/// Double-clicking the node opens the field, and typing into it renames the
/// band — as a real parameter write, not just local UI state.
#[tokio::test]
async fn double_clicking_a_band_node_names_it() -> dioxus_test::Result<()> {
    let fx = mount();
    let (x, y) = fx.band_point(1);
    fx.double_click_at(x, y).await;

    assert!(
        fx.label_editor_is_open(),
        "double-clicking a band node should open its name field"
    );

    fx.tester.type_text("Boxiness");
    fx.settle().await;

    assert!(
        fx.tester.root().inner_html().contains("Boxiness"),
        "typing into the name field should reach the band's name"
    );
    Ok(())
}

/// The field is autofocused, so the user can type the moment it appears —
/// that is the point of editing in place instead of through a menu.
#[tokio::test]
async fn the_name_field_takes_focus_when_it_opens() -> dioxus_test::Result<()> {
    let fx = mount();
    let (x, y) = fx.band_point(1);
    fx.double_click_at(x, y).await;

    assert!(fx.label_editor_is_open(), "the field should be open");
    assert!(
        fx.tester.blitz_focus().is_some(),
        "the name field should be focused as soon as it opens"
    );
    Ok(())
}

/// Enter closes the field. Whatever was typed is already committed —
/// `oninput` writes through on every keystroke — so this is about dismissing
/// the editor, not saving.
#[tokio::test]
async fn enter_closes_the_name_field() -> dioxus_test::Result<()> {
    let fx = mount();
    let (x, y) = fx.band_point(1);
    fx.double_click_at(x, y).await;
    assert!(fx.label_editor_is_open());

    fx.tester.press_key(Key::Enter, Modifiers::empty());
    fx.settle().await;

    assert!(
        !fx.label_editor_is_open(),
        "Enter should close the name field"
    );
    Ok(())
}

/// Double-clicking a node used to flatten it to 0 dB. It must not any more:
/// naming a band is not a reason to undo the move that made it worth naming.
#[tokio::test]
async fn double_clicking_a_band_no_longer_resets_its_gain() -> dioxus_test::Result<()> {
    let fx = mount();
    let bp = &fx.params.bands[1];

    // Give the band a gain worth preserving.
    let (bx, by) = fx.band_point(1);
    fx.tester.pointer_down(bx, by);
    fx.settle().await;
    fx.tester.pointer_move(bx, by - 40.0, true);
    fx.settle().await;
    fx.tester.pointer_up(bx, by - 40.0);
    fx.settle().await;

    let gain_before = bp.gain_db.value();
    assert!(
        gain_before.abs() > 0.5,
        "the drag should have moved the band off 0 dB: {gain_before}"
    );

    let (x, y) = fx.band_point(1);
    fx.double_click_at(x, y).await;

    assert!(fx.label_editor_is_open(), "the name field should be open");
    assert!(
        (bp.gain_db.value() - gain_before).abs() < 0.01,
        "double-click must not reset the gain: {gain_before} -> {}",
        bp.gain_db.value()
    );
    Ok(())
}

/// The field opens with the existing name SELECTED, so the first keystroke
/// replaces it — the behaviour every other text field has. Blitz has no
/// reachable selection API from the DOM side, so this is modelled: what
/// matters is that typing over an open field replaces rather than prepends.
#[tokio::test]
async fn typing_over_an_open_name_replaces_it() -> dioxus_test::Result<()> {
    let fx = mount();
    let (x, y) = fx.band_point(1);

    fx.double_click_at(x, y).await;
    fx.tester.type_text("Boxiness");
    fx.settle().await;
    fx.tester.press_key(Key::Enter, Modifiers::empty());
    fx.settle().await;
    assert_eq!(*fx.params.bands[1].name.read(), "Boxiness");

    // Reopen and type a different name over it.
    let (x, y) = fx.band_point(1);
    fx.double_click_at(x, y).await;
    assert!(fx.label_editor_is_open(), "the field should be open");
    fx.tester.type_text("Air");
    fx.settle().await;

    assert_eq!(
        *fx.params.bands[1].name.read(),
        "Air",
        "typing over the selected name should replace it, not prepend to it"
    );
    Ok(())
}

/// Opening the field does not, by itself, change the name. A double-click
/// that goes nowhere must leave the band exactly as it was.
#[tokio::test]
async fn opening_the_field_alone_does_not_change_the_name() -> dioxus_test::Result<()> {
    let fx = mount();
    let (x, y) = fx.band_point(1);

    fx.double_click_at(x, y).await;
    fx.tester.type_text("Boxiness");
    fx.settle().await;
    fx.tester.press_key(Key::Enter, Modifiers::empty());
    fx.settle().await;

    let (x, y) = fx.band_point(1);
    fx.double_click_at(x, y).await;
    assert!(fx.label_editor_is_open());
    fx.tester.press_key(Key::Escape, Modifiers::empty());
    fx.settle().await;

    assert!(!fx.label_editor_is_open(), "Escape should close the field");
    assert_eq!(
        *fx.params.bands[1].name.read(),
        "Boxiness",
        "opening and leaving the field must not touch the name"
    );
    Ok(())
}

/// Clicking into the empty field puts the old name back, so a rename can be
/// an edit rather than a retype. This is the other half of open-empty: the
/// name is offered, not thrown away.
#[tokio::test]
async fn clicking_into_the_empty_field_restores_the_old_name() -> dioxus_test::Result<()> {
    let fx = mount();
    let (x, y) = fx.band_point(1);

    fx.double_click_at(x, y).await;
    fx.tester.type_text("Boxiness");
    fx.settle().await;
    fx.tester.press_key(Key::Enter, Modifiers::empty());
    fx.settle().await;
    assert_eq!(*fx.params.bands[1].name.read(), "Boxiness");

    // Reopen — the field is empty and the name is held aside.
    let (x, y) = fx.band_point(1);
    fx.double_click_at(x, y).await;
    assert!(fx.label_editor_is_open());
    assert_eq!(*fx.params.bands[1].name.read(), "");

    // Click into the field.
    let el = fx
        .tester
        .query(dioxus_test::by_testid("eq-band-label-input"))
        .immediately()?;
    let (ex, ey) = el.document_origin();
    let (ew, eh) = el.size();
    fx.click_at(ex + f64::from(ew) / 2.0, ey + f64::from(eh) / 2.0)
        .await;

    assert_eq!(
        *fx.params.bands[1].name.read(),
        "Boxiness",
        "clicking into the empty field should bring the old name back to edit"
    );
    Ok(())
}

/// A space in a band name survives being typed.
///
/// It did not: the field is controlled, and `commit` trimmed on every
/// keystroke, so "Air " was stored as "Air", the value prop snapped the
/// editor back to "Air", and the space was gone before the next letter
/// arrived. Typing "Air Lift" produced "AirLift".
#[tokio::test]
async fn a_space_in_a_band_name_survives_typing() -> dioxus_test::Result<()> {
    let fx = mount();
    let (x, y) = fx.band_point(1);
    fx.double_click_at(x, y).await;

    fx.tester.type_text("Air Lift");
    fx.settle().await;

    assert_eq!(
        *fx.params.bands[1].name.read(),
        "Air Lift",
        "the space should still be there"
    );
    Ok(())
}

/// Trailing whitespace is tidied when the field closes, not while typing.
/// That distinction is the whole fix: trimming per keystroke is what ate
/// the spaces.
#[tokio::test]
async fn whitespace_is_tidied_when_the_field_closes() -> dioxus_test::Result<()> {
    let fx = mount();
    let (x, y) = fx.band_point(1);
    fx.double_click_at(x, y).await;

    fx.tester.type_text("Air ");
    fx.settle().await;
    assert_eq!(
        *fx.params.bands[1].name.read(),
        "Air ",
        "mid-edit the value is exactly what was typed"
    );

    fx.tester.press_key(Key::Enter, Modifiers::empty());
    fx.settle().await;
    assert_eq!(
        *fx.params.bands[1].name.read(),
        "Air",
        "closing tidies the trailing space"
    );
    Ok(())
}

/// The label sits nearest the node and the readout chip beyond it.
///
/// The label is the permanent one — a named band keeps it whether or not it
/// is focused, while the chip comes and goes — so it is what has to read as
/// belonging to the node.
#[test]
fn the_label_sits_between_the_node_and_the_chip() {
    use eq_ui::eq_graph_popup::{band_chip_rect, band_label_anchor};

    let (bx, by) = (400.0, 200.0);
    let (_, label_bottom) = band_label_anchor(bx, by, 800.0, 350.0);
    let (_, chip_y, _, chip_h) = band_chip_rect(bx, by, 800.0, 350.0);

    assert!(label_bottom < by, "the label should be above the node");
    assert!(
        chip_y + chip_h <= label_bottom,
        "the chip should be beyond the label, not between it and the node: \
         chip bottom {} vs label bottom {label_bottom}",
        chip_y + chip_h
    );
}

/// Near the top of the graph there is no room above, so both flip below the
/// node — and keep their order relative to it.
#[test]
fn near_the_top_they_flip_below_and_keep_their_order() {
    use eq_ui::eq_graph_popup::{band_chip_rect, band_label_anchor};

    let (bx, by) = (400.0, 4.0);
    let (_, label_bottom) = band_label_anchor(bx, by, 800.0, 350.0);
    let (_, chip_y, _, _) = band_chip_rect(bx, by, 800.0, 350.0);

    assert!(
        label_bottom > by,
        "with no room above, the label goes below"
    );
    assert!(
        chip_y >= label_bottom,
        "the chip should still be the far one: chip {chip_y} vs label {label_bottom}"
    );
}

/// Backspace deletes a character in the name field.
///
/// Sent as the macOS editing COMMAND rather than as `Key::Backspace`,
/// because that is what actually reaches blitz: its text input gates the
/// `Key::Backspace` arm behind `#[cfg(not(target_os = "macos"))]` and expects
/// the platform's own command instead. baseview never ran AppKit's
/// `interpretKeyEvents:`, so nothing produced one and backspace did nothing
/// at all — in a DAW or standalone. nice-plug-dioxus synthesises it now; this
/// holds up our end of that contract.
#[tokio::test]
async fn the_delete_command_removes_a_character_from_the_name_field() -> dioxus_test::Result<()> {
    let fx = mount();
    let (x, y) = fx.band_point(1);
    fx.double_click_at(x, y).await;

    fx.tester.type_text("Air");
    fx.settle().await;
    assert_eq!(*fx.params.bands[1].name.read(), "Air");

    fx.tester
        .send_ui_event(blitz_traits::events::UiEvent::AppleStandardKeybinding(
            "deleteBackward:".into(),
        ));
    fx.settle().await;

    assert_eq!(
        *fx.params.bands[1].name.read(),
        "Ai",
        "the delete command should have removed the last character"
    );
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────
// The detail panel's own dials
// ─────────────────────────────────────────────────────────────────────────

/// Document point of a dial in the open band panel, found by its readout.
///
/// A [`Knob`](fts_audio_ui::controls::knob::Knob) tags its gesture surface
/// `knob-{name}-dial` — the box a hand actually presses.
fn panel_dial(fx: &support::Fixture, name: &str) -> (f64, f64) {
    let id = format!("knob-{name}-dial");
    let el = fx
        .tester
        .query(dioxus_test::by_testid(id.as_str()))
        .immediately()
        .unwrap_or_else(|e| panic!("no {name} dial in the band panel: {e:?}"));
    let (x, y) = el.document_origin();
    let (w, h) = el.size();
    (x + f64::from(w) / 2.0, y + f64::from(h) / 2.0)
}

/// Dragging a dial in the band detail panel must move its parameter.
///
/// The panel stops pointer events so they never reach the graph underneath —
/// without that, its own coordinates get read as a band drag and the band
/// jumps across the plot. But `DragProvider` lives *above* the graph, so
/// stopping them cut the panel's own dials off from the drag layer: a press
/// on FREQ, GAIN or Q opened a host edit gesture and then nothing moved.
#[tokio::test]
async fn dragging_a_panel_dial_moves_its_parameter() -> dioxus_test::Result<()> {
    let fx = mount();
    let bp = &fx.params.bands[1];
    let before = bp.gain_db.value();

    let node = fx.band_point(1);
    fx.tester.pointer_move(node.0, node.1, false);
    fx.settle().await;
    let (dx, dy) = panel_dial(&fx, "GAIN");

    // Short travel, staying inside the panel. The dials now have the panel's
    // full width and sit in the middle of a 150 px box, so a 32 px drag walks
    // the pointer out of the top of it — and once it is over the graph the
    // panel is no longer the one pumping the drag layer, which made this pass
    // or fail depending on timing.
    fx.tester.pointer_down(dx, dy);
    fx.settle().await;
    for step in 1..=3 {
        fx.tester.pointer_move(dx, dy - f64::from(step) * 4.0, true);
        fx.settle().await;
    }
    fx.tester.pointer_up(dx, dy - 12.0);
    fx.settle().await;

    let after = bp.gain_db.value();
    assert!(
        (after - before).abs() > 0.01,
        "dragging the panel's GAIN dial left it at {before} dB",
    );
    Ok(())
}

/// And the gesture it opened has to close.
///
/// The swallowed `mouseup` is the half of that bug with teeth: the drag ends
/// as far as the hand is concerned, but `end_set_parameter` is never sent, so
/// the host is left inside an automation gesture on that parameter — and the
/// next press begins another one on top of it. A plugin does not survive many
/// of those.
#[tokio::test]
async fn a_panel_dial_closes_the_host_gesture_it_opened() -> dioxus_test::Result<()> {
    let fx = mount();
    let key = ptr_key(fx.params.bands[1].gain_db.as_ptr());

    let node = fx.band_point(1);
    fx.tester.pointer_move(node.0, node.1, false);
    fx.settle().await;
    let (dx, dy) = panel_dial(&fx, "GAIN");

    fx.log.lock().unwrap().clear();
    fx.tester.pointer_down(dx, dy);
    fx.settle().await;
    fx.tester.pointer_move(dx, dy - 16.0, true);
    fx.settle().await;
    fx.tester.pointer_up(dx, dy - 16.0);
    fx.settle().await;

    let log = fx.log.lock().unwrap().clone();
    let begins = log.iter().filter(|g| **g == Gesture::Begin(key)).count();
    let ends = log.iter().filter(|g| **g == Gesture::End(key)).count();
    assert_eq!(
        (begins, ends),
        (1, 1),
        "unbalanced host gesture on the panel's GAIN dial: {log:?}",
    );
    Ok(())
}

/// Pressing a dial on the panel pins the panel where it is.
///
/// The panel tracks its band horizontally, so turning the band's own FREQ
/// dial slides the panel out from under the hand turning it — the control
/// runs away from the cursor driving it. Pro-Q's answer, and ours: while the
/// panel is in use it holds still, and only catches up once you have left it
/// alone. The decision itself is
/// [`held_panel_x`](eq_ui::eq_graph_popup::held_panel_x), unit-tested beside
/// it; what this covers is that a press on a dial actually reaches it.
///
/// Not asserted here: that the panel fails to follow a moving band. It could
/// not fail — the band's screen position comes from a signal `control_view`
/// refreshes on re-render, and nothing re-renders it mid-drag without the
/// editor's frame loop, so headless the band does not move during a panel
/// drag at all and the assertion would pass against no fix.
#[tokio::test]
async fn a_press_on_a_panel_dial_does_not_move_the_panel() -> dioxus_test::Result<()> {
    let fx = mount();
    let bp = &fx.params.bands[1];
    let hz_before = bp.freq_hz.value();

    let node = fx.band_point(1);
    fx.tester.pointer_move(node.0, node.1, false);
    fx.settle().await;
    let before_x = fx
        .panel()
        .expect("panel is not mounted")
        .document_origin()
        .0;

    let (dx, dy) = panel_dial(&fx, "FREQ");
    fx.tester.pointer_down(dx, dy);
    fx.settle().await;
    for step in 1..=6 {
        fx.tester.pointer_move(dx, dy - f64::from(step) * 3.0, true);
        fx.settle().await;
        let x = fx
            .panel()
            .expect("panel closed mid-drag")
            .document_origin()
            .0;
        assert!(
            (x - before_x).abs() < 0.5,
            "the panel moved {} px while its FREQ dial was being turned",
            x - before_x,
        );
    }
    fx.tester.pointer_up(dx, dy - 18.0);
    fx.settle().await;

    assert!(
        bp.freq_hz.value() > hz_before,
        "the FREQ dial did not move: still {hz_before} Hz",
    );
    Ok(())
}

/// Adding a band by double-clicking empty graph, twice.
///
/// The second one crashed the host.
#[tokio::test]
async fn double_clicking_empty_graph_twice_adds_two_bands() -> dioxus_test::Result<()> {
    let fx = mount();
    let (ox, oy) = fx.graph_origin();

    let used = |fx: &support::Fixture| {
        (0..eq_ui::params::NUM_BANDS)
            .filter(|i| fx.params.bands[*i].enabled.value() > 0.5)
            .count()
    };
    let before = used(&fx);

    for (dx, dy) in [(-260.0_f64, 60.0_f64), (240.0, -70.0)] {
        let (x, y) = (ox + 400.0 + dx, oy + 175.0 + dy);
        fx.tap(x, y).await;
        fx.tap(x, y).await;
    }

    let after = used(&fx);
    assert!(
        after > before,
        "double-clicking empty graph added nothing: {before} → {after} bands",
    );
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────
// The keyboard layer
// ─────────────────────────────────────────────────────────────────────────

/// `m`, `s`, `l` and `r` route the band under the pointer.
///
/// Blocked on the transport, not on the mapping. Blitz sends a key to the
/// focused node and falls back to the root element when nothing is focused —
/// and a **printable** key never reaches a Dioxus handler at all: `Delete`
/// does (the test below proves it end to end), `Character("m")` does not.
/// Something consumes it on the way, most likely the text-input/KeyPress
/// path, which Blitz does not forward to the UI at all
/// (`DomEventData::KeyPress(_) => None`).
///
/// The key map itself is tested in `eq_graph_interaction::key_map_tests`, and
/// the acting code is shared with delete. What is missing is a way for a
/// letter to arrive — probably the same window-level interception
/// `nice-plug-dioxus` already does for space and backspace.
#[ignore = "printable keys do not reach a Dioxus handler in this Blitz; Delete does"]
#[tokio::test]
async fn the_placement_keys_route_the_hovered_band() -> dioxus_test::Result<()> {
    let fx = mount();
    let bp = &fx.params.bands[1];
    assert_eq!(bp.placement.value(), 0, "band 1 did not start in stereo");

    // 3 = Mid, in the placement parameter's own order.
    for (key, expected) in [("m", 3), ("s", 4), ("l", 1), ("r", 2), ("n", 0)] {
        let (x, y) = fx.band_point(1);
        fx.tester.pointer_move(x, y, false);
        fx.settle().await;
        assert!(
            fx.panel().is_some(),
            "{key}: the pointer is not on band 1 — nothing is hovered to route",
        );
        fx.tester.key_down(
            dioxus_test::keyboard_types::Key::Character(key.to_string()),
            Modifiers::empty(),
        );
        fx.settle().await;
        assert_eq!(
            bp.placement.value(),
            expected,
            "{key} did not route the band",
        );
    }
    Ok(())
}

/// Lasso a group of bands, then delete them all with one key.
#[tokio::test]
async fn drag_select_then_delete_removes_every_selected_band() -> dioxus_test::Result<()> {
    let fx = mount();
    let enabled = |fx: &support::Fixture| {
        (0..eq_ui::params::NUM_BANDS)
            .filter(|i| fx.params.bands[*i].enabled.value() > 0.5)
            .count()
    };
    let before = enabled(&fx);
    assert!(before >= 2, "fixture has only {before} bands to select");

    // Lasso the whole plot.
    let (ox, oy) = fx.graph_origin();
    fx.tester.pointer_down(ox + 4.0, oy + 4.0);
    fx.settle().await;
    for step in 1..=6 {
        let t = f64::from(step) / 6.0;
        fx.tester
            .pointer_move(ox + 4.0 + 780.0 * t, oy + 4.0 + 330.0 * t, true);
        fx.settle().await;
    }
    fx.tester.pointer_up(ox + 784.0, oy + 334.0);
    fx.settle().await;
    let after_lasso = enabled(&fx);
    assert_eq!(
        after_lasso, before,
        "the lasso itself removed bands ({before} → {after_lasso}); the key \
         press below would then be proving nothing",
    );

    fx.tester
        .key_down(dioxus_test::keyboard_types::Key::Delete, Modifiers::empty());
    fx.settle().await;

    let after = enabled(&fx);
    assert_eq!(after, 0, "delete left {after} of {before} bands enabled",);
    Ok(())
}

/// Same key, targeted by hover instead of by selection.
#[tokio::test]
async fn a_letter_routes_the_band_under_the_pointer() -> dioxus_test::Result<()> {
    let fx = mount();
    let bp = &fx.params.bands[1];
    assert_eq!(bp.placement.value(), 0);

    let (x, y) = fx.band_point(1);
    fx.tester.pointer_move(x, y, false);
    fx.settle().await;
    assert!(fx.panel().is_some(), "band 1 is not hovered");

    fx.tester.key_down(
        dioxus_test::keyboard_types::Key::Character("m".to_string()),
        Modifiers::empty(),
    );
    fx.settle().await;
    assert_eq!(
        bp.placement.value(),
        3,
        "hover did not name a target for `m`"
    );
    Ok(())
}

/// A key has to work without a click first.
///
/// Blitz routes a key to the focused node and falls back to the *document*
/// root, which sits above anything Dioxus renders — so with nothing focused
/// every key is quietly lost. Focus is only ever *set* by clicking, which is
/// what made this so confusing: the delete test passed because its lasso had
/// pressed the pointer down somewhere first, and the placement test failed
/// because hovering never clicks. The editor claims focus a frame after mount
/// instead.
#[tokio::test]
async fn a_key_works_without_clicking_the_editor_first() -> dioxus_test::Result<()> {
    let fx = mount();
    let enabled = |fx: &support::Fixture| {
        (0..eq_ui::params::NUM_BANDS)
            .filter(|i| fx.params.bands[*i].enabled.value() > 0.5)
            .count()
    };
    // Hover only. No press anywhere, ever.
    let (x, y) = fx.band_point(1);
    fx.tester.pointer_move(x, y, false);
    fx.settle().await;
    let before = enabled(&fx);

    fx.tester
        .key_down(dioxus_test::keyboard_types::Key::Delete, Modifiers::empty());
    fx.settle().await;
    assert!(
        enabled(&fx) < before,
        "Delete did nothing — the editor never took focus, so the key went to \
         the document root where nothing is listening",
    );
    Ok(())
}

/// `f` focuses the band under the pointer while it is held, and lets go.
#[tokio::test]
async fn holding_f_focuses_the_hovered_band() -> dioxus_test::Result<()> {
    let fx = mount();
    let bp = &fx.params.bands[1];
    assert!(bp.focus.value() < 0.5, "band 1 started focused");

    let (x, y) = fx.band_point(1);
    fx.tester.pointer_move(x, y, false);
    fx.settle().await;

    let f = || dioxus_test::keyboard_types::Key::Character("f".to_string());
    fx.tester.key_down(f(), Modifiers::empty());
    fx.settle().await;
    assert!(bp.focus.value() > 0.5, "holding f did not focus the band");

    fx.tester.key_up(f(), Modifiers::empty());
    fx.settle().await;
    assert!(
        bp.focus.value() < 0.5,
        "focus latched instead of releasing on key-up",
    );
    Ok(())
}

/// `d` toggles delta listening for the whole EQ, and is not per band.
#[tokio::test]
async fn d_toggles_delta_listening() -> dioxus_test::Result<()> {
    let fx = mount();
    assert!(fx.params.delta.value() < 0.5, "delta started on");

    let d = || dioxus_test::keyboard_types::Key::Character("d".to_string());
    fx.tester.key_down(d(), Modifiers::empty());
    fx.settle().await;
    assert!(fx.params.delta.value() > 0.5, "d did not turn delta on");

    fx.tester.key_up(d(), Modifiers::empty());
    fx.settle().await;
    fx.tester.key_down(d(), Modifiers::empty());
    fx.settle().await;
    assert!(
        fx.params.delta.value() < 0.5,
        "d did not turn delta back off"
    );

    // Nothing was hovered or selected: delta is a statement about the EQ.
    assert!(
        (0..eq_ui::params::NUM_BANDS).all(|i| fx.params.bands[i].focus.value() < 0.5),
        "delta focused a band as a side effect",
    );
    Ok(())
}

/// Alt-creating a band makes it dynamic; Alt+Shift makes it spectral.
///
/// The mapping has been in `create_mode` all along and the double-click
/// handler has always called `set_mode` — what was never checked is that a
/// band created that way comes out the other side actually dynamic.
#[tokio::test]
async fn alt_creating_a_band_makes_it_dynamic() -> dioxus_test::Result<()> {
    let fx = mount();
    let (ox, oy) = fx.graph_origin();

    let fresh = |fx: &support::Fixture| {
        (0..eq_ui::params::NUM_BANDS)
            .find(|i| fx.params.bands[*i].enabled.value() < 0.5)
            .expect("no free band slot")
    };

    for (mods, label, spectral) in [
        (Modifiers::ALT, "alt", false),
        (Modifiers::ALT | Modifiers::SHIFT, "alt+shift", true),
    ] {
        let idx = fresh(&fx);
        let (x, y) = (
            ox + 200.0 + f64::from(u8::from(spectral)) * 240.0,
            oy + 120.0,
        );
        // The two presses go in back to back. Double-click detection is on a
        // 400 ms wall-clock threshold, and settling between every event is
        // slow enough under a loaded test run to miss it — which showed up as
        // this test passing alone and failing in the suite.
        fx.tester.pointer_down_mods(x, y, mods);
        fx.tester.pointer_up_mods(x, y, mods);
        fx.tester.pointer_down_mods(x, y, mods);
        fx.tester.pointer_up_mods(x, y, mods);
        fx.settle().await;

        let bp = &fx.params.bands[idx];
        assert!(
            bp.enabled.value() > 0.5,
            "{label}-double-click did not create a band in slot {idx}",
        );
        assert!(
            bp.dyn_range_db.value().abs() > 0.05,
            "{label}-created band {idx} has no dynamic range: {}",
            bp.dyn_range_db.value(),
        );
        assert_eq!(
            bp.spectral.value() > 0.5,
            spectral,
            "{label}-created band {idx} has the wrong spectral flag",
        );
    }
    Ok(())
}

/// Holding `b` takes the band you are on, boosts it hard and narrow, and
/// hands it to the pointer.
///
/// It borrows the band you are already working on rather than creating one:
/// the gesture is "make *this* louder so I can hear what it is sitting on",
/// not "add a probe". Letting go keeps the frequency you landed on — that is
/// the answer you went looking for — and puts the shape back.
#[tokio::test]
async fn holding_b_sweeps_the_band_you_are_on() -> dioxus_test::Result<()> {
    let fx = mount();
    let enabled_now = |fx: &support::Fixture| {
        (0..eq_ui::params::NUM_BANDS)
            .filter(|i| fx.params.bands[*i].enabled.value() > 0.5)
            .count()
    };
    let enabled_before = enabled_now(&fx);
    let bp = &fx.params.bands[1];
    let gain_before = bp.gain_db.value();
    let q_before = bp.q.value();
    let shape_before = bp.filter_type.value();

    let (x, y) = fx.band_point(1);
    fx.tester.pointer_move(x, y, false);
    fx.settle().await;

    let b = || dioxus_test::keyboard_types::Key::Character("b".to_string());
    fx.tester.key_down(b(), Modifiers::empty());
    fx.settle().await;

    assert!(
        bp.gain_db.value() > 10.0,
        "the sweep is only {} dB — too quiet to hunt with",
        bp.gain_db.value(),
    );
    // As the editor shows it — the parameter carries √2 times the Q.
    let shown_q = bp.q.value() * std::f32::consts::FRAC_1_SQRT_2;
    assert!(
        shown_q > 12.0,
        "the sweep is too wide to find one ringing note: Q {shown_q}",
    );
    assert_eq!(
        enabled_now(&fx),
        enabled_before,
        "the sweep created a band instead of borrowing the one you are on",
    );

    // It follows the pointer — and the boost survives the move.
    //
    // The gain assertion below does not currently fail against the bug it
    // describes: putting the whole-band write back leaves it green, because
    // the harness settles the DOM between every event so the graph's copy of
    // the band is never stale. In the plugin it is — the editor renders at
    // around twelve frames a second, so a pointer move lands long before
    // control_view has re-rendered with the boosted gain, and the stale copy
    // gets written back over it. Kept as a guard on the outcome rather than
    // as proof of the cause.
    let at_start = bp.freq_hz.value();
    let (ox, oy) = fx.graph_origin();
    for step in 1..=4 {
        fx.tester
            .pointer_move(ox + 200.0 + f64::from(step) * 110.0, oy + 100.0, false);
        fx.settle().await;
        assert!(
            bp.gain_db.value() > 10.0,
            "the boost collapsed to {} dB as soon as the pointer moved",
            bp.gain_db.value(),
        );
    }
    let swept_to = bp.freq_hz.value();
    assert!(
        swept_to > at_start * 1.5,
        "the sweep did not follow the pointer: {at_start} -> {swept_to}",
    );

    // Letting go restores the shape but keeps where you landed.
    fx.tester.key_up(b(), Modifiers::empty());
    fx.settle().await;
    assert!(
        (bp.gain_db.value() - gain_before).abs() < 0.05,
        "the boost stayed behind: {gain_before} -> {}",
        bp.gain_db.value(),
    );
    assert!(
        (bp.q.value() - q_before).abs() < 0.05,
        "the narrow Q stayed behind: {q_before} -> {}",
        bp.q.value(),
    );
    assert_eq!(
        bp.filter_type.value(),
        shape_before,
        "the shape did not come back"
    );
    assert!(
        (bp.freq_hz.value() - swept_to).abs() < 1.0,
        "the frequency the sweep found was thrown away: {swept_to} -> {}",
        bp.freq_hz.value(),
    );
    Ok(())
}

/// Shift makes it a cut, which is the other half of the technique.
#[tokio::test]
async fn shift_b_sweeps_a_cut() -> dioxus_test::Result<()> {
    let fx = mount();
    let (x, y) = fx.band_point(1);
    fx.tester.pointer_move(x, y, false);
    fx.settle().await;

    fx.tester.key_down(
        dioxus_test::keyboard_types::Key::Character("B".to_string()),
        Modifiers::SHIFT,
    );
    fx.settle().await;

    assert!(
        fx.params.bands[1].gain_db.value() < -10.0,
        "shift+b boosted instead of cutting: {} dB",
        fx.params.bands[1].gain_db.value(),
    );
    Ok(())
}

/// Holding `b` *while dragging a band* must keep the boost.
///
/// This is how the gesture is actually used — you have hold of the band, you
/// want it loud and narrow while you hunt, and you drag it across the
/// spectrum. The drag writes gain from the pointer's height every frame, so
/// it overwrites the boost as fast as the sweep applies it: the band locks
/// itself to wherever the mouse is instead of staying up at +15.
#[tokio::test]
async fn a_sweep_held_during_a_drag_keeps_its_boost() -> dioxus_test::Result<()> {
    let fx = mount();
    let bp = &fx.params.bands[1];
    let (sx, sy) = fx.band_point(1);

    // Take hold of the band first, the way a hand does.
    fx.tester.pointer_down(sx, sy);
    fx.settle().await;
    fx.tester.pointer_move(sx + 8.0, sy - 4.0, true);
    fx.settle().await;

    let b = || dioxus_test::keyboard_types::Key::Character("b".to_string());
    fx.tester.key_down(b(), Modifiers::empty());
    // Twice: the sweep's target is set at the root and read by the graph, so
    // the suppression it turns on lands a render after the boost does.
    fx.settle().await;
    fx.settle().await;
    assert!(
        bp.gain_db.value() > 10.0,
        "holding b mid-drag did not boost: {} dB",
        bp.gain_db.value(),
    );

    // Now sweep, with the button still down and the pointer wandering in y.
    for step in 1..=5 {
        let t = f64::from(step);
        fx.tester
            .pointer_move(sx + t * 70.0, sy - 4.0 + t * 14.0, true);
        fx.settle().await;
        assert!(
            bp.gain_db.value() > 10.0,
            "the sweep locked to the pointer: {} dB after {step} moves",
            bp.gain_db.value(),
        );
    }

    fx.tester.key_up(b(), Modifiers::empty());
    fx.settle().await;
    fx.tester.pointer_up(sx + 350.0, sy + 66.0);
    fx.settle().await;
    Ok(())
}

/// And releasing the sweep mid-drag hands the gain back to the pointer.
///
/// The drag is still running, so the band should answer the mouse again
/// immediately — a sweep that left the gain stuck would be worse than one
/// that never held it.
#[tokio::test]
async fn releasing_a_sweep_mid_drag_returns_the_gain_to_the_pointer() -> dioxus_test::Result<()> {
    let fx = mount();
    let bp = &fx.params.bands[1];
    let (sx, sy) = fx.band_point(1);

    fx.tester.pointer_down(sx, sy);
    fx.settle().await;
    let b = || dioxus_test::keyboard_types::Key::Character("b".to_string());
    fx.tester.key_down(b(), Modifiers::empty());
    fx.settle().await;
    fx.tester.pointer_move(sx + 120.0, sy, true);
    fx.settle().await;
    fx.tester.key_up(b(), Modifiers::empty());
    fx.settle().await;

    let after_release = bp.gain_db.value();
    // Now drag well down the plot; the gain must follow again.
    fx.tester.pointer_move(sx + 120.0, sy + 90.0, true);
    fx.settle().await;
    assert!(
        bp.gain_db.value() < after_release - 0.5,
        "the gain stayed pinned after the sweep was released: \
         {after_release} -> {}",
        bp.gain_db.value(),
    );
    fx.tester.pointer_up(sx + 120.0, sy + 90.0);
    fx.settle().await;
    Ok(())
}

/// Diagnostic: where the graph actually puts things.
#[ignore = "diagnostic"]
#[tokio::test]
async fn where_is_everything() -> dioxus_test::Result<()> {
    let fx = mount();
    let surf = fx.graph_origin();
    eprintln!("graph origin {surf:?}");
    for sel in ["eq-db-label", "eq-freq-label"] {
        let all = fx
            .tester
            .query_all(dioxus_test::by_testid(sel))
            .immediately();
        let ys: Vec<(f64, f64)> = all.iter().map(|e| e.document_origin()).collect();
        eprintln!("{sel}: {ys:?}");
    }
    let (bx, by) = fx.band_point(1);
    eprintln!("band 1 node (mapper, from the fixture) = ({bx}, {by})");
    eprintln!(
        "band 1 params: {} Hz {} dB",
        fx.params.bands[1].freq_hz.value(),
        fx.params.bands[1].gain_db.value()
    );
    Ok(())
}
