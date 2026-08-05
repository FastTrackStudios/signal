//! Comp editor — Dioxus GUI root component.
//!
//! Layout, top to bottom:
//!
//! 1. **Header** — plugin identity, the hardware-profile picker (Control /
//!    LA-2A / SSL Bus / 1176, which re-tints the whole surface through
//!    [`crate::profile_view::profile_skin`]), and the Basic/Advanced toggle.
//! 2. **Graph** — [`crate::comp_graph::CompGraph`]: transfer curve with
//!    threshold/knee drag plus the rolling waveform + GR traces.
//! 3. **Control surface** — the engine grouped into labelled sections
//!    (Detector / Dynamics / Character / Output, then Sidechain / Expander /
//!    Upward when Advanced is on) beside the GR and I/O meters.
//!
//! Basic shows the classic eight params; Advanced adds the rest of
//! `ProC3Compressor`'s surface. The split is a UI concern only — every param
//! stays automatable from the host in both modes.
//!
//! Reusable widgets (knobs, meters, drag provider) come from [`fts_ui_audio`];
//! theme + layout primitives from [`fts_ui`]; the section wrappers from
//! [`crate::sections`].

use std::sync::atomic::Ordering;

use audiocore_core::prelude::*;
use fts_ui::prelude::{ThemeMode, ThemeProvider, ThemeState, default_theme_preset};
use fts_ui_audio::prelude::*;

use crate::param_adapter::param_handle;
use crate::params::{CHARACTER_LABELS, CompUiState, PROFILE_LABELS, STYLE_LABELS};
use crate::profile_view::{ProfileSkin, profile_skin};
use crate::sections::{ParamKnob, ParamSelector, Section};

/// Profile ids in [`PROFILE_LABELS`] order — the index the `profile` param
/// holds maps onto `comp_profiles::all_profiles()` through this table.
const PROFILE_IDS: &[&str] = &["control", "la2a", "ssl_bus", "urei_1176"];

/// Editor size the plugin shell requests from the host.
///
/// Lives here rather than in `comp-plugin` because the surface is what
/// constrains it: blitz does not overflow-scroll a height-constrained
/// container, so a section that does not fit collapses to 0×0 and becomes
/// unreachable rather than being clipped. Widening the Advanced page means
/// growing these — `advanced_page_fits_the_plugin_editor_size` is the guard.
pub const EDITOR_W: u32 = 980;
pub const EDITOR_H: u32 = 660;

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

    // Advanced disclosure. Local UI state — deliberately not a plugin param,
    // so switching views never shows up as an automatable change or dirties
    // the host's project state.
    let mut advanced = use_signal(|| false);

    // The profile param picks the skin; every section below tints from it.
    let profile_idx = params.profile.value().max(0) as usize;
    let skin = profile_skin(PROFILE_IDS.get(profile_idx).copied().unwrap_or("control"));

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

    let is_advanced = *advanced.read();

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

                    div {
                        style: "display:flex; align-items:center; gap:14px;",

                        // Hardware profile picker.
                        ParamSelector {
                            handle: param_handle(params.profile.as_ptr(), ctx.clone()),
                            testid: "profile".to_string(),
                            label: "Profile".to_string(),
                            options: PROFILE_LABELS.iter().map(|s| s.to_string()).collect(),
                            skin,
                        }

                        // Basic/Advanced disclosure.
                        div {
                            "data-testid": "advanced-toggle",
                            style: format!(
                                "cursor:pointer; padding:5px 12px; border-radius:6px; \
                                 font-size:11px; font-weight:600; letter-spacing:0.06em; \
                                 text-transform:uppercase; border:1px solid {}; color:{}; background:{};",
                                skin.border,
                                if is_advanced { "#fff" } else { skin.text },
                                if is_advanced { skin.accent } else { "transparent" },
                            ),
                            onclick: move |_| advanced.toggle(),
                            if is_advanced { "Advanced" } else { "Basic" }
                        }
                    }
                }

                // ── Compressor graph ────────────────────────────────
                // Height pinned to exactly GRAPH_H CSS px so pointer y maps
                // 1:1 onto the graph's fixed viewBox (see comp_graph.rs) —
                // the control surface below takes the remaining space.
                div {
                    "data-testid": "comp-graph",
                    style: format!(
                        "height:{}px; flex:none; position:relative; overflow:hidden;",
                        crate::comp_graph::GRAPH_H
                    ),
                    crate::comp_graph::CompGraph {}
                }

                // ── Control surface ─────────────────────────────────
                // One row of sections, always. Advanced *swaps the page*
                // rather than appending to it: blitz will not overflow-scroll
                // a height-constrained grid — a second row of sections is
                // simply allocated 0 px and collapses — and the editor is a
                // fixed 800×640 in most hosts, so there is no second row to be
                // had. Each page is sized to fit what remains under the graph.
                div {
                    class: "flex-1 min-h-0 flex items-stretch gap-6 px-5 py-4",

                    div {
                        style: "flex:1; min-width:0; display:flex; align-items:stretch; gap:10px;",

                        if is_advanced {
                            // Detector internals: everything shaping *when*
                            // and *how fast* the compressor reacts.
                            Section { label: "Detector".to_string(), skin,
                                ParamKnob {
                                    handle: param_handle(params.detector_rms_mix.as_ptr(), ctx.clone()),
                                    testid: "rmsmix".to_string(),
                                    size: KnobSize::Small,
                                }
                                ParamKnob {
                                    handle: param_handle(params.feedback.as_ptr(), ctx.clone()),
                                    testid: "feedback".to_string(),
                                    size: KnobSize::Small,
                                }
                                ParamKnob {
                                    handle: param_handle(params.hold_ms.as_ptr(), ctx.clone()),
                                    testid: "hold".to_string(),
                                    size: KnobSize::Small,
                                }
                                ParamKnob {
                                    handle: param_handle(params.lookahead_ms.as_ptr(), ctx.clone()),
                                    testid: "lookahead".to_string(),
                                    size: KnobSize::Small,
                                }
                                ParamKnob {
                                    handle: param_handle(params.inertia.as_ptr(), ctx.clone()),
                                    testid: "inertia".to_string(),
                                    size: KnobSize::Small,
                                }
                                ParamKnob {
                                    handle: param_handle(params.inertia_decay.as_ptr(), ctx.clone()),
                                    testid: "inertiadecay".to_string(),
                                    size: KnobSize::Small,
                                }
                            }

                            // Detector-path EQ. Both filters bypass at their
                            // 20 Hz floor, so parking a knob at minimum is the
                            // same as switching the filter out.
                            Section { label: "Sidechain".to_string(), skin,
                                ParamKnob {
                                    handle: param_handle(params.sidechain_freq.as_ptr(), ctx.clone()),
                                    testid: "schp".to_string(),
                                    size: KnobSize::Small,
                                }
                                ParamKnob {
                                    handle: param_handle(params.sidechain_lowpass_freq.as_ptr(), ctx.clone()),
                                    testid: "sclp".to_string(),
                                    size: KnobSize::Small,
                                }
                                ParamKnob {
                                    handle: param_handle(params.range_db.as_ptr(), ctx.clone()),
                                    testid: "range".to_string(),
                                    size: KnobSize::Small,
                                }
                            }

                            // Both extra dynamics stages are inert at ratio
                            // 1:1, which is where they default.
                            Section { label: "Expander".to_string(), skin,
                                ParamKnob {
                                    handle: param_handle(params.expander_threshold_db.as_ptr(), ctx.clone()),
                                    testid: "expthresh".to_string(),
                                    size: KnobSize::Small,
                                }
                                ParamKnob {
                                    handle: param_handle(params.expander_ratio.as_ptr(), ctx.clone()),
                                    testid: "expratio".to_string(),
                                    size: KnobSize::Small,
                                }
                            }

                            Section { label: "Upward".to_string(), skin,
                                ParamKnob {
                                    handle: param_handle(params.upward_threshold_db.as_ptr(), ctx.clone()),
                                    testid: "upthresh".to_string(),
                                    size: KnobSize::Small,
                                }
                                ParamKnob {
                                    handle: param_handle(params.upward_ratio.as_ptr(), ctx.clone()),
                                    testid: "upratio".to_string(),
                                    size: KnobSize::Small,
                                }
                            }

                            // Saturation shape + the soft output ceiling, plus
                            // the input trim and auto-makeup that belong with
                            // the extended gain staging.
                            Section { label: "Character".to_string(), skin,
                                ParamSelector {
                                    handle: param_handle(params.character_mode.as_ptr(), ctx.clone()),
                                    testid: "charmode".to_string(),
                                    label: "Shape".to_string(),
                                    options: CHARACTER_LABELS.iter().map(|s| s.to_string()).collect(),
                                    skin,
                                }
                                ParamKnob {
                                    handle: param_handle(params.ceiling.as_ptr(), ctx.clone()),
                                    testid: "ceiling".to_string(),
                                    size: KnobSize::Small,
                                }
                                ParamKnob {
                                    handle: param_handle(params.input_gain_db.as_ptr(), ctx.clone()),
                                    testid: "ingain".to_string(),
                                    size: KnobSize::Small,
                                }
                                div {
                                    "data-testid": "toggle-automake",
                                    style: "align-self:center;",
                                    Toggle {
                                        handle: param_handle(params.auto_makeup.as_ptr(), ctx.clone()),
                                        color: skin.accent.to_string(),
                                    }
                                }
                            }
                        } else {
                            // ── Dynamics — the curve itself ─────────────
                            Section { label: "Dynamics".to_string(), skin,
                                ParamKnob {
                                    handle: param_handle(params.threshold_db.as_ptr(), ctx.clone()),
                                    testid: "threshold".to_string(),
                                }
                                ParamKnob {
                                    handle: param_handle(params.ratio.as_ptr(), ctx.clone()),
                                    testid: "ratio".to_string(),
                                }
                                ParamKnob {
                                    handle: param_handle(params.attack_ms.as_ptr(), ctx.clone()),
                                    testid: "attack".to_string(),
                                }
                                ParamKnob {
                                    handle: param_handle(params.release_ms.as_ptr(), ctx.clone()),
                                    testid: "release".to_string(),
                                }
                                ParamKnob {
                                    handle: param_handle(params.knee_db.as_ptr(), ctx.clone()),
                                    testid: "knee".to_string(),
                                }
                            }

                            // ── Detector — how the level is measured ────
                            Section { label: "Detector".to_string(), skin,
                                ParamSelector {
                                    handle: param_handle(params.style.as_ptr(), ctx.clone()),
                                    testid: "style".to_string(),
                                    label: "Style".to_string(),
                                    options: STYLE_LABELS.iter().map(|s| s.to_string()).collect(),
                                    skin,
                                }
                                ParamKnob {
                                    handle: param_handle(params.stereo_link.as_ptr(), ctx.clone()),
                                    testid: "link".to_string(),
                                    size: KnobSize::Small,
                                }
                            }

                            // ── Character — the saturation stage ────────
                            Section { label: "Character".to_string(), skin,
                                ParamKnob {
                                    handle: param_handle(params.drive.as_ptr(), ctx.clone()),
                                    testid: "drive".to_string(),
                                }
                            }

                            // ── Output — gain staging + parallel blend ──
                            Section { label: "Output".to_string(), skin,
                                ParamKnob {
                                    handle: param_handle(params.makeup_db.as_ptr(), ctx.clone()),
                                    testid: "makeup".to_string(),
                                }
                                ParamKnob {
                                    handle: param_handle(params.mix.as_ptr(), ctx.clone()),
                                    testid: "mix".to_string(),
                                }
                            }
                        }
                    }

                    // Metering — GR (fed by the audio thread through
                    // CompUiState) flanked by I/O peak meters.
                    div {
                        class: "flex items-end gap-3 shrink-0",
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

/// Re-exported so callers (and tests) can resolve a profile index to its skin
/// the same way [`AppShell`] does.
pub fn skin_for_profile_index(index: usize) -> ProfileSkin {
    profile_skin(PROFILE_IDS.get(index).copied().unwrap_or("control"))
}
