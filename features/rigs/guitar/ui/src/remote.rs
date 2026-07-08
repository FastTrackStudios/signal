//! The self-contained rig remote — top bar (profile, mode toggle, meters),
//! Perform footswitch grid, chain strip, and the audio-settings modal.
//!
//! This is the whole guitar rig UI a *remote* needs: it consumes only the
//! vox clients from context, so it mounts identically in the browser
//! (`apps/web`, WebSocket transport) and in any native shell (in-process
//! transport). The desktop `signal-ui` mounts its richer grid around the
//! same building blocks.

use dioxus::prelude::*;
use lumen_blocks::components::button::{Button, ButtonVariant};

use signal_guitar_proto::AudioPrefs;
use signal_guitar_proto::audio::AudioSettingsClient;
use signal_guitar_proto::rig::RigClient;

use crate::meters::MeterPair;
use crate::perform::PerformGrid;
use crate::chain::ChainStrip;
use crate::settings::{AudioSettingsBridge, AudioSettingsModal};
use crate::state::use_rig_state;

/// Top-level UI mode.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// The active patch's chain, as a compact strip.
    Edit,
    /// Play the rig — footswitch folder grid.
    Perform,
}

/// The remote rig UI. Prop-less: everything arrives via context
/// (`RigClient`, `RigStreamClient`, `AudioSettingsClient`).
#[component]
pub fn GuitarRigRemote() -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let settings = use_hook(try_consume_context::<AudioSettingsClient>);
    let state = use_rig_state();

    let mut mode = use_signal(|| Mode::Perform);
    let mut audio_open = use_signal(|| false);

    // Device lists, fetched once over the settings service.
    let devices = use_resource({
        let settings = settings.clone();
        move || {
            let settings = settings.clone();
            async move {
                match settings {
                    Some(s) => s.devices().await.ok(),
                    None => None,
                }
            }
        }
    });

    // Editable prefs, seeded from the persisted ones once fetched.
    let mut prefs = use_signal(AudioPrefs::default);
    {
        let settings = settings.clone();
        use_future(move || {
            let settings = settings.clone();
            async move {
                if let Some(s) = settings {
                    if let Ok(p) = s.prefs().await {
                        prefs.set(p);
                    }
                }
            }
        });
    }

    // Apply = update shared state, persist, and re-open the live rig so
    // device / buffer changes take effect immediately.
    let apply = {
        let settings = settings.clone();
        let rig_for_apply = rig.clone();
        use_callback(move |p: AudioPrefs| {
            prefs.set(p.clone());
            let settings = settings.clone();
            let rig = rig_for_apply.clone();
            spawn(async move {
                if let Some(s) = settings {
                    let _ = s.save_prefs(p).await;
                }
                if let Some(r) = rig {
                    let _ = r.start().await;
                }
            });
        })
    };

    let live_bridge = devices
        .read()
        .as_ref()
        .and_then(|d| d.clone())
        .map(|d| AudioSettingsBridge {
            inputs: d.inputs,
            outputs: d.outputs,
            prefs: prefs(),
            on_save: apply,
        });

    let perf = state.perf;
    let blocks = state.blocks;
    let connected = rig.is_some();
    let profile = perf().profile_name;
    let running = state.running.cloned();

    // The header's patch lens: the active stack tints a pill with its folder
    // color and names the spot in the set ("CRUNCH · Edge"). Falls back to
    // the raw patch name before any footswitch has been pressed.
    let perf_now = perf();
    let active_stack = perf_now.stacks.iter().find(|s| s.is_active);
    let (lens_bg, lens_fg) = active_stack
        .map(|s| crate::perform::folder_color(&s.name))
        .unwrap_or(("#27272a", "#e4e4e7"));
    let lens_label = match active_stack {
        Some(s) => format!("{} · {}", s.name.to_uppercase(), s.current_patch),
        None => state
            .active_patch
            .cloned()
            .unwrap_or_else(|| "no patch".to_string()),
    };

    rsx! {
        div { class: "flex flex-col h-full bg-background text-foreground",
            // Top bar
            header {
                class: "flex items-center gap-2 px-3 py-2 border-b border-border bg-card",

                // Status dot: pulsing green while audio runs, still red when not.
                span {
                    class: if running {
                        "w-2.5 h-2.5 rounded-full ml-1 animate-pulse"
                    } else {
                        "w-2.5 h-2.5 rounded-full ml-1"
                    },
                    style: if running {
                        "background-color: #22c55e;"
                    } else {
                        "background-color: #ef4444;"
                    },
                    title: if running { "audio running" } else { "audio stopped" },
                }
                div { class: "flex flex-col mr-2",
                    span { class: "text-[10px] font-semibold uppercase tracking-[2px] text-muted-foreground",
                        "Guitar Rig"
                    }
                    span { class: "text-sm font-bold",
                        if !profile.is_empty() { "{profile}" } else { "— no profile —" }
                    }
                }

                // Active-patch lens — tinted by the active stack's color.
                div {
                    class: "flex items-center rounded-full px-3 py-1 ml-1 shadow-inner",
                    style: "background-color: {lens_bg}; color: {lens_fg};",
                    span { class: "text-xs font-bold tracking-wide whitespace-nowrap", "{lens_label}" }
                }

                // Edit | Perform mode toggle
                div { class: "flex items-center rounded-md border border-border overflow-hidden",
                    Button {
                        variant: if mode() == Mode::Edit { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                        on_click: move |_| mode.set(Mode::Edit),
                        "Edit"
                    }
                    Button {
                        variant: if mode() == Mode::Perform { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                        on_click: move |_| mode.set(Mode::Perform),
                        "Perform"
                    }
                }

                div { class: "flex-1" }

                // Global switch states — visible in every mode, not just Perform.
                if perf_now.fx_bypass {
                    span {
                        class: "text-[10px] font-bold uppercase tracking-wider rounded px-1.5 py-0.5 mr-1",
                        style: "background-color: #ec4899; color: #ffffff;",
                        "FX off"
                    }
                }
                if perf_now.boost {
                    span {
                        class: "text-[10px] font-bold uppercase tracking-wider rounded px-1.5 py-0.5 mr-1",
                        style: "background-color: #fafafa; color: #0a0a0a;",
                        "Boost +6"
                    }
                }

                if connected {
                    div { class: "flex items-center gap-2 mr-2",
                        MeterPair { input: state.in_level.cloned(), output: state.out_level.cloned() }
                    }
                }

                Button {
                    variant: if audio_open() { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                    on_click: move |_| audio_open.toggle(),
                    "Audio"
                }
            }

            // Body
            div { class: "flex-1 min-h-0 overflow-hidden p-4",
                if let Some(r) = rig.clone() {
                    if mode() == Mode::Perform {
                        PerformGrid {
                            model: perf(),
                            on_press: Callback::new({
                                let r = r.clone();
                                move |i: usize| {
                                    let r = r.clone();
                                    spawn(async move { let _ = r.press_stack(i as u32).await; });
                                }
                            }),
                            on_toggle_fx: Callback::new({
                                let r = r.clone();
                                move |_: ()| {
                                    let r = r.clone();
                                    spawn(async move { let _ = r.toggle_fx().await; });
                                }
                            }),
                            on_toggle_boost: Callback::new({
                                let r = r.clone();
                                move |_: ()| {
                                    let r = r.clone();
                                    spawn(async move { let _ = r.toggle_boost().await; });
                                }
                            }),
                            on_tap_tempo: Callback::new({
                                let r = r.clone();
                                move |_: ()| {
                                    let r = r.clone();
                                    spawn(async move { let _ = r.tap_tempo().await; });
                                }
                            }),
                        }
                    } else {
                        div { class: "flex flex-col gap-2 h-full min-h-0 overflow-hidden",
                            // The real editor: the zoomable/pannable module/wire
                            // graph with the live chain resolved into the
                            // guitar-rig template canvas. Must be a flex column —
                            // the grid panel sizes itself with `flex-1 h-full`.
                            div { class: "flex-1 min-h-0 flex flex-col overflow-hidden",
                                crate::grid::RigGraph { blocks: blocks() }
                            }
                            // Compact chain strip below — quick bypass toggles.
                            ChainStrip {
                                blocks: blocks(),
                                on_toggle: Callback::new({
                                    let r = r.clone();
                                    move |id: String| {
                                        let r = r.clone();
                                        spawn(async move { let _ = r.toggle_block_bypass(id).await; });
                                    }
                                }),
                            }
                        }
                    }
                } else {
                    div { class: "flex items-center justify-center h-full",
                        span { class: "text-sm text-muted-foreground italic", "Connecting to rig…" }
                    }
                }
            }
        }

        // Audio settings modal
        if audio_open() {
            if let Some(bridge) = live_bridge.clone() {
                AudioSettingsModal {
                    bridge: bridge,
                    on_close: move |_| audio_open.set(false),
                }
            }
        }
    }
}
