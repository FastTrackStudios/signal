//! Visual-inspection harness: rasterize the real EQ editor to PNGs.
//!
//! Same idea as `comp-ui`'s: mount `eq_ui::control_view::App` on the headless
//! Blitz DOM and paint it through `DocumentTester::render_png`, so a faceplate
//! can be looked at without opening a DAW. Nothing asserts — a wrong-looking
//! panel is a picture you have to look at:
//!
//! ```sh
//! just eq-shots      # or:
//! cargo test -p eq-ui --features native --test screenshots
//! ```
//!
//! Output lands in `target/gui-shots/eq/` (override with `FTS_SHOTS_DIR`).
//!
//! The Main face is deliberately not shot: the EQ curve is painted by a blitz
//! *custom widget* whose `paint()` never runs headless (that is exactly why
//! these tests need no GPU), so a shot of it would be an empty panel and would
//! tell you nothing.

#![cfg(feature = "native")]

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use dioxus::prelude::*;
use dioxus_test::{by_testid, render, DocumentTester};

use eq_ui::control_view::App;
use eq_ui::params::{EqUiState, FtsEqParams};

use audiocore_core::prelude::Param;
use nice_plug::context::gui::{GuiContext, GuiContextInner};
use nice_plug::prelude::*;
use nice_plug_dioxus::{ParamContext, SharedState};

/// Host stub that applies what the UI writes, so a panel re-renders against
/// the values a click produced.
struct ApplyingGuiContext;

impl GuiContextInner for ApplyingGuiContext {
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Clap
    }
    unsafe fn raw_begin_set_parameter(&self, _param: ParamPtr) {}
    unsafe fn raw_set_parameter_normalized(&self, param: ParamPtr, normalized: f32) {
        unsafe { param._internal_set_normalized_value(normalized) };
    }
    unsafe fn raw_end_set_parameter(&self, _param: ParamPtr) {}
    fn get_state(&self) -> PluginState {
        PluginState {
            version: String::new(),
            params: std::collections::BTreeMap::new(),
            fields: std::collections::BTreeMap::new(),
        }
    }
    fn set_state(&self, _state: PluginState) {}
}

#[component]
fn Harness() -> Element {
    rsx! {
        style { "html, body {{ width:100%; height:100%; margin:0; padding:0; overflow:hidden; }}" }
        style { {nice_plug_dioxus::TAILWIND_CSS} }
        style { {include_str!("../assets/tailwind.css")} }
        App {}
    }
}

struct Fixture {
    tester: DocumentTester,
    params: Arc<FtsEqParams>,
}

fn mount_sized(width: u32, height: u32) -> Fixture {
    mount_with(Arc::new(FtsEqParams::default()), width, height)
}

fn mount_with(params: Arc<FtsEqParams>, width: u32, height: u32) -> Fixture {
    let ui_state = Arc::new(EqUiState::new(params.clone()));
    let gui = GuiContext::new(Arc::new(ApplyingGuiContext));
    let param_ctx = ParamContext::new(gui, Arc::new(AtomicBool::new(true)));
    let track: Arc<dyn eq_ui::cheatsheet::TrackInfoProvider> =
        Arc::new(eq_ui::cheatsheet::StaticTrackProvider::none());

    let tester = render(Harness)
        .with_window_size(width, height)
        // The panels size themselves from the window `nice-plug-dioxus` puts
        // in context on resize; a headless mount has no window, so state it.
        .with_root_context(fts_audio_ui::hardware::panel::EditorSize(
            width as f64,
            height as f64,
        ))
        .with_root_context(param_ctx)
        .with_root_context(SharedState::new(ui_state))
        .with_root_context(track)
        .build();

    Fixture { tester, params }
}

impl Fixture {
    async fn settle(&mut self) {
        for _ in 0..10 {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            let _ = self.tester.pump().await;
        }
        self.tester
            .advance_time(std::time::Duration::from_millis(16))
            .await;
    }
}

fn shots_dir() -> PathBuf {
    let dir = std::env::var("FTS_SHOTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../target/gui-shots/eq")
        });
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("create {}: {e}", dir.display()));
    dir
}

fn shot(fx: &Fixture, name: &str) {
    let path = shots_dir().join(format!("{name}.png"));
    fx.tester.render_png(&path);
    println!("shot: {}", path.display());
}

/// Click the centre of whatever carries `testid`.
async fn click_testid(fx: &mut Fixture, testid: &str) {
    let el = fx
        .tester
        .query(by_testid(testid))
        .immediately()
        .unwrap_or_else(|e| panic!("{testid} missing: {e:?}"));
    let (ox, oy) = el.document_origin();
    let (w, h) = el.size();
    let (x, y) = (ox + w as f64 / 2.0, oy + h as f64 / 2.0);
    fx.tester.pointer_down(x, y);
    let _ = fx.tester.pump().await;
    fx.tester.pointer_up(x, y);
    fx.settle().await;
}

/// Mount at the size the model asks the host for, then select it in the rail —
/// cycling the family button as many times as it takes.
async fn mount_model(model: i32) -> Fixture {
    let (w, h) = eq_ui::faces::preferred_editor_size(model);
    let params = Arc::new(FtsEqParams::default());
    // Set the model *before* mounting. Reaching it by clicking the rail would
    // also work, but a shot should show the panel as a host opens it, not as
    // the tail of an interaction.
    unsafe {
        params
            .model
            .as_ptr()
            ._internal_set_normalized_value(model as f32 / 5.0)
    };
    let mut fx = mount_with(params, w, h);
    fx.settle().await;
    fx
}

/// Clicking the rail from the Main face to a hardware model — the path a user
/// takes, as opposed to a host opening the editor already on a model.
#[tokio::test]
async fn switching_models_by_rail_click_does_not_panic() {
    // Every family, from a fresh mount at the size a host opens the editor
    // at — including the two-model family, which needs a second click.
    for (rail, clicks, model, marker) in [
        ("pultec", 1, 1, "hw-knob-low-boost"),
        ("ssl", 1, 4, "hw-knob-lf-gain"),
        ("ssl", 2, 5, "hw-knob-lf-gain"),
        ("api", 1, 3, "hw-knob-mid-gain"),
        ("neve", 1, 2, "hw-knob-mid-freq"),
    ] {
        let mut fx = mount_sized(1000, 600);
        for _ in 0..clicks {
            click_testid(&mut fx, &format!("rail-item-{rail}")).await;
        }
        assert_eq!(fx.params.model.value(), model, "rail {rail} x{clicks}");
        fx.tester
            .query(by_testid(marker))
            .immediately()
            .unwrap_or_else(|e| panic!("{rail} panel missing {marker}: {e:?}"));
    }
}

/// Every size preset on the Pultec — whether a form is usable is not something
/// a size in a table can tell you.
#[tokio::test]
async fn shot_every_editor_form() {
    for form in fts_audio_ui::EDITOR_FORMS {
        let (w, h) = eq_ui::faces::editor_size_for(1, *form);
        let params = Arc::new(FtsEqParams::default());
        unsafe { params.model.as_ptr()._internal_set_normalized_value(1.0 / 5.0) };
        eq_ui::faces::store_model_id(&params, 1);
        eq_ui::faces::store_form(&params, *form);
        let mut fx = mount_with(params, w, h);
        fx.settle().await;
        shot(&fx, &format!("form-{}", form.id().replace('_', "-")));
    }
}

#[tokio::test]
async fn shot_every_hardware_model() {
    for (model, name) in [
        (1, "pultec"),
        (4, "ssl-e"),
        (5, "ssl-g"),
        (3, "api-550a"),
        (2, "neve-1073"),
    ] {
        let fx = mount_model(model).await;
        shot(&fx, name);
    }
}
