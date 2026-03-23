//! Dioxus editor UI for FTS Signal Controller.
//!
//! Displays 8 macro knobs with activity indicators, bank switching, and connection status.

use std::sync::atomic::Ordering;

use audio_gui::prelude::*;
use fts_plugin_core::prelude::*;

use crate::plugin::{ControllerUiState, MACRO_BANKS, NUM_MACROS};

/// Macro accent colors — one per knob, matching a studio-friendly palette.
const MACRO_COLORS: [&str; NUM_MACROS] = [
    "#F97316", // orange
    "#EAB308", // yellow
    "#22C55E", // green
    "#06B6D4", // cyan
    "#3B82F6", // blue
    "#8B5CF6", // violet
    "#EC4899", // pink
    "#EF4444", // red
];

#[component]
pub fn App() -> Element {
    tracing::debug!("App: rendering");
    let t = use_init_theme();
    let t = *t.read();
    let ctx = use_param_context();

    let shared = use_context::<SharedState>();
    let ui = shared
        .get::<ControllerUiState>()
        .expect("ControllerUiState missing from SharedState");

    tracing::debug!("App: got UI state");
    let shm_connected = ui.shm_connected.load(Ordering::Relaxed) != 0;
    let pending_writes = ui.pending_write_count.load(Ordering::Relaxed);
    let config_loaded = ui.config_loaded.load(Ordering::Relaxed);
    let active_bank_idx = ui.active_bank.load(Ordering::Relaxed) as usize;
    let active_bank = &MACRO_BANKS[active_bank_idx];
    let macro_ptrs = ui.macro_ptrs();

    // Get effective names: ext_state config takes priority over bank labels
    let macro_names = ui.macro_labels();
    let ext_colors = ui.macro_colors();
    tracing::debug!("App: config_loaded={config_loaded} bank={} macros ready", active_bank.name);

    rsx! {
        document::Style { {t.base_css()} }

        DragProvider {
            div {
            style: format!(
                "{} display:flex; flex-direction:column; gap:{};",
                t.root_style(),
                t.spacing_section,
            ),

            // ── Header ──────────────────────────────────────────────
            div {
                style: format!(
                    "display:flex; justify-content:space-between; align-items:center; \
                     padding-bottom:{}; border-bottom:1px solid {};",
                    t.spacing_label,
                    t.border,
                ),
                div {
                    style: format!(
                        "font-size:{}; font-weight:700; color:{}; letter-spacing:0.05em;",
                        t.font_size_title,
                        t.text_bright,
                    ),
                    "SIGNAL CONTROLLER"
                }
                // Connection status
                div {
                    style: format!(
                        "display:flex; align-items:center; gap:6px; font-size:{}; color:{};",
                        t.font_size_label,
                        t.text_dim,
                    ),
                    div {
                        style: format!(
                            "width:8px; height:8px; border-radius:50%; background:{};",
                            if shm_connected { t.signal_safe } else { t.text_dim },
                        ),
                    }
                    if shm_connected {
                        "SHM CONNECTED"
                    } else {
                        "OFFLINE"
                    }
                    if pending_writes > 0 {
                        span {
                            style: format!("color:{};", t.accent),
                            " · {pending_writes} writes"
                        }
                    }
                }
            }

            // ── Bank Selector ───────────────────────────────────────
            div {
                style: format!(
                    "display:flex; gap:6px; align-items:center;",
                ),
                div {
                    style: format!(
                        "{}; margin-right:8px;",
                        t.style_label(),
                    ),
                    "BANK"
                }
                for (bank_idx, bank) in MACRO_BANKS.iter().enumerate() {
                    {
                        let is_active = bank_idx == active_bank_idx;
                        let ui_clone = ui.clone();
                        let ctx_clone = ctx.clone();
                        rsx! {
                            BankButton {
                                label: bank.name,
                                active: is_active,
                                on_click: move |_| {
                                    switch_bank(&ui_clone, &ctx_clone, bank_idx);
                                },
                            }
                        }
                    }
                }
            }

            // ── Macro Knobs ─────────────────────────────────────────
            div {
                style: format!(
                    "{} padding:{};",
                    t.style_card(),
                    t.spacing_card,
                ),

                div {
                    style: format!(
                        "display:flex; justify-content:space-between; align-items:center; \
                         margin-bottom:{};",
                        t.spacing_control,
                    ),
                    div {
                        style: format!("{}", t.style_label()),
                        if config_loaded {
                        "MACROS — Track Config"
                    } else {
                        "MACROS — {active_bank.name}"
                    }
                    }
                }

                div {
                    style: "display:flex; flex-wrap:wrap; justify-content:center; gap:12px;",

                    for i in 0..NUM_MACROS {
                        {
                            // Use ext_state color if available, otherwise fall back to default palette
                            let color = if !ext_colors[i].is_empty() {
                                ext_colors[i].clone()
                            } else {
                                MACRO_COLORS[i].to_string()
                            };
                            rsx! {
                                MacroKnob {
                                    index: i,
                                    param_ptr: macro_ptrs[i],
                                    color: color,
                                    activity: ui.macro_activity[i].load(Ordering::Relaxed),
                                    label: macro_names[i].clone(),
                                }
                            }
                        }
                    }
                }
            }

            // ── Status Bar ──────────────────────────────────────────
            div {
                style: format!(
                    "display:flex; justify-content:space-between; align-items:center; \
                     font-size:{}; color:{}; padding-top:{}; border-top:1px solid {};",
                    t.font_size_tiny,
                    t.text_dim,
                    t.spacing_label,
                    t.border_subtle,
                ),
                { let version = env!("CARGO_PKG_VERSION"); rsx! { div { "FTS Signal Controller v{version}" } } }
                div { "Track FX Control · Passthrough" }
            }
        }
        } // DragProvider
    }
}

/// Switch to a new macro bank: update param display names and request host rescan.
fn switch_bank(ui: &ControllerUiState, ctx: &ParamContext, bank_idx: usize) {
    let bank = &MACRO_BANKS[bank_idx];
    ui.active_bank.store(bank_idx as u8, Ordering::Relaxed);
    ui.params.apply_bank(bank);
    // Tell the host to re-query parameter names
    ctx.rescan_param_info();
}

/// A bank selector button.
#[component]
fn BankButton(label: &'static str, active: bool, on_click: EventHandler<()>) -> Element {
    let t = use_theme();
    let t = *t.read();

    let (bg, color, border) = if active {
        (t.accent, t.bg, t.accent)
    } else {
        (t.surface, t.text_dim, t.border_subtle)
    };

    rsx! {
        div {
            style: format!(
                "padding:4px 10px; border-radius:4px; font-size:{}; font-weight:600; \
                 letter-spacing:0.03em; cursor:pointer; user-select:none; \
                 background:{}; color:{}; border:1px solid {}; \
                 transition:{};",
                t.font_size_label,
                bg, color, border,
                t.transition_fast,
            ),
            onclick: move |_| on_click.call(()),
            "{label}"
        }
    }
}

/// A single macro knob with label, activity indicator, and color.
#[component]
fn MacroKnob(
    index: usize,
    param_ptr: ParamPtr,
    color: String,
    activity: f32,
    label: String,
) -> Element {
    let t = use_theme();
    let _t = *t.read();

    let glow_opacity = (activity * 2.0).min(0.6);

    rsx! {
        div {
            style: "display:flex; flex-direction:column; align-items:center; gap:4px; \
                    position:relative;",

            // Activity glow behind the knob
            if glow_opacity > 0.01 {
                div {
                    style: format!(
                        "position:absolute; top:4px; left:50%; transform:translateX(-50%); \
                         width:48px; height:48px; border-radius:50%; \
                         background:{color}; opacity:{glow_opacity}; filter:blur(12px); \
                         pointer-events:none;",
                    ),
                }
            }

            Knob {
                param_ptr: param_ptr,
                size: KnobSize::Large,
                label: label,
                color: color,
            }
        }
    }
}
