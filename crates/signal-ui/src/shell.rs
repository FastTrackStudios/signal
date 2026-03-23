//! Embeddable Signal shell — the top-level UI entry point.
//!
//! [`SignalRoot`] provides the Signal controller as Dioxus context, then
//! renders the mode-selector nav bar and dock-based panel layout.  Any app
//! that has a [`signal::Signal`] controller can drop this component in to
//! get the full Signal UI without duplicating bootstrap logic.

use std::rc::Rc;

use dioxus::prelude::*;

use dock_dioxus::{DockProvider, DockRoot, PanelRenderer, PanelRendererRegistry};
use signal::Signal;

use crate::register_panels;

/// Signal view mode — determines which top-level browser/editor is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Preset,
    Profile,
    Song,
}

/// Top-level Signal UI component.
///
/// Provides the given [`Signal`] controller as Dioxus context (so
/// `use_signal_service()` works in all child components), then renders
/// the mode selector bar and dock layout.
///
/// # Usage
///
/// ```rust,ignore
/// // In the parent app's component:
/// rsx! { SignalRoot { controller: my_signal_controller } }
/// ```
#[component]
pub fn SignalRoot(controller: Signal) -> Element {
    provide_context(controller);

    rsx! { SignalShell {} }
}

/// Main shell: mode selector + dock-based panel layout.
#[component]
fn SignalShell() -> Element {
    let mut mode = use_signal(|| Mode::Preset);

    let render_panel = use_hook(|| {
        let mut registry = PanelRendererRegistry::new();
        register_panels(&mut registry);
        let registry = Rc::new(registry);
        PanelRenderer::new(move |panel_id| registry.render(panel_id))
    });

    rsx! {
        div { class: "flex flex-col h-full bg-background text-foreground",
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
