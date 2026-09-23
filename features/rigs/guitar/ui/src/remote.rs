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
    #[expect(
        dead_code,
        reason = "not yet wired to a page switch — reserved for the Preset/Profile/Setlist full-screen perform view"
    )]
    Perform,
    /// The TONE3000 catalog: find a capture, download it, load it as a
    /// preset. Lives beside the rig rather than behind a file dialog
    /// because picking an amp is a playing decision, not a filing one.
    Tones,
    /// The rig as a tree, with a preset picker on every node that has a
    /// choice — blocks *and* the modules holding them. The chain views
    /// render a flat block list, which is the right shape for playing and
    /// has nowhere to put a module.
    Presets,
}

/// How much of the page the footswitch grid takes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Switches {
    /// The full grid: a third of the page, big tiles.
    Full,
    /// A short strip — both rows compact.
    Compact,
    /// No grid: the page gets all the height.
    Hidden,
}

impl Switches {
    fn next(self) -> Self {
        match self {
            Self::Full => Self::Compact,
            Self::Compact => Self::Hidden,
            Self::Hidden => Self::Full,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Full => "Full",
            Self::Compact => "Compact",
            Self::Hidden => "Hidden",
        }
    }
}

/// The remote rig UI. Prop-less: everything arrives via context
/// (`RigClient`, `RigStreamClient`, `AudioSettingsClient`).
#[component]
pub fn GuitarRigRemote() -> Element {
    // Drags that outlive their control: every knob, fader and threshold
    // below routes its moves through the root (Blitz has no pointer capture).
    let drag_bus = signal_widgets::DragBus::provide();
    // Menus drawn at the root, over everything: a panel that clips its
    // overflow cannot cut them off (Blitz has no `fixed`, no portals).
    signal_widgets::PopupHost::provide();
    // Callbacks made once per site, not once per render (see `stable`).
    let cbs = crate::stable::use_stable();
    let rig = use_hook(try_consume_context::<RigClient>);
    // Every parameter write below goes through one coalescing writer.
    crate::param_writer::use_param_writer(rig.clone());
    let settings = use_hook(try_consume_context::<AudioSettingsClient>);
    let state = use_rig_state();

    // Control is home; Routing is where the wiring lives; Perform is the
    // stage view; Setlist manages the set (toggle away if unused).
    let mut mode = use_signal(|| Mode::Control);
    let mut switches = use_signal(|| Switches::Full);
    let mut audio_open = use_signal(|| false);
    // One bar: this header replaces the app's (crumbs and window controls
    // included) instead of stacking under it. No-op outside the app.
    fts_chrome::use_bar_claim();
    let mut left_open = use_signal(|| true);
    let palette_open = use_signal(|| false);
    // The library picker, open on a kind — or closed.
    let library_open = use_signal(|| None::<crate::library::Kind>);
    use_context_provider(|| crate::library::OpenLibrary(library_open));
    // The view actions the palette and the keymap can ask for.
    let on_local = use_callback(move |e: crate::palette::Effect| {
        let mut library_open = library_open;
        match e {
            crate::palette::Effect::Browse(k) => library_open.set(Some(k)),
            crate::palette::Effect::CycleSwitches => switches.set(switches().next()),
            _ => {}
        }
    });

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
        let settings = settings;
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
        .and_then(std::clone::Clone::clone)
        .map(|d| AudioSettingsBridge {
            inputs: d.inputs,
            outputs: d.outputs,
            prefs: prefs(),
            on_save: apply,
        });

    let perf = state.perf;
    let blocks = state.blocks;
    let _connected = rig.is_some();
    // Presets and Tones only exist in Preset mode: leaving Preset mode while
    // on one lands on Control rather than a tab that is no longer there.
    use_effect(move || {
        if perf().perform_mode != 0 && matches!(mode(), Mode::Presets | Mode::Tones) {
            mode.set(Mode::Control);
        }
    });
    let perf_now = perf();

    // The five rig controls, shared by the standalone Perform view and the
    // Edit view's bottom dock.
    let controls = rig.clone().map(|r| {
        (
            cbs.cb({
                let r = r.clone();
                move |i: usize| {
                    let r = r.clone();
                    spawn(async move {
                        let _ = r.press_stack(i as u32).await;
                    });
                }
            }),
            cbs.cb({
                let r = r.clone();
                move |(): ()| {
                    let r = r.clone();
                    spawn(async move {
                        let _ = r.toggle_fx().await;
                    });
                }
            }),
            cbs.cb({
                let r = r.clone();
                move |(): ()| {
                    let r = r.clone();
                    spawn(async move {
                        let _ = r.toggle_boost().await;
                    });
                }
            }),
            cbs.cb({
                let r = r.clone();
                move |(): ()| {
                    let r = r.clone();
                    spawn(async move {
                        let _ = r.cycle_boost().await;
                    });
                }
            }),
            cbs.cb({
                let r = r.clone();
                move |(): ()| {
                    let r = r.clone();
                    spawn(async move {
                        let _ = r.tap_tempo().await;
                    });
                }
            }),
            cbs.cb({
                let r = r.clone();
                move |(): ()| {
                    let r = r.clone();
                    spawn(async move {
                        let _ = r.prev_song().await;
                    });
                }
            }),
            cbs.cb({
                let r = r.clone();
                move |(): ()| {
                    let r = r.clone();
                    spawn(async move {
                        let _ = r.next_song().await;
                    });
                }
            }),
            cbs.cb({
                let r = r;
                move |i: usize| {
                    let r = r.clone();
                    spawn(async move {
                        let _ = r.select_song(i as u32).await;
                    });
                }
            }),
        )
    });

    rsx! {
        // Every `fts_audio_ui` widget in the rig — the EQ's band popup and its
        // knobs among them — reads a `DragState` from context, and without it
        // the first one built panics with "Could not find context" and leaves
        // a half-constructed DOM that takes the window down a frame later with
        // an unrelated-looking unwrap deep in blitz. The plugin wraps its
        // editor root in this; the rig never did.
        //
        // At the ROOT, not around the EQ: it has to span everything for a
        // drag to end where it ends, since mouseup is captured here. `fill`:
        // the rig sits inside the app's chrome (the top bar), so it takes
        // its parent's box — the provider's default `100vw x 100vh` drew the
        // surface past the window and clipped its right and bottom edges.
        fts_audio_ui::drag::DragProvider { fill: true,
        div {
            class: "flex flex-col h-full bg-background text-foreground outline-none",
            // The library picker covers this box (Blitz has no `fixed`).
            style: "position: relative;",
            tabindex: "0",
            // Every pointer move bubbles here: the control holding a drag
            // follows it anywhere in the window, and a release ends it.
            onpointermove: move |e: PointerEvent| drag_bus.root_move(&e),
            onpointerup: move |_| drag_bus.root_up(),
            // Cmd/Ctrl+P: the command palette. Everything else: the
            // keymap (keymap.styx) — "ctrl+1" strings → rig actions.
            onkeydown: {
                let mut palette_open = palette_open;
                let rig = rig.clone();
                let bindings = perf_now.key_bindings.clone();
                let perf_mode = perf_now.perform_mode;
                move |e: KeyboardEvent| {
                    let mods = e.modifiers();
                    if e.key() == Key::Character("p".to_string())
                        && (mods.ctrl() || mods.meta())
                    {
                        e.prevent_default();
                        palette_open.toggle();
                        return;
                    }
                    // Cmd/Ctrl+L: the library, on what the mode plays from.
                    if e.key() == Key::Character("l".to_string())
                        && (mods.ctrl() || mods.meta())
                    {
                        e.prevent_default();
                        let mut library_open = library_open;
                        let at = crate::library::Kind::for_perform_mode(perf_mode);
                        library_open.set(if library_open().is_some() { None } else { Some(at) });
                        return;
                    }
                    if palette_open() || library_open().is_some() {
                        return; // the palette / library owns the keyboard while open
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
                    if let Some(effect) = hit.and_then(|b| crate::palette::effect_from_action(&b.action)) {
                        e.prevent_default();
                        if effect.is_local() {
                            on_local.call(effect);
                        } else if let Some(r) = rig.clone() {
                            crate::palette::execute(r, effect, String::new());
                        }
                    }
                }
            },
            // Where menus draw (see `PopupHost`): above everything via its
            // z-index, so first among the children is fine.
            signal_widgets::PopupLayer {}
            // The bar — the app's and the rig's in one: crumbs, the rig's own
            // controls, the window's drag space and controls.
            header {
                class: "flex items-center gap-2 px-2 border-b border-border bg-card select-none",
                style: "height: 40px; flex-shrink: 0; min-width: 0;",

                // macOS: close / minimise / zoom in the corner, as the
                // window's own title bar would put them.
                fts_chrome::TrafficLights {}

                // Sidebar toggles bookend the bar: presets left, songs right.
                button {
                    class: if left_open() {
                        "flex items-center justify-center w-7 h-7 rounded-md bg-accent text-accent-foreground text-sm"
                    } else {
                        "flex items-center justify-center w-7 h-7 rounded-md border border-border text-muted-foreground hover:text-foreground text-sm"
                    },
                    title: "Preset sidebar",
                    onclick: move |_| left_open.toggle(),
                    fts_chrome::Glyph { icon: fts_chrome::Icon::RailLeft, size: 15 }
                }

                // Where you are, and the way back (Signal ▾ ▸ Rigs ▸ Guitar ▾).
                fts_chrome::Crumbs { current_only: true }

                // Play group: Preset / Profile / Setlist — jumps to the
                // perform grid in that mode (synced to every remote).
                div { class: "flex items-center rounded-md border border-border bg-background/40 p-0.5 gap-0.5 ml-1",
                    for (pm, label, icon) in [
                        (0u32, "Preset", fts_chrome::Icon::Preset),
                        (1, "Profile", fts_chrome::Icon::Profile),
                        (2, "Setlist", fts_chrome::Icon::Setlist),
                    ] {
                        button {
                            key: "{label}",
                            title: if perf_now.perform_mode == pm { format!("{label} — click to browse") } else { label.to_string() },
                            style: "display: flex; align-items: center; gap: 5px;",
                            // The play mode is always one of the three —
                            // highlight it regardless of which work view is
                            // up (brighter when the grid itself is showing).
                            class: if perf_now.perform_mode == pm {
                                "rounded px-2.5 py-1 text-xs font-semibold bg-accent text-accent-foreground"
                            } else {
                                "rounded px-2.5 py-1 text-xs text-muted-foreground hover:text-foreground"
                            },
                            // A second click on the mode you are in opens the
                            // picker for it: the mode names what you play
                            // from, the picker is where you choose it.
                            onclick: {
                                let rig = rig.clone();
                                let current = perf_now.perform_mode;
                                let mut library_open = library_open;
                                move |_| {
                                    if current == pm {
                                        library_open.set(Some(crate::library::Kind::for_perform_mode(pm)));
                                    } else if let Some(r) = rig.clone() {
                                        spawn(async move { let _ = r.set_perform_mode(pm).await; });
                                    }
                                }
                            },
                            fts_chrome::Glyph { icon, size: 13 }
                            "{label}"
                        }
                    }
                }
                // The library: setlists, songs, profiles, patches, presets.
                button {
                    class: if library_open().is_some() {
                        "flex items-center h-7 px-2 rounded-md bg-accent text-accent-foreground text-xs font-semibold"
                    } else {
                        "flex items-center h-7 px-2 rounded-md border border-border text-muted-foreground hover:text-foreground text-xs"
                    },
                    style: "display: flex; align-items: center; gap: 5px;",
                    title: "Library (⌘L)",
                    onclick: {
                        let mut library_open = library_open;
                        let at = crate::library::Kind::for_perform_mode(perf_now.perform_mode);
                        move |_| library_open.set(if library_open().is_some() { None } else { Some(at) })
                    },
                    fts_chrome::Glyph { icon: fts_chrome::Icon::Browser, size: 13 }
                    "Library"
                }
                // The bar's slack moves the window (and double-click maximises).
                fts_chrome::DragSpace {}

                // Work views, on the right: they change with the mode on the left
                // (Presets / Tones exist only in Preset mode).
                div { class: "flex items-center rounded-md border border-border bg-background/40 p-0.5 gap-0.5 ml-1",
                    // Presets and Tones choose the sound, so they only exist
                    // in Preset mode; Routing and Control are always there.
                    for (m, label, icon) in [
                        (Mode::Routing, "Routing", fts_chrome::Icon::Routing),
                        (Mode::Control, "Control", fts_chrome::Icon::Control),
                        (Mode::Presets, "Presets", fts_chrome::Icon::Browser),
                        (Mode::Tones, "Tones", fts_chrome::Icon::Tones),
                    ]
                    .into_iter()
                    .filter(|(m, _, _)| perf_now.perform_mode == 0 || !matches!(m, Mode::Presets | Mode::Tones))
                    {
                        button {
                            key: "{label}",
                            title: "{label}",
                            style: "display: flex; align-items: center; gap: 5px;",
                            class: if mode() == m {
                                "rounded px-2.5 py-1 text-xs font-semibold bg-accent text-accent-foreground"
                            } else {
                                "rounded px-2.5 py-1 text-xs text-muted-foreground hover:text-foreground"
                            },
                            onclick: move |_| mode.set(m),
                            fts_chrome::Glyph { icon, size: 13 }
                            "{label}"
                        }
                    }
                }


                // The footswitch grid: full, a compact strip, or hidden —
                // click to cycle. Hidden gives the page all the height.
                button {
                    class: if switches() == Switches::Hidden {
                        "flex items-center h-7 px-2 rounded-md border border-border text-muted-foreground hover:text-foreground text-xs"
                    } else {
                        "flex items-center h-7 px-2 rounded-md bg-accent text-accent-foreground text-xs"
                    },
                    style: "display: flex; align-items: center; gap: 5px;",
                    title: "Switches: {switches().label()} — click for {switches().next().label()}",
                    onclick: move |_| switches.set(switches().next()),
                    fts_chrome::Glyph { icon: fts_chrome::Icon::Perform, size: 13 }
                    "Switches"
                }

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

                // A levelling pass (started from ⌘P): progress, then result.
                crate::sidebars::LevellingChip {}

                // Command palette (also Cmd/Ctrl+P).
                button {
                    class: "flex items-center justify-center h-7 px-2 rounded-md border border-border text-muted-foreground hover:text-foreground text-[10px] font-mono",
                    title: "Command palette",
                    onclick: {
                        let mut palette_open = palette_open;
                        move |_| palette_open.toggle()
                    },
                    fts_chrome::Glyph { icon: fts_chrome::Icon::Command, size: 13 }
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
                    fts_chrome::Glyph { icon: fts_chrome::Icon::Refresh, size: 14 }
                }

                // Indicators, not buttons: MIDI and audio at a glance, their
                // options behind a right-click or double-click.
                // The whole app's CPU, as a share of the machine.
                crate::meters::CpuMeter { perf: (state.dsp)() }
                crate::control::MidiIndicator {
                    on_settings: move |()| audio_open.set(true),
                }
                {
                    let is_running = (state.running)();
                    let rig_toggle = rig.clone();
                    let rig_di = rig.clone();
                    rsx! {
                        crate::indicators::Indicator {
                            label: "Audio".to_string(),
                            dot: if is_running { "#22c55e".to_string() } else { "#ef4444".to_string() },
                            title: if is_running { "Audio running".to_string() } else { "Audio stopped".to_string() },
                            // What the rig spends of its realtime budget,
                            // measured on the chain actually playing.
                            extra: rsx! {
                                div { style: "padding: 6px 8px 2px; border-top: 1px solid #1c1c21; margin-top: 4px;",
                                    crate::meters::DspReadout { perf: (state.dsp)() }
                                }
                            },
                            items: vec![
                                crate::indicators::IndicatorItem::new(
                                    "Audio settings…",
                                    cbs.cb(move |()| audio_open.set(true)),
                                ),
                                crate::indicators::IndicatorItem::new(
                                    if is_running { "Stop audio" } else { "Start audio" },
                                    cbs.cb(move |()| {
                                        if let Some(r) = rig_toggle.clone() {
                                            spawn(async move {
                                                let _ = if is_running { r.stop().await } else { r.start().await };
                                            });
                                        }
                                    }),
                                ),
                                // Play for ~15 s after choosing it; the
                                // library re-measures loudness against it.
                                crate::indicators::IndicatorItem::new(
                                    "Record DI reference (15 s)",
                                    cbs.cb(move |()| {
                                        if let Some(r) = rig_di.clone() {
                                            spawn(async move { let _ = r.capture_di_reference(15).await; });
                                        }
                                    }),
                                ),
                            ],
                        }
                    }
                }

                // Settings + minimise / maximise / close.
                fts_chrome::WindowCluster {}
            }

            // Body: [presets] [rig] [songs]
            //
            // Not drawn while the library or the palette covers it — mounted, so nothing
            // loses its state, but out of layout and paint. The rig redraws
            // every frame (meters, visualisers), and winit's macOS loop runs
            // redraws ahead of everything else: with the rig *and* the
            // library to lay out each frame, the frame outgrew its budget and
            // Dioxus never got a turn to apply anything — the library never
            // appeared and the window looked frozen.
            div {
                class: "flex-1 min-h-0 flex flex-row overflow-hidden",
                style: if library_open().is_some() || palette_open() { "display: none;" } else { "" },
                // The left sidebar follows the mode: the pool for Preset,
                // the profile tree for Profile, the set and its songs for
                // Setlist.
                if left_open() {
                    if perf_now.perform_mode == 2 {
                        crate::setlist_bar::SetlistSidebar {
                            model: perf_now.clone(),
                            on_browse: move |k: crate::library::Kind| {
                                let mut library_open = library_open;
                                library_open.set(Some(k));
                            },
                        }
                    } else if perf_now.perform_mode == 0 {
                        crate::preset_bar::PresetSidebar {
                            model: perf_now.clone(),
                            on_browse: move |k: crate::library::Kind| {
                                let mut library_open = library_open;
                                library_open.set(Some(k));
                            },
                            on_tones: move |()| mode.set(Mode::Tones),
                        }
                    } else {
                        crate::sidebars::LeftSidebar { model: perf_now.clone() }
                    }
                }
                div { class: "flex-1 min-w-0 min-h-0 overflow-hidden", style: "padding: 0 10px 10px;",
                if let Some((on_press, on_toggle_fx, on_toggle_boost, on_cycle_boost, on_tap_tempo, on_prev_song, on_next_song, on_select_song)) = controls {
                        // Routing / Control / Session share the layout: the
                        // page on top (~2/3), the switch grid docked beneath.
                        div { class: "flex flex-col gap-3 h-full min-h-0 overflow-hidden",
                            div {
                                class: "min-h-0 flex flex-col overflow-hidden",
                                style: "flex: 3 1 0%; min-height: 0; display: flex; flex-direction: column; overflow: hidden;",
                                if mode() == Mode::Routing {
                                    crate::grid::RigGraph {
                                        blocks: blocks(),
                                        nodes: state.nodes.read().clone(),
                                    }
                                } else if mode() == Mode::Presets {
                                    {
                                        // The tree is shared (signal-widgets);
                                        // this supplies what to do with it,
                                        // since every rig has its own wire.
                                        let select = rig.clone();
                                        let save = rig.clone();
                                        let replace = rig.clone();
                                        rsx! {
                                            div { class: "h-full min-h-0 overflow-hidden rounded-xl border border-border bg-card",
                                                signal_widgets::PresetTree {
                                                    nodes: state.nodes.read().clone(),
                                                    on_select: move |(node, preset): (String, String)| {
                                                        let Some(rig) = select.clone() else { return };
                                                        spawn(async move {
                                                            let _ = rig.select_preset(node, preset).await;
                                                        });
                                                    },
                                                    on_save: move |(node, name): (String, String)| {
                                                        let Some(rig) = save.clone() else { return };
                                                        spawn(async move {
                                                            let _ = rig.save_preset(node, name).await;
                                                        });
                                                    },
                                                    on_replace: move |(node, with): (String, String)| {
                                                        let Some(rig) = replace.clone() else { return };
                                                        spawn(async move {
                                                            let _ = rig.replace_node(node, with).await;
                                                        });
                                                    },
                                                }
                                            }
                                        }
                                    }
                                } else if mode() == Mode::Tones {
                                    // A downloaded capture goes to the engine
                                    // as what it is — the browser reports the
                                    // gear and the tone it came from, and the
                                    // rig decides whether that is an amp
                                    // preset or a pedal on the drive board.
                                    {
                                        let rig = rig.clone();
                                        rsx! {
                                            div { class: "h-full min-h-0 overflow-hidden rounded-xl border border-border bg-card",
                                                signal_tone3000_ui::ToneBrowser {
                                                    on_loaded: move |i: signal_tone3000_ui::ToneImport| {
                                                        let rig = rig.clone();
                                                        spawn(async move {
                                                            if let Some(r) = rig {
                                                                let _ = r
                                                                    .import_capture(i.name, i.path, i.gear, i.group)
                                                                    .await;
                                                            }
                                                        });
                                                    },
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    crate::control::ControlView {
                                        model: perf_now.clone(),
                                        state,
                                    }
                                }
                            }
                            if switches() != Switches::Hidden {
                            div {
                                // A whisker of padding so tile rings render
                                // inside the clipping ancestor instead of
                                // being shaved off at the dock edges.
                                class: "min-h-0 p-1",
                                style: if switches() == Switches::Compact {
                                    "flex: 0 0 116px;"
                                } else {
                                    "flex: 1.2 1 0%;"
                                },
                                PerformGrid {
                                    compact: switches() == Switches::Compact,
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
            }
            // Last, so they paint over the bar and the body.
            crate::library::LibraryPicker { model: perf_now.clone(), open: library_open }
            crate::palette::CommandPalette {
                model: perf_now.clone(),
                open: palette_open,
                on_local: move |e| on_local.call(e),
            }
        }

        // Model-driven: the footswitch (hold tap-tempo), any remote, or
        // the grid tile toggles it for everyone.
        if perf_now.tuner_visible {
            crate::perform::TunerOverlay {
                on_close: {
                    let rig = rig.clone();
                    move |()| {
                        if let Some(r) = rig.clone() {
                            spawn(async move { let _ = r.toggle_tuner().await; });
                        }
                    }
                },
            }
        }

        // Audio settings modal
        if audio_open() {
            if let Some(bridge) = live_bridge {
                AudioSettingsModal {
                    bridge: bridge,
                    on_close: move |()| audio_open.set(false),
                }
            }
        }
        }
    }
}
