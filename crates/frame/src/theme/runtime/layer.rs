//! A theme from several documents, each over the ones before it.
//!
//! A base theme is a set of files — the strip, the rail, the controls they
//! place — and a theme built on it is the files it changes, nothing else.
//! Layered, later over earlier:
//!
//! - **Pages** (the roles a host draws) by name: a later `mcp.strip`
//!   replaces the earlier one whole.
//! - **Components** by name, wherever each is defined: a file that draws
//!   only `button` changes every mute and solo, and every other control
//!   is still the base's.
//! - **Tokens** by name, and modes in the order first met: a file can
//!   recolour one thing.
//!
//! So any one piece swaps without copying the rest.

use std::collections::BTreeMap;

use crate::document::proto::{Node, NodeId, NodeKind, Scene};

/// The page layered components are gathered on.
const COMPONENTS: &str = "components";

/// `scenes`, later over earlier, as one.
#[must_use]
pub fn layer(scenes: &[Scene]) -> Scene {
    let mut root = Node::new(NodeKind::Document, "Document");
    let mut out = Scene {
        root: root.id,
        nodes: BTreeMap::new(),
        assets: BTreeMap::new(),
        variables: crate::document::proto::Variables::default(),
    };

    // What each name resolves to: the last file's, in first-met order.
    let mut roles: Vec<(String, usize, NodeId)> = Vec::new();
    let mut components: Vec<(String, usize, NodeId)> = Vec::new();
    for (i, scene) in scenes.iter().enumerate() {
        for m in &scene.variables.modes {
            if !out.variables.modes.contains(m) {
                out.variables.modes.push(m.clone());
            }
        }
        for (name, var) in &scene.variables.vars {
            out.variables.vars.insert(name.clone(), var.clone());
        }
        out.assets
            .extend(scene.assets.iter().map(|(k, v)| (k.clone(), v.clone())));
        for &page in scene.pages() {
            let Some(p) = scene.node(page) else { continue };
            if p.children.iter().any(|c| {
                scene
                    .node(*c)
                    .is_some_and(|n| n.kind != NodeKind::Component)
            }) {
                set(&mut roles, &p.name, i, page);
            }
        }
        for n in scene
            .nodes
            .values()
            .filter(|n| n.kind == NodeKind::Component)
        {
            set(&mut components, &n.name, i, n.id);
        }
    }

    for (_, i, page) in &roles {
        copy(&scenes[*i], *page, root.id, &mut out);
        root.children.push(*page);
    }
    if !components.is_empty() {
        let mut shelf = Node::new(NodeKind::Page, COMPONENTS);
        shelf.parent = Some(root.id);
        for (_, i, id) in &components {
            copy(&scenes[*i], *id, shelf.id, &mut out);
            shelf.children.push(*id);
        }
        root.children.push(shelf.id);
        out.nodes.insert(shelf.id, shelf);
    }
    out.nodes.insert(root.id, root);
    out
}

fn set(list: &mut Vec<(String, usize, NodeId)>, name: &str, file: usize, id: NodeId) {
    match list.iter_mut().find(|(n, ..)| n == name) {
        Some(entry) => (entry.1, entry.2) = (file, id),
        None => list.push((name.to_owned(), file, id)),
    }
}

/// `id`'s subtree from `from` into `out`, under `parent` — leaving out any
/// component in it: those are layered on their own.
fn copy(from: &Scene, id: NodeId, parent: NodeId, out: &mut Scene) {
    let Some(mut n) = from.node(id).cloned() else {
        return;
    };
    n.parent = Some(parent);
    let top = n.kind == NodeKind::Component;
    n.children
        .retain(|c| top || from.node(*c).is_some_and(|c| c.kind != NodeKind::Component));
    let children = n.children.clone();
    out.nodes.insert(id, n);
    for c in children {
        copy(from, c, id, out);
    }
}
