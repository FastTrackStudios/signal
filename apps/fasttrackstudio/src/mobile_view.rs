//! The iPhone shell — a phone-sized layout over the SAME in-process rig
//! the desktop/engine builds run. Three pages behind a bottom tab bar:
//!
//! - **Scenes**: the perform grid (footswitch stacks, tap tempo, hold
//!   layer) — the shared `PerformGrid` component, full-screen.
//! - **Control**: the guitar instrument panel (chain, params, meters) —
//!   the shared `ControlView`.
//! - **Audio**: input/output device + buffer/sample-rate selection over
//!   `AudioSettingsClient`, with a rig restart to apply.
//!
//! The rig clients come from `rig_engine.rs` (in-process LocalServer) and
//! are provided as context, so every shared component works unchanged.

use dioxus::prelude::*;
use signal_guitar_ui::proto::audio::AudioSettingsClient;
use signal_guitar_ui::proto::AudioPrefs;
use signal_guitar_ui::proto::rig::RigClient;
use signal_guitar_ui::{use_rig_state, AudioSettingsBridge, AudioSettingsModal, ControlView, PerformGrid};

const SIGNAL_TAILWIND: &str = include_str!("../assets/tailwind-signal.css");

#[derive(Clone, Copy, PartialEq)]
enum MobilePage {
    Scenes,
    Control,
    Audio,
}

/// The phone app root: bootstrap status + tabbed pages.
#[component]
pub fn MobileApp() -> Element {
    let engine = crate::rig_engine::engine();
    rsx! {
        document::Style { {SIGNAL_TAILWIND} }
        div {
            style: "display: flex; flex-direction: column; height: 100dvh; width: 100vw; \
                    background: #09090b; color: #e4e4e7; overflow: hidden; \
                    padding-top: env(safe-area-inset-top); \
                    padding-bottom: env(safe-area-inset-bottom);",
            match engine {
                Some(engine) => {
                    let _ = provide_context(engine.rig.clone());
                    let _ = provide_context(engine.stream.clone());
                    let _ = provide_context(engine.settings.clone());
                    rsx! { MobileShell {} }
                }
                None => rsx! {
                    div { style: "display: flex; flex: 1; align-items: center; justify-content: center; flex-direction: column; gap: 8px;",
                        span { style: "width: 12px; height: 12px; border-radius: 999px; background: #ef4444;" }
                        span { style: "font-size: 14px; font-weight: 600;", "Engine failed to start" }
                        span { style: "font-size: 12px; color: #71717a;", "Check the logs (audio device / library load)." }
                    }
                },
            }
        }
    }
}

#[component]
fn MobileShell() -> Element {
    let mut page = use_signal(|| MobilePage::Scenes);
    let state = use_rig_state();
    let perf = state.perf;

    rsx! {
        div { style: "flex: 1; min-height: 0; display: flex; flex-direction: column; overflow: hidden;",
            match page() {
                MobilePage::Scenes => rsx! { ScenesPage { state: state.clone() } },
                MobilePage::Control => rsx! {
                    div { style: "flex: 1; min-height: 0; overflow-y: auto;",
                        ControlView { model: perf.cloned(), state: state.clone() }
                    }
                },
                MobilePage::Audio => rsx! { AudioPage {} },
            }
        }
        // Bottom tab bar.
        div {
            style: "display: flex; border-top: 1px solid #27272a; background: #0f0f10;",
            for (p, label, icon) in [
                (MobilePage::Scenes, "Scenes", "🎛"),
                (MobilePage::Control, "Control", "🎚"),
                (MobilePage::Audio, "Audio", "🔊"),
            ] {
                button {
                    style: format!(
                        "flex: 1; padding: 10px 0 8px; background: none; border: none; \
                         display: flex; flex-direction: column; align-items: center; gap: 2px; \
                         font-size: 11px; font-weight: 600; color: {};",
                        if page() == p { "#38bdf8" } else { "#71717a" }
                    ),
                    onclick: move |_| page.set(p),
                    span { style: "font-size: 18px;", "{icon}" }
                    "{label}"
                }
            }
        }
    }
}

/// Scenes: connection dot + patch line + the shared perform grid, sized
/// for a phone in portrait.
#[component]
fn ScenesPage(state: signal_guitar_ui::RigViewState) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let perf = state.perf.cloned();
    let running = state.running.cloned();
    let active = state.active_patch.cloned().unwrap_or_default();

    macro_rules! rig_call {
        ($method:ident $(, $arg:expr)*) => {{
            let rig = rig.clone();
            move |_| {
                if let Some(rig) = rig.clone() {
                    spawn(async move { let _ = rig.$method($($arg),*).await; });
                }
            }
        }};
    }

    let on_press = {
        let rig = rig.clone();
        Callback::new(move |i: usize| {
            if let Some(rig) = rig.clone() {
                spawn(async move {
                    let _ = rig.press_stack(i as u32).await;
                });
            }
        })
    };
    let on_select_song = {
        let rig = rig.clone();
        Callback::new(move |i: usize| {
            if let Some(rig) = rig.clone() {
                spawn(async move {
                    let _ = rig.select_song(i as u32).await;
                });
            }
        })
    };

    rsx! {
        div { style: "display: flex; align-items: center; gap: 6px; padding: 6px 10px 2px;",
            span {
                style: format!(
                    "width: 8px; height: 8px; border-radius: 999px; background: {};",
                    if running { "#22c55e" } else { "#ef4444" }
                )
            }
            span { style: "font-size: 12px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                "{active}"
            }
            span { style: "margin-left: auto; font-size: 12px; font-weight: 700; color: #a1a1aa; font-variant-numeric: tabular-nums;",
                "{perf.tempo_bpm} bpm"
            }
        }
        div { style: "flex: 1; min-height: 0; padding: 4px;",
            PerformGrid {
                model: perf,
                on_press,
                on_toggle_fx: Callback::new(rig_call!(toggle_fx)),
                on_toggle_boost: Callback::new(rig_call!(toggle_boost)),
                on_cycle_boost: Callback::new(rig_call!(cycle_boost)),
                on_tap_tempo: Callback::new(rig_call!(tap_tempo)),
                on_prev_song: Callback::new(rig_call!(prev_song)),
                on_next_song: Callback::new(rig_call!(next_song)),
                on_select_song,
            }
        }
    }
}

/// Audio: device pickers over the AudioSettings service. Saving persists
/// prefs and restarts the rig so they take effect.
#[component]
fn AudioPage() -> Element {
    let settings = use_hook(try_consume_context::<AudioSettingsClient>);
    let rig = use_hook(try_consume_context::<RigClient>);

    let bridge = use_resource(move || {
        let settings = settings.clone();
        async move {
            let settings = settings?;
            let devices = settings.devices().await.ok()?;
            let prefs = settings.prefs().await.ok()?;
            Some((devices, prefs))
        }
    });

    let on_save = {
        let settings = use_hook(try_consume_context::<AudioSettingsClient>);
        let rig = rig.clone();
        Callback::new(move |prefs: AudioPrefs| {
            let settings = settings.clone();
            let rig = rig.clone();
            spawn(async move {
                if let Some(settings) = settings {
                    let _ = settings.save_prefs(prefs).await;
                }
                if let Some(rig) = rig {
                    // Reopen the device with the new prefs.
                    let _ = rig.start().await;
                }
            });
        })
    };

    rsx! {
        div { style: "flex: 1; min-height: 0; overflow-y: auto; padding: 8px;",
            match bridge.read().as_ref() {
                Some(Some((devices, prefs))) => rsx! {
                    AudioSettingsModal {
                        bridge: AudioSettingsBridge {
                            inputs: devices.inputs.clone(),
                            outputs: devices.outputs.clone(),
                            prefs: prefs.clone(),
                            on_save,
                        },
                        on_close: move |_| {},
                    }
                },
                Some(None) => rsx! {
                    span { style: "font-size: 13px; color: #71717a;", "Audio settings unavailable." }
                },
                None => rsx! {
                    span { style: "font-size: 13px; color: #71717a;", "Loading devices…" }
                },
            }
        }
    }
}
