//! The self-contained rig remote — top bar (profile, mode toggle, meters),
//! Perform footswitch grid, chain strip, and the audio-settings modal.
//!
//! This is the whole guitar rig UI a *remote* needs: it consumes only the
//! vox clients from context, so it mounts identically in the browser
//! (`apps/web`, WebSocket transport) and in any native shell (in-process
//! transport). The desktop `signal-ui` mounts its richer grid around the
//! same building blocks.

use dioxus::prelude::*;

use signal_guitar_proto::AudioPrefs;
use signal_guitar_proto::audio::AudioSettingsClient;
use signal_guitar_proto::rig::RigClient;

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
                class: "flex items-center gap-2 px-2 py-1.5 border-b border-border bg-card",

                // Sidebar toggles bookend the bar: presets left, songs right.
                button {
                    class: if left_open() {
                        "flex items-center justify-center w-7 h-7 rounded-md bg-accent text-accent-foreground text-sm"
                    } else {
                        "flex items-center justify-center w-7 h-7 rounded-md border border-border text-muted-foreground hover:text-foreground text-sm"
                    },
                    title: "Preset sidebar",
                    onclick: move |_| left_open.toggle(),
                    "☰"
                }

                // Identity block: status dot + rig/profile stack.
                div { class: "flex items-center gap-2 ml-1 mr-1",
                    span {
                        class: if running {
                            "w-2 h-2 rounded-full animate-pulse flex-shrink-0"
                        } else {
                            "w-2 h-2 rounded-full flex-shrink-0"
                        },
                        style: if running {
                            "background-color: #22c55e;"
                        } else {
                            "background-color: #ef4444;"
                        },
                        title: if running { "audio running" } else { "audio stopped" },
                    }
                    div { class: "flex flex-col leading-none gap-0.5",
                        span { class: "text-[9px] font-semibold uppercase tracking-[2px] text-muted-foreground",
                            "Guitar Rig"
                        }
                        span { class: "text-sm font-bold leading-none",
                            if !profile.is_empty() { "{profile}" } else { "— no profile —" }
                        }
                    }
                }

                // Active-patch lens — tinted by the active stack's color.
                div {
                    class: "flex items-center rounded-md px-2.5 py-1",
                    style: "background-color: {lens_bg}; color: {lens_fg};",
                    span { class: "text-xs font-bold tracking-wide whitespace-nowrap", "{lens_label}" }
                }

                // View switcher — one segmented control.
                div { class: "flex items-center rounded-md border border-border bg-background/40 p-0.5 gap-0.5 ml-1",
                    for (m, label) in [
                        (Mode::Routing, "Routing"),
                        (Mode::Control, "Control"),
                        (Mode::Perform, "Perform"),
                        (Mode::Setlist, "Setlist"),
                    ] {
                        button {
                            key: "{label}",
                            class: if mode() == m {
                                "rounded px-2.5 py-1 text-xs font-semibold bg-accent text-accent-foreground"
                            } else {
                                "rounded px-2.5 py-1 text-xs text-muted-foreground hover:text-foreground"
                            },
                            onclick: move |_| mode.set(m),
                            "{label}"
                        }
                    }
                }

                div { class: "flex-1" }

                // Global switch states — visible in every mode.
                if perf_now.fx_bypass {
                    span {
                        class: "text-[10px] font-bold uppercase tracking-wider rounded px-1.5 py-0.5",
                        style: "background-color: #ec4899; color: #ffffff;",
                        "FX off"
                    }
                }
                if perf_now.boost_db != 0.0 {
                    span {
                        class: "text-[10px] font-bold uppercase tracking-wider rounded px-1.5 py-0.5",
                        style: "background-color: #fafafa; color: #0a0a0a;",
                        if perf_now.boost_db < 0.0 {
                            "Cut −{-perf_now.boost_db as i32} dB"
                        } else {
                            "Boost +{perf_now.boost_db as i32} dB"
                        }
                    }
                }

                crate::control::MidiMonitorButton {}

                button {
                    class: if audio_open() {
                        "flex items-center justify-center h-7 px-2.5 rounded-md bg-accent text-accent-foreground text-xs font-semibold"
                    } else {
                        "flex items-center justify-center h-7 px-2.5 rounded-md border border-border text-muted-foreground hover:text-foreground text-xs font-semibold"
                    },
                    onclick: move |_| audio_open.toggle(),
                    "Audio"
                }
                button {
                    class: if right_open() {
                        "flex items-center justify-center w-7 h-7 rounded-md bg-accent text-accent-foreground text-sm"
                    } else {
                        "flex items-center justify-center w-7 h-7 rounded-md border border-border text-muted-foreground hover:text-foreground text-sm"
                    },
                    title: "Songs sidebar",
                    onclick: move |_| right_open.toggle(),
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
                                div { class: "flex items-center gap-1 px-3 py-2 border-b border-border",
                                    span { class: "text-[10px] font-semibold uppercase tracking-wider text-muted-foreground mr-2", "Setlist" }
                                    for (si, set) in perf_now.setlists.iter().enumerate() {
                                        {
                                            let set = set.clone();
                                            let active = si == perf_now.setlist_index as usize;
                                            let rig2 = rig.clone();
                                            rsx! {
                                                button {
                                                    key: "{si}",
                                                    class: if active {
                                                        "rounded px-2 py-0.5 text-xs font-bold bg-accent text-accent-foreground"
                                                    } else {
                                                        "rounded px-2 py-0.5 text-xs text-muted-foreground border border-border hover:bg-accent/40"
                                                    },
                                                    onclick: move |_| {
                                                        if let Some(r) = rig2.clone() {
                                                            spawn(async move { let _ = r.select_setlist(si as u32).await; });
                                                        }
                                                    },
                                                    "{set}"
                                                }
                                            }
                                        }
                                    }
                                }
                                div { class: "flex-1 overflow-y-auto p-2 flex flex-col gap-1",
                                    for (i, song) in perf_now.songs.iter().enumerate() {
                                        {
                                            let name = song.name.clone();
                                            let meta = format!("{} · {} bpm", song.key, song.bpm);
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
                                                    span { class: "ml-auto font-mono text-xs opacity-70 flex-shrink-0", "{meta}" }
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
