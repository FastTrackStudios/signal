//! The iPhone shell.
//!
//! A portrait **home** page opens onto surfaces; the guitar rig is the
//! first. The rig (`RigShell`) is a wide control panel that switches the
//! phone to landscape on entry (and back to portrait on exit) via
//! `ios_orientation`. Inside the rig, three pages sit behind a slim left
//! rail:
//!
//! - **Scenes**: the perform grid (footswitch stacks, tap tempo, hold layer)
//! - **Control**: the guitar instrument panel (chain, params, meters)
//! - **Audio**: input/output device + buffer/rate selection
//!
//! The rig clients come from `rig_engine.rs` (in-process LocalServer) and
//! are provided as context, so every shared component works unchanged.

use dioxus::prelude::*;
use signal_guitar_ui::proto::audio::AudioSettingsClient;
use signal_guitar_ui::proto::AudioPrefs;
use signal_guitar_ui::proto::rig::RigClient;
use signal_guitar_ui::{use_rig_state, AudioSettingsBridge, AudioSettingsModal, ControlView, PerformGrid};

const SIGNAL_TAILWIND: &str = include_str!("../assets/tailwind-signal.css");

/// Which top-level screen is showing.
#[derive(Clone, Copy, PartialEq)]
enum MobileScreen {
    Home,
    Rig,
}

/// Which rig page is showing (within `RigShell`).
#[derive(Clone, Copy, PartialEq)]
enum MobilePage {
    Scenes,
    Control,
    Audio,
}

/// The phone app root: bootstrap the engine into context, then route
/// between the home page and the surfaces.
#[component]
pub fn MobileApp() -> Element {
    let engine = crate::rig_engine::engine();
    rsx! {
        // Without device-width + viewport-fit=cover, WKWebView lays out a
        // 980px legacy viewport and the safe-area env() vars stay zero.
        document::Meta {
            name: "viewport",
            content: "width=device-width, initial-scale=1, viewport-fit=cover",
        }
        document::Style { {SIGNAL_TAILWIND} }
        div {
            style: "display: flex; flex-direction: column; height: 100dvh; width: 100vw; \
                    box-sizing: border-box; \
                    background: #09090b; color: #e4e4e7; overflow: hidden; \
                    padding-top: env(safe-area-inset-top); \
                    padding-bottom: env(safe-area-inset-bottom); \
                    padding-left: env(safe-area-inset-left); \
                    padding-right: env(safe-area-inset-right);",
            match engine {
                Some(engine) => {
                    let _ = provide_context(engine.rig.clone());
                    let _ = provide_context(engine.stream.clone());
                    let _ = provide_context(engine.settings.clone());
                    rsx! { Router {} }
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

/// Home ⇄ Rig. Each screen owns its orientation (set on mount), so
/// navigating swaps the component and rotates the phone.
#[component]
fn Router() -> Element {
    let mut screen = use_signal(|| MobileScreen::Home);
    match screen() {
        MobileScreen::Home => rsx! {
            HomePage { on_open_rig: move |_| screen.set(MobileScreen::Rig) }
        },
        MobileScreen::Rig => rsx! {
            RigShell { on_home: move |_| screen.set(MobileScreen::Home) }
        },
    }
}

/// Portrait landing: the app's surfaces as tappable cards. Guitar Rig is
/// live; the rest are placeholders until their mobile ports land.
#[component]
fn HomePage(on_open_rig: EventHandler<()>) -> Element {
    use_hook(crate::ios_orientation::portrait);

    rsx! {
        div { style: "flex: 1; min-height: 0; overflow-y: auto; padding: 20px 18px; display: flex; flex-direction: column; gap: 18px;",
            div { style: "display: flex; flex-direction: column; gap: 2px; padding-top: 8px;",
                span { style: "font-size: 24px; font-weight: 800; letter-spacing: -0.5px;", "FastTrackStudio" }
                span { style: "font-size: 13px; color: #71717a;", "Live rig & session control" }
            }
            // Guitar Rig — the live surface.
            button {
                style: "text-align: left; border: none; border-radius: 16px; padding: 18px; \
                        background: linear-gradient(135deg, #1e3a5f, #0c4a6e); color: #e0f2fe; \
                        display: flex; flex-direction: column; gap: 4px;",
                onclick: move |_| on_open_rig.call(()),
                div { style: "display: flex; align-items: center; gap: 8px;",
                    span { style: "font-size: 24px;", "🎸" }
                    span { style: "font-size: 18px; font-weight: 700;", "Guitar Rig" }
                }
                span { style: "font-size: 12px; opacity: 0.75;",
                    "The signal engine, live on this phone — scenes, chain, tuner. Rotates to landscape."
                }
            }
            // Placeholders for the surfaces still to port.
            for (icon, title, sub) in [
                ("🎵", "Session", "Setlists, transport & the mixer"),
                ("🎼", "Charts", "Keyflow chart writing"),
            ] {
                div {
                    style: "border: 1px solid #27272a; border-radius: 16px; padding: 16px; \
                            background: #131316; color: #52525b; display: flex; flex-direction: column; gap: 3px;",
                    div { style: "display: flex; align-items: center; gap: 8px;",
                        span { style: "font-size: 20px; opacity: 0.5;", "{icon}" }
                        span { style: "font-size: 16px; font-weight: 700;", "{title}" }
                        span { style: "margin-left: auto; font-size: 10px; font-weight: 600; color: #3f3f46;", "SOON" }
                    }
                    span { style: "font-size: 12px;", "{sub}" }
                }
            }
        }
    }
}

/// Landscape rig shell: a slim left rail (Home + page tabs + engine status)
/// beside the full-width page — the grid gets the whole screen, like the
/// floor unit.
#[component]
fn RigShell(on_home: EventHandler<()>) -> Element {
    use_hook(crate::ios_orientation::landscape);

    let mut page = use_signal(|| MobilePage::Scenes);
    let state = use_rig_state();
    let perf = state.perf;
    let running = state.running.cloned();
    let bpm = perf.cloned().tempo_bpm;

    rsx! {
        div { style: "flex: 1; min-height: 0; display: flex; flex-direction: row; overflow: hidden;",
            // Left rail.
            div {
                style: "width: 64px; display: flex; flex-direction: column; align-items: stretch; \
                        border-right: 1px solid #27272a; background: #0f0f10; \
                        padding: 6px 0 8px; gap: 2px;",
                // Home / back.
                button {
                    style: "padding: 8px 0 6px; background: transparent; border: none; border-radius: 8px; \
                            margin: 0 6px 4px; display: flex; flex-direction: column; align-items: center; \
                            gap: 1px; font-size: 9px; font-weight: 600; color: #71717a;",
                    onclick: move |_| on_home.call(()),
                    span { style: "font-size: 17px;", "‹" }
                    "Home"
                }
                for (p, label, icon) in [
                    (MobilePage::Scenes, "Scenes", "🎛"),
                    (MobilePage::Control, "Control", "🎚"),
                    (MobilePage::Audio, "Audio", "🔊"),
                ] {
                    button {
                        style: format!(
                            "padding: 8px 0 6px; background: {}; border: none; border-radius: 8px; \
                             margin: 0 6px; display: flex; flex-direction: column; align-items: center; \
                             gap: 1px; font-size: 9px; font-weight: 600; color: {};",
                            if page() == p { "#1e293b" } else { "transparent" },
                            if page() == p { "#38bdf8" } else { "#71717a" }
                        ),
                        onclick: move |_| page.set(p),
                        span { style: "font-size: 17px;", "{icon}" }
                        "{label}"
                    }
                }
                div { style: "flex: 1;" }
                // Engine status: dot + bpm.
                div {
                    style: "display: flex; flex-direction: column; align-items: center; gap: 3px;",
                    span {
                        style: format!(
                            "width: 8px; height: 8px; border-radius: 999px; background: {};",
                            if running { "#22c55e" } else { "#ef4444" }
                        )
                    }
                    span { style: "font-size: 10px; font-weight: 700; color: #a1a1aa; font-variant-numeric: tabular-nums;",
                        "{bpm}"
                    }
                }
            }
            // Page content.
            div { style: "flex: 1; min-width: 0; min-height: 0; display: flex; flex-direction: column; overflow: hidden;",
                match page() {
                    MobilePage::Scenes => rsx! { ScenesPage { state: state.clone() } },
                    MobilePage::Control => rsx! {
                        div { style: "flex: 1; min-height: 0; overflow-y: auto;",
                            ControlView { model: perf.cloned(), state: state.clone() }
                        }
                    },
                    MobilePage::Audio => rsx! {
                        AudioPage { on_close: move |_| page.set(MobilePage::Scenes) }
                    },
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

    let _ = running;
    rsx! {
        // A whisper of a header: just the active patch, centered.
        if !active.is_empty() {
            div { style: "text-align: center; font-size: 11px; font-weight: 600; color: #a1a1aa; padding: 2px 0 0;",
                "{active}"
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
/// prefs and restarts the rig so they take effect. `on_close` returns to
/// the Scenes page (the shared modal is full-screen — without a working
/// close it traps the UI).
#[component]
fn AudioPage(on_close: EventHandler<()>) -> Element {
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
                        on_close: move |_| on_close.call(()),
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
