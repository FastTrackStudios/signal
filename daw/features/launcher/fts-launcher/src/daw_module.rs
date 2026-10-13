//! DawModule implementation for fts-launcher.

use architect_launcher_ui::components::Launcher;
use architect_ui::prelude::{ThemeMode, ThemeProvider, ThemeState, default_theme_preset};
use daw::module::{
    ActionDef, DawModule, DockPosition, ModuleContext, PanelComponent, PanelDef, PanelRenderer,
};
use daw::reaper_ui::prelude::*;

use crate::LauncherEngine;

const LAUNCHER_PANEL_ID: &str = "FTS_LAUNCHER";
const LAUNCHER_CSS: &str = include_str!("../assets/tailwind.css");

pub struct LauncherModule;

impl DawModule for LauncherModule {
    fn name(&self) -> &str {
        "launcher"
    }
    fn display_name(&self) -> &str {
        "FTS Launcher"
    }

    fn actions(&self) -> Vec<ActionDef> {
        vec![
            ActionDef::new("FTS_LAUNCHER_TOGGLE", "FTS: Toggle Launcher", || {
                daw::reaper_ui::dock::toggle_panel(LAUNCHER_PANEL_ID);
            })
            .in_menu(),
            ActionDef::new("FTS_LAUNCHER_FOCUS", "FTS: Focus Launcher Search", || {
                daw::reaper_ui::dock::show_panel(LAUNCHER_PANEL_ID);
            }),
        ]
    }

    fn panels(&self) -> Vec<PanelDef> {
        vec![PanelDef {
            id: LAUNCHER_PANEL_ID,
            title: "FTS Launcher",
            component: PanelComponent::from_fn_ptr(LauncherPanel as fn() -> _ as *const ()),
            default_dock: DockPosition::Floating,
            renderer: PanelRenderer::Native,
            default_size: (800.0, 520.0),
            toggle_action: Some("FTS_LAUNCHER_TOGGLE"),
        }]
    }

    fn init(&self, _ctx: &ModuleContext) {
        tracing::info!("FTS Launcher module initialized");
    }
}

#[component]
pub fn LauncherPanel() -> Element {
    let state = use_signal(|| {
        let start = std::time::Instant::now();
        tracing::info!("Creating launcher engine");
        let state = LauncherEngine::new().into_state();
        tracing::info!(
            elapsed_ms = start.elapsed().as_millis(),
            "Launcher engine created"
        );
        state
    });
    let theme_state = use_signal(|| ThemeState::new(default_theme_preset(), ThemeMode::Dark));
    let on_close = move |_: ()| {
        daw::reaper_ui::dock::hide_panel(LAUNCHER_PANEL_ID);
    };

    rsx! {
        document::Style { {LAUNCHER_CSS} }
        ThemeProvider { state: theme_state,
            Launcher {
                state,
                on_close,
            }
        }
    }
}

pub fn module() -> Box<dyn DawModule> {
    Box::new(LauncherModule)
}
