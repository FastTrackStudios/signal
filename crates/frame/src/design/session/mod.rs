//! A design session: one document, one catalog, one undo history, and one
//! tool registry.
//!
//! Every AI surface — MCP, in-app chat, the CLI — lists [`tools`] and
//! dispatches [`Session::call`]. Each call is a single atomic transaction:
//! it applies fully or not at all, and one `undo` reverts it. Tools speak
//! frame markup (see `document/text/REFERENCE.md`) and return text meant
//! for a model to read: what changed, ids to refer to, and lint results.

use std::collections::BTreeMap;

// The `animate` tool: motions on nodes.
mod animate;

use crate::design::lint::{Config, Issue, Severity};
use crate::document::proto::{History, Node, NodeId, NodeKind, Op, OpError, Prop, Scene};
use crate::surface::proto::{
    Binding, ButtonMode, Catalog, Control, ControlKind, EntryId, Orientation, ParamKind, Slot,
    SlotRole, Source,
};
use crate::surface::widgets::{Builder, ControlSize, Options, Theme, surface_for_catalog};
use serde_json::{Value, json};

/// A tool as adapters advertise it.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    /// JSON Schema for the arguments object.
    pub input_schema: Value,
    /// Whether the tool edits the document.
    pub mutates: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolError(pub String);

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ToolError {}

impl From<OpError> for ToolError {
    fn from(e: OpError) -> Self {
        Self(e.to_string())
    }
}

type Result<T> = std::result::Result<T, ToolError>;

fn err<T>(msg: impl Into<String>) -> Result<T> {
    Err(ToolError(msg.into()))
}

pub struct Session {
    scene: Scene,
    catalog: Catalog,
    theme: Theme,
    history: History,
    lint: Config,
}

impl Session {
    #[must_use]
    pub fn new(scene: Scene, catalog: Catalog) -> Self {
        Self {
            scene,
            catalog,
            theme: Theme::default(),
            history: History::default(),
            lint: Config::default(),
        }
    }

    /// An empty page, ready to design against `catalog`.
    #[must_use]
    pub fn blank(catalog: Catalog) -> Self {
        Self::new(Scene::new("Surface"), catalog)
    }

    /// Edit the surface at `path` (`.frame` with its `.frame.assets/` textures
    /// and fonts, a legacy `.fm`, or a `.frame` bundle).
    ///
    /// # Errors
    /// See [`crate::document::io::load_surface`].
    pub fn open(
        path: impl AsRef<std::path::Path>,
        catalog: Catalog,
    ) -> std::result::Result<Self, crate::document::io::Error> {
        Ok(Self::new(crate::document::io::load_surface(path)?, catalog))
    }

    /// Write the document to `path`, assets and all (see
    /// [`crate::document::io::save_surface`]).
    ///
    /// # Errors
    /// I/O, or an extension that isn't `.frame` / `.fm`.
    pub fn save(
        &self,
        path: impl AsRef<std::path::Path>,
    ) -> std::result::Result<(), crate::document::io::Error> {
        crate::document::io::save_surface(path, &self.scene)
    }

    #[must_use]
    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    #[must_use]
    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    /// Design against a different catalog (e.g. a live engine's, re-fetched).
    /// The document is untouched; bindings the new catalog lacks show up
    /// in [`issues`](Self::issues).
    pub fn set_catalog(&mut self, catalog: Catalog) {
        self.catalog = catalog;
    }

    #[must_use]
    pub fn issues(&self) -> Vec<Issue> {
        crate::design::lint::lint(&self.scene, &self.catalog, &self.lint)
    }

    /// Run a tool by name with a JSON arguments object.
    ///
    /// # Errors
    /// A message for the model: unknown tool, bad arguments, parse errors
    /// (with line:col), or ops that don't fit the document.
    pub fn call(&mut self, name: &str, args: &Value) -> Result<String> {
        let a = Args(args);
        match name {
            "describe" => self.describe(a.opt_node(&self.scene, "node")?),
            "read" => self.read(a.opt_node(&self.scene, "node")?),
            "catalog" => Ok(self.catalog_text()),
            "lint" => Ok(self.lint_text()),
            "write" => {
                let parent = a.node(&self.scene, "parent")?;
                self.write(parent, a.str("markup")?, a.opt_u32("index")?)
            }
            "replace" => {
                let node = a.node(&self.scene, "node")?;
                self.replace(node, a.str("markup")?)
            }
            "remove" => {
                let node = a.node(&self.scene, "node")?;
                self.commit(vec![Op::Remove { id: node }])
                    .map(|()| format!("removed {node}"))
            }
            "move" => {
                let node = a.node(&self.scene, "node")?;
                let parent = a.node(&self.scene, "parent")?;
                let index = a.opt_u32("index")?.unwrap_or(u32::MAX);
                self.commit(vec![Op::Move {
                    id: node,
                    parent,
                    index,
                }])
                .map(|()| format!("moved {node}"))
            }
            "add_control" => {
                let parent = a.node(&self.scene, "parent")?;
                self.add_control(
                    parent,
                    a.str("kind")?,
                    a.opt_str("bind")?,
                    a.opt_str("size")?,
                    a.opt_str("name")?,
                )
            }
            "components" => Ok(components_text(a.opt_str("family")?)),
            "fonts" => Ok(fonts_text()),
            "add_component" => {
                let parent = a.node(&self.scene, "parent")?;
                self.add_component(parent, &a)
            }
            "bind" => {
                let node = a.node(&self.scene, "node")?;
                self.bind(
                    node,
                    a.opt_str("source")?,
                    a.opt_str("kind")?,
                    a.0.get("slots"),
                )
            }
            "generate" => self.generate(a.opt_str("theme")?),
            "animate" => {
                let node = a.node(&self.scene, "node")?;
                self.animate(node, &a)
            }
            "set_theme" => {
                self.theme = parse_theme(a.str("theme")?)?;
                Ok(format!(
                    "theme is now {} (applies to controls added from here on)",
                    self.theme.name
                ))
            }
            "undo" => Ok(if self.history.undo(&mut self.scene)? {
                "undone".into()
            } else {
                "nothing to undo".into()
            }),
            "redo" => Ok(if self.history.redo(&mut self.scene)? {
                "redone".into()
            } else {
                "nothing to redo".into()
            }),
            other => err(format!(
                "unknown tool `{other}`; tools: {}",
                tools()
                    .iter()
                    .map(|t| t.name)
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }

    /// Apply `ops` as one undoable step, re-hugging containers in the
    /// same transaction.
    fn commit(&mut self, mut ops: Vec<Op>) -> Result<()> {
        let mut probe = self.scene.clone();
        probe.transact(ops.clone())?;
        ops.extend(probe.hug_ops(probe.root));
        Ok(self.history.commit(&mut self.scene, ops)?)
    }

    fn page(&self) -> NodeId {
        self.scene
            .pages()
            .first()
            .copied()
            .unwrap_or(self.scene.root)
    }

    // ── reading ──────────────────────────────────────────────────────────

    fn describe(&self, node: Option<NodeId>) -> Result<String> {
        let root = node.unwrap_or(self.scene.root);
        let mut out = String::new();
        let mut stack = vec![(root, 0usize)];
        while let Some((id, depth)) = stack.pop() {
            let Some(n) = self.scene.node(id) else {
                continue;
            };
            out.push_str(&"  ".repeat(depth));
            out.push_str(&outline_line(n));
            out.push('\n');
            for c in n.children.iter().rev() {
                stack.push((*c, depth + 1));
            }
        }
        let issues = self.issues();
        let (e, w, i) = crate::design::lint::counts(&issues);
        out.push_str(&format!(
            "\nlint: {e} errors, {w} warnings, {i} notes (run `lint` for details)\n"
        ));
        Ok(out)
    }

    fn read(&self, node: Option<NodeId>) -> Result<String> {
        match node {
            None => Ok(crate::document::text::print(&self.scene)),
            Some(id) => crate::document::text::print_subtree(&self.scene, id)
                .ok_or_else(|| ToolError(format!("no node {id}"))),
        }
    }

    fn catalog_text(&self) -> String {
        let c = &self.catalog;
        let mut out = format!(
            "engine `{}` v{}\n\nparams (bind as param:<addr>; ★ = featured, what a performer reaches for):\n",
            c.id, c.version
        );
        for (group, params) in c.groups() {
            out.push_str(&format!("  [{}]\n", group.unwrap_or("Other")));
            for p in params {
                let kind = match &p.kind {
                    ParamKind::Continuous => format!(
                        "{}..{} {}{}",
                        p.range.format(p.range.min),
                        p.range.format(p.range.max),
                        if p.bipolar { "bipolar " } else { "" },
                        match p.range.scale {
                            crate::surface::proto::Scale::Logarithmic => "log",
                            _ => "",
                        }
                    ),
                    ParamKind::Toggle => "toggle".into(),
                    ParamKind::Text => "text (bind a text-field)".into(),
                    ParamKind::Choice { options } => format!("choice: {}", options.join(" | ")),
                };
                let star = if p.featured { "★ " } else { "  " };
                out.push_str(&format!(
                    "  {star}{:<18} {:<12} {}  default {}\n",
                    p.addr.0,
                    p.name,
                    kind.trim(),
                    p.display(p.default)
                ));
            }
        }
        out.push_str("\nactions (bind as action:<id>):\n");
        for a in &c.actions {
            out.push_str(&format!("    {:<18} {}\n", a.id.0, a.name));
        }
        out.push_str("\nstreams (bind as stream:<id>[:channel]):\n");
        for s in &c.streams {
            out.push_str(&format!(
                "    {:<18} {} ({:?}, {} Hz)\n",
                s.id.0, s.name, s.kind, s.rate_hz
            ));
        }
        out
    }

    fn lint_text(&self) -> String {
        let issues = self.issues();
        if issues.is_empty() {
            return "clean: no issues".into();
        }
        let (e, w, i) = crate::design::lint::counts(&issues);
        let mut out = format!("{e} errors, {w} warnings, {i} notes\n");
        for issue in issues
            .iter()
            .filter(|i| i.severity != Severity::Info)
            .chain(
                issues
                    .iter()
                    .filter(|i| i.severity == Severity::Info)
                    .take(10),
            )
        {
            out.push_str(&format!("{issue}\n"));
        }
        out
    }

    // ── writing ──────────────────────────────────────────────────────────

    fn write(&mut self, parent: NodeId, markup: &str, index: Option<u32>) -> Result<String> {
        let nodes = crate::document::text::parse_fragment(markup)
            .map_err(|e| ToolError(format!("markup {e}")))?;
        let ops = insert_ops(&self.scene, parent, index, nodes)?;
        let created: Vec<String> = ops
            .iter()
            .filter_map(|o| match o {
                Op::Insert {
                    node, parent: p, ..
                } if *p == parent => Some(format!("{} \"{}\"", node.id, node.name)),
                _ => None,
            })
            .collect();
        let n = ops.len();
        self.commit(ops)?;
        Ok(format!(
            "inserted {n} nodes; top-level: {}\n{}",
            created.join(", "),
            self.lint_summary()
        ))
    }

    fn replace(&mut self, node: NodeId, markup: &str) -> Result<String> {
        let n = self
            .scene
            .node(node)
            .ok_or_else(|| ToolError(format!("no node {node}")))?;
        let parent = n
            .parent
            .ok_or_else(|| ToolError("can't replace the root".into()))?;
        let index = self
            .scene
            .node(parent)
            .and_then(|p| p.children.iter().position(|c| *c == node))
            .unwrap_or(0);
        let mut nodes = crate::document::text::parse_fragment(markup)
            .map_err(|e| ToolError(format!("markup {e}")))?;
        // Replacing in place keeps the node's identity (and anything bound to it).
        if let Some(top) = nodes.iter_mut().find(|n| n.parent.is_none()) {
            let new_id = top.id;
            top.id = node;
            for n in &mut nodes {
                if n.parent == Some(new_id) {
                    n.parent = Some(node);
                }
            }
        }
        let mut ops = vec![Op::Remove { id: node }];
        // The remove happens first inside the transaction; build inserts
        // against a scene without the old subtree.
        let mut probe = self.scene.clone();
        probe.apply(Op::Remove { id: node })?;
        ops.extend(insert_ops(
            &probe,
            parent,
            Some(u32::try_from(index).unwrap_or(u32::MAX)),
            nodes,
        )?);
        self.commit(ops)?;
        Ok(format!("replaced {node}\n{}", self.lint_summary()))
    }

    fn add_control(
        &mut self,
        parent: NodeId,
        kind: &str,
        bind: Option<&str>,
        size: Option<&str>,
        name: Option<&str>,
    ) -> Result<String> {
        let binding = bind.map(parse_source).transpose()?;
        let size = match size.unwrap_or("medium") {
            "small" => ControlSize::Small,
            "medium" => ControlSize::Medium,
            "large" => ControlSize::Large,
            "stage" => ControlSize::Stage,
            other => {
                return err(format!(
                    "unknown size `{other}`; use small | medium | large | stage"
                ));
            }
        };
        let default_name = binding
            .as_ref()
            .and_then(|b| match &b.source {
                Source::Param { addr } => self.catalog.param(addr).map(|p| p.name.clone()),
                Source::Action { id } => self.catalog.action(id).map(|a| a.name.clone()),
                Source::Stream { id, .. } => self.catalog.stream(id).map(|s| s.name.clone()),
                Source::Collection { id } => self.catalog.collection(id).map(|c| c.name.clone()),
            })
            .unwrap_or_else(|| kind.to_string());
        // Keep control names (and so labels) unique: "Mix" twice becomes
        // "Delay Mix" / "Reverb Mix".
        let group = binding.as_ref().and_then(|b| match &b.source {
            Source::Param { addr } => self.catalog.param(addr).and_then(|p| p.group.clone()),
            _ => None,
        });
        let taken = |n: &str| {
            self.scene
                .nodes
                .values()
                .any(|x| x.name == n && x.control.is_some())
        };
        let name = match (name, group) {
            (Some(n), _) => n.to_string(),
            (None, Some(g)) if taken(&default_name) && !default_name.starts_with(&g) => {
                format!("{g} {default_name}")
            }
            (None, _) => default_name.clone(),
        };
        let authored_label = (name != default_name).then(|| name.clone());
        let theme = self.theme.clone();
        let (ops, root) = self.build(parent, |b| match kind {
            "knob" => Ok(b.knob(parent, &name, size, binding.clone())),
            "fader" => Ok(b.fader(parent, &name, size.px() * 3.0, binding.clone())),
            "button" => Ok(b.button(parent, &name, size, ButtonMode::Toggle, binding.clone())),
            "trigger" => Ok(b.button(parent, &name, size, ButtonMode::Trigger, binding.clone())),
            "footswitch" => Ok(b.footswitch(parent, &name, binding.clone())),
            "meter" => Ok(b.meter(parent, &name, size.px() * 3.5, binding.clone())),
            "readout" => Ok(b.readout(parent, &name, size.px() * 1.5, binding.clone())),
            other => err(format!("unknown control kind `{other}`; use knob | fader | button | trigger | footswitch | meter | readout")),
        }, &theme)?;
        let ops = match authored_label {
            Some(label) => with_label(ops, root, &label),
            None => ops,
        };
        self.commit(ops)?;
        Ok(format!(
            "added {kind} {root} \"{name}\"\n{}",
            self.lint_summary()
        ))
    }

    fn add_component(&mut self, parent: NodeId, a: &Args<'_>) -> Result<String> {
        let id = a.str("component")?;
        let binding = a.opt_str("bind")?.map(parse_source).transpose()?;
        let list = |k: &str| -> Result<Option<Vec<String>>> {
            match a.0.get(k) {
                None | Some(Value::Null) => Ok(None),
                Some(Value::Array(v)) => Ok(Some(
                    v.iter()
                        .map(|x| {
                            x.as_str()
                                .map_or_else(|| x.to_string(), ToString::to_string)
                        })
                        .collect(),
                )),
                Some(_) => err(format!("`{k}` must be an array of strings")),
            }
        };
        let default_name = binding
            .as_ref()
            .and_then(|b| match &b.source {
                Source::Param { addr } => self.catalog.param(addr).map(|p| p.name.clone()),
                Source::Action { id } => self.catalog.action(id).map(|a| a.name.clone()),
                Source::Stream { id, .. } => self.catalog.stream(id).map(|s| s.name.clone()),
                Source::Collection { id } => self.catalog.collection(id).map(|c| c.name.clone()),
            })
            .unwrap_or_else(|| id.rsplit('.').next().unwrap_or(id).to_string());
        // A choice param's options label a bank / selector unless given.
        let options = list("options")?.or_else(|| match binding.as_ref().map(|b| &b.source) {
            Some(Source::Param { addr }) => match self.catalog.param(addr).map(|p| &p.kind) {
                Some(ParamKind::Choice { options }) => Some(options.clone()),
                _ => None,
            },
            _ => None,
        });
        let number = |k: &str| -> Result<Option<f64>> {
            match a.0.get(k) {
                None | Some(Value::Null) => Ok(None),
                Some(v) => {
                    Ok(Some(v.as_f64().ok_or_else(|| {
                        ToolError(format!("`{k}` must be a number"))
                    })?))
                }
            }
        };
        let flag = |k: &str| a.0.get(k).and_then(Value::as_bool).unwrap_or(false);
        let ink = match a.opt_str("ink")? {
            None => None,
            Some("light") => Some(crate::surface::kit::Ink::Light),
            Some("dark") => Some(crate::surface::kit::Ink::Dark),
            Some(c) if c.starts_with('#') => Some(crate::surface::kit::Ink::Color(c.to_string())),
            Some(other) => return err(format!("`ink` is light | dark | #hex, not `{other}`")),
        };
        let place = crate::surface::kit::Place {
            name: a.opt_str("name")?.map_or(default_name, ToString::to_string),
            size: number("size")?,
            binding,
            variant: a.opt_str("variant")?.map(ToString::to_string),
            legend: a.opt_str("legend")?.map(ToString::to_string),
            scale: list("scale")?,
            options,
            ink,
            keep_case: flag("keep_case"),
            sweep: number("sweep")?,
            touch: number("touch")?,
            reverse: flag("reverse"),
            ..crate::surface::kit::Place::default()
        };
        let theme = self.theme.clone();
        let (ops, root) = self.build(
            parent,
            |b| crate::surface::kit::place(b, parent, id, &place).map_err(ToolError),
            &theme,
        )?;
        self.commit(ops)?;
        Ok(format!(
            "added {id} {root} \"{}\"\n{}",
            place.name,
            self.lint_summary()
        ))
    }

    /// Run a widget builder against a scratch copy and turn what it added
    /// into `Insert` ops (so it's one undoable transaction).
    fn build(
        &self,
        parent: NodeId,
        f: impl FnOnce(&mut Builder<'_>) -> Result<NodeId>,
        theme: &Theme,
    ) -> Result<(Vec<Op>, NodeId)> {
        if self.scene.node(parent).is_none() {
            return err(format!("no node {parent}"));
        }
        let mut scratch = self.scene.clone();
        let mut controls = BTreeMap::new();
        let root = {
            let mut b = Builder {
                scene: &mut scratch,
                controls: &mut controls,
                theme,
            };
            f(&mut b)?
        };
        for (id, c) in controls {
            if let Some(n) = scratch.node_mut(id) {
                n.control = Some(c);
            }
        }
        // Assets the builder added (a material's texture tiles) go in first.
        let mut ops: Vec<Op> = scratch
            .assets
            .values()
            .filter(|a| self.scene.asset(&a.id).is_none())
            .map(|a| Op::PutAsset { asset: a.clone() })
            .collect();
        for id in scratch.walk(root) {
            let mut n = scratch
                .node(id)
                .cloned()
                .ok_or_else(|| ToolError("builder lost a node".into()))?;
            let p = n.parent.unwrap_or(parent);
            let index = if id == root {
                u32::MAX
            } else {
                position(&scratch, p, id)
            };
            n.children.clear();
            ops.push(Op::Insert {
                node: n,
                parent: p,
                index,
            });
        }
        Ok((ops, root))
    }

    fn bind(
        &mut self,
        node: NodeId,
        source: Option<&str>,
        kind: Option<&str>,
        slots: Option<&Value>,
    ) -> Result<String> {
        let n = self
            .scene
            .node(node)
            .ok_or_else(|| ToolError(format!("no node {node}")))?;
        let binding = source.map(parse_source).transpose()?;
        let mut control = match (n.control.clone(), kind) {
            (_, Some(k)) => {
                let mut c = Control::new(parse_kind(k)?);
                if let Some(old) = &n.control {
                    c.binding = old.binding.clone();
                    c.slots = old.slots.clone();
                    c.midi = old.midi.clone();
                }
                c
            }
            (Some(c), None) => c,
            (None, None) => {
                return err(
                    "this node isn't a control yet; pass `kind` (knob | fader | button | trigger | footswitch | meter | readout | xy | selector | plot | spectrum | history)",
                );
            }
        };
        if binding.is_some() {
            control.binding = binding;
        }
        if let Some(slots) = slots {
            let map = slots.as_object().ok_or_else(|| {
                ToolError("`slots` must be an object like {\"needle\": \"<node>\"}".into())
            })?;
            control.slots.clear();
            for (role, v) in map {
                let target = v
                    .as_str()
                    .ok_or_else(|| ToolError(format!("slot `{role}` must name a node")))?;
                control.slots.push(Slot {
                    role: parse_role(role)?,
                    node: resolve(&self.scene, target)?.0,
                });
            }
        }
        self.commit(vec![Op::Set {
            id: node,
            prop: Prop::Control(Some(control)),
        }])?;
        Ok(format!("bound {node}\n{}", self.lint_summary()))
    }

    fn generate(&mut self, theme: Option<&str>) -> Result<String> {
        let theme = parse_theme(theme.unwrap_or("stage-dark"))?;
        self.theme = theme.clone();
        let generated = surface_for_catalog(
            &self.catalog,
            &Options {
                theme,
                ..Options::default()
            },
        )
        .to_scene();
        let gen_page = generated.pages()[0];
        let page = self.page();
        let mut ops: Vec<Op> = self
            .scene
            .node(page)
            .map(|p| p.children.iter().map(|c| Op::Remove { id: *c }).collect())
            .unwrap_or_default();
        for id in generated.walk(gen_page).into_iter().skip(1) {
            let mut n = generated
                .node(id)
                .cloned()
                .ok_or_else(|| ToolError("generator lost a node".into()))?;
            let p = if n.parent == Some(gen_page) {
                page
            } else {
                n.parent.unwrap_or(page)
            };
            n.children.clear();
            ops.push(Op::Insert {
                node: n,
                parent: p,
                index: u32::MAX,
            });
        }
        self.commit(ops)?;
        Ok(format!(
            "generated a surface for `{}` ({} controls)\n{}",
            self.catalog.id,
            self.scene
                .nodes
                .values()
                .filter(|n| n.control.is_some())
                .count(),
            self.lint_summary()
        ))
    }

    fn lint_summary(&self) -> String {
        let issues = self.issues();
        let (e, w, _) = crate::design::lint::counts(&issues);
        let mut s = format!("lint: {e} errors, {w} warnings");
        for i in issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .take(5)
        {
            s.push_str(&format!("\n  {i}"));
        }
        s
    }
}

/// Write `label` into the control's Label slot node (an authored label wins
/// over the parameter name at runtime).
fn with_label(mut ops: Vec<Op>, root: NodeId, label: &str) -> Vec<Op> {
    let slot = ops.iter().find_map(|o| match o {
        Op::Insert { node, .. } if node.id == root => {
            node.control.as_ref()?.slots_of(SlotRole::Label).next()
        }
        _ => None,
    });
    if let Some(slot) = slot {
        for o in &mut ops {
            if let Op::Insert { node, .. } = o
                && node.id.0 == slot
                && let NodeKind::Text(t) = &mut node.kind
            {
                t.characters = label.to_string();
            }
        }
    }
    ops
}

/// The bundled font families, for a model lettering a surface.
fn fonts_text() -> String {
    let mut out = String::from(
        "fonts every renderer bundles — <Text font=\"…\" weight=100–900 style=\"italic\" stretch=75> (runs: font_family, font_weight, font_style, font_width).\n\
         Unknown families and missing glyphs fall back to Inter; missing weights are emboldened, missing italics slanted. \
         Use real weight and family, never overlapping copies or outlines to fake them.\n\n",
    );
    for f in crate::render::font_book::BUNDLED {
        let weights = if f.weights.0 == f.weights.1 {
            f.weights.0.to_string()
        } else {
            format!("{}–{}", f.weights.0, f.weights.1)
        };
        let mut extra = Vec::new();
        if f.italic {
            extra.push("italic".to_string());
        }
        if f.widths.0 != f.widths.1 {
            extra.push(format!("stretch {}–{}", f.widths.0, f.widths.1));
        }
        let extra = if extra.is_empty() {
            String::new()
        } else {
            format!(", {}", extra.join(", "))
        };
        out.push_str(&format!(
            "  {:<18} weight {weights}{extra} — {}\n",
            f.name, f.role
        ));
    }
    out.push_str("\ngeneric: sans-serif (Inter), serif (Playfair Display), monospace (JetBrains Mono), cursive (Dancing Script), fantasy (Pirata One); also DSEG7, DSEG14, lcd, dot-matrix, marker, blackletter.\n");
    out
}

/// The kit, grouped by family, for a model choosing components.
fn components_text(family: Option<&str>) -> String {
    let all: Vec<_> = crate::surface::kit::components()
        .into_iter()
        .filter(|c| family.is_none_or(|f| c.family.name() == f))
        .collect();
    if all.is_empty() {
        return "no components (for that family); families: knob, meter, button, switch, lever, panel, material".into();
    }
    let mut out = String::from(
        "hardware kit — place with add_component {parent, component, bind, size?, variant?, legend?, scale?, options?}\n",
    );
    let mut fam = None;
    for c in all {
        if fam != Some(c.family) {
            out.push_str(&format!("\n[{}]\n", c.family.name()));
            fam = Some(c.family);
        }
        let variants = if c.variants.is_empty() {
            String::new()
        } else {
            format!(" variants: {}.", c.variants.join(" | "))
        };
        out.push_str(&format!(
            "  {:<24} binds {:<13} {}px — {}{}\n",
            c.id, c.binds, c.default_size, c.description, variants
        ));
    }
    out
}

fn position(scene: &Scene, parent: NodeId, id: NodeId) -> u32 {
    scene
        .node(parent)
        .and_then(|p| p.children.iter().position(|c| *c == id))
        .map_or(u32::MAX, |i| u32::try_from(i).unwrap_or(u32::MAX))
}

/// Insert ops for a parsed fragment (pre-order, parents wired; top-level
/// nodes go under `parent` starting at `index`).
fn insert_ops(
    scene: &Scene,
    parent: NodeId,
    index: Option<u32>,
    nodes: Vec<Node>,
) -> Result<Vec<Op>> {
    if scene.node(parent).is_none() {
        return err(format!("no node {parent}"));
    }
    let mut next = index.unwrap_or(u32::MAX);
    let mut ops = Vec::new();
    for mut n in nodes {
        if scene.node(n.id).is_some() {
            return err(format!(
                "id {} is already in the document; omit `id` to get a fresh one",
                n.id
            ));
        }
        n.children.clear();
        match n.parent {
            Some(p) => ops.push(Op::Insert {
                node: n,
                parent: p,
                index: u32::MAX,
            }),
            None => {
                ops.push(Op::Insert {
                    node: n,
                    parent,
                    index: next,
                });
                if next != u32::MAX {
                    next += 1;
                }
            }
        }
    }
    Ok(ops)
}

fn outline_line(n: &Node) -> String {
    let kind = match &n.kind {
        NodeKind::Text(t) => format!(
            "Text \"{}\"",
            t.characters.chars().take(40).collect::<String>()
        ),
        NodeKind::Instance { .. } => "Instance".into(),
        NodeKind::BooleanOperation { .. } => "BooleanOperation".into(),
        NodeKind::Other { tag } => tag.clone(),
        k => format!("{k:?}"),
    };
    let mut s = format!(
        "{kind} \"{}\" {} {}×{}",
        n.name,
        n.id,
        n.size.width.round(),
        n.size.height.round()
    );
    if let Some(l) = &n.layout {
        s.push_str(&format!(
            " [{}]",
            if matches!(
                l.direction,
                crate::document::proto::LayoutDirection::Horizontal
            ) {
                "row"
            } else {
                "column"
            }
        ));
    }
    if let Some(c) = &n.control {
        s.push_str(&format!(" ⟨{}", c.kind.name()));
        if let Some(b) = &c.binding {
            s.push_str(&format!(" → {}", source_text(&b.source)));
        }
        s.push('⟩');
    }
    s
}

fn source_text(s: &Source) -> String {
    match s {
        Source::Param { addr } => format!("param:{}", addr.0),
        Source::Action { id } => format!("action:{}", id.0),
        Source::Stream {
            id,
            channel: Some(c),
        } => format!("stream:{}:{c}", id.0),
        Source::Stream { id, channel: None } => format!("stream:{}", id.0),
        Source::Collection { id } => format!("collection:{}", id.0),
    }
}

/// `param:amp/gain` · `action:tap_tempo` · `stream:out_level[:ch]` · `collection:songs`.
fn parse_source(s: &str) -> Result<Binding> {
    let (kind, rest) = s.split_once(':').ok_or_else(|| {
        ToolError(format!(
            "source `{s}` needs a prefix: param: | action: | stream: | collection:"
        ))
    })?;
    let source = match kind {
        "param" => return Ok(Binding::param(rest)),
        "action" => Source::Action {
            id: EntryId::new(rest),
        },
        "stream" => match rest.rsplit_once(':') {
            Some((id, ch)) if ch.parse::<u8>().is_ok() => Source::Stream {
                id: EntryId::new(id),
                channel: ch.parse().ok(),
            },
            _ => Source::Stream {
                id: EntryId::new(rest),
                channel: None,
            },
        },
        "collection" => Source::Collection {
            id: EntryId::new(rest),
        },
        other => {
            return err(format!(
                "unknown source kind `{other}`; use param | action | stream | collection"
            ));
        }
    };
    Ok(Binding {
        source,
        ..Binding::param("")
    })
}

fn parse_theme(t: &str) -> Result<Theme> {
    match t {
        "stage-dark" => Ok(Theme::stage_dark()),
        "daylight" => Ok(Theme::daylight()),
        other => err(format!(
            "unknown theme `{other}`; use stage-dark | daylight"
        )),
    }
}

fn parse_kind(k: &str) -> Result<ControlKind> {
    Ok(match k {
        "knob" => ControlKind::knob(),
        "fader" => ControlKind::fader(),
        "hfader" => ControlKind::Fader {
            orientation: Orientation::Horizontal,
        },
        "button" => ControlKind::Button {
            mode: ButtonMode::Toggle,
        },
        "momentary" => ControlKind::Button {
            mode: ButtonMode::Momentary,
        },
        "trigger" => ControlKind::Button {
            mode: ButtonMode::Trigger,
        },
        "footswitch" => ControlKind::footswitch(),
        "meter" => ControlKind::Meter {
            orientation: Orientation::Vertical,
        },
        "gauge" | "vu" => ControlKind::Gauge {
            sweep_degrees: 96.0,
            floor_db: -20.0,
            ceiling_db: 3.0,
        },
        "xy" => ControlKind::XyPad,
        "selector" => ControlKind::Selector,
        "readout" => ControlKind::Readout,
        // Plots of a feed; curves and handles carry their own inputs and
        // axes, so they are authored in markup or placed from the viz kit.
        "plot" | "scope" => ControlKind::Plot {
            style: crate::surface::proto::PlotStyle::Line,
            x: crate::surface::proto::Axis::Linear { min: 0.0, max: 1.0 },
            y: crate::surface::proto::PlotRange::linear(-1.0, 1.0),
        },
        "spectrum" => ControlKind::Plot {
            style: crate::surface::proto::PlotStyle::Line,
            x: crate::surface::proto::Axis::Log {
                f_min: 20.0,
                f_max: 20_000.0,
            },
            y: crate::surface::proto::PlotRange::linear(-90.0, 0.0),
        },
        "history" => ControlKind::Plot {
            style: crate::surface::proto::PlotStyle::Area { baseline: 0.0 },
            x: crate::surface::proto::Axis::Time { len: 240 },
            y: crate::surface::proto::PlotRange::linear(-24.0, 0.0),
        },
        other => return err(format!("unknown control kind `{other}`")),
    })
}

fn parse_role(r: &str) -> Result<SlotRole> {
    Ok(
        match r.to_ascii_lowercase().replace(['-', '_'], "").as_str() {
            "needle" | "pointer" => SlotRole::Needle,
            "valuearc" | "arc" => SlotRole::ValueArc,
            "modarc" => SlotRole::ModArc,
            "fill" => SlotRole::Fill,
            "thumb" | "cap" => SlotRole::Thumb,
            "value" | "readout" => SlotRole::Value,
            "label" => SlotRole::Label,
            "led" => SlotRole::Led,
            "peakhold" | "peak" => SlotRole::PeakHold,
            "glow" => SlotRole::Glow,
            "trace" | "line" => SlotRole::Trace,
            "area" | "region" => SlotRole::Area,
            "marker" | "ball" => SlotRole::Marker,
            _ => {
                return err(format!(
                    "unknown slot role `{r}`; use needle | value-arc | mod-arc | fill | thumb | value | label | led | peak-hold | glow | trace | area | marker"
                ));
            }
        },
    )
}

/// A node by uuid, or by name when the name is unique.
fn resolve(scene: &Scene, r: &str) -> Result<NodeId> {
    if let Ok(u) = uuid::Uuid::parse_str(r) {
        return scene
            .node(NodeId(u))
            .map(|n| n.id)
            .ok_or_else(|| ToolError(format!("no node {r}")));
    }
    let hits: Vec<&Node> = scene.nodes.values().filter(|n| n.name == r).collect();
    match hits.as_slice() {
        [one] => Ok(one.id),
        [] => err(format!(
            "no node named \"{r}\"; use `describe` to list nodes"
        )),
        many => err(format!(
            "{} nodes are named \"{r}\"; pass an id instead ({})",
            many.len(),
            many.iter()
                .take(4)
                .map(|n| n.id.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

struct Args<'a>(&'a Value);

impl Args<'_> {
    fn opt_str(&self, k: &str) -> Result<Option<&str>> {
        match self.0.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s)),
            Some(_) => err(format!("`{k}` must be a string")),
        }
    }
    fn str(&self, k: &str) -> Result<&str> {
        self.opt_str(k)?
            .ok_or_else(|| ToolError(format!("missing `{k}`")))
    }
    fn opt_u32(&self, k: &str) -> Result<Option<u32>> {
        match self.0.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => v
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .map(Some)
                .ok_or_else(|| ToolError(format!("`{k}` must be a non-negative integer"))),
        }
    }
    fn opt_node(&self, scene: &Scene, k: &str) -> Result<Option<NodeId>> {
        self.opt_str(k)?.map(|r| resolve(scene, r)).transpose()
    }
    fn node(&self, scene: &Scene, k: &str) -> Result<NodeId> {
        resolve(scene, self.str(k)?)
    }
}

/// The tool registry every adapter advertises.
#[must_use]
pub fn tools() -> Vec<ToolDef> {
    let node = json!({"type": "string", "description": "node id (uuid) or unique node name"});
    let obj = |props: Value, required: &[&str]| json!({"type": "object", "properties": props, "required": required});
    vec![
        ToolDef {
            name: "describe",
            description: "Outline the document (or a subtree): one line per node with kind, name, id, size, layout and control binding; plus a lint summary. Start here.",
            input_schema: obj(json!({"node": node}), &[]),
            mutates: false,
        },
        ToolDef {
            name: "read",
            description: "Frame markup of the whole document or one subtree (lossless; see the markup reference).",
            input_schema: obj(json!({"node": node}), &[]),
            mutates: false,
        },
        ToolDef {
            name: "catalog",
            description: "What the engine exposes: parameters (addr, name, range/unit, default), actions, telemetry streams. Bind controls only to these.",
            input_schema: obj(json!({}), &[]),
            mutates: false,
        },
        ToolDef {
            name: "lint",
            description: "Check the surface: wiring, missing slots, overlaps, touch targets, consistency, contrast. Each issue says how to fix it. Iterate until there are no errors.",
            input_schema: obj(json!({}), &[]),
            mutates: false,
        },
        ToolDef {
            name: "write",
            description: "Insert a frame-markup fragment (one or more elements) under `parent` at `index` (default: end). Omit ids to get fresh ones. Returns the new top-level ids and lint. Letter text with font/weight/style/stretch (see `fonts`).",
            input_schema: obj(
                json!({"parent": node, "markup": {"type": "string"}, "index": {"type": "integer", "minimum": 0}}),
                &["parent", "markup"],
            ),
            mutates: true,
        },
        ToolDef {
            name: "replace",
            description: "Replace a node's subtree with a markup fragment; the node keeps its id.",
            input_schema: obj(
                json!({"node": node, "markup": {"type": "string"}}),
                &["node", "markup"],
            ),
            mutates: true,
        },
        ToolDef {
            name: "remove",
            description: "Delete a node and its subtree.",
            input_schema: obj(json!({"node": node}), &["node"]),
            mutates: true,
        },
        ToolDef {
            name: "move",
            description: "Reparent/reorder a node.",
            input_schema: obj(
                json!({"node": node, "parent": node, "index": {"type": "integer", "minimum": 0}}),
                &["node", "parent"],
            ),
            mutates: true,
        },
        ToolDef {
            name: "add_control",
            description: "Add a canonical, themed control under `parent`, fully wired. kind: knob | fader | button | trigger | footswitch | meter | readout. bind: param:<addr> | action:<id> | stream:<id>[:channel]. size: small | medium | large | stage.",
            input_schema: obj(
                json!({"parent": node, "kind": {"type": "string", "enum": ["knob", "fader", "button", "trigger", "footswitch", "meter", "readout"]}, "bind": {"type": "string"}, "size": {"type": "string", "enum": ["small", "medium", "large", "stage"]}, "name": {"type": "string"}}),
                &["parent", "kind"],
            ),
            mutates: true,
        },
        ToolDef {
            name: "components",
            description: "The hardware component kit: knobs (Bakelite, Daka, Marconi, Collet, silver-top…), VU meters, buttons, switches, levers, panels — each with what it binds to, its default size and variants. Use these for hardware-style surfaces.",
            input_schema: obj(
                json!({"family": {"type": "string", "enum": ["knob", "meter", "button", "switch", "lever", "panel", "material"]}}),
                &[],
            ),
            mutates: false,
        },
        ToolDef {
            name: "fonts",
            description: "The font families every renderer bundles — UI sans (Inter), condensed display (Oswald, Saira with a width axis), engraved and tech caps (Michroma, Orbitron), scripts (Dancing Script, Pacifico), markers (Caveat Brush, Knewave), serif (Playfair Display), 7/14-segment LCD (DSEG7/DSEG14 Classic), dot matrix (Doto, VT323), monospace (JetBrains Mono), blackletter (UnifrakturCook, Pirata One) — with their weights and what each suits. Set them on <Text> with font, weight, style and stretch.",
            input_schema: obj(json!({}), &[]),
            mutates: false,
        },
        ToolDef {
            name: "add_component",
            description: "Place a kit component (see `components`) under `parent`, fully wired. bind: param:<addr> | stream:<id>[:ch] | action:<id>. size: px (primary dimension). variant: a colour/finish from the component's list. legend: printed name. scale: printed scale labels (start → end of travel). options: labels for banks/selectors (defaults to the choice param's options). Panels bind nothing and return a frame to place other components into. ink: light | dark | #hex for the printed scale and legends (dark on light panels). keep_case: print legends as written, not in capitals. sweep: degrees of travel for rotaries (a stepped knob with options picks its own). touch: px — a tight hit box (the knob's body, padded to this; 44 for the lint) with the print hanging outside. reverse: lay a bank/ladder out last option first.",
            input_schema: obj(
                json!({"parent": node, "component": {"type": "string"}, "bind": {"type": "string"}, "size": {"type": "number"}, "variant": {"type": "string"}, "legend": {"type": "string"}, "name": {"type": "string"}, "scale": {"type": "array", "items": {"type": "string"}}, "options": {"type": "array", "items": {"type": "string"}}, "ink": {"type": "string"}, "keep_case": {"type": "boolean"}, "sweep": {"type": "number"}, "touch": {"type": "number"}, "reverse": {"type": "boolean"}}),
                &["parent", "component"],
            ),
            mutates: true,
        },
        ToolDef {
            name: "bind",
            description: "Make any drawn node a control, or rewire one. kind (to create/change), source (param:… | action:… | stream:… | collection:…), slots: {role: node} with roles needle, value-arc, mod-arc, fill, thumb, value, label, led, peak-hold.",
            input_schema: obj(
                json!({"node": node, "kind": {"type": "string"}, "source": {"type": "string"}, "slots": {"type": "object", "additionalProperties": {"type": "string"}}}),
                &["node"],
            ),
            mutates: true,
        },
        ToolDef {
            name: "generate",
            description: "Replace the page with a complete, bound surface generated from the catalog (a panel per group, output strip, footswitch row). A good starting point to refine.",
            input_schema: obj(
                json!({"theme": {"type": "string", "enum": ["stage-dark", "daylight"]}}),
                &[],
            ),
            mutates: true,
        },
        ToolDef {
            name: "set_theme",
            description: "Theme for controls added from now on (add_control): stage-dark (dim stages) | daylight (bright rooms). Existing nodes keep their colors.",
            input_schema: obj(
                json!({"theme": {"type": "string", "enum": ["stage-dark", "daylight"]}}),
                &["theme"],
            ),
            mutates: false,
        },
        animate::def(),
        ToolDef {
            name: "undo",
            description: "Revert the last edit (one tool call).",
            input_schema: obj(json!({}), &[]),
            mutates: true,
        },
        ToolDef {
            name: "redo",
            description: "Re-apply the last undone edit.",
            input_schema: obj(json!({}), &[]),
            mutates: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surface::memory::demo_catalog;

    fn call(s: &mut Session, name: &str, args: Value) -> String {
        s.call(name, &args)
            .unwrap_or_else(|e| panic!("{name}: {e}"))
    }

    #[test]
    fn a_textured_file_survives_open_edit_save() {
        let dir =
            std::env::temp_dir().join(format!("design-session-assets-{}", std::process::id()));
        let path = dir.join("tex.frame");
        let mut scene = crate::document::text::parse(
            r#"<Document><Page><Frame name="Panel" w=100 h=50 /></Page></Document>"#,
            Default::default(),
        )
        .unwrap();
        let id = scene.add_asset("image/png", b"texture".to_vec());
        let fill = format!(
            r#"fill={{[{{"kind":{{"Image":{{"asset":"{id}","scale_mode":"Stretch"}}}}}}]}}"#
        );
        crate::document::io::save_surface(&path, &scene).unwrap();

        let mut s = Session::open(&path, demo_catalog()).unwrap();
        assert_eq!(s.scene().assets, scene.assets);
        // An agent rewrites the panel, naming the texture by id as `read` shows it.
        let panel = s
            .scene()
            .nodes
            .values()
            .find(|n| n.name == "Panel")
            .unwrap()
            .id;
        call(
            &mut s,
            "replace",
            json!({"node": panel.to_string(), "markup": format!(r#"<Frame name="Panel" w=120 h=50 {fill} />"#)}),
        );
        assert!(call(&mut s, "read", json!({})).contains(&id.0));
        s.save(&path).unwrap();
        let back = crate::document::io::load_surface(&path).unwrap();
        assert_eq!(back, *s.scene());
        assert_eq!(
            back.asset(&id).map(|a| a.bytes.as_slice()),
            Some(&b"texture"[..])
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn registry_matches_dispatch() {
        let mut s = Session::blank(demo_catalog());
        for t in tools() {
            // Every advertised tool is dispatched (unknown-tool is the only
            // error that names the tool list).
            let r = s.call(t.name, &json!({}));
            assert!(
                !matches!(&r, Err(ToolError(m)) if m.starts_with("unknown tool")),
                "{}",
                t.name
            );
            assert_eq!(t.input_schema["type"], "object");
        }
    }

    #[test]
    fn fonts_lists_every_bundled_family_and_markup_takes_them() {
        let mut s = Session::blank(demo_catalog());
        let out = call(&mut s, "fonts", json!({}));
        for f in crate::render::font_book::BUNDLED {
            assert!(out.contains(f.name), "{} missing from:\n{out}", f.name);
        }
        let page = s.scene().pages()[0].to_string();
        let markup = r##"<Text name="Logo" w=200 h=40 size=30 font="Oswald" weight=700 style="italic" stretch=75 fill="#ffffff">AMP</Text>"##;
        call(&mut s, "write", json!({"parent": page, "markup": markup}));
        let out = call(&mut s, "read", json!({"node": "Logo"}));
        assert!(
            out.contains("font=\"Oswald\"")
                && out.contains("weight=700")
                && out.contains("style=\"italic\"")
                && out.contains("stretch=75"),
            "{out}"
        );
    }

    #[test]
    fn build_a_panel_with_tools_then_undo() {
        let mut s = Session::blank(demo_catalog());
        let page = s.scene().pages()[0].to_string();
        let out = call(
            &mut s,
            "write",
            json!({"parent": page, "markup": r##"<Frame name="Amp" layout="row" gap=12 padding=16 fill="#17191e" radius=10 w=400 h=160 />"##}),
        );
        assert!(out.contains("inserted 1 nodes"), "{out}");
        for (addr, name) in [
            ("amp/gain", "Gain"),
            ("amp/bass", "Bass"),
            ("amp/treble", "Treble"),
        ] {
            let out = call(
                &mut s,
                "add_control",
                json!({"parent": "Amp", "kind": "knob", "bind": format!("param:{addr}")}),
            );
            assert!(out.contains(&format!("\"{name}\"")), "{out}");
        }
        let out = call(&mut s, "lint", json!({}));
        assert!(!out.contains("error["), "{out}");
        let described = call(&mut s, "describe", json!({}));
        assert!(
            described.contains("⟨knob → param:amp/treble⟩"),
            "{described}"
        );

        let before = s.scene().clone();
        call(
            &mut s,
            "add_control",
            json!({"parent": "Amp", "kind": "footswitch", "bind": "action:tap_tempo"}),
        );
        assert_ne!(s.scene(), &before);
        call(&mut s, "undo", json!({}));
        assert_eq!(s.scene(), &before, "one tool call = one undo step");
    }

    #[test]
    fn errors_are_actionable() {
        let mut s = Session::blank(demo_catalog());
        let page = s.scene().pages()[0].to_string();
        let e = s
            .call("write", &json!({"parent": page, "markup": "<Rect w=10 />"}))
            .unwrap_err();
        assert!(e.0.contains("Rectangle"), "{e}");
        let e = s
            .call("add_control", &json!({"parent": page, "kind": "wheel"}))
            .unwrap_err();
        assert!(e.0.contains("knob | fader"), "{e}");
        let e = s
            .call("bind", &json!({"node": page, "source": "amp/gain"}))
            .unwrap_err();
        assert!(e.0.contains("prefix"), "{e}");
        // A bad binding still commits, and lint explains it.
        call(
            &mut s,
            "add_control",
            json!({"parent": page, "kind": "knob", "bind": "param:amp/gian"}),
        );
        assert!(call(&mut s, "lint", json!({})).contains("did you mean `amp/gain`"));
    }

    #[test]
    fn bind_a_hand_drawn_knob() {
        let mut s = Session::blank(demo_catalog());
        let page = s.scene().pages()[0].to_string();
        call(
            &mut s,
            "write",
            json!({"parent": page, "markup": r##"
            <Frame name="MyKnob" w=64 h=64>
              <Ellipse name="face" w=64 h=64 fill="#333333" />
              <Rectangle name="tick" x=30 y=4 w=4 h=20 fill="#ffffff" />
            </Frame>"##}),
        );
        let out = call(
            &mut s,
            "bind",
            json!({"node": "MyKnob", "kind": "knob", "source": "param:delay/mix", "slots": {"needle": "tick"}}),
        );
        assert!(out.contains("0 errors"), "{out}");
        let n = s
            .scene()
            .nodes
            .values()
            .find(|n| n.name == "MyKnob")
            .unwrap();
        assert_eq!(n.control.as_ref().unwrap().slots.len(), 1);
    }

    #[test]
    fn same_named_params_get_distinct_labels() {
        let mut s = Session::blank(demo_catalog());
        let page = s.scene().pages()[0].to_string();
        call(
            &mut s,
            "write",
            json!({"parent": page, "markup": r##"<Frame name="Fx" layout="row" gap=12 padding=12 fill="#17191e" hug=true />"##}),
        );
        call(
            &mut s,
            "add_control",
            json!({"parent": "Fx", "kind": "knob", "bind": "param:delay/mix"}),
        );
        let out = call(
            &mut s,
            "add_control",
            json!({"parent": "Fx", "kind": "knob", "bind": "param:reverb/mix"}),
        );
        assert!(
            out.contains("\"Reverb Mix\"") && out.contains("0 warnings"),
            "{out}"
        );
        let texts: Vec<_> = s
            .scene()
            .nodes
            .values()
            .filter_map(|n| n.text().map(|t| t.characters.clone()))
            .collect();
        assert!(
            texts.contains(&"Reverb Mix".to_string()),
            "the label is authored"
        );
    }

    #[test]
    fn generate_then_catalog_text() {
        let mut s = Session::blank(demo_catalog());
        let out = call(&mut s, "generate", json!({}));
        assert!(
            out.contains("26 controls") && out.contains("0 errors"),
            "{out}"
        );
        let c = call(&mut s, "catalog", json!({}));
        assert!(
            c.contains("amp/channel")
                && c.contains("Clean | Crunch | Lead")
                && c.contains("tap_tempo"),
            "{c}"
        );
        call(&mut s, "undo", json!({}));
        assert_eq!(s.scene().nodes.len(), 2, "generate undoes as one step");
    }

    #[test]
    fn a_new_catalog_relints_the_same_document() {
        let mut s = Session::blank(demo_catalog());
        call(&mut s, "generate", json!({}));
        let nodes = s.scene().nodes.len();
        let mut smaller = demo_catalog();
        smaller.params.retain(|p| !p.addr.0.starts_with("delay/"));
        s.set_catalog(smaller);
        assert_eq!(s.scene().nodes.len(), nodes, "document untouched");
        assert!(
            s.catalog()
                .param(&crate::surface::proto::ParamAddr::new("delay/time"))
                .is_none()
        );
        assert!(
            s.issues().iter().any(|i| i.severity == Severity::Error),
            "delay controls now dangle"
        );
    }
}
