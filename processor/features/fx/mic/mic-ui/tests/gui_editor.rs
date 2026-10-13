//! The FTS Mic editor mounts headless (dioxus-test + Blitz) with every
//! section and control laid out, at the size they open at and at their
//! declared minimum — blitz collapses a container that does not fit to 0×0,
//! so a minimum that is too small shows up here as a missing control.
//!
//! ```sh
//! cargo test -p mic-ui --test gui_editor
//! ```

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use dioxus::prelude::*;
use dioxus_test::{DocumentTester, by_testid, render};
use mic_ui::params::{MicParams, MicUiState};
use mic_ui::view::{App, EDITOR_H, EDITOR_W, MIN_EDITOR_H, MIN_EDITOR_W};
use nice_plug::context::gui::{GuiContext, GuiContextInner};
use nice_plug::prelude::*;
use nice_plug_dioxus::{ParamContext, SharedState};

/// A host that applies whatever the editor sets.
struct Host;

impl GuiContextInner for Host {
    fn request_restart(&self) {}
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Clap
    }
    unsafe fn raw_begin_set_parameter(&self, _param: ParamPtr) {}
    unsafe fn raw_set_parameter_normalized(&self, param: ParamPtr, normalized: f32) {
        unsafe { param._internal_set_normalized_value(normalized) };
    }
    unsafe fn raw_end_set_parameter(&self, _param: ParamPtr) {}
    fn get_state(&self) -> PluginState {
        PluginState { version: String::new(), params: BTreeMap::new(), fields: BTreeMap::new() }
    }
    fn set_state(&self, _state: PluginState) {}
}

#[component]
fn MicHarness() -> Element {
    rsx! {
        style { "html, body {{ width:100%; height:100%; margin:0; padding:0; overflow:hidden; }}" }
        style { {nice_plug_dioxus::TAILWIND_CSS} }
        style { {include_str!("../assets/tailwind.css")} }
        App {}
    }
}


fn context() -> ParamContext {
    ParamContext::new(GuiContext::new(Arc::new(Host)), Arc::new(AtomicBool::new(true)))
}

fn mount(w: u32, h: u32) -> DocumentTester {
    let ui = Arc::new(MicUiState::new(Arc::new(MicParams::default())));
    render(MicHarness).with_window_size(w, h).with_root_context(context()).with_root_context(SharedState::new(ui)).build()
}


fn laid_out(t: &DocumentTester, ids: &[&str]) -> Vec<String> {
    let mut missing = Vec::new();
    for id in ids {
        match t.query(by_testid(id)).immediately() {
            Ok(el) => {
                let (w, h) = el.size();
                if w < 8.0 || h < 8.0 {
                    missing.push(format!("{id} collapsed to {w}x{h}"));
                }
            }
            Err(_) => missing.push(format!("{id} not in the DOM")),
        }
    }
    missing
}

const MIC: [&str; 20] = [
    "section-mic-1", "section-dual", "section-180", "section-mic-2", "section-proximity", "section-capsules", "section-output",
    "select-type1", "select-pattern1", "knob-axis1", "select-type2", "knob-mix", "knob-align",
    "knob-proximity", "knob-reartrim", "knob-output", "toggle-phase", "toggle-stereo180", "knob-pan", "knob-width",
];


fn sizes() -> [(u32, u32); 2] {
    // the minimum is declared in logical f32 pixels; whole pixels here
    let px = |v: f32| format!("{v:.0}").parse().unwrap_or(0);
    [(EDITOR_W, EDITOR_H), (px(MIN_EDITOR_W), px(MIN_EDITOR_H))]
}

#[tokio::test]
async fn fts_mic_editor_lays_out_every_control() {
    for (w, h) in sizes() {
        let t = mount(w, h);
        let html = t.query(":root").immediately().map(|e| e.inner_html()).unwrap_or_default();
        assert!(html.contains("FTS Mic"), "title missing at {w}x{h}");
        let bad = laid_out(&t, &MIC);
        assert!(bad.is_empty(), "at {w}x{h}:\n  {}", bad.join("\n  "));
    }
}

