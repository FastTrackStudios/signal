//! **Routing view** — the composition tree as it actually renders.
//!
//! The guitar rig's Routing view is a wire graph of FX blocks; the keys rig's
//! is the containment tree: Preset → Engines → Layers → Modules → Blocks,
//! with live (sounding) leaves lit. This is the structure the renderer walks,
//! so it's the honest picture of what the rig is doing.

use dioxus::prelude::*;
use signal_keys_proto::KeysNode;

#[component]
pub fn RoutingView(tree: KeysNode) -> Element {
    if tree.id.is_empty() {
        return rsx! {
            div { style: "flex: 1; display: flex; align-items: center; justify-content: center; \
                          color: #52525b; font-size: 12px;",
                "No program loaded — the rig builds one when audio opens."
            }
        };
    }
    rsx! {
        div { style: "flex: 1; min-height: 0; overflow: auto; padding: 14px;",
            NodeBox { node: tree, depth: 0 }
        }
    }
}

/// One node, recursively. Engines/layers get their role color; blocks light
/// green when they have a live backend.
#[component]
fn NodeBox(node: KeysNode, depth: u32) -> Element {
    let (border, bg) = match node.role.as_str() {
        "preset" => ("#2b2b31", "#0d0d10"),
        "engine" => ("#4c3f6b", "#12101a"),
        "layer" => ("#2b4a6b", "#0f1620"),
        "module" => ("#27272a", "#111113"),
        "block" if node.live => ("#166534", "#0f2417"),
        _ => ("#26262b", "#111113"),
    };
    let dim = node.role == "block" && !node.live;
    let kids = node.children.clone();
    // Layers lay their children out in a row (the chain), engines stack.
    let row = node.role == "layer" || node.role == "module";

    rsx! {
        div {
            style: format!(
                "display: flex; flex-direction: column; gap: 6px; padding: 8px 10px; \
                 border: 1px solid {border}; border-radius: 10px; background: {bg}; \
                 opacity: {};",
                if dim { "0.5" } else { "1" },
            ),
            div { style: "display: flex; align-items: center; gap: 8px;",
                span {
                    style: "font-size: 8px; letter-spacing: 0.12em; text-transform: uppercase; color: #52525b;",
                    "{node.role}"
                }
                span { style: "font-size: 12px; font-weight: 600; color: #e4e4e7;", "{node.label}" }
                if node.role == "block" {
                    span {
                        style: format!(
                            "font-size: 8px; font-weight: 700; letter-spacing: 0.1em; color: {};",
                            if node.live { "#4ade80" } else { "#52525b" },
                        ),
                        if node.live { "LIVE" } else { "EMPTY" }
                    }
                }
            }
            if !kids.is_empty() {
                div {
                    style: format!(
                        "display: flex; gap: 8px; {}",
                        if row { "flex-direction: row; align-items: stretch; flex-wrap: wrap;" } else { "flex-direction: column;" },
                    ),
                    for child in kids.iter() {
                        NodeBox { key: "{child.id}", node: child.clone(), depth: depth + 1 }
                    }
                }
            }
        }
    }
}
