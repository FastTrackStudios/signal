//! Components placed: every `<Instance of="…">` replaced by a copy of the
//! component it names, its arguments substituted in.
//!
//! A theme draws a control once — a button with its lit, hover and pressed
//! looks as bindings on `arg.on` and `arg.control` — and places it as many
//! times as the view needs, each instance passing its own:
//!
//! ```text
//! <Component name="button" w=21 h=20 arg:on="false" arg:control="\"\"">
//!   <Rectangle w=21 h=20 bind:fill="arg.on ? $mute : $hardware" />
//! </Component>
//! …
//! <Instance name="@mute" of="button" x=30 y=60 arg:on="track.mute" arg:control="\"mute\"" />
//! ```
//!
//! Expansion is a compile step: the compiled role is the copies, with
//! nothing left to resolve when it is drawn, so a component costs what
//! drawing its parts by hand would. The copy's root takes the instance's
//! place — its id, name (so `@mute` is still the hit target), position,
//! size and own bindings — and the component's look inside it.
//!
//! Components are found by name anywhere in the document, so a theme names
//! what it places and a different theme (or a file layered over this one)
//! can draw it differently.

use std::collections::{BTreeMap, HashMap};

use document_proto::{Node, NodeId, NodeKind, Scene};
use theme_expr::Expr;

use crate::compile::Diagnostic;

/// How deep instances may nest in components: a component that places
/// itself would otherwise never finish.
const DEPTH: usize = 32;

/// A binding parsed out of a component copy, its arguments substituted.
pub(crate) struct Parsed {
    pub(crate) prop: String,
    pub(crate) expr: Result<Expr, String>,
}

/// The document with its instances expanded.
pub(crate) struct Expanded {
    pub(crate) scene: Scene,
    /// The bindings of every node copied out of a component, already
    /// parsed in its instance's scope (the node's own `bind` is emptied).
    pub(crate) parsed: HashMap<NodeId, Vec<Parsed>>,
}

/// A component's arguments in one instance: name → the expression it
/// stands for.
type Scope = BTreeMap<String, Expr>;

/// The prefix a component reads its arguments by.
const ARG: &str = "arg.";

pub(crate) fn expand(source: &Scene, diags: &mut Vec<Diagnostic>) -> Expanded {
    let mut components: HashMap<&str, NodeId> = HashMap::new();
    for n in source.nodes.values().filter(|n| n.kind == NodeKind::Component) {
        if components.insert(n.name.as_str(), n.id).is_some() {
            diags.push(Diagnostic { role: None, node: Some(n.name.clone()), message: "two components with this name".into() });
        }
    }
    let mut out = Expanded { scene: source.clone(), parsed: HashMap::new() };
    // The instances outside any component: those inside one are placed as
    // that component is.
    let top: Vec<NodeId> = source
        .nodes
        .values()
        .filter(|n| n.of.is_some() && !inside_component(source, n))
        .map(|n| n.id)
        .collect();
    let mut cx = Cx { source, components: &components, out: &mut out, diags };
    for id in top {
        cx.place(id, None, 0);
    }
    out
}

fn inside_component(scene: &Scene, node: &Node) -> bool {
    let mut at = node.parent;
    while let Some(id) = at {
        let Some(n) = scene.node(id) else { return false };
        if n.kind == NodeKind::Component {
            return true;
        }
        at = n.parent;
    }
    false
}

struct Cx<'a> {
    source: &'a Scene,
    components: &'a HashMap<&'a str, NodeId>,
    out: &'a mut Expanded,
    diags: &'a mut Vec<Diagnostic>,
}

impl Cx<'_> {
    fn err(&mut self, node: &str, message: String) {
        self.diags.push(Diagnostic { role: None, node: Some(node.to_owned()), message });
    }

    /// Replace the instance `id` (in the output scene) by a copy of its
    /// component. `outer` is the scope the instance itself sits in, when
    /// it is inside another component.
    fn place(&mut self, id: NodeId, outer: Option<&Scope>, depth: usize) {
        let Some(instance) = self.out.scene.node(id).cloned() else { return };
        let Some(of) = instance.of.clone() else { return };
        if depth > DEPTH {
            self.err(&instance.name, format!("components nest more than {DEPTH} deep — does `{of}` place itself?"));
            return;
        }
        let Some(&comp_id) = self.components.get(of.as_str()) else {
            let mut known: Vec<&str> = self.components.keys().copied().collect();
            known.sort_unstable();
            self.err(&instance.name, format!("no component `{of}` (components: {})", known.join(", ")));
            return;
        };
        let Some(component) = self.source.node(comp_id).cloned() else { return };

        // The scope: the component's defaults, then what this instance
        // passes, read where the instance is.
        let mut scope = Scope::new();
        for a in &component.args {
            match theme_expr::parse(&a.expr) {
                Ok(e) => {
                    scope.insert(a.name.clone(), e);
                }
                Err(e) => self.err(&component.name, format!("arg:{}: {e}", a.name)),
            }
        }
        for a in &instance.args {
            if !component.args.iter().any(|c| c.name == a.name) {
                let takes: Vec<&str> = component.args.iter().map(|c| c.name.as_str()).collect();
                self.err(&instance.name, format!("`{of}` takes no argument `{}` (it takes: {})", a.name, takes.join(", ")));
                continue;
            }
            match theme_expr::parse(&a.expr).map_err(|e| e.to_string()).and_then(|e| substitute(e, outer)) {
                Ok(e) => {
                    scope.insert(a.name.clone(), e);
                }
                Err(e) => self.err(&instance.name, format!("arg:{}: {e}", a.name)),
            }
        }

        // The copy: the component's subtree under fresh ids, its root on
        // the instance's id so the instance's parent still holds it.
        let subtree = self.source.walk(comp_id);
        let ids: HashMap<NodeId, NodeId> =
            subtree.iter().map(|&old| (old, if old == comp_id { id } else { NodeId::new() })).collect();
        let mut nested = Vec::new();
        for &old in &subtree {
            let Some(mut n) = self.source.node(old).cloned() else { continue };
            n.id = ids[&old];
            n.children = n.children.iter().filter_map(|c| ids.get(c).copied()).collect();
            if old == comp_id {
                n.kind = NodeKind::Frame;
                n.name.clone_from(&instance.name);
                n.parent = instance.parent;
                n.transform = instance.transform;
                if instance.size.width > 0.0 || instance.size.height > 0.0 {
                    n.size = instance.size;
                }
                n.visible = instance.visible && n.visible;
                n.layout_child = instance.layout_child.clone();
                n.args.clear();
            } else {
                n.parent = n.parent.and_then(|p| ids.get(&p).copied());
            }
            if n.of.is_some() {
                // An instance inside: placed below, in this scope.
                nested.push(n.id);
            } else {
                let binds = std::mem::take(&mut n.bind);
                let mut parsed: Vec<Parsed> = binds
                    .into_iter()
                    .map(|b| Parsed {
                        expr: theme_expr::parse(&b.expr).map_err(|e| e.to_string()).and_then(|e| substitute(e, Some(&scope))),
                        prop: b.prop,
                    })
                    .collect();
                if old == comp_id {
                    // The instance's own bindings (where it is, whether it
                    // shows), read where the instance is; after the
                    // component's, so they win.
                    parsed.extend(instance.bind.iter().map(|b| Parsed {
                        prop: b.prop.clone(),
                        expr: theme_expr::parse(&b.expr).map_err(|e| e.to_string()).and_then(|e| substitute(e, outer)),
                    }));
                }
                if !parsed.is_empty() {
                    self.out.parsed.insert(n.id, parsed);
                }
            }
            self.out.scene.nodes.insert(n.id, n);
        }
        for inner in nested {
            self.place(inner, Some(&scope), depth + 1);
        }
    }
}

/// `e` with every `arg.<name>` replaced by the scope's expression for it.
/// Outside a component (`None`) there are no arguments to read.
fn substitute(mut e: Expr, scope: Option<&Scope>) -> Result<Expr, String> {
    let mut missing = None;
    e.substitute_fields(&mut |name| {
        let arg = name.strip_prefix(ARG)?;
        let found = scope.and_then(|s| s.get(arg)).cloned();
        if found.is_none() {
            missing.get_or_insert_with(|| arg.to_owned());
        }
        found
    });
    match (missing, scope) {
        (None, _) => Ok(e),
        (Some(a), Some(s)) => Err(format!(
            "no argument `{a}` (the component takes: {})",
            s.keys().map(String::as_str).collect::<Vec<_>>().join(", ")
        )),
        (Some(a), None) => Err(format!("`arg.{a}` is read outside a component")),
    }
}
