//! The rig as a tree of nodes, each with the presets it can be recalled as.
//!
//! # Why this is not the chain strip
//!
//! [`crate::chain`] and the control surface both render `LiveBlock` — a flat
//! list of the blocks that process audio. That is the right shape for playing
//! and the wrong one for *choosing*: a Module has nowhere to appear in a flat
//! list, so nothing but a drive slot could ever offer a preset.
//!
//! This renders `LiveNode`, which is the whole tree the rig resolved —
//! containers included, in order, with their depth. A preset picker sits on
//! any node that has more than one, whatever level it is.
//!
//! # A preset is a variant
//!
//! "Preset" is the player's word for what the domain calls a variant of a
//! node: a pedal's captures, an amp's models, a module's combinations. One
//! mechanism, so one picker, and it reads the same wherever it appears.
//!
//! Only nodes with a real choice get one. A node with a single default has
//! nothing to pick between, and a dropdown with one entry is noise.

use dioxus::prelude::*;
use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{LiveNode, LivePreset};

/// The node tree with a preset picker on every node that has a choice.
#[component]
pub fn PresetTree(nodes: Vec<LiveNode>) -> Element {
    if nodes.is_empty() {
        return rsx! {
            div {
                class: "h-full flex items-center justify-center text-muted-foreground text-sm",
                "No rig loaded."
            }
        };
    }

    rsx! {
        div { class: "h-full min-h-0 overflow-y-auto p-3 flex flex-col gap-1",
            for node in nodes {
                NodeRow { node: node.clone() }
            }
        }
    }
}

/// One node: its name, what kind of thing it is, and its presets.
#[component]
fn NodeRow(node: LiveNode) -> Element {
    // Depth as an indent, so the tree reads as a tree. Inline rather than a
    // Tailwind class because the value is computed — see the repo's UI rules
    // on why signal surfaces carry their layout-critical styles inline.
    let indent = format!("margin-left: {}px;", node.depth.min(8) * 14);
    let is_container = !node.is_block;

    rsx! {
        div {
            class: "flex items-center gap-2 py-1 px-2 rounded-md border border-border/60 bg-card/60",
            style: "{indent}",

            // What kind of thing this is. A container's role is the useful
            // word ("module"); a block's type is (“Reverb”), since every
            // block's role is the same.
            span {
                class: "shrink-0 text-[10px] uppercase tracking-wide px-1.5 py-0.5 rounded bg-muted text-muted-foreground",
                if is_container {
                    "{node.role}"
                } else {
                    {node.block_type.map_or_else(|| "block".to_string(), |t| format!("{t:?}").to_lowercase())}
                }
            }

            span {
                class: if node.bypassed {
                    "flex-1 min-w-0 truncate text-sm line-through opacity-50"
                } else if is_container {
                    "flex-1 min-w-0 truncate text-sm font-semibold"
                } else {
                    "flex-1 min-w-0 truncate text-sm"
                },
                "{node.name}"
            }

            if node.bypassed {
                span { class: "shrink-0 text-[10px] text-muted-foreground", "bypassed" }
            }

            if node.presets.len() > 1 {
                PresetPicker {
                    node: node.id.clone(),
                    presets: node.presets.clone(),
                    current: node.preset_id.clone(),
                }
            }

            SavePreset { node: node.id.clone(), name: node.name.clone() }
        }
    }
}

/// The picker itself — presets by **id**, never by list position.
///
/// A list's order changes the moment a capture is imported; an index into it
/// does not survive that, and the wrong pedal setting is a silent failure.
#[component]
fn PresetPicker(node: String, presets: Vec<LivePreset>, current: String) -> Element {
    // The client comes from context rather than a prop: it is not `PartialEq`,
    // which every Dioxus prop must be, and the rest of this UI reads it the
    // same way.
    let rig = use_hook(try_consume_context::<RigClient>);
    rsx! {
        select {
            class: "shrink-0 max-w-[12rem] bg-background/80 border border-border rounded px-1.5 py-0.5 text-xs",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            onchange: {
                let rig = rig.clone();
                let node = node.clone();
                move |e: FormEvent| {
                    let Some(rig) = rig.clone() else { return };
                    let (node, preset) = (node.clone(), e.value());
                    spawn(async move {
                        let _ = rig.select_preset(node, preset).await;
                    });
                }
            },
            for preset in presets {
                option {
                    key: "{preset.id}",
                    value: "{preset.id}",
                    selected: preset.id == current,
                    "{preset.name}"
                }
            }
        }
    }
}

/// Save what a node sounds like now as a preset of it.
///
/// The gesture the rig has never had. Every knob move is already recorded as
/// an override on the active patch — silently and permanently — so a player
/// could tweak endlessly and keep nothing by name. This is the other half:
/// name it, and it becomes a preset of that node, recallable from the picker
/// beside this button.
#[component]
fn SavePreset(node: String, name: String) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut naming = use_signal(|| false);
    let mut draft = use_signal(String::new);

    // A plain fn rather than a closure, so both the Enter key and the button
    // can call it without fighting over one `FnMut`.
    fn save(
        rig: Option<RigClient>,
        mut naming: Signal<bool>,
        mut draft: Signal<String>,
        node: String,
    ) {
        let Some(rig) = rig else { return };
        let preset = draft().trim().to_string();
        if preset.is_empty() {
            return;
        }
        naming.set(false);
        draft.set(String::new());
        spawn(async move {
            let _ = rig.save_preset(node, preset).await;
        });
    }

    if !naming() {
        return rsx! {
            button {
                class: "shrink-0 px-1.5 py-0.5 rounded border border-border text-[10px] text-muted-foreground hover:text-foreground",
                title: "Save {name}'s current settings as a preset",
                onclick: move |_| naming.set(true),
                "Save"
            }
        };
    }

    rsx! {
        div { class: "shrink-0 flex items-center gap-1",
            input {
                class: "w-28 bg-background border border-border rounded px-1.5 py-0.5 text-xs",
                placeholder: "Preset name",
                autofocus: true,
                value: "{draft}",
                oninput: move |e| draft.set(e.value()),
                onkeydown: {
                    let (rig, node) = (rig.clone(), node.clone());
                    move |e: KeyboardEvent| match e.key() {
                        Key::Enter => save(rig.clone(), naming, draft, node.clone()),
                        Key::Escape => {
                            naming.set(false);
                            draft.set(String::new());
                        }
                        _ => {}
                    }
                },
            }
            button {
                class: "px-1.5 py-0.5 rounded bg-accent text-accent-foreground text-[10px]",
                onclick: {
                    let (rig, node) = (rig.clone(), node.clone());
                    move |_| save(rig.clone(), naming, draft, node.clone())
                },
                "Keep"
            }
        }
    }
}
