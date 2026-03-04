//! Dock panel registration for the signal domain.
//!
//! Registers signal-ui panels with the dock renderer registry,
//! decoupling panel definitions from the central app binary.
//!
//! All panels read [`signal::Signal`] from Dioxus context
//! (the app must `provide_context` it before the dock renders).

use dioxus::prelude::*;
use dock_dioxus::PanelRendererRegistry;
use dock_proto::PanelId;
use signal::Signal;

use crate::components::GridSlot;
use crate::views::{
    engines_to_grid_slots, resolve_scene_engines, CollectionBrowser, EditorInspectorPanel,
    EngineParamLookup, RigGridPanel, RigSceneGrid,
};

/// Register all signal panels with the renderer registry.
pub fn register_panels(registry: &mut PanelRendererRegistry) {
    registry.register(PanelId::RigGrid, || {
        rsx! { RigGridDockPanel {} }
    });
    registry.register(PanelId::PresetBrowser, || {
        rsx! { PresetBrowserDockPanel {} }
    });
    registry.register(PanelId::ProfileBrowser, || {
        rsx! { ProfileBrowserDockPanel {} }
    });
    registry.register(PanelId::SongParts, || {
        rsx! { SongPartsDockPanel {} }
    });
    registry.register(PanelId::SongSelector, || {
        rsx! { SongSelectorDockPanel {} }
    });
    registry.register(PanelId::SceneGrid, || {
        rsx! { SceneGridDockPanel {} }
    });
    registry.register(PanelId::RigEditor, || {
        rsx! { RigEditorDockPanel {} }
    });
    registry.register(PanelId::RigGridEditor, || {
        rsx! { RigGridEditorDockPanel {} }
    });
    registry.register(PanelId::RigDetailEditor, || {
        rsx! { RigDetailEditorDockPanel {} }
    });
    registry.register(PanelId::SnapshotTest, || {
        rsx! { SnapshotTestDockPanel {} }
    });
}

// ---------------------------------------------------------------------------
// Helper: try to get Signal from context
// ---------------------------------------------------------------------------

fn use_controller() -> Option<Signal> {
    try_consume_context::<Signal>()
}

fn no_controller() -> Element {
    rsx! {
        div { class: "flex items-center justify-center h-full w-full text-sm text-muted-foreground",
            "Signal service not available"
        }
    }
}

fn placeholder(label: &str) -> Element {
    rsx! {
        div { class: "flex items-center justify-center h-full w-full text-sm text-muted-foreground",
            "{label}"
        }
    }
}

// ---------------------------------------------------------------------------
// Rig Grid — resolves first rig's default scene into a block grid
// ---------------------------------------------------------------------------

#[component]
fn RigGridDockPanel() -> Element {
    let Some(signal) = use_controller() else {
        return no_controller();
    };

    let mut slots = use_signal(Vec::<GridSlot>::new);
    let mut loaded = use_signal(|| false);

    {
        let signal = signal.clone();
        use_effect(move || {
            let signal = signal.clone();
            spawn(async move {
                let rigs = signal.rigs().list().await.unwrap_or_default();
                if let Some(rig) = rigs.first() {
                    if let Some(scene) = rig.variants.first() {
                        let rig_id = rig.id.to_string();
                        let scene_id = scene.id.to_string();
                        if let Some((engines, params)) =
                            resolve_scene_engines(&signal, &rig_id, &scene_id).await
                        {
                            slots.set(engines_to_grid_slots(&engines, &params));
                        }
                    }
                }
                loaded.set(true);
            });
        });
    }

    if !loaded() {
        return placeholder("Loading rig grid...");
    }

    let grid_slots = slots();
    if grid_slots.is_empty() {
        return placeholder("No rig data");
    }

    rsx! {
        RigGridPanel { initial_slots: grid_slots }
    }
}

// ---------------------------------------------------------------------------
// Preset Browser — full collection browser scoped to presets
// ---------------------------------------------------------------------------

#[component]
fn PresetBrowserDockPanel() -> Element {
    let Some(_signal) = use_controller() else {
        return no_controller();
    };
    rsx! {
        CollectionBrowser {}
    }
}

// ---------------------------------------------------------------------------
// Profile Browser — collection browser (profiles are a browse level)
// ---------------------------------------------------------------------------

#[component]
fn ProfileBrowserDockPanel() -> Element {
    let Some(_signal) = use_controller() else {
        return no_controller();
    };
    rsx! {
        CollectionBrowser {}
    }
}

// ---------------------------------------------------------------------------
// Song Parts — shows sections for the current song
// ---------------------------------------------------------------------------

#[component]
fn SongPartsDockPanel() -> Element {
    let Some(signal) = use_controller() else {
        return no_controller();
    };

    use crate::views::{SectionEntry, SongEditor};

    let mut song_name = use_signal(|| String::new());
    let mut sections = use_signal(Vec::<SectionEntry>::new);
    let mut selected_section = use_signal(|| None::<String>);

    {
        let signal = signal.clone();
        use_effect(move || {
            let signal = signal.clone();
            spawn(async move {
                let songs = signal.songs().list().await.unwrap_or_default();
                if let Some(song) = songs.first() {
                    song_name.set(song.name.clone());
                    let entries: Vec<SectionEntry> = song
                        .sections()
                        .iter()
                        .map(|s| SectionEntry {
                            id: s.id.to_string(),
                            name: s.name.clone(),
                            rig_scene_name: None,
                            profile_patch_name: None,
                            tempo: None,
                            key_signature: None,
                            notes: None,
                            is_base_profile_section: None,
                        })
                        .collect();
                    if let Some(first) = entries.first() {
                        selected_section.set(Some(first.id.clone()));
                    }
                    sections.set(entries);
                }
            });
        });
    }

    rsx! {
        SongEditor {
            song_name: song_name(),
            sections: sections(),
            selected_section_id: selected_section(),
            on_select_section: move |id: String| {
                selected_section.set(Some(id));
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Song Selector — list of songs in the current setlist
// ---------------------------------------------------------------------------

#[component]
fn SongSelectorDockPanel() -> Element {
    let Some(signal) = use_controller() else {
        return no_controller();
    };

    use crate::views::SongEntry;

    let mut songs = use_signal(Vec::<SongEntry>::new);
    let mut selected_song = use_signal(|| None::<String>);

    {
        let signal = signal.clone();
        use_effect(move || {
            let signal = signal.clone();
            spawn(async move {
                let song_list = signal.songs().list().await.unwrap_or_default();
                let entries: Vec<SongEntry> = song_list
                    .iter()
                    .map(|s| SongEntry {
                        id: s.id.to_string(),
                        name: s.name.clone(),
                        section_count: s.sections().len(),
                        duration_display: None,
                    })
                    .collect();
                if let Some(first) = entries.first() {
                    selected_song.set(Some(first.id.clone()));
                }
                songs.set(entries);
            });
        });
    }

    let selected = selected_song();
    rsx! {
        div { class: "h-full w-full flex flex-col overflow-hidden",
            div { class: "px-3 py-2 border-b border-border flex-shrink-0 bg-zinc-900/40",
                h3 { class: "text-[10px] font-semibold text-zinc-500 uppercase tracking-wider", "Songs" }
            }
            div { class: "flex-1 overflow-y-auto",
                for (i, song) in songs().iter().enumerate() {
                    {
                        let is_sel = selected.as_deref() == Some(song.id.as_str());
                        let song_id = song.id.clone();
                        rsx! {
                            button {
                                key: "{song.id}",
                                class: if is_sel {
                                    "w-full text-left px-3 py-2 border-b border-zinc-800/50 bg-zinc-700/60 text-zinc-200"
                                } else {
                                    "w-full text-left px-3 py-2 border-b border-zinc-800/50 hover:bg-zinc-800/60 text-zinc-300"
                                },
                                onclick: move |_| {
                                    selected_song.set(Some(song_id.clone()));
                                },
                                div { class: "flex items-center gap-2",
                                    span { class: "text-[10px] text-zinc-500 w-5 text-right flex-shrink-0",
                                        "{i + 1}"
                                    }
                                    span { class: "text-sm truncate", "{song.name}" }
                                    span { class: "text-[10px] text-zinc-500 ml-auto",
                                        "{song.section_count} sections"
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

// ---------------------------------------------------------------------------
// Scene Grid — tile grid of scenes for the active rig
// ---------------------------------------------------------------------------

#[component]
fn SceneGridDockPanel() -> Element {
    let Some(signal) = use_controller() else {
        return no_controller();
    };

    let mut rig_id = use_signal(|| None::<String>);
    let mut active_scene = use_signal(|| None::<String>);

    {
        let signal = signal.clone();
        use_effect(move || {
            let signal = signal.clone();
            spawn(async move {
                let rigs = signal.rigs().list().await.unwrap_or_default();
                if let Some(rig) = rigs.first() {
                    rig_id.set(Some(rig.id.to_string()));
                    if let Some(scene) = rig.variants.first() {
                        active_scene.set(Some(scene.id.to_string()));
                    }
                }
            });
        });
    }

    if let Some(rid) = rig_id() {
        rsx! {
            RigSceneGrid {
                rig_id: rid,
                active_scene_id: active_scene(),
                on_scene_select: move |id: String| {
                    active_scene.set(Some(id));
                },
            }
        }
    } else {
        placeholder("Loading scenes...")
    }
}

// ---------------------------------------------------------------------------
// Rig Editor — sub-tab host (Performance / Manage / Editor)
// ---------------------------------------------------------------------------

#[component]
fn RigEditorDockPanel() -> Element {
    placeholder("Rig Editor — use the Signal tab instead")
}

// ---------------------------------------------------------------------------
// Rig Grid Editor — 2D block/module grid editor
// ---------------------------------------------------------------------------

#[component]
fn RigGridEditorDockPanel() -> Element {
    let Some(signal) = use_controller() else {
        return no_controller();
    };

    let mut slots = use_signal(Vec::<GridSlot>::new);
    let mut params = use_signal(EngineParamLookup::new);
    let mut loaded = use_signal(|| false);

    {
        let signal = signal.clone();
        use_effect(move || {
            let signal = signal.clone();
            spawn(async move {
                let rigs = signal.rigs().list().await.unwrap_or_default();
                if let Some(rig) = rigs.first() {
                    if let Some(scene) = rig.variants.first() {
                        let rig_id = rig.id.to_string();
                        let scene_id = scene.id.to_string();
                        if let Some((engines, p)) =
                            resolve_scene_engines(&signal, &rig_id, &scene_id).await
                        {
                            slots.set(engines_to_grid_slots(&engines, &p));
                            params.set(p);
                        }
                    }
                }
                loaded.set(true);
            });
        });
    }

    if !loaded() {
        return placeholder("Loading grid editor...");
    }
    let grid_slots = slots();
    if grid_slots.is_empty() {
        return placeholder("No rig data");
    }

    rsx! {
        RigGridPanel { initial_slots: grid_slots }
    }
}

// ---------------------------------------------------------------------------
// Rig Detail Editor — inspector for selected grid slot
// ---------------------------------------------------------------------------

#[component]
fn RigDetailEditorDockPanel() -> Element {
    rsx! {
        EditorInspectorPanel {
            selection: None,
            chain: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Snapshot Test — placeholder (DAW bridge integration is separate)
// ---------------------------------------------------------------------------

#[component]
fn SnapshotTestDockPanel() -> Element {
    placeholder("Snapshot Test — DAW bridge not connected")
}
