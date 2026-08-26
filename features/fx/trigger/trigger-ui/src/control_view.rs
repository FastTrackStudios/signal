//! Trigger editor — Dioxus GUI root component.
//!
//! The analysis waveform ([`crate::trigger_waveform::TriggerWaveform`] —
//! scrolling input peaks, dB grid, hit markers, draggable threshold line) on
//! top, with the trigger control surface below: knobs for the detection
//! params (threshold / sensitivity / retrigger / dynamics, sidechain
//! HPF/LPF, velocity min/max), segmented selects for the detection algorithm
//! and velocity curve, a MIDI-note stepper, and the Listen toggle. Reusable
//! widgets (knobs, toggle, segmented, drag provider) come from
//! [`fts_audio_ui`]; theme + layout primitives from [`architect_ui`].

use architect_ui::prelude::{default_theme_preset, ThemeMode, ThemeProvider, ThemeState};
use audiocore_core::prelude::*;
use fts_audio_ui::prelude::*;

use crate::param_adapter::param_handle;
use crate::params::TriggerUiState;

/// Root editor component.
///
/// Wraps the trigger shell in `architect_ui::ThemeProvider` so themed widgets pick
/// up the active preset. The plugin embedded path and any standalone path
/// both go through here.
#[component]
pub fn App() -> Element {
    let theme_state = use_signal(|| ThemeState::new(default_theme_preset(), ThemeMode::Dark));
    rsx! {
        document::Style { {nice_plug_dioxus::TAILWIND_CSS} }
        // trigger-ui's own compiled utilities + theme tokens — the framework
        // CSS above only covers nice-plug-dioxus's widgets, so without this
        // every layout-critical class (flex-1, min-h-0, …) is undefined and
        // the layout collapses in DAW hosts. `just tailwind-trigger`.
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
    let ui = shared
        .get::<TriggerUiState>()
        .expect("TriggerUiState missing");
    let ctx = use_param_context();
    let params = &ui.params;

    // Redraw tick. AppShell owns the read-side of every Param atomic, so it
    // must re-render for fresh values to reach the DOM. Spawn the OS thread
    // exactly once via use_hook and have it call schedule_update on this
    // scope — same driver as comp-ui's control view.
    let mut app_tick: Signal<u64> = use_signal(|| 0);
    use_hook(|| {
        let updater = dioxus_core::schedule_update();
        std::thread::spawn(move || {
            loop {
                // ~30 Hz — plenty for display ballistics; keeps the headless
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
    // and the waveform freezes.
    let frame_counter = *app_tick.read();

    // Note stepper display + values.
    let note_value = params.note.value();
    let note_text = params
        .note
        .normalized_value_to_string(params.note.modulated_normalized_value(), true);

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
                            "FTS Trigger"
                        }
                        div {
                            class: "text-xs text-muted-foreground uppercase tracking-wider",
                            "Drum Trigger"
                        }
                    }
                }

                // ── Analysis waveform ───────────────────────────────
                // Height pinned to exactly GRAPH_H CSS px so pointer y maps
                // 1:1 onto the graph's fixed viewBox (see
                // trigger_waveform.rs) — the control row below takes the
                // remaining space.
                div {
                    "data-testid": "trigger-graph",
                    style: format!(
                        "height:{}px; flex:none; position:relative; overflow:hidden;",
                        crate::trigger_waveform::GRAPH_H
                    ),
                    crate::trigger_waveform::TriggerWaveform {}
                }

                // ── Control surface ─────────────────────────────────
                div {
                    class: "flex-1 min-h-0 flex items-center justify-center gap-8 px-6 py-4",

                    // Knob grid — detection + velocity params.
                    div {
                        class: "grid grid-cols-4 gap-x-6 gap-y-4 justify-items-center",
                        div { "data-testid": "knob-threshold",
                            Knob { handle: param_handle(params.threshold_db.as_ptr(), ctx.clone()) }
                        }
                        div { "data-testid": "knob-sensitivity",
                            Knob { handle: param_handle(params.sensitivity_ms.as_ptr(), ctx.clone()) }
                        }
                        div { "data-testid": "knob-retrigger",
                            Knob { handle: param_handle(params.retrigger_ms.as_ptr(), ctx.clone()) }
                        }
                        div { "data-testid": "knob-dynamics",
                            Knob { handle: param_handle(params.dynamics.as_ptr(), ctx.clone()) }
                        }
                        div { "data-testid": "knob-sc-hpf",
                            Knob { handle: param_handle(params.sc_hpf_hz.as_ptr(), ctx.clone()) }
                        }
                        div { "data-testid": "knob-sc-lpf",
                            Knob { handle: param_handle(params.sc_lpf_hz.as_ptr(), ctx.clone()) }
                        }
                        div { "data-testid": "knob-vel-min",
                            Knob { handle: param_handle(params.vel_min.as_ptr(), ctx.clone()) }
                        }
                        div { "data-testid": "knob-vel-max",
                            Knob { handle: param_handle(params.vel_max.as_ptr(), ctx.clone()) }
                        }
                    }

                    // Selects + note picker + listen.
                    div {
                        class: "flex flex-col gap-4",

                        div {
                            class: "flex flex-col gap-1",
                            div { class: "text-[10px] uppercase tracking-wider text-muted-foreground", "Algorithm" }
                            div { "data-testid": "select-algorithm",
                                Segmented {
                                    handle: param_handle(params.algorithm.as_ptr(), ctx.clone()),
                                    options: vec![
                                        "Peak Env".to_string(),
                                        "Flux".to_string(),
                                        "SuperFlux".to_string(),
                                        "HFC".to_string(),
                                        "Complex".to_string(),
                                        "Rect C".to_string(),
                                        "Mod KL".to_string(),
                                    ],
                                }
                            }
                        }

                        div {
                            class: "flex flex-col gap-1",
                            div { class: "text-[10px] uppercase tracking-wider text-muted-foreground", "Velocity Curve" }
                            div { "data-testid": "select-curve",
                                Segmented {
                                    handle: param_handle(params.vel_curve.as_ptr(), ctx.clone()),
                                    options: vec![
                                        "Linear".to_string(),
                                        "Log".to_string(),
                                        "Exp".to_string(),
                                        "Fixed".to_string(),
                                    ],
                                }
                            }
                        }

                        div {
                            class: "flex items-center gap-6",

                            // MIDI note stepper.
                            div {
                                class: "flex flex-col gap-1",
                                div { class: "text-[10px] uppercase tracking-wider text-muted-foreground", "Note" }
                                div {
                                    class: "flex items-center gap-2",
                                    div { "data-testid": "note-dec",
                                        class: "w-6 h-6 flex items-center justify-center rounded border border-border cursor-pointer text-sm",
                                        onclick: {
                                            let ctx = ctx.clone();
                                            let params = params.clone();
                                            move |_| {
                                                let ptr = params.note.as_ptr();
                                                let v = (params.note.value() - 1).clamp(0, 127);
                                                ctx.begin_set_raw(ptr);
                                                ctx.set_normalized_raw(ptr, params.note.preview_normalized(v));
                                                ctx.end_set_raw(ptr);
                                            }
                                        },
                                        "-"
                                    }
                                    div { "data-testid": "note-value",
                                        class: "min-w-12 text-center font-mono text-sm",
                                        "{note_text} ({note_value})"
                                    }
                                    div { "data-testid": "note-inc",
                                        class: "w-6 h-6 flex items-center justify-center rounded border border-border cursor-pointer text-sm",
                                        onclick: {
                                            let ctx = ctx.clone();
                                            let params = params.clone();
                                            move |_| {
                                                let ptr = params.note.as_ptr();
                                                let v = (params.note.value() + 1).clamp(0, 127);
                                                ctx.begin_set_raw(ptr);
                                                ctx.set_normalized_raw(ptr, params.note.preview_normalized(v));
                                                ctx.end_set_raw(ptr);
                                            }
                                        },
                                        "+"
                                    }
                                }
                            }

                            // Listen (threshold-tuning click) toggle.
                            div { "data-testid": "toggle-listen",
                                Toggle { handle: param_handle(params.listen.as_ptr(), ctx.clone()) }
                            }
                        }
                    }
                }
            }
        }
    }
}
