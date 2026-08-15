//! Shared fixture for the delay editor's headless tests.
//!
//! Mounts `delay_ui::control_view::App` — the exact Dioxus surface the
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

use delay_ui::control_view::{App, DelayUi};
use delay_ui::params::{DelayParams, DelayUiState};

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
        App {}
    }
}

pub struct Fixture {
    pub tester: DocumentTester,
    pub params: Arc<DelayParams>,
    /// Shared UI state — the meter atomics a test can drive to pose the
    /// editor (a VU pinned at 0 tells you nothing about the meter).
    pub ui: Arc<DelayUiState>,
    pub log: Arc<Mutex<Vec<Gesture>>>,
}

/// Mounts the real editor with every context `AppShell` consumes:
///
/// - `ParamContext` over the [`RecordingGuiContext`] stub (what
///   `use_param_context()` / all `ctx.begin_set_raw(..)` call sites hit),
/// - `DelayUi`, the params + meters the editor consumes.
pub fn mount() -> Fixture {
    mount_sized(delay_ui::control_view::EDITOR_W, delay_ui::control_view::EDITOR_H)
}

/// Mount at an explicit window size — used to check the surface against the
/// editor size the plugin shell actually asks the host for.
pub fn mount_sized(width: u32, height: u32) -> Fixture {
    mount_with(Arc::new(DelayParams::default()), width, height)
}

/// Mount against a params tree the caller has already set up — a persisted
/// profile id, a chosen editor form — so a test can open the editor the way a
/// host restoring a session would.
pub fn mount_with(params: Arc<DelayParams>, width: u32, height: u32) -> Fixture {
    let ui_state = Arc::new(DelayUiState::default());
    let log = Arc::new(Mutex::new(Vec::new()));

    let gui = GuiContext::new(Arc::new(RecordingGuiContext { log: log.clone() }));
    let param_ctx = ParamContext::new(gui, Arc::new(AtomicBool::new(true)));

    let tester = render(Harness)
        .with_window_size(width, height)
        // The editor sizes its faceplates and its graph from the window
        // size `nice-plug-dioxus` puts in context on resize. A headless
        // mount has no window, so state the size explicitly — without it
        // every mount looks like the design size and nothing scales.
        .with_root_context(fts_audio_ui::hardware::panel::EditorSize(
            width as f64,
            height as f64,
        ))
        .with_root_context(param_ctx)
        .with_root_context(SharedState::new(Arc::new(DelayUi {
            params: params.clone(),
            state: ui_state.clone(),
        })))
        .build();

    Fixture {
        tester,
        params,
        ui: ui_state,
        log,
    }
}

impl Fixture {
    /// Let the editor re-render *and* re-lay-out after a change: `pump()`
    /// drives the VirtualDom but does not recompute layout, so anything it
    /// creates keeps a 0x0 box until the document is resolved.
    pub async fn settle(&mut self) {
        for _ in 0..10 {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            let _ = self.tester.pump().await;
        }
        self.tester.advance_time(std::time::Duration::from_millis(16)).await;
    }

    /// Click a rail family by its category id.
    pub async fn click_family(&mut self, id: &str) {
        let el = self
            .tester
            .query(by_testid(&format!("rail-item-{id}")))
            .immediately()
            .unwrap_or_else(|e| panic!("rail entry {id} not in DOM: {e:?}"));
        let (ox, oy) = el.document_origin();
        let (w, h) = el.size();
        let (x, y) = (ox + w as f64 / 2.0, oy + h as f64 / 2.0);
        self.tester.pointer_down(x, y);
        let _ = self.tester.pump().await;
        self.tester.pointer_up(x, y);
        self.settle().await;
    }
}
