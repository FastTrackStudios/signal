//! Comp editor — Dioxus GUI root component.
//!
//! The compressor graph ([`crate::comp_graph::CompGraph`] — transfer curve,
//! threshold/knee drag, rolling waveform + GR traces) on top, with the
//! classic comp control surface below: a knob row (threshold / ratio /
//! attack / release / knee / makeup / mix / stereo link) beside live
//! gain-reduction and I/O level meters. Reusable widgets (knobs, meters,
//! drag provider) come from [`fts_ui_audio`]; theme + layout primitives
//! from [`fts_ui`].

use std::sync::atomic::Ordering;

use audiocore_core::prelude::*;
use fts_ui::prelude::{ThemeMode, ThemeProvider, ThemeState, default_theme_preset};
use fts_ui_audio::prelude::*;

use crate::param_adapter::param_handle;
use crate::params::CompUiState;

/// Root editor component.
///
/// Wraps the comp shell in `fts_ui::ThemeProvider` so themed widgets pick up
/// the active preset. The plugin embedded path and any standalone path both
/// go through here.
#[component]
pub fn App() -> Element {
    let theme_state = use_signal(|| ThemeState::new(default_theme_preset(), ThemeMode::Dark));
    rsx! {
        document::Style { {nice_plug_dioxus::TAILWIND_CSS} }
        // comp-ui's own compiled utilities + theme tokens — the framework CSS
        // above only covers nice-plug-dioxus's widgets, so without this every
        // layout-critical class (flex-1, min-h-0, …) is undefined and the
        // layout collapses in DAW hosts. `just tailwind-comp`.
        document::Style { {include_str!("../assets/tailwind.css")} }
        ThemeProvider { state: theme_state, AppShell {} }
    }
}

/// Inner shell component — runs after the ThemeProvider context is in scope
/// so themed primitives can resolve their tokens.
#[component]
fn AppShell() -> Element {
    let _theme = use_init_theme();

    let shared = use_context::<SharedState>();
    let ui = shared.get::<CompUiState>().expect("CompUiState missing");
    let ctx = use_param_context();
    let params = &ui.params;

    // Redraw tick. AppShell owns the read-side of every Param atomic and the
    // meter atomics, so it must re-render for fresh values to reach the DOM.
    // Spawn the OS thread exactly once via use_hook and have it call
    // schedule_update on this scope — same driver as eq-ui's control view.
    let mut app_tick: Signal<u64> = use_signal(|| 0);
    use_hook(|| {
        let updater = dioxus_core::schedule_update();
        std::thread::spawn(move || {
            loop {
                // ~30 Hz — plenty for meter ballistics; keeps the headless
                // event loop unclogged.
                std::thread::sleep(std::time::Duration::from_millis(33));
                updater();
            }
        });
    });
    app_tick += 1;
    // Frame counter sneaks into a `data-frame` attribute on the root — the
    // DOM mutation forces blitz to consider the document dirty every render,
    // which forces a window.request_redraw. Without it idle redraws collapse
    // and the meters freeze.
    let frame_counter = *app_tick.read();

    let gr_db = ui.gain_reduction_db.load(Ordering::Relaxed);
    let input_db = ui.input_peak_db.load(Ordering::Relaxed);
    let output_db = ui.output_peak_db.load(Ordering::Relaxed);

    let base_css = "*, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; } \
         html, body { width: 100%; height: 100%; overflow: hidden; \
         background: var(--background); color: var(--foreground); \
         font-family: var(--font-sans, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, sans-serif); \
         font-size: 13px; }";
    let root_style = "width:100vw; height:100vh; \
         display:flex; flex-direction:column; \
         color:var(--foreground); \
         background:var(--background); \
         font-family:var(--font-sans, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, sans-serif); \
         font-size:13px; \
         user-select:none; position:relative;";

    rsx! {
        document::Style { {base_css} }

        DragProvider {
            div {
                style: format!("{root_style} overflow:hidden;"),
                "data-frame": "{frame_counter}",

                // ── Header ───────────────────────────────────────────
                div {
                    class: "flex justify-between items-center px-4 py-3 border-b border-border bg-card/50",
                    div { class: "flex items-baseline gap-3 shrink-0",
                        div {
                            class: "text-base font-bold tracking-wide text-foreground",
                            "FTS Comp"
                        }
                        div {
                            class: "text-xs text-muted-foreground uppercase tracking-wider",
                            "Stereo Compressor"
                        }
                    }
                }

                // ── Compressor graph ────────────────────────────────
                // Height pinned to exactly GRAPH_H CSS px so pointer y maps
                // 1:1 onto the graph's fixed viewBox (see comp_graph.rs) —
                // the knob row below takes the remaining space.
                div {
                    "data-testid": "comp-graph",
                    style: format!(
                        "height:{}px; flex:none; position:relative; overflow:hidden;",
                        crate::comp_graph::GRAPH_H
                    ),
                    crate::comp_graph::CompGraph {}
                }

                // ── Control surface ─────────────────────────────────
                div {
                    class: "flex-1 min-h-0 flex items-center justify-center gap-8 px-6 py-4",

                    // Knob grid — the classic comp param set.
                    div {
                        class: "grid grid-cols-4 gap-x-6 gap-y-4 justify-items-center",
                        div { "data-testid": "knob-threshold",
                            Knob { handle: param_handle(params.threshold_db.as_ptr(), ctx.clone()) }
                        }
                        div { "data-testid": "knob-ratio",
                            Knob { handle: param_handle(params.ratio.as_ptr(), ctx.clone()) }
                        }
                        div { "data-testid": "knob-attack",
                            Knob { handle: param_handle(params.attack_ms.as_ptr(), ctx.clone()) }
                        }
                        div { "data-testid": "knob-release",
                            Knob { handle: param_handle(params.release_ms.as_ptr(), ctx.clone()) }
                        }
                        div { "data-testid": "knob-knee",
                            Knob { handle: param_handle(params.knee_db.as_ptr(), ctx.clone()) }
                        }
                        div { "data-testid": "knob-makeup",
                            Knob { handle: param_handle(params.makeup_db.as_ptr(), ctx.clone()) }
                        }
                        div { "data-testid": "knob-mix",
                            Knob { handle: param_handle(params.mix.as_ptr(), ctx.clone()) }
                        }
                        div { "data-testid": "knob-link",
                            Knob { handle: param_handle(params.stereo_link.as_ptr(), ctx.clone()) }
                        }
                    }

                    // Metering — GR (fed by the audio thread through
                    // CompUiState) flanked by I/O peak meters.
                    div {
                        class: "flex items-end gap-3",
                        "data-testid": "meters",
                        LevelMeterDb { level_db: input_db, label: "IN".to_string(), height: 160.0 }
                        GrMeter { gain_reduction_db: gr_db, height: 160.0 }
                        LevelMeterDb { level_db: output_db, label: "OUT".to_string(), height: 160.0 }
                    }
                }
            }
        }
    }
}
