//! **Layer zoom** — the play surface for one lane, and the deepest level of
//! the Control view.
//!
//! Every lane runs the same Signal Engine program, so this one surface is the
//! editor for a Keyscape piano, an Omnisphere soundsource and a wavetable
//! alike. Pages:
//!
//! - **Play** — the macro panels (Source · Tone · Filter · Filter Env ·
//!   Amp Env · Vibrato · Ambience · Effects). The knobs a player reaches for
//!   mid-set.
//! - **Edit** — the Signal Editor: the lane's articulations / zones and the
//!   engine program it's running through.

use dioxus::prelude::*;
use signal_keys_proto::keys::KeysRigClient;
use signal_keys_proto::{KeysLayerDetail, KeysMacro, KeysNode};

use crate::control::engine_color;
use crate::fader::{Fader, fmt_db};
use crate::knob::Knob;
use crate::zoom::{Zoom, ZoomBar};

/// Which page of the layer zoom.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Play,
    Edit,
}

/// The zoomed layer. Fetches its own detail and refreshes on change.
#[component]
pub fn LayerView(layer: String, zoom: Signal<Zoom>) -> Element {
    let rig = use_hook(try_consume_context::<KeysRigClient>);
    let mut zoom = zoom;
    let mut page = use_signal(|| Page::Play);
    let mut detail = use_signal(KeysLayerDetail::default);

    // Pull the lane's detail; re-pull after every edit (cheap, local call).
    let refresh = use_callback({
        let rig = rig.clone();
        let layer = layer.clone();
        move |_: ()| {
            let (rig, layer) = (rig.clone(), layer.clone());
            spawn(async move {
                if let Some(r) = rig {
                    if let Ok(d) = r.layer_detail(layer).await {
                        detail.set(d);
                    }
                }
            });
        }
    });
    use_hook(move || refresh.call(()));

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
                    div { style: "display: flex; align-items: center; gap: 6px;",
                        span {
                            style: format!(
                                "padding: 3px 10px; border-radius: 999px; font-size: 11px; \
                                 font-weight: 600; background: #101821; color: {accent};",
                            ),
                            "{patch}"
                        }
                        for (p, label) in [(Page::Play, "Play"), (Page::Edit, "Edit")] {
                            button {
                                key: "{label}",
                                style: format!(
                                    "appearance: none; border: none; border-radius: 7px; padding: 4px 12px; \
                                     font-size: 11px; font-weight: 600; background: {}; color: {};",
                                    if page() == p { "#101821" } else { "transparent" },
                                    if page() == p { "#38bdf8" } else { "#52525b" },
                                ),
                                onclick: move |_| page.set(p),
                                "{label}"
                            }
                        }
                    }
                },
            }
            match page() {
                Page::Play => rsx! {
                    PlayPage { detail: d.clone(), accent: accent.clone(), refresh }
                },
                Page::Edit => rsx! { EditPage { detail: d.clone() } },
            }
        }
    }
}

/// The macro panels + the lane's own fader.
#[component]
fn PlayPage(detail: KeysLayerDetail, accent: String, refresh: Callback<()>) -> Element {
    let rig = use_hook(try_consume_context::<KeysRigClient>);
    // Panels in declaration order, de-duplicated.
    let mut groups: Vec<String> = Vec::new();
    for m in &detail.macros {
        if !groups.contains(&m.group) {
            groups.push(m.group.clone());
        }
    }

    rsx! {
        div {
            style: "flex: 1; min-height: 0; overflow: auto; padding: 14px; display: flex; gap: 14px; \
                    align-items: flex-start; flex-wrap: wrap;",
            // Lane strip: the fader that also lives in the mixer.
            div {
                style: "display: flex; flex-direction: column; align-items: center; gap: 8px; \
                        padding: 12px 16px; border: 1px solid #1f1f23; border-radius: 14px; \
                        background: #0e0e11; min-width: 96px;",
                span { style: "font-size: 10px; letter-spacing: 0.1em; text-transform: uppercase; color: #52525b;",
                    "Lane"
                }
                Fader {
                    db: detail.gain_db,
                    height_px: 150,
                    accent: accent.clone(),
                    dimmed: detail.muted,
                    on_change: {
                        let rig = rig.clone();
                        let lane = detail.layer.clone();
                        move |db: f32| {
                            let (rig, lane) = (rig.clone(), lane.clone());
                            spawn(async move {
                                if let Some(r) = rig { let _ = r.set_layer_gain(lane, db).await; }
                            });
                        }
                    },
                }
                span { style: "font-size: 9px; color: #52525b;", {fmt_db(detail.gain_db)} }
                span { style: "font-size: 9px; color: #3f3f46;",
                    {format!("keys {}–{}", detail.key_lo, detail.key_hi)}
                }
            }
            // Macro panels.
            for group in groups {
                {
                    let knobs: Vec<KeysMacro> = detail
                        .macros
                        .iter()
                        .filter(|m| m.group == group)
                        .cloned()
                        .collect();
                    let any_live = knobs.iter().any(|k| k.live);
                    rsx! {
                        div {
                            key: "{group}",
                            style: "display: flex; flex-direction: column; gap: 8px; padding: 12px; \
                                    border: 1px solid #1f1f23; border-radius: 14px; background: #0e0e11;",
                            div { style: "display: flex; align-items: center; gap: 6px;",
                                span {
                                    style: format!(
                                        "width: 6px; height: 6px; border-radius: 999px; background: {};",
                                        if any_live { accent.clone() } else { "#3f3f46".to_string() },
                                    ),
                                }
                                span { style: "font-size: 10px; font-weight: 700; letter-spacing: 0.08em; \
                                               text-transform: uppercase; color: #a1a1aa;",
                                    "{group}"
                                }
                            }
                            div { style: "display: flex; gap: 4px;",
                                for m in knobs.iter() {
                                    Knob {
                                        key: "{m.id}",
                                        label: m.name.clone(),
                                        value: m.value,
                                        min: m.min,
                                        max: m.max,
                                        unit: m.unit.clone(),
                                        live: m.live,
                                        accent: accent.clone(),
                                        on_change: {
                                            let rig = rig.clone();
                                            let lane = detail.layer.clone();
                                            let id = m.id.clone();
                                            move |v: f32| {
                                                let (rig, lane, id) = (rig.clone(), lane.clone(), id.clone());
                                                spawn(async move {
                                                    if let Some(r) = rig {
                                                        let _ = r.set_layer_macro(lane, id, v).await;
                                                    }
                                                });
                                                refresh.call(());
                                            }
                                        },
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

/// The **Signal Editor** page: what this lane is actually running — the
/// engine program, and (as the mapping surface lands) its articulations and
/// zones.
#[component]
fn EditPage(detail: KeysLayerDetail) -> Element {
    rsx! {
        div {
            style: "flex: 1; min-height: 0; overflow: auto; padding: 14px; display: flex; \
                    flex-direction: column; gap: 12px;",
            div {
                style: "display: flex; align-items: center; gap: 10px; padding: 10px 12px; \
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
            span { style: "font-size: 10px; letter-spacing: 0.1em; text-transform: uppercase; color: #52525b;",
                "Engine program"
            }
            ProgramTree { node: detail.tree.clone() }
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
