//! **Layer zoom** — the play surface for one lane, and the deepest level of
//! the Control view.
//!
//! Every lane runs the same Signal Engine program, so this one surface is the
//! editor for a Keyscape piano, an Omnisphere soundsource and a wavetable
//! alike. Pages:
//!
//! - **Layer** — the Global Controls: one Filter / Envelope / Vibrato /
//!   Unison / Ambience that drive every audible module beneath, plus the
//!   layer's own Tone and Limiter.
//! - **Module** — the macro panels (Source · Tone · Filter · Filter Env ·
//!   Amp Env · Vibrato · Ambience · Effects). The knobs a player reaches for
//!   mid-set.
//! - **Edit** — the Signal Editor: the lane's articulations / zones and the
//!   engine program it's running through.

use dioxus::prelude::*;
use signal_keys_proto::keys::KeysRigClient;
use signal_keys_proto::{KeysLayerDetail, KeysNode};

use crate::control::engine_color;
use crate::zoom::{Zoom, ZoomBar};
use signal_keys_proto::{KeysMixer, KeysPreset};

/// Which page of the layer zoom.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    /// The layer's Global Controls — Omnisphere's Main page.
    Layer,
    /// One module's whole surface.
    Module,
    Edit,
}

/// The zoomed layer. Fetches its own detail and refreshes on change.
#[component]
pub fn LayerView(
    layer: String,
    zoom: Signal<Zoom>,
    /// The mixer, for the engine's sibling lanes (the A/B/C/D switcher).
    #[props(default)]
    mixer: KeysMixer,
    /// The library, for the Source browser.
    #[props(default)]
    presets: Vec<KeysPreset>,
) -> Element {
    let rig = use_hook(try_consume_context::<KeysRigClient>);
    let mut zoom = zoom;
    let mut page = use_signal(|| Page::Layer);
    let mut detail = use_signal(KeysLayerDetail::default);
    // Which module (A/B/C/D) the zoom is editing. Each module is its own
    // engine instance — own source, filter, envelopes.
    let mut module = use_signal(|| 0u32);

    // Pull the lane's detail; re-pull after every edit (cheap, local call).
    let refresh = use_callback({
        let rig = rig.clone();
        let layer = layer.clone();
        move |_: ()| {
            let (rig, layer, slot) = (rig.clone(), layer.clone(), module());
            spawn(async move {
                if let Some(r) = rig {
                    if let Ok(d) = r.layer_detail(layer, slot).await {
                        detail.set(d);
                    }
                }
            });
        }
    });
    // Re-pull when the lane or the selected module changes.
    {
        let layer = layer.clone();
        use_effect(move || {
            let _ = &layer;
            let _ = module();
            refresh.call(());
        });
    }

    let d = detail.read().clone();
    let accent = engine_color(&d.engine).to_string();
    let patch = if d.patch.is_empty() { "empty lane".to_string() } else { d.patch.clone() };
    // The back target: up one level, to this lane's engine.
    let back_to = d.engine.clone();

    rsx! {
        div { style: "flex: 1; min-height: 0; display: flex; flex-direction: column;",
            ZoomBar {
                crumbs: vec![d.engine.clone(), d.layer.clone()],
                on_back: move |_| zoom.set(Zoom::Engine(back_to.clone())),
                trailing: rsx! {
                    div { style: "display: flex; align-items: center; gap: 14px;",
                        // The layer's four MODULES — Omnisphere's Quadzone.
                        // Each is a whole engine: own source, filter, envelopes.
                        div { style: "display: flex; gap: 4px; padding: 3px; background: #0b0b0e; \
                                      border: 1px solid #1f1f23; border-radius: 8px;",
                            for m in d.modules.iter() {
                                {
                                    let is_here = m.index == d.module;
                                    let idx = m.index;
                                    let title = if m.patch.is_empty() {
                                        format!("Module {} — empty", m.slot)
                                    } else {
                                        format!("Module {} — {}", m.slot, m.patch)
                                    };
                                    rsx! {
                                        button {
                                            key: "{m.index}",
                                            style: format!(
                                                "appearance: none; border: none; border-radius: 6px; \
                                                 min-width: 30px; padding: 3px 8px; font-size: 10px; \
                                                 font-weight: 700; background: {}; color: {};",
                                                if is_here { "#101821" } else { "transparent" },
                                                if is_here {
                                                    accent.clone()
                                                } else if m.live {
                                                    "#a1a1aa".to_string()
                                                } else {
                                                    "#3f3f46".to_string()
                                                },
                                            ),
                                            title: "{title}",
                                            onclick: move |_| module.set(idx),
                                            if !m.enabled {
                                                span { style: "opacity: 0.5;", "{m.slot}" }
                                            } else {
                                                span { "{m.slot}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        span {
                            style: format!(
                                "padding: 4px 12px; border-radius: 999px; font-size: 11px; \
                                 font-weight: 600; background: #101821; color: {accent}; \
                                 max-width: 260px; overflow: hidden; text-overflow: ellipsis; \
                                 white-space: nowrap;",
                            ),
                            "{patch}"
                        }
                        // Page tabs share the module switcher's segmented shell
                        // so the bar reads as two controls, not five buttons.
                        div { style: "display: flex; gap: 4px; padding: 3px; background: #0b0b0e; \
                                      border: 1px solid #1f1f23; border-radius: 8px;",
                            for (p, label) in [
                                (Page::Layer, "Layer"),
                                (Page::Module, "Module"),
                                (Page::Edit, "Edit"),
                            ] {
                                button {
                                    key: "{label}",
                                    style: format!(
                                        "appearance: none; border: none; border-radius: 6px; padding: 4px 12px; \
                                         font-size: 11px; font-weight: 600; background: {}; color: {};",
                                        if page() == p { "#101821" } else { "transparent" },
                                        if page() == p { "#38bdf8" } else { "#52525b" },
                                    ),
                                    onclick: move |_| page.set(p),
                                    "{label}"
                                }
                            }
                        }
                    }
                },
            }
            match page() {
                Page::Layer => rsx! {
                    LayerPage {
                        detail: d.clone(),
                        accent: accent.clone(),
                        page,
                        module,
                        refresh,
                    }
                },
                Page::Module => rsx! {
                    PlayPage {
                        detail: d.clone(),
                        accent: accent.clone(),
                        presets: presets.clone(),
                        module: d.module,
                        refresh,
                    }
                },
                Page::Edit => rsx! { EditPage { detail: d.clone() } },
            }
        }
    }
}

/// The **Layer page** — the Global Controls, and the module strip they act
/// on. Editing a module here jumps to that module's surface.
#[component]
fn LayerPage(
    detail: KeysLayerDetail,
    accent: String,
    page: Signal<Page>,
    module: Signal<u32>,
    refresh: Callback<()>,
) -> Element {
    let rig = use_hook(try_consume_context::<KeysRigClient>);
    let lane = detail.layer.clone();
    let mut page = page;
    let mut module = module;

    rsx! {
        crate::layer_macros::LayerMacros {
            detail: detail.clone(),
            accent: accent.clone(),
            on_global: {
                let (rig, lane) = (rig.clone(), lane.clone());
                move |(id, v): (String, f32)| {
                    let (rig, lane) = (rig.clone(), lane.clone());
                    spawn(async move {
                        if let Some(r) = rig { let _ = r.set_layer_global(lane, id, v).await; }
                    });
                    refresh.call(());
                }
            },
            on_module_gain: {
                let (rig, lane) = (rig.clone(), lane.clone());
                move |(idx, db): (u32, f32)| {
                    let (rig, lane) = (rig.clone(), lane.clone());
                    spawn(async move {
                        if let Some(r) = rig { let _ = r.set_module_gain(lane, idx, db).await; }
                    });
                    refresh.call(());
                }
            },
            on_module_enabled: {
                let (rig, lane) = (rig.clone(), lane.clone());
                move |(idx, on): (u32, bool)| {
                    let (rig, lane) = (rig.clone(), lane.clone());
                    spawn(async move {
                        if let Some(r) = rig { let _ = r.set_module_enabled(lane, idx, on).await; }
                    });
                    refresh.call(());
                }
            },
            on_open_module: move |idx: u32| {
                module.set(idx);
                page.set(Page::Module);
            },
        }
    }
}

/// The Play page: the module surface + the soundsource browser overlay.
#[component]
fn PlayPage(
    detail: KeysLayerDetail,
    accent: String,
    presets: Vec<KeysPreset>,
    module: u32,
    refresh: Callback<()>,
) -> Element {
    let rig = use_hook(try_consume_context::<KeysRigClient>);
    let browsing = use_signal(|| false);
    let lane = detail.layer.clone();

    rsx! {
        div { style: "position: relative; flex: 1; min-height: 0; display: flex;",
            crate::module_edit::ModuleEdit {
                detail: detail.clone(),
                accent: accent.clone(),
                module,
                browsing,
                on_macro: {
                    let (rig, lane) = (rig.clone(), lane.clone());
                    move |(id, v): (String, f32)| {
                        let (rig, lane) = (rig.clone(), lane.clone());
                        spawn(async move {
                            if let Some(r) = rig { let _ = r.set_layer_macro(lane, module, id, v).await; }
                        });
                        refresh.call(());
                    }
                },
                on_enabled: {
                    let (rig, lane) = (rig.clone(), lane.clone());
                    move |on: bool| {
                        let (rig, lane) = (rig.clone(), lane.clone());
                        spawn(async move {
                            if let Some(r) = rig { let _ = r.set_module_enabled(lane, module, on).await; }
                        });
                        refresh.call(());
                    }
                },
                on_gain: {
                    let (rig, lane) = (rig.clone(), lane.clone());
                    move |db: f32| {
                        let (rig, lane) = (rig.clone(), lane.clone());
                        spawn(async move {
                            if let Some(r) = rig { let _ = r.set_module_gain(lane, module, db).await; }
                        });
                        refresh.call(());
                    }
                },
            }
            if browsing() {
                div {
                    style: "position: absolute; top: 12px; left: 12px; z-index: 50; width: 300px;",
                    SourceBrowser {
                        layer: detail.layer.clone(),
                        module,
                        presets: presets.clone(),
                        open: browsing,
                        refresh,
                    }
                }
            }
        }
    }
}

/// The **Signal Editor** page: what this lane is actually running — the
/// engine program, and (as the mapping surface lands) its articulations and
/// zones.
#[component]
fn EditPage(detail: KeysLayerDetail) -> Element {
    rsx! {
        div {
            style: "flex: 1; min-height: 0; overflow: auto; padding: 16px 18px 24px; display: flex; \
                    flex-direction: column; gap: 16px;",
            div {
                style: "display: flex; align-items: center; gap: 12px; padding: 12px 14px; \
                        border: 1px solid #1f1f23; border-radius: 12px; background: #0e0e11;",
                span { style: "font-size: 11px; font-weight: 700; color: #e4e4e7;", "Source" }
                span { style: "font-size: 11px; color: #7dd3fc;",
                    if detail.patch.is_empty() { "— nothing loaded —" } else { "{detail.patch}" }
                }
                div { style: "flex: 1;" }
                span { style: "font-size: 10px; color: #52525b;",
                    "Articulations · zones · round-robins land here (Signal Editor)"
                }
            }
            // Caption and tree are one block — the caption belongs to what it
            // labels, not to the gap above it.
            div { style: "display: flex; flex-direction: column; gap: 8px;",
                span { style: "font-size: 10px; letter-spacing: 0.1em; text-transform: uppercase; color: #52525b;",
                    "Engine program"
                }
                ProgramTree { node: detail.tree.clone() }
            }
        }
    }
}

/// The lane's block stack, rendered compactly (the Signal Engine program).
#[component]
fn ProgramTree(node: KeysNode) -> Element {
    if node.id.is_empty() {
        return rsx! {
            span { style: "font-size: 11px; color: #52525b;", "Program builds when audio opens." }
        };
    }
    let (border, bg) = match node.role.as_str() {
        "layer" => ("#2b4a6b", "#0f1620"),
        "module" => ("#27272a", "#111113"),
        "block" if node.live => ("#166534", "#0f2417"),
        _ => ("#26262b", "#111113"),
    };
    let dim = node.role == "block" && !node.live;
    let kids = node.children.clone();
    rsx! {
        div {
            style: format!(
                "display: flex; flex-direction: column; gap: 5px; padding: 7px 9px; \
                 border: 1px solid {border}; border-radius: 9px; background: {bg}; opacity: {};",
                if dim { "0.45" } else { "1" },
            ),
            div { style: "display: flex; align-items: center; gap: 7px;",
                span { style: "font-size: 8px; letter-spacing: 0.1em; text-transform: uppercase; color: #52525b;",
                    "{node.role}"
                }
                span { style: "font-size: 11px; font-weight: 600; color: #d4d4d8;", "{node.label}" }
                if node.role == "block" && node.live {
                    span { style: "font-size: 8px; font-weight: 700; color: #4ade80;", "LIVE" }
                }
            }
            if !kids.is_empty() {
                div { style: "display: flex; gap: 6px; flex-wrap: wrap;",
                    for child in kids.iter() {
                        ProgramTree { key: "{child.id}", node: child.clone() }
                    }
                }
            }
        }
    }
}

/// Searchable soundsource browser — the whole library (Keyscape packs and
/// Omnisphere soundsources alike), filtered as you type. Picking one loads it
/// into this lane's Soundsource block.
#[component]
fn SourceBrowser(
    layer: String,
    module: u32,
    presets: Vec<KeysPreset>,
    open: Signal<bool>,
    refresh: Callback<()>,
) -> Element {
    let rig = use_hook(try_consume_context::<KeysRigClient>);
    let mut open = open;
    let mut query = use_signal(String::new);

    let q = query().to_lowercase();
    let hits: Vec<(usize, KeysPreset)> = presets
        .iter()
        .enumerate()
        .filter(|(_, p)| q.is_empty() || p.name.to_lowercase().contains(&q))
        .take(80)
        .map(|(i, p)| (i, p.clone()))
        .collect();
    let total = presets.len();

    rsx! {
        div {
            style: "display: flex; flex-direction: column; gap: 10px; padding: 12px; \
                    border: 1px solid #2b2b31; border-radius: 10px; background: #0b0b0d;",
            input {
                style: "background: #131316; border: 1px solid #1f1f23; border-radius: 8px; \
                        padding: 8px 10px; color: #e4e4e7; font-size: 11px;",
                placeholder: "search {total} soundsources…",
                value: "{query}",
                oninput: move |e| query.set(e.value()),
            }
            div {
                style: "display: flex; flex-direction: column; gap: 1px; max-height: 260px; overflow-y: auto;",
                button {
                    style: "appearance: none; text-align: left; border: none; border-radius: 6px; \
                            padding: 7px 9px; background: transparent; color: #a1a1aa; font-size: 11px;",
                    onclick: {
                        let rig = rig.clone();
                        let layer = layer.clone();
                        move |_| {
                            let (rig, layer) = (rig.clone(), layer.clone());
                            open.set(false);
                            spawn(async move {
                                if let Some(r) = rig { let _ = r.clear_layer(layer, module).await; }
                            });
                            refresh.call(());
                        }
                    },
                    "— empty —"
                }
                for (i, preset) in hits {
                    button {
                        // Library names repeat across banks — key by index.
                        key: "{i}",
                        style: "appearance: none; text-align: left; border: none; border-radius: 6px; \
                                padding: 7px 9px; background: transparent; color: #e4e4e7; font-size: 11px; \
                                display: flex; align-items: center; gap: 6px;",
                        onclick: {
                            let rig = rig.clone();
                            let layer = layer.clone();
                            move |_| {
                                let (rig, layer) = (rig.clone(), layer.clone());
                                open.set(false);
                                spawn(async move {
                                    if let Some(r) = rig { let _ = r.set_layer_patch(layer, module, i as u32).await; }
                                });
                                refresh.call(());
                            }
                        },
                        span { style: "flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                            "{preset.name}"
                        }
                        span { style: "font-size: 8px; color: #52525b;", "{preset.kind}" }
                    }
                }
            }
        }
    }
}
