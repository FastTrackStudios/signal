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
use crate::zoom::Zoom;
use signal_keys_proto::KeysMixer;

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
) -> Element {
    let rig = use_hook(try_consume_context::<KeysRigClient>);
    let zoom = zoom;
    let mut page = use_signal(|| Page::Layer);
    let mut detail = use_signal(KeysLayerDetail::default);
    // Which module (A/B/C/D) the zoom is editing. Each module is its own
    // engine instance — own source, filter, envelopes.
    let module = use_signal(|| 0u32);

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

    // ── the chrome, level 3: engine ▸ lane ▸ module ─────────────────────
    //
    // Every control the zoom bar used to hold is here: the lane crumb picks a
    // sibling lane, the module crumb is the A/B/C/D switcher, the pages are
    // the bar's tabs and the patch is a readout. Clicking a parent crumb is
    // what "back" means now.
    let level = fts_chrome::use_chrome_level(3);
    // The browser follows the zoom: inside a lane it lists what fits the
    // module you are editing.
    let mut selection = crate::selection::use_selection();
    {
        let (engine, lane, slot) = (d.engine.clone(), d.layer.clone(), d.module);
        use_effect(move || {
            if engine.is_empty() {
                return;
            }
            let next = crate::selection::Selection::Module {
                engine: engine.clone(),
                layer: lane.clone(),
                module: slot,
            };
            if *selection.peek() != next {
                selection.set(next);
            }
        });
    }
    let lanes: Vec<(String, bool, Callback<()>)> = mixer
        .engines
        .iter()
        .filter(|e| e.name == d.engine)
        .flat_map(|e| e.layers.iter())
        .map(|l| {
            let name = l.name.clone();
            let is_here = name == d.layer;
            let target = name.clone();
            let mut zoom_to = zoom;
            (
                name,
                is_here,
                Callback::new(move |_| zoom_to.set(Zoom::Layer(target.clone()))),
            )
        })
        .collect();
    let modules: Vec<(String, bool, Callback<()>)> = d
        .modules
        .iter()
        .map(|m| {
            let idx = m.index;
            let label = if m.patch.is_empty() {
                format!("{} — empty", m.slot)
            } else {
                format!("{} — {}", m.slot, m.patch)
            };
            let mut pick = module;
            (label, m.index == d.module, Callback::new(move |_| pick.set(idx)))
        })
        .collect();
    let module_label = d
        .modules
        .iter()
        .find(|m| m.index == d.module)
        .map(|m| format!("Module {}", m.slot))
        .unwrap_or_else(|| "Module".to_string());
    {
        let up = back_to.clone();
        let mut zoom_up = zoom;
        level.crumbs(vec![
            fts_chrome::Crumb::new(
                d.engine.clone(),
                Callback::new(move |_| zoom_up.set(Zoom::Engine(up.clone()))),
            ),
            fts_chrome::Crumb::here(d.layer.clone()).with_menu(lanes),
            fts_chrome::Crumb::here(module_label).with_menu(modules),
        ]);
    }
    level.tabs(vec![
        fts_chrome::ChromeTab::new(
            "layer",
            "Layer",
            page() == Page::Layer,
            Callback::new(move |_| page.set(Page::Layer)),
        ),
        fts_chrome::ChromeTab::new(
            "module",
            "Module",
            page() == Page::Module,
            Callback::new(move |_| page.set(Page::Module)),
        ),
        fts_chrome::ChromeTab::new(
            "edit",
            "Edit",
            page() == Page::Edit,
            Callback::new(move |_| page.set(Page::Edit)),
        ),
    ]);
    level.status(vec![fts_chrome::StatusItem::pill(patch.clone(), accent.clone(), "#101821")]);

    rsx! {
        div { style: "flex: 1; min-height: 0; display: flex;",
            div { style: "display: flex; flex-direction: column; flex: 1; min-width: 0; min-height: 0;",
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
                            module: d.module,
                            refresh,
                        }
                    },
                    Page::Edit => rsx! { EditPage { detail: d.clone() } },
                }
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

/// The Play page: the module surface. The library it loads from is the
/// chrome's Soundsources panel.
#[component]
fn PlayPage(
    detail: KeysLayerDetail,
    accent: String,
    module: u32,
    refresh: Callback<()>,
) -> Element {
    let rig = use_hook(try_consume_context::<KeysRigClient>);
    let lane = detail.layer.clone();

    rsx! {
        div { style: "flex: 1; min-height: 0; display: flex;",
            crate::module_edit::ModuleEdit {
                detail: detail.clone(),
                accent: accent.clone(),
                module,
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
