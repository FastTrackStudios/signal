//! Behavioral tests for the REAL FTS Limiter editor UI.
//!
//! Mounts `limiter_ui::control_view::App` — the exact Dioxus surface the
//! CLAP/VST3 plugin embeds — on the vendored dioxus-test harness (headless
//! Blitz DOM, no GPU, no window) and drives it with real hit-tested pointer
//! events.
//!
//! ```sh
//! cargo test -p limiter-ui --test gui_editor
//! ```
//!
//! ## Harness notes
//!
//! - `App` injects its CSS through `document::Style` head elements, which the
//!   headless harness drops (it falls back to `NoOpDocument`). The
//!   [`support::Harness`] wrapper re-injects the same stylesheets as ordinary
//!   body `<style>` elements — blitz-dom processes `<style>` anywhere in the
//!   tree. Without them every layout utility is undefined and the layout
//!   collapses.
//! - `pump()` drives the VirtualDom but does **not** recompute layout, so any
//!   assertion on size or hit-testing after a re-render needs
//!   [`support::Fixture::settle`], which calls `advance_time`.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use dioxus_test::{by_testid, render, DocumentTester};

use limiter_ui::control_view::App;
use limiter_ui::gr_trace_svg::{GR_RANGE_DB, TRACE_H, TRACE_W, gr_area_path, gr_to_y};
use limiter_ui::params::{LimiterParams, LimiterUiState};

use audiocore_core::prelude::Param;
use nice_plug_dioxus::{ParamContext, SharedState};

mod support {
    use super::*;
    use dioxus::prelude::*;
    use nice_plug::context::gui::{GuiContext, GuiContextInner};
    use nice_plug::prelude::*;
    use std::collections::BTreeMap;

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub enum Gesture {
        Begin(usize),
        Set(usize, f32),
        End(usize),
    }

    /// Stable identity for a parameter across threads (raw `ParamPtr` is not
    /// `Send`, so the log stores the pointer address).
    pub fn ptr_key(p: ParamPtr) -> usize {
        match p {
            ParamPtr::FloatParam(p) => p as usize,
            ParamPtr::IntParam(p) => p as usize,
            ParamPtr::BoolParam(p) => p as usize,
            ParamPtr::EnumParam(p) => p as usize,
        }
    }

    /// Host stub: records every gesture AND applies it, so the UI re-renders
    /// against new values and multi-step drags accumulate.
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
        pub params: Arc<LimiterParams>,
        pub ui: Arc<LimiterUiState>,
        pub log: Arc<Mutex<Vec<Gesture>>>,
    }

    pub fn mount() -> Fixture {
        mount_sized(
            limiter_ui::control_view::EDITOR_W,
            limiter_ui::control_view::EDITOR_H,
        )
    }

    pub fn mount_sized(width: u32, height: u32) -> Fixture {
        let params = Arc::new(LimiterParams::default());
        let ui = Arc::new(LimiterUiState::new(params.clone()));
        let log = Arc::new(Mutex::new(Vec::new()));

        let gui = GuiContext::new(Arc::new(RecordingGuiContext { log: log.clone() }));
        let param_ctx = ParamContext::new(gui, Arc::new(AtomicBool::new(true)));

        let tester = render(Harness)
            .with_window_size(width, height)
            .with_root_context(param_ctx)
            .with_root_context(SharedState::new(ui.clone()))
            .build();

        Fixture { tester, params, ui, log }
    }

    impl Fixture {
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

        /// Re-render *and* re-lay-out. `AppShell` reads the meter atomics
        /// directly rather than through signals, so it only picks up an
        /// audio-thread write on its ~30 Hz tick; and `pump()` alone leaves
        /// newly created nodes with a 0x0 box.
        pub async fn settle(&mut self) {
            for _ in 0..10 {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                let _ = self.tester.pump().await;
            }
            self.tester
                .advance_time(std::time::Duration::from_millis(16))
                .await;
        }
    }
}

use support::{mount, mount_sized, ptr_key, Gesture};

/// The editor mounts headless with every section, knob and meter laid out.
#[tokio::test]
async fn editor_mounts_headless_with_controls_and_meters() -> dioxus_test::Result<()> {
    let fx = mount();

    let html = fx.tester.query(":root").immediately()?.inner_html();
    assert!(html.contains("FTS Limiter"), "header title missing");
    for label in ["Input", "Release", "Ceiling", "Character", "True Peak"] {
        assert!(html.contains(label), "control label {label:?} missing");
    }
    for label in ["GR", "IN", "OUT"] {
        assert!(html.contains(label), "meter label {label:?} missing");
    }

    for id in ["section-gain", "section-release", "section-ceiling"] {
        let (w, h) = fx.tester.query(by_testid(id)).immediately()?.size();
        assert!(w > 40.0 && h > 30.0, "{id} collapsed to {w}x{h}px");
    }
    for id in ["knob-ingain", "knob-release", "knob-ceiling", "knob-character"] {
        let (w, h) = fx.tester.query(by_testid(id)).immediately()?.size();
        assert!(w > 20.0 && h > 20.0, "{id} collapsed to {w}x{h}px");
    }
    fx.tester.query(by_testid("toggle-truepeak")).immediately()?;
    Ok(())
}

/// The GR trace container gets its pinned height, so the fixed viewBox maps 1:1
/// onto the element.
#[tokio::test]
async fn gr_trace_renders_at_its_pinned_height() -> dioxus_test::Result<()> {
    let fx = mount();

    let el = fx.tester.query(by_testid("limiter-trace")).immediately()?;
    let (w, h) = el.size();
    assert!(w > 300.0, "trace too narrow: {w}px");
    assert!(
        (h as f64 - TRACE_H).abs() < 1.0,
        "trace height {h}px != pinned {TRACE_H}px"
    );
    Ok(())
}

/// Gain reduction pushed from the audio thread reaches the DOM: the trace draws
/// a GR area, and it moves when the reduction deepens.
#[tokio::test]
async fn gain_reduction_from_the_audio_thread_reaches_the_trace() -> dioxus_test::Result<()> {
    let mut fx = mount();

    // Silence: no reduction, so the area hugs the top edge.
    for _ in 0..limiter_ui::params::LimiterUiState::WAVE_LEN_HINT {
        fx.ui.gr_wave.push(0.0);
    }
    fx.settle().await;
    let quiet = fx
        .tester
        .query(by_testid("limiter-trace"))
        .immediately()?
        .inner_html();

    // Now push a real reduction through the same path the plugin uses.
    for _ in 0..limiter_ui::params::LimiterUiState::WAVE_LEN_HINT {
        fx.ui.gr_wave.push(12.0);
    }
    fx.settle().await;
    let limiting = fx
        .tester
        .query(by_testid("limiter-trace"))
        .immediately()?
        .inner_html();

    assert_ne!(
        quiet, limiting,
        "the GR trace did not change when the audio thread reported reduction"
    );
    // 12 dB of the 24 dB range is halfway down the trace.
    let expected_y = gr_to_y(12.0, TRACE_H);
    assert!(
        limiting.contains(&format!("{expected_y:.2}")),
        "trace does not plot 12 dB at y={expected_y:.2}"
    );
    Ok(())
}

/// Dragging the Ceiling knob drives its parameter through real host gestures.
#[tokio::test]
async fn dragging_ceiling_lowers_the_parameter() -> dioxus_test::Result<()> {
    let fx = mount();
    let cp = &fx.params.ceiling;
    let key = ptr_key(cp.as_ptr());
    let before = cp.value();

    let (sx, sy) = fx.knob_center("knob-ceiling");
    fx.tester.pointer_down(sx, sy);
    let _ = fx.tester.pump().await;
    for step in 1..=3 {
        fx.tester.pointer_move(sx, sy + 10.0 * step as f64, true);
        let _ = fx.tester.pump().await;
    }
    fx.tester.pointer_up(sx, sy + 30.0);
    let _ = fx.tester.pump().await;

    let after = cp.value();
    assert!(after < before, "drag down did not lower ceiling: {before} -> {after}");
    // 30 px at 150 px per sweep over the linear -20..0 dB range = -4 dB.
    let expected = before - (30.0 / 150.0) * 20.0;
    assert!(
        (after - expected).abs() < 0.5,
        "ceiling landed at {after} dB, expected ~{expected} dB"
    );

    let log = fx.log.lock().unwrap();
    assert!(log.iter().any(|g| matches!(g, Gesture::Begin(k) if *k == key)));
    assert!(log.iter().any(|g| matches!(g, Gesture::End(k) if *k == key)));
    Ok(())
}

/// The True Peak switch is a real control, not decoration.
#[tokio::test]
async fn true_peak_toggle_flips_its_parameter() -> dioxus_test::Result<()> {
    let fx = mount();
    let before = fx.params.true_peak.value();

    let el = fx.tester.query(by_testid("toggle-truepeak")).immediately()?;
    let (ox, oy) = el.document_origin();
    let (w, h) = el.size();
    fx.tester.pointer_down(ox + w as f64 / 2.0, oy + h as f64 / 2.0);
    let _ = fx.tester.pump().await;
    fx.tester.pointer_up(ox + w as f64 / 2.0, oy + h as f64 / 2.0);
    let _ = fx.tester.pump().await;

    assert_ne!(fx.params.true_peak.value(), before, "True Peak did not toggle");
    Ok(())
}

/// The surface must still lay out at the smallest size the editor tells hosts
/// it supports. Blitz collapses what does not fit to 0x0 rather than clipping
/// it, so a minimum that is too small yields unreachable controls.
#[tokio::test]
async fn surface_survives_the_declared_minimum_size() -> dioxus_test::Result<()> {
    let fx = mount_sized(
        limiter_ui::control_view::MIN_EDITOR_W as u32,
        limiter_ui::control_view::MIN_EDITOR_H as u32,
    );

    for id in ["section-gain", "section-release", "section-ceiling"] {
        let (w, h) = fx
            .tester
            .query(by_testid(id))
            .immediately()
            .unwrap_or_else(|e| panic!("{id} missing at the declared minimum: {e:?}"))
            .size();
        assert!(
            w > 40.0 && h > 30.0,
            "{id} collapsed to {w}x{h}px at the declared minimum {}x{}",
            limiter_ui::control_view::MIN_EDITOR_W,
            limiter_ui::control_view::MIN_EDITOR_H,
        );
    }
    for id in ["knob-ingain", "knob-release", "knob-ceiling", "knob-character"] {
        let (w, h) = fx.tester.query(by_testid(id)).immediately()?.size();
        assert!(w > 20.0 && h > 20.0, "{id} collapsed to {w}x{h}px at the minimum");
    }
    Ok(())
}

/// Guards the geometry the component relies on: the trace spans its full width
/// and the GR area is closed.
#[test]
fn gr_area_spans_the_trace() {
    let d = gr_area_path(&[0.0, 6.0, 24.0], TRACE_W, TRACE_H);
    assert!(d.starts_with("M 0 0"));
    assert!(d.ends_with(" Z"));
    assert!(d.contains(&format!("{TRACE_W:.2}")));
    assert_eq!(gr_to_y(GR_RANGE_DB, TRACE_H), TRACE_H);
}
