//! Shared fixture for the comp editor's headless tests.
//!
//! Mounts `comp_ui::control_view::App` — the exact Dioxus surface the
//! CLAP/VST3 plugin embeds — on the vendored dioxus-test harness, with a
//! recording host stub standing in for the DAW. Used by the behavioural tests
//! (`gui_editor.rs`) and by the screenshot harness (`screenshots.rs`).

// Shared by two test binaries, and neither uses all of it — the screenshot
// harness never inspects the gesture log, the behavioural tests never pose the
// meters.
#![allow(dead_code)]

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use dioxus_test::{by_testid, render, DocumentTester};

use comp_ui::control_view::App;
use comp_ui::params::{CompParams, CompUiState};

use nice_plug_dioxus::{ParamContext, SharedState};

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
/// elements are dropped by the headless harness's NoOpDocument fallback).
#[component]
pub fn Harness() -> Element {
    rsx! {
        style {
            "html, body {{ width:100%; height:100%; margin:0; padding:0; overflow:hidden; }}"
        }
        style { {nice_plug_dioxus::TAILWIND_CSS} }
        style { {include_str!("../../assets/tailwind.css")} }
        App {}
    }
}

pub struct Fixture {
    pub tester: DocumentTester,
    pub params: Arc<CompParams>,
    /// Shared UI state — the meter atomics a test can drive to pose the
    /// editor (a VU pinned at 0 tells you nothing about the meter).
    pub ui: Arc<CompUiState>,
    pub log: Arc<Mutex<Vec<Gesture>>>,
}

/// Mounts the real editor with every context `AppShell` consumes:
///
/// - `ParamContext` over the [`RecordingGuiContext`] stub (what
///   `use_param_context()` / all `ctx.begin_set_raw(..)` call sites hit),
/// - `SharedState` wrapping `Arc<CompUiState>` (params + meters).
pub fn mount() -> Fixture {
    mount_sized(1200, 700)
}

/// Mount at an explicit window size — used to check the surface against the
/// editor size the plugin shell actually asks the host for.
pub fn mount_sized(width: u32, height: u32) -> Fixture {
    mount_with(Arc::new(CompParams::default()), width, height)
}

/// Mount against a params tree the caller has already set up — a persisted
/// profile id, a chosen editor form — so a test can open the editor the way a
/// host restoring a session would.
pub fn mount_with(params: Arc<CompParams>, width: u32, height: u32) -> Fixture {
    let ui_state = Arc::new(CompUiState::new(params.clone()));
    let log = Arc::new(Mutex::new(Vec::new()));

    let gui = GuiContext::new(Arc::new(RecordingGuiContext { log: log.clone() }));
    let param_ctx = ParamContext::new(gui, Arc::new(AtomicBool::new(true)));

    let tester = render(Harness)
        .with_window_size(width, height)
        // The editor sizes its faceplates and its graph from the window
        // size `nice-plug-dioxus` puts in context on resize. A headless
        // mount has no window, so state the size explicitly — without it
        // every mount looks like the design size and nothing scales.
        .with_root_context(comp_ui::hardware::panel::EditorSize(
            width as f64,
            height as f64,
        ))
        .with_root_context(param_ctx)
        .with_root_context(SharedState::new(ui_state.clone()))
        .build();

    Fixture {
        tester,
        params,
        ui: ui_state,
        log,
    }
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

    /// Let the editor re-render *and* re-lay-out after a change.
    ///
    /// Two separate things need coaxing here:
    ///
    /// - `AppShell` reads plugin params directly rather than through
    ///   signals, so a widget that only writes a param (the Segmented
    ///   selectors — unlike knobs, which also drive the shared
    ///   `DragProvider` signal) re-renders itself immediately but does not
    ///   re-render the shell. In the plugin the ~30 Hz tick thread picks
    ///   the change up within ~33 ms; here we have to wait for one.
    /// - `pump()` only drives the VirtualDom. It does **not** recompute
    ///   layout, so nodes created by the re-render keep a 0×0 box and every
    ///   size / hit-test assertion against them fails. `advance_time()` is
    ///   the call that resolves the document, so it has to follow.
    pub async fn settle(&mut self) {
        for _ in 0..10 {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            let _ = self.tester.pump().await;
        }
        self.tester
            .advance_time(std::time::Duration::from_millis(16))
            .await;
    }

    /// Rendered height of the graph.
    ///
    /// The graph is no longer pinned to `GRAPH_H` — it takes its share of
    /// the editor height — so tests that compute a point in dB space have
    /// to ask what height it actually rendered at. That single number is
    /// also the SVG's viewBox height, which is what keeps element y and
    /// viewBox y the same thing.
    pub fn graph_h(&self) -> f64 {
        self.tester
            .query(by_testid("comp-graph"))
            .immediately()
            .expect("comp-graph container not in DOM")
            .size()
            .1 as f64
    }

    /// Document-space origin of the compressor-graph interaction surface.
    /// The graph container's height and its viewBox height are the same
    /// number, so element y IS viewBox y — points computed with
    /// `db_to_y(.., graph_h())` plus this origin land exactly where the
    /// component hit-tests them.
    pub fn graph_origin(&self) -> (f64, f64) {
        self.tester
            .query(by_testid("comp-graph"))
            .immediately()
            .expect("comp-graph container not in DOM")
            .document_origin()
    }

    /// The current `d` attribute of the transfer-curve path.
    pub fn transfer_curve_d(&self) -> String {
        let html = self
            .tester
            .query(by_testid("comp-graph"))
            .immediately()
            .expect("comp-graph container not in DOM")
            .inner_html();
        let i = html
            .find("transfer-curve")
            .expect("transfer-curve path missing from graph DOM");
        let tag_start = html[..i]
            .rfind('<')
            .expect("malformed html around transfer-curve");
        let tag_end = i + html[i..].find('>').expect("unclosed transfer-curve tag");
        let tag = &html[tag_start..tag_end];
        let di = tag
            .find(" d=\"")
            .expect("transfer-curve path has no d attribute");
        let rest = &tag[di + 4..];
        rest[..rest.find('"').expect("unterminated d attribute")].to_string()
    }
}
