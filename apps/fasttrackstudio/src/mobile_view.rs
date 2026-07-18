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
    /// The Signal domain: pick an instrument rig.
    Signal,
    /// The guitar rig (the one live instrument today).
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

/// Home → Signal (instrument chooser) → Rig. Each screen owns its
/// orientation (set on mount), so navigating swaps the component and rotates
/// the phone.
#[component]
fn Router() -> Element {
    let mut screen = use_signal(|| MobileScreen::Home);
    match screen() {
        MobileScreen::Home => rsx! {
            HomePage { on_open: move |s| screen.set(s) }
        },
        MobileScreen::Signal => rsx! {
            SignalChooser {
                on_back: move |_| screen.set(MobileScreen::Home),
                on_pick_guitar: move |_| screen.set(MobileScreen::Rig),
            }
        },
        MobileScreen::Rig => rsx! {
            RigShell { on_home: move |_| screen.set(MobileScreen::Home) }
        },
    }
}

/// Portrait landing: the app's domains as tappable cards. No wordmark — the
/// app icon carries the identity.
#[component]
fn HomePage(on_open: EventHandler<MobileScreen>) -> Element {
    use_hook(crate::ios_orientation::portrait);

    rsx! {
        div { style: "flex: 1; min-height: 0; overflow-y: auto; padding: 24px 18px; \
                      display: flex; flex-direction: column; gap: 14px;",
            SurfaceCard {
                title: "Signal",
                sub: "The live rig — guitar, keys, drums, vocals",
                live: true,
                on_open: move |_| on_open.call(MobileScreen::Signal),
            }
            SurfaceCard { title: "Session", sub: "Setlists, sections & the click", live: false }
            SurfaceCard { title: "Charts", sub: "Keyflow chart writing", live: false }
        }
    }
}

/// A domain/surface card. `live` cards glow with the accent edge-LED and are
/// tappable; the rest read as quietly coming-soon.
#[component]
fn SurfaceCard(
    title: &'static str,
    sub: &'static str,
    live: bool,
    on_open: Option<EventHandler<()>>,
) -> Element {
    if live {
        rsx! {
            button {
                style: "position: relative; text-align: left; border: none; border-radius: 14px; \
                        padding: 18px 18px 18px 22px; overflow: hidden; \
                        background: linear-gradient(135deg, #10283f, #0b3a52); color: #e0f2fe; \
                        display: flex; flex-direction: column; gap: 5px;",
                onclick: move |_| { if let Some(h) = on_open { h.call(()); } },
                span {
                    style: "position: absolute; left: 0; top: 14px; bottom: 14px; width: 4px; \
                            border-radius: 0 2px 2px 0; background: #38bdf8; box-shadow: 0 0 12px #38bdf8;",
                }
                span { style: "font-size: 11px; font-weight: 600; letter-spacing: 0.14em; \
                               text-transform: uppercase; color: #7dd3fc;", "Live" }
                span { style: "font-size: 20px; font-weight: 700;", "{title}" }
                span { style: "font-size: 12px; opacity: 0.72;", "{sub}" }
            }
        }
    } else {
        rsx! {
            div {
                style: "border: 1px solid #1f1f23; border-radius: 14px; padding: 16px 18px; \
                        background: #131316; color: #52525b; display: flex; flex-direction: column; gap: 4px;",
                div { style: "display: flex; align-items: center; gap: 8px;",
                    span { style: "font-size: 17px; font-weight: 700;", "{title}" }
                    span { style: "margin-left: auto; font-size: 9px; font-weight: 600; \
                                   letter-spacing: 0.12em; color: #3f3f46;", "SOON" }
                }
                span { style: "font-size: 12px;", "{sub}" }
            }
        }
    }
}

/// Instrument chooser under Signal. Guitar is live; the rest await their
/// mobile ports (their UIs pull desktop-only render deps today).
#[component]
fn SignalChooser(on_back: EventHandler<()>, on_pick_guitar: EventHandler<()>) -> Element {
    use_hook(crate::ios_orientation::portrait);

    rsx! {
        div { style: "flex: 1; min-height: 0; overflow-y: auto; padding: 16px 18px 24px; \
                      display: flex; flex-direction: column; gap: 12px;",
            // Back + section label.
            button {
                style: "align-self: flex-start; appearance: none; background: transparent; border: none; \
                        color: #71717a; font-size: 13px; font-weight: 600; padding: 4px 0; \
                        display: flex; align-items: center; gap: 4px;",
                onclick: move |_| on_back.call(()),
                span { style: "font-size: 15px;", "‹" }
                "Signal"
            }
            SurfaceCard {
                title: "Guitar",
                sub: "NAM amp, drives, footswitch scenes — live on this phone",
                live: true,
                on_open: move |_| on_pick_guitar.call(()),
            }
            for (title, sub) in [
                ("Keys", "Sampled pianos, pads & synths"),
                ("Drums", "Sampled kits & e-drums"),
                ("Bass", "DI + amp + synth bass"),
                ("Vocals", "Live vocal FX chain"),
            ] {
                SurfaceCard { title, sub, live: false }
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

    rsx! {
        div { style: "flex: 1; min-height: 0; display: flex; flex-direction: row; overflow: hidden;",
            // Rack rail: anodized panel, active page marked by a glowing
            // accent LED on the inner edge (echoes the stompbox LEDs).
            div {
                style: "width: 62px; flex-shrink: 0; display: flex; flex-direction: column; \
                        align-items: stretch; background: #0a0a0c; \
                        border-right: 1px solid #1b1b1f; padding: 4px 0 6px;",
                // Home / back.
                button {
                    style: "appearance: none; background: transparent; border: none; \
                            padding: 9px 0 7px; display: flex; flex-direction: column; \
                            align-items: center; gap: 3px; color: #52525b;",
                    onclick: move |_| on_home.call(()),
                    HomeIcon {}
                    RailLabel { text: "Home" }
                }
                div { style: "height: 1px; background: #1b1b1f; margin: 2px 12px 4px;" }
                for (p, label) in [
                    (MobilePage::Scenes, "Scenes"),
                    (MobilePage::Control, "Control"),
                    (MobilePage::Audio, "Audio"),
                ] {
                    {
                        let active = page() == p;
                        rsx! {
                            button {
                                style: format!(
                                    "position: relative; appearance: none; border: none; \
                                     background: {}; padding: 11px 0 9px; display: flex; \
                                     flex-direction: column; align-items: center; gap: 4px; color: {};",
                                    if active { "#101821" } else { "transparent" },
                                    if active { "#38bdf8" } else { "#52525b" },
                                ),
                                onclick: move |_| page.set(p),
                                // Accent LED on the inner (content-side) edge.
                                if active {
                                    span {
                                        style: "position: absolute; right: 0; top: 8px; bottom: 8px; \
                                                width: 3px; border-radius: 2px 0 0 2px; background: #38bdf8; \
                                                box-shadow: 0 0 8px #38bdf8, 0 0 2px #38bdf8;",
                                    }
                                }
                                match p {
                                    MobilePage::Scenes => rsx! { ScenesIcon {} },
                                    MobilePage::Control => rsx! { ControlIcon {} },
                                    MobilePage::Audio => rsx! { AudioIcon {} },
                                }
                                RailLabel { text: label }
                            }
                        }
                    }
                }
                div { style: "flex: 1;" }
                // Engine status LED — live/idle. (Tempo lives on the perform
                // grid's Tap Tempo, so no BPM readout here.)
                div {
                    style: "display: flex; flex-direction: column; align-items: center; gap: 4px; \
                            padding-top: 8px; border-top: 1px solid #1b1b1f; margin: 0 12px;",
                    span {
                        style: format!(
                            "width: 7px; height: 7px; border-radius: 999px; background: {}; box-shadow: 0 0 6px {};",
                            if running { "#22c55e" } else { "#3f3f46" },
                            if running { "#22c55e88" } else { "transparent" },
                        )
                    }
                    span {
                        style: "font-size: 7px; font-weight: 600; color: #3f3f46; letter-spacing: 0.14em;",
                        if running { "LIVE" } else { "IDLE" }
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

    tracing::info!(
        stacks = perf.stacks.len(),
        mode = perf.perform_mode,
        songs = perf.songs.len(),
        profile = %perf.profile_name,
        rig_ctx = rig.is_some(),
        "scenes render"
    );
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

// ── Rail glyphs ─────────────────────────────────────────────────────────────
// Stroke-based line icons drawn from the rig's own world; they inherit the
// button's `color` via `currentColor`, so active/inactive tint is free.

/// Uppercase micro-caps rail label — a rack-panel legend.
#[component]
fn RailLabel(text: &'static str) -> Element {
    rsx! {
        span {
            style: "font-size: 8px; font-weight: 600; letter-spacing: 0.1em; \
                    text-transform: uppercase; color: currentColor;",
            "{text}"
        }
    }
}

/// Back-to-home chevron.
#[component]
fn HomeIcon() -> Element {
    rsx! {
        svg {
            width: "22", height: "22", view_box: "0 0 24 24", fill: "none",
            stroke: "currentColor", stroke_width: "1.75",
            stroke_linecap: "round", stroke_linejoin: "round",
            path { d: "M14 6 L8 12 L14 18" }
        }
    }
}

/// Scenes: the footswitch grid itself (6 pads, floor-unit layout).
#[component]
fn ScenesIcon() -> Element {
    rsx! {
        svg {
            width: "22", height: "22", view_box: "0 0 24 24", fill: "none",
            stroke: "currentColor", stroke_width: "1.6",
            rect { x: "3.5", y: "3.5", width: "7", height: "5", rx: "1.4" }
            rect { x: "13.5", y: "3.5", width: "7", height: "5", rx: "1.4" }
            rect { x: "3.5", y: "9.5", width: "7", height: "5", rx: "1.4" }
            rect { x: "13.5", y: "9.5", width: "7", height: "5", rx: "1.4" }
            rect { x: "3.5", y: "15.5", width: "7", height: "5", rx: "1.4" }
            rect { x: "13.5", y: "15.5", width: "7", height: "5", rx: "1.4" }
        }
    }
}

/// Control: a fader stack (the instrument panel — chain, params, meters).
#[component]
fn ControlIcon() -> Element {
    rsx! {
        svg {
            width: "22", height: "22", view_box: "0 0 24 24", fill: "none",
            stroke: "currentColor", stroke_width: "1.6", stroke_linecap: "round",
            line { x1: "3", y1: "6.5", x2: "21", y2: "6.5" }
            circle { cx: "8", cy: "6.5", r: "2.4", fill: "currentColor", stroke: "none" }
            line { x1: "3", y1: "12", x2: "21", y2: "12" }
            circle { cx: "15.5", cy: "12", r: "2.4", fill: "currentColor", stroke: "none" }
            line { x1: "3", y1: "17.5", x2: "21", y2: "17.5" }
            circle { cx: "11", cy: "17.5", r: "2.4", fill: "currentColor", stroke: "none" }
        }
    }
}

/// Audio: speaker + sound waves (device I/O).
#[component]
fn AudioIcon() -> Element {
    rsx! {
        svg {
            width: "22", height: "22", view_box: "0 0 24 24", fill: "none",
            stroke: "currentColor", stroke_width: "1.6",
            stroke_linecap: "round", stroke_linejoin: "round",
            path { d: "M4 9 H7 L11 5.5 V18.5 L7 15 H4 Z" }
            path { d: "M15 9.5 A4 4 0 0 1 15 14.5" }
            path { d: "M17.5 7 A7.5 7.5 0 0 1 17.5 17" }
        }
    }
}
