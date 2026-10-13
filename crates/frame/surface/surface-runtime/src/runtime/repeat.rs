//! Dynamic collections: `Repeat` templates drawn once per item, and
//! context subtrees (a popover, a menu) rebound to one item at a time.
//!
//! Expansion happens once, when a runtime is built: each template subtree
//! is cloned per index with ids derived from the template's (`uuid v5`), so
//! the same item keeps the same node ids across rebuilds and live reloads;
//! `{n}` resolves in every clone's addresses, names and text; slot, anchor
//! and menu references to template nodes are remapped to the clone's.

use std::collections::{BTreeMap, HashMap, HashSet};

use document_proto::{Node, NodeId, NodeKind};
use surface_proto::{Control, ControlKind, ParamAddr, substitute};
use uuid::Uuid;

use super::Surface;

/// A clone's id: stable per (template node, index).
#[must_use]
pub fn instance_id(template: NodeId, n: u32) -> NodeId {
    NodeId(Uuid::new_v5(&template.0, &n.to_be_bytes()))
}

/// One repeat template.
#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    pub first: u32,
    pub count: u32,
    /// Templated (`{n}`) address of the item's `used` flag.
    pub used: Option<ParamAddr>,
    /// Instance roots, by index.
    pub instances: BTreeMap<u32, NodeId>,
}

impl Template {
    /// The `used` address of item `n`.
    #[must_use]
    pub fn used_addr(&self, n: u32) -> Option<ParamAddr> {
        self.used
            .as_ref()
            .map(|u| ParamAddr::new(substitute(&u.0, n)))
    }

    /// The prefix every param of item `n` shares (`eq/b3_`): the `used`
    /// address up to its index and the separator after it.
    #[must_use]
    pub fn family_prefix(&self, n: u32) -> Option<String> {
        let used = &self.used.as_ref()?.0;
        let at = used.find(surface_proto::INDEX)? + surface_proto::INDEX.len();
        let sep = used[at..]
            .chars()
            .next()
            .filter(|c| matches!(c, '_' | '/' | '.' | '-'))
            .map_or(0, char::len_utf8);
        Some(substitute(&used[..at + sep], n))
    }
}

/// A subtree whose controls follow one item at a time.
#[derive(Debug, Clone, PartialEq)]
pub struct Context {
    /// The repeat template it follows.
    pub repeat: NodeId,
    /// Its controls as authored (templated).
    pub controls: BTreeMap<NodeId, Control>,
    /// Its text nodes' templated characters.
    pub texts: BTreeMap<NodeId, String>,
    /// The item it is bound to now.
    pub bound: Option<u32>,
}

/// What expansion made.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Expansion {
    pub templates: BTreeMap<NodeId, Template>,
    /// Every node of an instance → (instance root, template, index).
    pub owner: HashMap<NodeId, (NodeId, NodeId, u32)>,
    /// Nodes of templates themselves (never drawn, never hit).
    pub template_nodes: HashSet<NodeId>,
    /// Context roots (popovers and menus with a `context`).
    pub contexts: BTreeMap<NodeId, Context>,
}

impl Expansion {
    /// The item (template, index) a node belongs to.
    #[must_use]
    pub fn item_of(&self, id: NodeId) -> Option<(NodeId, u32)> {
        self.owner.get(&id).map(|(_, t, n)| (*t, *n))
    }

    /// The instance of `template` for index `n`, mapped from a node of the
    /// template (`None` = not a template node).
    #[must_use]
    pub fn map_node(&self, template_node: NodeId, n: u32) -> NodeId {
        instance_id(template_node, n)
    }
}

/// Expand every `Repeat` in `surface`; returns the expanded surface and
/// what was made.
#[must_use]
pub fn expand(mut surface: Surface) -> (Surface, Expansion) {
    let mut ex = Expansion::default();
    let order = surface.scene.walk(surface.scene.root);
    let repeats: Vec<(NodeId, u32, u32, Option<ParamAddr>)> = order
        .iter()
        .filter_map(|id| match surface.controls.get(id).map(|c| &c.kind) {
            Some(ControlKind::Repeat { first, count, used }) => {
                Some((*id, *first, *count, used.clone()))
            }
            _ => None,
        })
        .collect();
    for (root, first, count, used) in repeats {
        // Nested templates expand with their outer one only.
        if ex.template_nodes.contains(&root) {
            continue;
        }
        let nodes = surface.scene.walk(root);
        ex.template_nodes.extend(nodes.iter().copied());
        let ids: Vec<String> = nodes.iter().map(|id| id.0.to_string()).collect();
        let remap = |json: String, n: u32| {
            let mut json = json;
            for (id, s) in nodes.iter().zip(&ids) {
                if json.contains(s.as_str()) {
                    json = json.replace(s.as_str(), &instance_id(*id, n).0.to_string());
                }
            }
            json
        };
        let parent = surface.scene.node(root).and_then(|n| n.parent);
        let mut template = Template {
            first,
            count,
            used,
            instances: BTreeMap::new(),
        };
        let mut at = parent
            .and_then(|p| surface.scene.node(p))
            .and_then(|p| p.children.iter().position(|c| *c == root))
            .map_or(usize::MAX, |i| i + 1);
        for n in first..first.saturating_add(count) {
            let inst_root = instance_id(root, n);
            for id in &nodes {
                let Some(src) = surface.scene.node(*id) else {
                    continue;
                };
                let mut node: Node = src.clone();
                node.id = instance_id(*id, n);
                node.parent = if *id == root {
                    parent
                } else {
                    src.parent.map(|p| instance_id(p, n))
                };
                node.children = src.children.iter().map(|c| instance_id(*c, n)).collect();
                node.name = substitute(&src.name, n);
                if let NodeKind::Text(t) = &mut node.kind {
                    t.characters = substitute(&t.characters, n);
                    for r in &mut t.runs {
                        r.text = substitute(&r.text, n);
                    }
                }
                if *id == root {
                    node.visible = src.visible;
                }
                let control = surface
                    .controls
                    .get(id)
                    .filter(|c| !matches!(c.kind, ControlKind::Repeat { .. }));
                node.control = control.map(|c| c.instantiate(n, |j| remap(j, n)));
                if let Some(c) = &node.control {
                    surface.controls.insert(node.id, c.clone());
                }
                ex.owner.insert(node.id, (inst_root, root, n));
                surface.scene.nodes.insert(node.id, node);
            }
            if let Some(p) = parent.and_then(|p| surface.scene.nodes.get_mut(&p)) {
                let i = at.min(p.children.len());
                p.children.insert(i, inst_root);
                at = i + 1;
            }
            template.instances.insert(n, inst_root);
        }
        // The template stays in the tree, hidden and inert.
        for id in &nodes {
            surface.controls.remove(id);
        }
        if let Some(n) = surface.scene.node_mut(root) {
            n.visible = false;
        }
        ex.templates.insert(root, template);
    }

    // Contexts: one subtree, rebound to the active item.
    let roots: Vec<(NodeId, NodeId)> = surface
        .controls
        .iter()
        .filter_map(|(id, c)| match &c.kind {
            ControlKind::Popover {
                context: Some(r), ..
            }
            | ControlKind::Menu {
                context: Some(r), ..
            } => Some((*id, NodeId(*r))),
            _ => None,
        })
        .collect();
    for (root, repeat) in roots {
        let mut ctx = Context {
            repeat,
            controls: BTreeMap::new(),
            texts: BTreeMap::new(),
            bound: None,
        };
        for id in surface.scene.walk(root) {
            if let Some(c) = surface.controls.get(&id) {
                ctx.controls.insert(id, c.clone());
            }
            if let Some(t) = surface.scene.node(id).and_then(Node::text)
                && surface_proto::is_templated(&t.characters)
            {
                ctx.texts.insert(id, t.characters.clone());
            }
        }
        ex.contexts.insert(root, ctx);
    }
    let firsts: Vec<(NodeId, u32)> = ex
        .contexts
        .iter()
        .map(|(root, c)| (*root, ex.templates.get(&c.repeat).map_or(1, |t| t.first)))
        .collect();
    for (root, n) in firsts {
        bind_context(&mut surface, &mut ex, root, n);
    }
    (surface, ex)
}

/// Bind context `root` to item `n`: its controls resolve `{n}` (and point
/// at item `n`'s instance of any template node they name).
pub fn bind_context(surface: &mut Surface, ex: &mut Expansion, root: NodeId, n: u32) -> bool {
    let Some(ctx) = ex.contexts.get_mut(&root) else {
        return false;
    };
    if ctx.bound == Some(n) {
        return false;
    }
    ctx.bound = Some(n);
    let template_ids: Vec<(String, NodeId)> = ex
        .template_nodes
        .iter()
        .filter(|id| !ex.owner.contains_key(id))
        .map(|id| (id.0.to_string(), *id))
        .collect();
    for (id, c) in &ctx.controls {
        let bound = c.instantiate(n, |mut json| {
            for (s, tid) in &template_ids {
                if json.contains(s.as_str()) {
                    json = json.replace(s.as_str(), &instance_id(*tid, n).0.to_string());
                }
            }
            json
        });
        surface.controls.insert(*id, bound);
    }
    true
}
