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
use crate::settings::{AudioSettingsBridge, AudioSettingsModal};
use crate::state::use_rig_state;

/// Top-level UI mode — MainStage-style pages.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Wire the rig: the zoomable module/wire graph.
    Routing,
    /// Play & shape it: the control surface (default).
    Control,
    /// Full-screen footswitch grid.
    Perform,
    /// The set: songs + sections, full page.
    Setlist,
}

/// The remote rig UI. Prop-less: everything arrives via context
/// (`RigClient`, `RigStreamClient`, `AudioSettingsClient`).
#[component]
pub fn GuitarRigRemote() -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let settings = use_hook(try_consume_context::<AudioSettingsClient>);
    let state = use_rig_state();

    // Control is home; Routing is where the wiring lives; Perform is the
    // stage view; Setlist manages the set (toggle away if unused).
    let mut mode = use_signal(|| Mode::Control);
    let mut audio_open = use_signal(|| false);
    let mut left_open = use_signal(|| true);
    let mut right_open = use_signal(|| true);
    let mut tuner_open = use_signal(|| false);

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

    // The five rig controls, shared by the standalone Perform view and the
    // Edit view's bottom dock.
    let controls = rig.clone().map(|r| {
        (
            Callback::new({
                let r = r.clone();
                move |i: usize| {
                    let r = r.clone();
                    spawn(async move { let _ = r.press_stack(i as u32).await; });
                }
            }),
            Callback::new({
                let r = r.clone();
                move |_: ()| {
                    let r = r.clone();
                    spawn(async move { let _ = r.toggle_fx().await; });
                }
            }),
            Callback::new({
                let r = r.clone();
                move |_: ()| {
                    let r = r.clone();
                    spawn(async move { let _ = r.toggle_boost().await; });
                }
            }),
            Callback::new({
                let r = r.clone();
                move |_: ()| {
                    let r = r.clone();
                    spawn(async move { let _ = r.cycle_boost().await; });
                }
            }),
            Callback::new({
                let r = r.clone();
                move |_: ()| {
                    let r = r.clone();
                    spawn(async move { let _ = r.tap_tempo().await; });
                }
            }),
            Callback::new({
                let r = r.clone();
                move |_: ()| {
                    let r = r.clone();
                    spawn(async move { let _ = r.prev_song().await; });
                }
            }),
            Callback::new({
                let r = r.clone();
                move |_: ()| {
                    let r = r.clone();
                    spawn(async move { let _ = r.next_song().await; });
                }
            }),
            Callback::new({
                let r = r.clone();
                move |i: usize| {
                    let r = r.clone();
                    spawn(async move { let _ = r.select_song(i as u32).await; });
                }
            }),
        )
    });

    rsx! {
        div { class: "flex flex-col h-full bg-background text-foreground",
            // Top bar
            header {
                class: "flex items-center gap-2 px-3 py-2 border-b border-border bg-card",

                // Sidebar toggles bookend the bar: presets left, songs right.
                Button {
                    variant: if left_open() { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                    is_icon_button: true,
                    aria_label: "Toggle preset sidebar".to_string(),
                    on_click: move |_| left_open.toggle(),
                    "☰"
                }

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

                // View switcher: Routing | Control | Perform | Setlist.
                div { class: "flex items-center rounded-md border border-border overflow-hidden",
                    Button {
                        variant: if mode() == Mode::Routing { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                        on_click: move |_| mode.set(Mode::Routing),
                        "Routing"
                    }
                    Button {
                        variant: if mode() == Mode::Control { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                        on_click: move |_| mode.set(Mode::Control),
                        "Control"
                    }
                    Button {
                        variant: if mode() == Mode::Perform { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                        on_click: move |_| mode.set(Mode::Perform),
                        "Perform"
                    }
                    Button {
                        variant: if mode() == Mode::Setlist { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                        on_click: move |_| mode.set(Mode::Setlist),
                        "Setlist"
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
                if perf_now.boost_db != 0.0 {
                    span {
                        class: "text-[10px] font-bold uppercase tracking-wider rounded px-1.5 py-0.5 mr-1",
                        style: "background-color: #fafafa; color: #0a0a0a;",
                        if perf_now.boost_db < 0.0 {
                            "Cut −{-perf_now.boost_db as i32} dB"
                        } else {
                            "Boost +{perf_now.boost_db as i32} dB"
                        }
                    }
                }

                if connected {
                    div { class: "flex items-center gap-2 mr-2",
                        MeterPair { input: state.in_level.cloned(), output: state.out_level.cloned() }
                    }
                }

                crate::control::MidiMonitorButton {}

                Button {
                    variant: if audio_open() { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                    on_click: move |_| audio_open.toggle(),
                    "Audio"
                }
                Button {
                    variant: if right_open() { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                    is_icon_button: true,
                    aria_label: "Toggle songs sidebar".to_string(),
                    on_click: move |_| right_open.toggle(),
                    "♪"
                }
            }

            // Body: [presets] [rig] [songs]
            div { class: "flex-1 min-h-0 flex flex-row overflow-hidden",
                if left_open() {
                    crate::sidebars::LeftSidebar { model: perf_now.clone() }
                }
                div { class: "flex-1 min-w-0 min-h-0 overflow-hidden p-4",
                if let Some((on_press, on_toggle_fx, on_toggle_boost, on_cycle_boost, on_tap_tempo, on_prev_song, on_next_song, on_select_song)) = controls {
                    if mode() == Mode::Perform {
                        PerformGrid {
                            model: perf(),
                            on_press,
                            on_toggle_fx,
                            on_toggle_boost,
                            on_cycle_boost,
                            on_tap_tempo,
                            on_prev_song,
                            on_next_song,
                            on_select_song,
                        }
                    } else if mode() == Mode::Setlist {
                        // Full-page set management: songs left, sections right.
                        div { class: "grid grid-cols-2 gap-4 h-full min-h-0",
                            div { class: "flex flex-col rounded-xl border border-border bg-card min-h-0 overflow-hidden",
                                div { class: "px-3 py-2 border-b border-border",
                                    span { class: "text-[10px] font-semibold uppercase tracking-wider text-muted-foreground", "Setlist" }
                                }
                                div { class: "flex-1 overflow-y-auto p-2 flex flex-col gap-1",
                                    for (i, song) in perf_now.songs.iter().enumerate() {
                                        {
                                            let name = song.clone();
                                            let is_current = i == perf_now.song_index as usize;
                                            rsx! {
                                                button {
                                                    key: "{i}",
                                                    class: if is_current {
                                                        "flex items-center gap-3 rounded-md px-3 py-2 text-left text-base font-bold bg-accent text-accent-foreground"
                                                    } else {
                                                        "flex items-center gap-3 rounded-md px-3 py-2 text-left text-base text-muted-foreground hover:bg-accent/40"
                                                    },
                                                    onclick: move |_| on_select_song.call(i),
                                                    span { class: "font-mono text-xs opacity-60 w-5", "{i + 1}" }
                                                    span { class: "truncate", "{name}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            div { class: "flex flex-col rounded-xl border border-border bg-card min-h-0 overflow-hidden",
                                div { class: "px-3 py-2 border-b border-border",
                                    span { class: "text-[10px] font-semibold uppercase tracking-wider text-muted-foreground", "Song Sections" }
                                }
                                div { class: "p-3 grid grid-cols-2 gap-2 content-start overflow-y-auto",
                                    for (i, section) in perf_now.sections.iter().enumerate() {
                                        {
                                            let name = section.clone();
                                            let is_current = i == perf_now.section_index as usize;
                                            let rig2 = rig.clone();
                                            rsx! {
                                                button {
                                                    key: "{i}",
                                                    class: if is_current {
                                                        "rounded-md px-3 py-4 text-sm font-bold bg-accent text-accent-foreground"
                                                    } else {
                                                        "rounded-md px-3 py-4 text-sm text-muted-foreground border border-border hover:bg-accent/40"
                                                    },
                                                    onclick: move |_| {
                                                        if let Some(r) = rig2.clone() {
                                                            spawn(async move { let _ = r.select_section(i as u32).await; });
                                                        }
                                                    },
                                                    "{name}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        // Routing & Control share the layout: the page on top
                        // (~2/3), the live footswitch surface docked beneath.
                        div { class: "flex flex-col gap-3 h-full min-h-0 overflow-hidden",
                            div {
                                class: "min-h-0 flex flex-col overflow-hidden",
                                style: "flex: 2 1 0%;",
                                if mode() == Mode::Routing {
                                    crate::grid::RigGraph { blocks: blocks() }
                                } else {
                                    crate::control::ControlView {
                                        model: perf_now.clone(),
                                        state,
                                    }
                                }
                            }
                            div {
                                // A whisker of padding so tile rings render
                                // inside the clipping ancestor instead of
                                // being shaved off at the dock edges.
                                class: "min-h-0 p-1",
                                style: "flex: 1 1 0%;",
                                PerformGrid {
                                    model: perf(),
                                    on_press,
                                    on_toggle_fx,
                                    on_toggle_boost,
                                    on_cycle_boost,
                                    on_tap_tempo,
                                    on_prev_song,
                                    on_next_song,
                                    on_select_song,
                                }
                            }
                        }
                    }
                } else {
                    div { class: "flex items-center justify-center h-full",
                        span { class: "text-sm text-muted-foreground italic", "Connecting to rig…" }
                    }
                }
                }
                if right_open() {
                    crate::sidebars::RightSidebar { model: perf_now.clone() }
                }
            }
        }

        if tuner_open() {
            crate::perform::TunerOverlay { on_close: move |_| tuner_open.set(false) }
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
