//! Guitar rig showcase — the default guitar rig *template* in the interactive
//! module/wire grid, with a left-hand preset list, a top bar with sidebar
//! toggles, and audio settings (quick input picker + full modal).
//!
//! This is the static-RSX entry point the desktop app builds on. It takes the
//! canonical [`guitar_rig_template`](signal_proto::defaults::guitar::guitar_rig_template)
//! (11 modules — Source, Dynamics, Special, Drive, Volume, Pre-FX, Amp,
//! Modulation, Time, Motion, Mastering — Amp and Time using parallel splits)
//! and renders every block as a dashed template placeholder in [`RigGridPanel`].
//!
//! Interactive controls use the themed `lumen_blocks` components (Button,
//! Dropdown) so they follow the active theme.

use dioxus::prelude::*;
use lumen_blocks::components::button::{Button, ButtonVariant};
use lumen_blocks::components::dropdown::{Dropdown, DropdownContent, DropdownItem, DropdownTrigger};

use signal_proto::defaults::guitar::guitar_rig_template;
use signal::{BlockType, Preset, Signal};
use signal_browser::grid_conversion::template_to_grid_slots;

use signal_guitar_proto::audio::AudioSettingsClient;
use signal_guitar_proto::rig::{RigClient, RigEvent, RigStreamClient};
use signal_guitar_ui::{MeterPair, PerformGrid, meter_level};

use crate::components::{block_color, BlockColor, GridSelection, GridSlot};
use crate::views::{
    AudioPrefs, AudioSettingsBridge, AudioSettingsModal, LiveBlock, PerformanceModel,
    RigGridPanel,
};

/// Stable Uuid derived from a block's string id (so the grid keeps a consistent
/// identity across polls without re-diffing every frame).
fn slot_uuid(id: &str) -> uuid::Uuid {
    use std::hash::{Hash, Hasher};
    let mut h1 = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut h1);
    let mut h2 = std::collections::hash_map::DefaultHasher::new();
    (id, 0x9e37_79b9u64).hash(&mut h2);
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&h1.finish().to_le_bytes());
    bytes[8..].copy_from_slice(&h2.finish().to_le_bytes());
    uuid::Uuid::from_bytes(bytes)
}

/// Overlay the live rig's active blocks onto the guitar-rig-template grid slots:
/// the template is the full canvas (every module + slot); a live block *resolves*
/// its matching slot (matched by name) — filling in real bypass state, its param,
/// and its live id (in `preset_id`) for control. Unmatched slots stay dashed
/// template placeholders.
fn resolve_template(base: &[GridSlot], live: &[LiveBlock]) -> Vec<GridSlot> {
    let mut slots = base.to_vec();
    for slot in slots.iter_mut() {
        let Some(slot_name) = slot.block_preset_name.clone() else {
            continue;
        };
        if let Some(b) = live
            .iter()
            .find(|b| b.name.eq_ignore_ascii_case(&slot_name) && b.block_type == slot.block_type)
        {
            slot.is_template = false;
            slot.bypassed = b.bypassed;
            slot.id = slot_uuid(&b.id);
            slot.preset_id = Some(b.id.clone());
            slot.plugin_name = Some(b.name.clone());
            slot.parameters = b
                .param_name
                .as_ref()
                .map(|n| vec![(n.clone(), b.param_value)])
                .unwrap_or_default();
        }
    }
    slots
}

/// Top-level UI mode.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Build the rig — module/wire grid + preset sidebar.
    Edit,
    /// Play the rig — footswitch folder grid.
    Perform,
}

/// Guitar-relevant block types offered in the preset sidebar selector.
const PRESET_BLOCK_TYPES: &[(BlockType, &str)] = &[
    (BlockType::Amp, "Amp"),
    (BlockType::Drive, "Drive"),
    (BlockType::Reverb, "Reverb"),
    (BlockType::Delay, "Delay"),
    (BlockType::Compressor, "Comp"),
    (BlockType::Eq, "EQ"),
];

/// Display label for an audio-device value ("" → "Default input").
fn input_label(name: &str) -> String {
    if name.is_empty() {
        "Default input".to_string()
    } else {
        name.to_string()
    }
}

/// Static showcase: preset list + the default guitar rig template grid.
#[component]
pub fn GuitarRigView() -> Element {
    let module_count = 11;
    // The full guitar-rig template — the grid canvas that the live rig resolves into.
    let base_slots = use_hook(|| template_to_grid_slots(&guitar_rig_template()));

    // UI mode + sidebar visibility + audio-settings modal state.
    let mut mode = use_signal(|| Mode::Edit);
    let mut left_open = use_signal(|| true);
    let mut right_open = use_signal(|| false);
    let mut audio_open = use_signal(|| false);

    // Rig + audio-settings service clients (host-provided vox connections —
    // in-process on desktop, WebSocket from a browser; the view can't tell).
    let rig = use_hook(try_consume_context::<RigClient>);
    let rig_stream = use_hook(try_consume_context::<RigStreamClient>);
    let settings = use_hook(try_consume_context::<AudioSettingsClient>);

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

    // Shared, editable prefs — both the quick picker and the modal read/write
    // this one signal so they never disagree. Seeded from the persisted prefs
    // once the settings service answers.
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

    // Apply = update shared state, persist, and re-open the live rig so device /
    // buffer changes take effect immediately (the engine is always running).
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

    // A per-render bridge carrying the *current* shared prefs + device lists,
    // handed to the modal + quick picker so their edits round-trip through
    // `apply`.
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

    let mut running = use_signal(|| false);
    let mut in_level = use_signal(|| 0.0f64);
    let mut out_level = use_signal(|| 0.0f64);
    let mut perf = use_signal(PerformanceModel::default);
    // The live FX chain of the active patch, and the currently-selected block.
    let mut live_blocks = use_signal(Vec::<LiveBlock>::new);
    let mut selected_block = use_signal(|| None::<String>);

    // Seed the perf model + chain once (the event stream only carries
    // *changes*; a fresh subscriber needs the current state to start from).
    {
        let rig = rig.clone();
        use_future(move || {
            let rig = rig.clone();
            async move {
                let Some(rig) = rig else { return };
                if let Ok(s) = rig.status().await {
                    running.set(s.running);
                    in_level.set(meter_level(s.input_peak));
                    out_level.set(meter_level(s.output_peak));
                }
                if let Ok(p) = rig.perf().await {
                    perf.set(p);
                }
                if let Ok(c) = rig.chain().await {
                    live_blocks.set(c);
                }
            }
        });
    }

    // Live updates: subscribe to the rig's `#[subscribe]` event stream —
    // meters at meter rate, perf/chain on mutation. No polling; a remote
    // (WebSocket) GUI gets the same pushes.
    {
        let rig_stream = rig_stream.clone();
        architect::use_stream(
            move |sink| {
                let rig_stream = rig_stream.clone();
                async move {
                    match rig_stream {
                        Some(s) => s.events(sink).await.is_ok(),
                        None => false,
                    }
                }
            },
            move |ev: RigEvent| {
                // Signals are Copy; rebind mutably inside the `Fn` closure.
                let (mut running, mut in_level, mut out_level, mut perf, mut live_blocks) =
                    (running, in_level, out_level, perf, live_blocks);
                match ev {
                    RigEvent::Status(s) => {
                        running.set(s.running);
                        in_level.set(meter_level(s.input_peak));
                        out_level.set(meter_level(s.output_peak));
                    }
                    RigEvent::Perf(p) => perf.set(p),
                    RigEvent::Chain(c) => live_blocks.set(c),
                    RigEvent::Spectrum(_) | RigEvent::CompWave(..) => {}
                }
            },
        );
    }

    let block_count = live_blocks().len();
    // The template grid with live blocks resolved into their matching slots.
    let slots = {
        let base = base_slots.clone();
        use_memo(move || resolve_template(&base, &live_blocks()))
    };

    rsx! {
        div { class: "flex flex-col h-full bg-background text-foreground",
            // Top bar
            header {
                class: "flex items-center gap-2 px-3 py-2 border-b border-border bg-card",

                // Left: toggle preset sidebar
                Button {
                    variant: if left_open() { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                    is_icon_button: true,
                    aria_label: "Toggle preset list".to_string(),
                    on_click: move |_| left_open.toggle(),
                    "☰"
                }

                div { class: "flex flex-col ml-1 mr-2",
                    span { class: "text-[10px] font-semibold uppercase tracking-[2px] text-muted-foreground",
                        "Guitar Rig"
                    }
                    span { class: "text-sm font-bold",
                        if !perf().profile_name.is_empty() { "{perf().profile_name}" } else { "Default Template" }
                    }
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

                // Live IN/OUT meters (engine is always running)
                if rig.is_some() {
                    div { class: "flex items-center gap-2 mr-2",
                        MeterPair { input: in_level(), output: out_level() }
                    }
                }

                // Quick audio input picker
                if let Some(bridge) = live_bridge.clone() {
                    QuickInputPicker { bridge }
                }

                span { class: "text-xs text-muted-foreground mx-1",
                    "{module_count} modules · {block_count} slots"
                }

                // Audio settings
                Button {
                    variant: if audio_open() { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                    on_click: move |_| audio_open.toggle(),
                    "Audio"
                }

                // Right: toggle right sidebar (inspector — coming soon)
                Button {
                    variant: if right_open() { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                    is_icon_button: true,
                    aria_label: "Toggle right panel".to_string(),
                    on_click: move |_| right_open.toggle(),
                    "⚙"
                }
            }

            // Body: Edit (the live module/wire grid) or Perform (footswitch folders)
            if mode() == Mode::Edit {
                // Focusable wrapper so Space toggles the selected block's bypass.
                div {
                    class: "flex-1 min-h-0 flex flex-row overflow-hidden outline-none",
                    tabindex: "0",
                    onkeydown: {
                        let rig = rig.clone();
                        move |e: KeyboardEvent| {
                            if e.code() == Code::Space {
                                e.prevent_default();
                                if let (Some(r), Some(id)) = (rig.clone(), selected_block()) {
                                    spawn(async move {
                                        let _ = r.toggle_block_bypass(id).await;
                                    });
                                }
                            }
                        }
                    },
                    if left_open() {
                        PresetSidebar {}
                    }
                    div { class: "flex-1 min-w-0 min-h-0 flex flex-col overflow-hidden",
                        RigGridPanel {
                            initial_slots: slots(),
                            on_selection_change: move |sel: Option<GridSelection>| {
                                selected_block.set(match sel {
                                    Some(GridSelection::Block(uuid)) => slots()
                                        .iter()
                                        .find(|s| s.id == uuid)
                                        .and_then(|s| s.preset_id.clone()),
                                    _ => None,
                                });
                            },
                            on_param_change: {
                                let rig = rig.clone();
                                move |(uuid, name, value): (uuid::Uuid, String, f32)| {
                                    if let Some(r) = rig.clone() {
                                        if let Some(id) = slots().iter().find(|s| s.id == uuid).and_then(|s| s.preset_id.clone()) {
                                            spawn(async move {
                                                let _ = r.set_block_param(id, name, value).await;
                                            });
                                        }
                                    }
                                }
                            },
                        }
                    }
                }
            } else {
                div { class: "flex-1 min-h-0 overflow-hidden p-4",
                    if let Some(r) = rig.clone() {
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
                            on_cycle_boost: Callback::new({
                                let r = r.clone();
                                move |_: ()| {
                                    let r = r.clone();
                                    spawn(async move { let _ = r.cycle_boost().await; });
                                }
                            }),
                            on_tap_tempo: Callback::new({
                                let r = r.clone();
                                move |_: ()| {
                                    let r = r.clone();
                                    spawn(async move { let _ = r.tap_tempo().await; });
                                }
                            }),
                            on_prev_song: Callback::new({
                                let r = r.clone();
                                move |_: ()| {
                                    let r = r.clone();
                                    spawn(async move { let _ = r.prev_song().await; });
                                }
                            }),
                            on_next_song: Callback::new({
                                let r = r.clone();
                                move |_: ()| {
                                    let r = r.clone();
                                    spawn(async move { let _ = r.next_song().await; });
                                }
                            }),
                            on_select_song: Callback::new({
                                let r = r.clone();
                                move |i: usize| {
                                    let r = r.clone();
                                    spawn(async move { let _ = r.select_song(i as u32).await; });
                                }
                            }),
                        }
                    } else {
                        div { class: "flex items-center justify-center h-full",
                            span { class: "text-sm text-muted-foreground italic", "Audio backend not connected." }
                        }
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
            } else {
                AudioUnavailableModal { on_close: move |_| audio_open.set(false) }
            }
        }
    }
}

/// Compact top-bar audio input picker — sets the input device and persists.
#[component]
fn QuickInputPicker(bridge: AudioSettingsBridge) -> Element {
    let current = bridge.prefs.input_device.clone();
    let inputs = bridge.inputs.clone();
    let on_save = bridge.on_save;
    let base = bridge.prefs.clone();

    rsx! {
        Dropdown {
            DropdownTrigger {
                Button { variant: ButtonVariant::Outline,
                    span { class: "max-w-[180px] truncate text-xs", "🎸 {input_label(&current)}" }
                }
            }
            DropdownContent { align: "end".to_string(), width: "w-64".to_string(), class: "max-h-80 overflow-y-auto",
                DropdownItem {
                    value: String::new(),
                    index: 0,
                    on_select: {
                        let base = base.clone();
                        move |v: String| on_save.call(AudioPrefs { input_device: v, input_channel: 0, ..base.clone() })
                    },
                    "Default input"
                }
                for (i, d) in inputs.iter().enumerate() {
                    DropdownItem {
                        key: "{i}",
                        value: d.name.clone(),
                        index: i + 1,
                        on_select: {
                            let base = base.clone();
                            move |v: String| on_save.call(AudioPrefs { input_device: v, input_channel: 0, ..base.clone() })
                        },
                        "{d.name} ({d.channels} ch)"
                    }
                }
            }
        }
    }
}

/// Fallback shown when no audio backend bridge is provided by the host app.
#[component]
fn AudioUnavailableModal(on_close: EventHandler<()>) -> Element {
    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center",
            style: "background: rgba(0,0,0,0.55);",
            onclick: move |_| on_close.call(()),
            div {
                class: "w-96 max-w-[92vw] rounded-lg border border-border bg-popover text-popover-foreground shadow-2xl p-6",
                onclick: move |e| e.stop_propagation(),
                h2 { class: "text-sm font-semibold mb-2", "Audio Settings" }
                p { class: "text-xs text-muted-foreground",
                    "No audio backend is connected in this build, so devices can't be "
                    "enumerated. Run the desktop app to configure audio."
                }
                div { class: "flex justify-end mt-4",
                    Button { variant: ButtonVariant::Outline, on_click: move |_| on_close.call(()), "Close" }
                }
            }
        }
    }
}

/// Left-hand preset list. Lists presets for the selected block type, pulled from
/// the seeded [`Signal`] controller (via context). Self-contained: if no
/// controller is provided, or the preset library is empty, it shows a graceful
/// empty state rather than panicking.
#[component]
fn PresetSidebar() -> Element {
    let controller = use_hook(try_consume_context::<Signal>);

    let mut block_type = use_signal(|| BlockType::Amp);
    let mut presets = use_signal(Vec::<Preset>::new);
    let mut selected = use_signal(|| None::<usize>);
    let mut loading = use_signal(|| controller.is_some());

    // Reload the preset list whenever the selected block type changes.
    {
        let controller = controller.clone();
        use_effect(move || {
            let selected_type = block_type();
            let Some(signal) = controller.clone() else {
                return;
            };
            loading.set(true);
            spawn(async move {
                let list = signal
                    .block_presets()
                    .list(selected_type)
                    .await
                    .unwrap_or_default();
                presets.set(list);
                selected.set(None);
                loading.set(false);
            });
        });
    }

    let items = presets();
    let sel = selected();

    rsx! {
        aside { class: "w-64 flex-shrink-0 flex flex-col border-r border-border bg-card min-h-0",
            // Title
            div { class: "px-3 py-2 border-b border-border flex-shrink-0",
                h3 { class: "text-[10px] font-semibold text-muted-foreground uppercase tracking-wider", "Presets" }
            }

            // Block-type selector
            div { class: "flex flex-wrap gap-1 px-2 py-2 border-b border-border flex-shrink-0",
                for (bt, label) in PRESET_BLOCK_TYPES.iter().copied() {
                    Button {
                        key: "{label}",
                        variant: if block_type() == bt { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                        on_click: move |_| block_type.set(bt),
                        "{label}"
                    }
                }
            }

            // Preset list
            div { class: "flex-1 overflow-y-auto min-h-0",
                if controller.is_none() {
                    div { class: "p-4 text-xs text-muted-foreground italic", "No Signal controller connected." }
                } else if loading() {
                    div { class: "p-4 text-xs text-muted-foreground italic", "Loading presets…" }
                } else if items.is_empty() {
                    div { class: "p-4 text-xs text-muted-foreground italic",
                        "No presets found for this block type."
                    }
                } else {
                    // Keyed by index: the seeded catalog can contain presets with
                    // colliding ids, which would panic a name/id-keyed list.
                    for (i, preset) in items.iter().enumerate() {
                        {
                            let name = preset.name().to_string();
                            let variants = preset.snapshots().len();
                            let is_sel = sel == Some(i);
                            let color: BlockColor = block_color(preset.block_type().as_str());
                            rsx! {
                                button {
                                    key: "{i}",
                                    class: if is_sel {
                                        "w-full text-left px-3 py-2 border-b border-border/50 bg-accent text-accent-foreground"
                                    } else {
                                        "w-full text-left px-3 py-2 border-b border-border/50 hover:bg-accent/50 text-foreground"
                                    },
                                    onclick: move |_| selected.set(Some(i)),
                                    div { class: "flex items-center gap-2",
                                        span {
                                            class: "w-2 h-2 rounded-full flex-shrink-0",
                                            style: "background-color: {color.bg};",
                                        }
                                        span { class: "text-sm truncate", "{name}" }
                                        span { class: "text-[10px] text-muted-foreground ml-auto flex-shrink-0",
                                            "{variants}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
