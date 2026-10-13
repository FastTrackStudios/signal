//! Deterministic test-panel REAPER extension for visual e2e tests.
//!
//! Loaded into REAPER alongside `daw-bridge` and `daw-reaper-dioxus-ext`.
//! Mounts a single dock panel under id `FTS_TEST_PANEL` showing a click
//! counter rendered with a fixed style so pixel snapshots are stable
//! across machines (modulo GPU vendor — see daw-vja for the test).
//!
//! NOT for production. Only used by `crates/daw-reaper/tests/reaper_visual_e2e.rs`.

use std::error::Error;

use daw_module::{DockPosition, PanelComponent, PanelDef, PanelRenderer};
use daw_reaper_dioxus::prelude::*;
use reaper_high::Reaper as HighReaper;
use reaper_low::PluginContext;
use reaper_macros::reaper_extension_plugin;
use tracing::info;

pub const TEST_PANEL_ID: &str = "FTS_TEST_PANEL";
pub const TEST_PANEL_WIDTH: u32 = 320;
pub const TEST_PANEL_HEIGHT: u32 = 200;

#[reaper_extension_plugin]
fn plugin_main(_context: PluginContext) -> Result<(), Box<dyn Error>> {
    info!("daw-reaper-dioxus-ext-test starting");

    if let Err(e) = HighReaper::load(_context).setup() {
        // Already loaded by another FTS extension — fine.
        let _ = e;
    }

    // Init the dioxus service singleton + dock module.
    daw_reaper_dioxus::service::init();
    daw_reaper_dioxus::dock::init(reaper_low::Reaper::get(), reaper_low::Swell::get());

    // Register the test panel.
    daw_reaper_dioxus::dock::register_panel_from_service(&PanelDef {
        id: TEST_PANEL_ID,
        title: "Test Panel",
        component: PanelComponent::from_fn_ptr(test_panel as *const ()),
        default_dock: DockPosition::Floating,
        renderer: PanelRenderer::Native,
        default_size: (TEST_PANEL_WIDTH as f64, TEST_PANEL_HEIGHT as f64),
        toggle_action: None,
    });

    info!("daw-reaper-dioxus-ext-test: registered FTS_TEST_PANEL");
    Ok(())
}

/// Deterministic click-counter panel. Fixed colors + a default system
/// font, so pixel output is stable across runs on the same GPU/driver.
///
/// Layout: a single button centered in the panel showing
/// `Count: <n>` text. Click increments the signal.
fn test_panel() -> Element {
    let mut count = use_signal(|| 0u32);
    rsx! {
        div {
            style: "width: 100%; height: 100%; background: #101418; \
                    color: #f0f0f0; display: flex; align-items: center; \
                    justify-content: center; font-family: sans-serif;",
            button {
                style: "padding: 12px 24px; background: #2a6df5; color: white; \
                        border: 0; border-radius: 6px; font-size: 18px;",
                onclick: move |_| { count += 1; },
                "Count: {count}"
            }
        }
    }
}
