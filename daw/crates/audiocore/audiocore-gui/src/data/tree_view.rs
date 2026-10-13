use crate::theme::use_theme;
use nice_plug_dioxus::prelude::*;
use std::collections::HashSet;

/// A node in a tree hierarchy.
#[derive(Clone, Debug, PartialEq)]
pub struct TreeNode {
    pub id: String,
    pub label: String,
    pub children: Vec<TreeNode>,
}

/// Hierarchical expandable tree view.
#[component]
pub fn TreeView(
    root: Vec<TreeNode>,
    #[props(default)] selected: Option<String>,
    on_select: EventHandler<String>,
    #[props(default = "200px")] height: &'static str,
) -> Element {
    let t = use_theme();
    let t = *t.read();
    let expanded = use_signal(HashSet::<String>::new);
    let hovered_id = use_signal(|| None::<String>);

    rsx! {
        div {
            style: format!(
                "height:{height}; overflow-y:auto; {INSET}",
                INSET = t.style_inset(),
            ),

            for node in root.iter() {
                TreeNodeRow {
                    key: "{node.id}",
                    node: node.clone(),
                    depth: 0,
                    selected: selected.clone(),
                    on_select: on_select,
                    expanded: expanded,
                    hovered_id: hovered_id,
                }
            }
        }
    }
}

#[component]
fn TreeNodeRow(
    node: TreeNode,
    depth: usize,
    selected: Option<String>,
    on_select: EventHandler<String>,
    expanded: Signal<HashSet<String>>,
    hovered_id: Signal<Option<String>>,
) -> Element {
    let t = use_theme();
    let t = *t.read();

    let has_children = !node.children.is_empty();
    let is_expanded = expanded.read().contains(&node.id);
    let is_selected = selected.as_ref() == Some(&node.id);
    let is_hovered = hovered_id.read().as_ref() == Some(&node.id);

    let indent = depth * 16;
    let chevron = if !has_children {
        "  "
    } else if is_expanded {
        "\u{25be} "
    } else {
        "\u{25b8} "
    };

    let row_bg = if is_selected {
        t.accent_dim
    } else if is_hovered {
        t.surface_hover
    } else {
        "transparent"
    };
    let row_color = if is_selected { t.text_bright } else { t.text };

    let node_id = node.id.clone();
    let node_id2 = node.id.clone();
    let node_id3 = node.id.clone();

    let mut expanded = expanded;
    let mut hovered_id = hovered_id;

    rsx! {
        div {
            // Row
            div {
                style: format!(
                    "padding:3px 8px 3px {pad}px; cursor:pointer; \
                     font-size:{FSIZE}; color:{row_color}; \
                     background:{row_bg}; display:flex; align-items:center; \
                     user-select:none;",
                    pad = indent + 8,
                    FSIZE = t.font_size_value,
                ),
                onmouseenter: move |_| hovered_id.set(Some(node_id.clone())),
                onmouseleave: move |_| hovered_id.set(None),
                onclick: move |_| {
                    on_select.call(node_id2.clone());
                },

                if has_children {
                    span {
                        style: format!("cursor:pointer; color:{DIM}; width:14px;", DIM = t.text_dim),
                        onclick: move |evt: MouseEvent| {
                            evt.stop_propagation();
                            let mut set = expanded.write();
                            if set.contains(&node_id3) {
                                set.remove(&node_id3);
                            } else {
                                set.insert(node_id3.clone());
                            }
                        },
                        "{chevron}"
                    }
                } else {
                    span {
                        style: "width:14px;",
                        " "
                    }
                }

                span { "{node.label}" }
            }

            // Children
            if has_children && is_expanded {
                for child in node.children.iter() {
                    TreeNodeRow {
                        key: "{child.id}",
                        node: child.clone(),
                        depth: depth + 1,
                        selected: selected.clone(),
                        on_select: on_select,
                        expanded: expanded,
                        hovered_id: hovered_id,
                    }
                }
            }
        }
    }
}
