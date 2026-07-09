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
    /// Full-screen footswitch grid (Preset/Profile/Setlist select this
    /// view AND the grid's perform mode).
    Perform,
    /// Integration layer (DAW sync, external control) — landing here.
    Session,
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
    let palette_open = use_signal(|| false);
    let mut right_open = use_signal(|| true);

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
        Some(s) => {
            // Folder-as-main naming: the stack IS the sound; variations
            // show their short name ("CLEAN · Verb", plain "CLEAN" on main).
            let patch = &s.current_patch;
            let lower = patch.to_lowercase();
            let sl = s.name.to_lowercase();
            let short = if lower == sl || lower == format!("{sl} default") || patch == "Default" {
                String::new()
            } else if lower.starts_with(&sl) && patch.len() > s.name.len() {
                patch[s.name.len()..].trim().to_string()
            } else {
                patch.clone()
            };
            if short.is_empty() {
                s.name.to_uppercase()
            } else {
                format!("{} · {}", s.name.to_uppercase(), short)
            }
        }
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
        div {
            class: "flex flex-col h-full bg-background text-foreground outline-none",
            tabindex: "0",
            // Grab focus on mount so Cmd/Ctrl+P works before any click.
            onmounted: move |e| {
                spawn(async move {
                    let _ = e.data().set_focus(true).await;
                });
            },
            // Cmd/Ctrl+P: the command palette. Everything else: the
            // keymap (keymap.styx) — "ctrl+1" strings → rig actions.
            onkeydown: {
                let mut palette_open = palette_open;
                let rig = rig.clone();
                let bindings = perf_now.key_bindings.clone();
                move |e: KeyboardEvent| {
                    let mods = e.modifiers();
                    if e.key() == Key::Character("p".to_string())
                        && (mods.ctrl() || mods.meta())
                    {
                        e.prevent_default();
                        palette_open.toggle();
                        return;
                    }
                    if palette_open() {
                        return; // the palette owns the keyboard while open
                    }
                    // Normalize the pressed combo to "ctrl+shift+x" form.
                    let key_name = match e.key() {
                        Key::Character(c) => if c == " " { "space".to_string() } else { c.to_lowercase() },
                        Key::ArrowLeft => "arrowleft".into(),
                        Key::ArrowRight => "arrowright".into(),
                        Key::ArrowUp => "arrowup".into(),
                        Key::ArrowDown => "arrowdown".into(),
                        Key::Enter | Key::Escape | Key::Tab => return,
                        k => format!("{k:?}").to_lowercase(),
                    };
                    let mut combo = String::new();
                    if mods.ctrl() {
                        combo.push_str("ctrl+");
                    }
                    if mods.meta() {
                        combo.push_str("meta+");
                    }
                    if mods.alt() {
                        combo.push_str("alt+");
                    }
                    if mods.shift() {
                        combo.push_str("shift+");
                    }
                    combo.push_str(&key_name);
                    let hit = bindings.iter().find(|b| {
                        // meta and ctrl are interchangeable (mac ⌘ = ctrl).
                        b.keys.eq_ignore_ascii_case(&combo)
                            || b.keys.replace("ctrl+", "meta+").eq_ignore_ascii_case(&combo)
                    });
                    if let (Some(b), Some(r)) = (hit, rig.clone()) {
                        if let Some(effect) = crate::palette::effect_from_action(&b.action) {
                            e.prevent_default();
                            crate::palette::execute(r, effect, String::new());
                        }
                    }
                }
            },
            crate::palette::CommandPalette { model: perf_now.clone(), open: palette_open }
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

                // Play group: Preset / Profile / Setlist — jumps to the
                // perform grid in that mode (synced to every remote).
                div { class: "flex items-center rounded-md border border-border bg-background/40 p-0.5 gap-0.5 ml-1",
                    for (pm, label) in [(0u32, "Preset"), (1, "Profile"), (2, "Setlist")] {
                        button {
                            key: "{label}",
                            // The play mode is always one of the three —
                            // highlight it regardless of which work view is
                            // up (brighter when the grid itself is showing).
                            class: if perf_now.perform_mode == pm {
                                "rounded px-2.5 py-1 text-xs font-semibold bg-accent text-accent-foreground"
                            } else {
                                "rounded px-2.5 py-1 text-xs text-muted-foreground hover:text-foreground"
                            },
                            onclick: {
                                let rig = rig.clone();
                                move |_| {
                                    if let Some(r) = rig.clone() {
                                        spawn(async move { let _ = r.set_perform_mode(pm).await; });
                                    }
                                }
                            },
                            "{label}"
                        }
                    }
                }
                // Work group: Routing / Control / Session.
                div { class: "flex items-center rounded-md border border-border bg-background/40 p-0.5 gap-0.5 ml-1",
                    for (m, label) in [
                        (Mode::Routing, "Routing"),
                        (Mode::Control, "Control"),
                        (Mode::Session, "Session"),
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

                // Command palette (also Cmd/Ctrl+P).
                button {
                    class: "flex items-center justify-center h-7 px-2 rounded-md border border-border text-muted-foreground hover:text-foreground text-[10px] font-mono",
                    title: "Command palette",
                    onclick: {
                        let mut palette_open = palette_open;
                        move |_| palette_open.toggle()
                    },
                    "⌘P"
                }

                // Reload the styx rig library (external edits: text
                // editor, LLM, git pull).
                button {
                    class: "flex items-center justify-center w-7 h-7 rounded-md border border-border text-muted-foreground hover:text-foreground text-sm",
                    title: "Reload the rig library (styx files)",
                    onclick: {
                        let rig = rig.clone();
                        move |_| {
                            if let Some(r) = rig.clone() {
                                spawn(async move { let _ = r.reload_library().await; });
                            }
                        }
                    },
                    "↻"
                }

                // Capture a real calibration DI from the live guitar —
                // play for ~15 s after tapping; the library re-measures.
                button {
                    class: "flex items-center justify-center w-7 h-7 rounded-md border border-border text-muted-foreground hover:text-foreground text-[10px] font-bold",
                    title: "Record 15 s of your guitar as the loudness-calibration reference",
                    onclick: {
                        let rig = rig.clone();
                        move |_| {
                            if let Some(r) = rig.clone() {
                                spawn(async move { let _ = r.capture_di_reference(15).await; });
                            }
                        }
                    },
                    "DI"
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
                        // Routing / Control / Session share the layout: the
                        // page on top (~2/3), the switch grid docked beneath.
                        div { class: "flex flex-col gap-3 h-full min-h-0 overflow-hidden",
                            div {
                                class: "min-h-0 flex flex-col overflow-hidden",
                                style: "flex: 2 1 0%;",
                                if mode() == Mode::Routing {
                                    crate::grid::RigGraph { blocks: blocks() }
                                } else if mode() == Mode::Session {
                                    // The session domain's performance view —
                                    // songs, sections, charts + chords — fed
                                    // by the standalone session engine (the
                                    // web shell owns the :3030 stream).
                                    div { class: "h-full min-h-0 overflow-hidden rounded-xl border border-border bg-card",
                                        session_ui::PerformanceLayout {}
                                    }
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
                } else {
                    div { class: "flex items-center justify-center h-full",
                        span { class: "text-sm text-muted-foreground italic", "Connecting to rig…" }
                    }
                }
                }
                // The setlist/parts sidebar only matters in Setlist mode.
                if right_open() && perf_now.perform_mode == 2 {
                    crate::sidebars::RightSidebar { model: perf_now.clone() }
                }
            }
        }

        // Model-driven: the footswitch (hold tap-tempo), any remote, or
        // the grid tile toggles it for everyone.
        if perf_now.tuner_visible {
            crate::perform::TunerOverlay {
                on_close: {
                    let rig = rig.clone();
                    move |_| {
                        if let Some(r) = rig.clone() {
                            spawn(async move { let _ = r.toggle_tuner().await; });
                        }
                    }
                },
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
