//! **The browser** — the rig's left sidebar, like the guitar rig's.
//!
//! One browser, pointed at whatever is selected. Pick a lane in Keys and it is
//! the Keys layer presets; pick the engine card and it is engine programs;
//! zoom into the lane and pick a module and it is that engine's soundsources.
//! The list narrows by the selection's engine tag, so a Pad lane never shows
//! you a grand piano.
//!
//! Loading follows the same shape as the selection:
//!
//! | Selected | Click loads |
//! |---|---|
//! | Engine | the whole engine program (`load_preset`) |
//! | Layer  | into that lane's module A (`set_layer_patch`) |
//! | Module | into that module |

use dioxus::prelude::*;
use signal_keys_proto::keys::KeysRigClient;
use signal_keys_proto::KeysPreset;

use crate::control::engine_color;
use crate::selection::{Selection, use_selection};

/// The left sidebar.
#[component]
pub fn Browser(presets: Vec<KeysPreset>) -> Element {
    let rig = use_hook(try_consume_context::<KeysRigClient>);
    let selection = use_selection();
    let mut query = use_signal(String::new);
    // Escape hatch: the tag filter is a help until the moment it hides the one
    // sound you want, so it can be turned off without losing the selection.
    let mut all_engines = use_signal(|| false);

    let sel = selection.read().clone();
    let engine = sel.engine().map(|e| e.to_string());
    let accent = engine.as_deref().map(engine_color).unwrap_or("#94a3b8").to_string();
    let q = query().to_lowercase();

    let hits: Vec<(usize, KeysPreset)> = presets
        .iter()
        .enumerate()
        .filter(|(_, p)| sel.accepts(&p.scope))
        .filter(|(_, p)| match (&engine, all_engines()) {
            (Some(e), false) => p.tags.iter().any(|t| t == e),
            _ => true,
        })
        .filter(|(_, p)| {
            q.is_empty()
                || p.name.to_lowercase().contains(&q)
                || p.kind.to_lowercase().contains(&q)
        })
        .map(|(i, p)| (i, p.clone()))
        .collect();

    // Where a click lands, spelled out — the browser is the one surface that
    // loads sounds, so it says what it is about to do.
    let target = match &sel {
        Selection::None => "select a lane to load into".to_string(),
        Selection::Engine(e) => format!("→ {e}"),
        Selection::Layer { layer, .. } => format!("→ {layer}"),
        Selection::Module { layer, module, .. } => {
            format!("→ {layer} · module {}", (b'A' + *module as u8) as char)
        }
    };
    let loadable = !matches!(sel, Selection::None);

    rsx! {
        div {
            style: "display: flex; flex-direction: column; width: 246px; flex-shrink: 0; \
                    min-height: 0; border-right: 1px solid #1c1c1f; background: #0a0a0d;",
            // Header: the level, and what a click will load into.
            div {
                style: "display: flex; flex-direction: column; gap: 8px; padding: 12px; \
                        border-bottom: 1px solid #1c1c1f;",
                div { style: "display: flex; align-items: center; gap: 8px;",
                    span {
                        style: "font-size: 10px; font-weight: 700; letter-spacing: 0.1em; \
                                text-transform: uppercase; color: #a1a1aa;",
                        {sel.level()}
                    }
                    div { style: "flex: 1;" }
                    span { style: "font-size: 9px; color: #52525b;", "{hits.len()}" }
                }
                span {
                    style: format!(
                        "font-size: 10px; color: {}; overflow: hidden; text-overflow: ellipsis; \
                         white-space: nowrap;",
                        if loadable { accent.clone() } else { "#52525b".to_string() },
                    ),
                    "{target}"
                }
                input {
                    style: "background: #131316; border: 1px solid #1f1f23; border-radius: 8px; \
                            padding: 7px 9px; color: #e4e4e7; font-size: 11px;",
                    placeholder: "search",
                    value: "{query}",
                    oninput: move |e| query.set(e.value()),
                }
                if let Some(e) = engine.clone() {
                    button {
                        style: format!(
                            "appearance: none; align-self: flex-start; border: 1px solid {}; \
                             border-radius: 999px; padding: 2px 9px; cursor: pointer; \
                             font-size: 9px; font-weight: 700; background: {}; color: {};",
                            if all_engines() { "#26262b" } else { "#1f2b3a" },
                            if all_engines() { "#131316" } else { "#101821" },
                            if all_engines() { "#52525b".to_string() } else { accent.clone() },
                        ),
                        title: "Filter to this engine's sounds",
                        onclick: move |_| all_engines.toggle(),
                        if all_engines() { "all engines" } else { "{e}" }
                    }
                }
            }
            // The list.
            div { style: "flex: 1; min-height: 0; overflow-y: auto; padding: 6px;",
                if hits.is_empty() {
                    span { style: "display: block; padding: 10px; font-size: 10px; color: #52525b; line-height: 1.5;",
                        if presets.is_empty() {
                            "The library is empty — build a pack, or point the rig at one."
                        } else {
                            "Nothing here for this engine. Try 'all engines'."
                        }
                    }
                }
                for (i, preset) in hits {
                    {
                        let rig = rig.clone();
                        let sel = sel.clone();
                        let accent = accent.clone();
                        rsx! {
                            button {
                                // Library names repeat across banks — key by index.
                                key: "{i}",
                                style: format!(
                                    "width: 100%; appearance: none; text-align: left; border: none; \
                                     border-radius: 7px; padding: 7px 9px; cursor: pointer; \
                                     display: flex; flex-direction: column; gap: 2px; \
                                     background: {}; color: {};",
                                    if preset.loaded { "#101821" } else { "transparent" },
                                    if preset.loaded { accent.clone() } else { "#e4e4e7".to_string() },
                                ),
                                disabled: !loadable,
                                onclick: move |_| {
                                    let (rig, sel) = (rig.clone(), sel.clone());
                                    spawn(async move {
                                        let Some(r) = rig else { return };
                                        match &sel {
                                            // A whole engine program.
                                            Selection::Engine(_) => r.load_preset(i as u32).await.ok(),
                                            // A lane, or one module of it.
                                            Selection::Layer { layer, .. }
                                            | Selection::Module { layer, .. } => {
                                                r.set_layer_patch(
                                                    layer.clone(),
                                                    sel.module(),
                                                    i as u32,
                                                )
                                                .await
                                                .ok()
                                            }
                                            Selection::None => None,
                                        };
                                    });
                                },
                                span {
                                    style: "font-size: 11px; font-weight: 600; line-height: 1.25; \
                                            overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                                    "{preset.name}"
                                }
                                span { style: "font-size: 9px; color: #52525b;", "{preset.kind}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
