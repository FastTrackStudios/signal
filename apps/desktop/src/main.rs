//! Signal Desktop — standalone Dioxus desktop app for signal chain management.
//!
//! Provides the same signal UI as fts-control but without session, DAW discovery,
//! or input-actions dependencies. Useful for quick iteration on signal domain UI.

use std::rc::Rc;

use dioxus::desktop::{tao::window::WindowBuilder, Config};
use dioxus::prelude::*;

use dock_dioxus::{DockProvider, DockRoot, PanelRenderer, PanelRendererRegistry};
use signal::connect_db_seeded;
use signal_ui::register_panels;

/// Signal view mode — determines which top-level browser/editor is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Preset,
    Profile,
    Song,
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!("Starting Signal Desktop");

    let cfg = Config::new().with_window(
        WindowBuilder::new()
            .with_title("Signal")
            .with_inner_size(dioxus::desktop::tao::dpi::LogicalSize::new(1400.0, 900.0)),
    );

    LaunchBuilder::desktop().with_cfg(cfg).launch(App);
}

/// Root component — initializes the Signal controller and provides it as context.
#[component]
fn App() -> Element {
    // Database path: use $SIGNAL_DB or default to ~/.local/share/signal/signal.db
    let db_path = std::env::var("SIGNAL_DB").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        let dir = std::path::PathBuf::from(home).join(".local/share/signal");
        std::fs::create_dir_all(&dir).ok();
        dir.join("signal.db").to_string_lossy().into_owned()
    });

    let controller = use_resource(move || {
        let path = db_path.clone();
        async move { connect_db_seeded(&path).await }
    });

    let read = controller.read();
    match &*read {
        Some(Ok(signal)) => {
            // Provide the Signal controller so use_signal_service() works everywhere
            provide_context(signal.clone());

            rsx! { SignalShell {} }
        }
        Some(Err(e)) => {
            let msg = format!("Failed to initialize Signal: {e}");
            rsx! {
                div { class: "flex items-center justify-center h-screen bg-zinc-950 text-red-400",
                    "{msg}"
                }
            }
        }
        None => rsx! {
            div { class: "flex items-center justify-center h-screen bg-zinc-950 text-zinc-500",
                "Loading…"
            }
        },
    }
}

/// Main shell: mode selector + dock-based panel layout.
#[component]
fn SignalShell() -> Element {
    let mut mode = use_signal(|| Mode::Preset);

    // Build PanelRenderer from the signal-ui registry (same pattern as fts-control)
    let render_panel = use_hook(|| {
        let mut registry = PanelRendererRegistry::new();
        register_panels(&mut registry);
        let registry = Rc::new(registry);
        PanelRenderer::new(move |panel_id| registry.render(panel_id))
    });

    rsx! {
        document::Stylesheet { href: asset!("/assets/tailwind.css") }

        div { class: "flex flex-col h-screen bg-background text-foreground",
            // Mode selector bar
            nav { class: "flex items-center gap-1 px-4 py-2 border-b border-zinc-800 bg-zinc-900",
                span { class: "text-sm font-semibold text-zinc-400 mr-3", "Signal" }
                ModeButton { label: "Preset", active: mode() == Mode::Preset, onclick: move |_| mode.set(Mode::Preset) }
                ModeButton { label: "Profile", active: mode() == Mode::Profile, onclick: move |_| mode.set(Mode::Profile) }
                ModeButton { label: "Song", active: mode() == Mode::Song, onclick: move |_| mode.set(Mode::Song) }
            }
            // Content area — delegate to the dock system
            div { class: "flex-1 overflow-hidden",
                DockProvider { render_panel: render_panel.clone(),
                    DockRoot {}
                }
            }
        }
    }
}

/// A mode selector button.
#[component]
fn ModeButton(label: &'static str, active: bool, onclick: EventHandler<MouseEvent>) -> Element {
    let cls = if active {
        "px-3 py-1 text-sm rounded bg-zinc-700 text-zinc-100"
    } else {
        "px-3 py-1 text-sm rounded text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800"
    };
    rsx! {
        button { class: "{cls}", onclick: move |e| onclick.call(e), "{label}" }
    }
}
