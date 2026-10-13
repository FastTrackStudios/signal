//! Standalone dock panel wrappers for DAW components.
//!
//! Each function is a thin wrapper that initializes the FX browser hook
//! (if needed) and renders the underlying component.

use crate::prelude::*;

/// First-pass DAW application shell.
///
/// This composes the existing track, arrangement, mixer, and FX
/// parameter panels into one dense workspace. It intentionally stays
/// panel-first: no landing page, no marketing copy, just the surfaces
/// needed to operate a DAW session once a `daw_control::Daw`
/// connection has been installed.
#[component]
pub fn DawApplication() -> Element {
    crate::hooks::fx_browser::use_fx_browser_subscription();

    rsx! {
        div { class: "h-screen w-screen min-h-0 min-w-0 bg-zinc-950 text-zinc-100 overflow-hidden flex flex-col",
            div { class: "h-9 flex-shrink-0 border-b border-zinc-800 bg-zinc-950 flex items-center px-3 gap-3",
                div { class: "text-xs font-semibold text-zinc-200", "FastTrack Studio" }
                div { class: "h-4 w-px bg-zinc-800" }
                div { class: "text-[11px] text-zinc-500", "Standalone DAW" }
                div { class: "flex-1" }
                div { class: "text-[10px] text-zinc-500", "Tracks · FX · Mixer" }
            }

            div { class: "flex-1 min-h-0 min-w-0 grid grid-cols-[280px_minmax(360px,1fr)_320px] grid-rows-[minmax(280px,1fr)_220px] bg-zinc-950",
                div { class: "min-h-0 min-w-0 border-r border-zinc-800 row-span-2",
                    crate::components::track_control_panel::TrackControlPanel {}
                }

                div { class: "min-h-0 min-w-0 border-r border-zinc-800 border-b border-zinc-800",
                    crate::components::arrangement_view::ArrangementView {}
                }

                div { class: "min-h-0 min-w-0 border-b border-zinc-800",
                    crate::components::fx_parameter_browser::FxParameterBrowser {}
                }

                div { class: "min-h-0 min-w-0 col-span-2",
                    crate::components::mixer::MixerPanel {}
                }
            }
        }
    }
}

/// FX parameter browser dock panel — "no-GUI DAW" style parameter list.
///
/// Shows real DAW FX parameters with live bidirectional updates.
/// Reads directly from the DAW connection via global signals.
#[component]
pub fn FxBrowserDockPanel() -> Element {
    crate::hooks::fx_browser::use_fx_browser_subscription();

    rsx! {
        crate::components::fx_parameter_browser::FxParameterBrowser {}
    }
}
