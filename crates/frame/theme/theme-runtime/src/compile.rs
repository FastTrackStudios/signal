//! Document → [`Theme`]: tokens resolved for a mode, bindings parsed,
//! classified and folded, constants applied.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::sync::{Arc, Mutex};

use document_proto::{Color, Node, NodeId, NodeKind, Paint, Scene};
use render_canvas::Font;
use theme_expr::{Expr, Value};

use crate::plan::{Plan, PlanKey};
use crate::schema::Schema;

/// Something wrong with a theme, located as far as it can be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// The role (page) it is in.
    pub role: Option<String>,
    /// The node's name.
    pub node: Option<String>,
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.role, &self.node) {
            (Some(r), Some(n)) => write!(f, "{r} › {n}: {}", self.message),
            (Some(r), None) => write!(f, "{r}: {}", self.message),
            _ => f.write_str(&self.message),
        }
    }
}

/// A bindable property.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Prop {
    Visible,
    X,
    Y,
    W,
    H,
    /// Degrees clockwise, about the node's origin (a knob's pointer drawn
    /// out from its centre).
    Rotation,
    /// The node and everything in it scaled about its origin.
    Scale,
    /// The same along one axis: a control squashed to a row too short
    /// for it, its width kept.
    ScaleX,
    ScaleY,
    Opacity,
    Radius,
    /// One corner's radius: a pill rounded at one end.
    RadiusTopLeft,
    RadiusTopRight,
    RadiusBottomRight,
    RadiusBottomLeft,
    Fill,
    Stroke,
    /// How much of the stroke is drawn, px from its start.
    Dash,
    /// The node's own box (its fill, its clip) shrunk from one side; its
    /// children stay put.
    InsetTop,
    InsetBottom,
    InsetLeft,
    InsetRight,
    /// A text's font size.
    Size,
    /// A vector's path data (`arc(…)`, or any text that is path data).
    Geometry,
    Text,
}

impl Prop {
    /// Painted per instance whether constant or not: no document
    /// property says it.
    pub(crate) const fn is_inset(self) -> bool {
        matches!(
            self,
            Prop::InsetTop
                | Prop::InsetBottom
                | Prop::InsetLeft
                | Prop::InsetRight
                | Prop::Scale
                | Prop::ScaleX
                | Prop::ScaleY
        )
    }

    fn named(name: &str) -> Option<Prop> {
        Some(match name {
            "visible" => Prop::Visible,
            "x" => Prop::X,
            "y" => Prop::Y,
            "w" => Prop::W,
            "h" => Prop::H,
            "rotation" => Prop::Rotation,
            "scale" => Prop::Scale,
            "scale-x" => Prop::ScaleX,
            "scale-y" => Prop::ScaleY,
            "opacity" => Prop::Opacity,
            "radius" => Prop::Radius,
            "radius-tl" => Prop::RadiusTopLeft,
            "radius-tr" => Prop::RadiusTopRight,
            "radius-br" => Prop::RadiusBottomRight,
            "radius-bl" => Prop::RadiusBottomLeft,
            "fill" => Prop::Fill,
            "stroke" => Prop::Stroke,
            "dash" => Prop::Dash,
            "inset-top" => Prop::InsetTop,
            "inset-bottom" => Prop::InsetBottom,
            "inset-left" => Prop::InsetLeft,
            "inset-right" => Prop::InsetRight,
            "size" => Prop::Size,
            "geometry" => Prop::Geometry,
            "text" => Prop::Text,
            _ => return None,
        })
    }
}

/// When a binding is evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Class {
    /// Reads the box only: solved with the layout, once per size.
    Size,
    /// Reads static host data: evaluated when an instance is recorded.
    Static,
    /// Reads live host data: evaluated every frame.
    Live,
}

#[derive(Debug, Clone)]
pub(crate) struct Bound {
    pub(crate) node: NodeId,
    pub(crate) prop: Prop,
    pub(crate) expr: Expr,
    pub(crate) class: Class,
}

/// One view the host draws (a page of the theme).
pub(crate) struct Role {
    /// The component's subtree, constants applied; its root at the origin.
    pub(crate) scene: Scene,
    pub(crate) root: NodeId,
    /// The size it was drawn at: what constraints resize from.
    pub(crate) authored: (f64, f64),
    /// Every binding that is not a constant.
    pub(crate) binds: Vec<Bound>,
    /// Layouts by size, built on first use.
    pub(crate) plans: Mutex<HashMap<PlanKey, Arc<Plan>>>,
}

/// A compiled theme: one document, one mode, one host schema.
pub struct Theme {
    pub(crate) source: Arc<Scene>,
    pub(crate) mode: String,
    pub(crate) schema: Schema,
    pub(crate) font: Font,
    pub(crate) roles: HashMap<String, Role>,
    pub(crate) tokens: BTreeMap<String, Value>,
}

impl fmt::Debug for Theme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Theme")
            .field("mode", &self.mode)
            .field("roles", &self.roles.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

impl Theme {
    /// Parse markup and compile it.
    ///
    /// # Errors
    /// The markup's parse error, or everything [`Theme::compile`] found.
    pub fn from_markup(
        src: &str,
        schema: Schema,
        mode: Option<&str>,
    ) -> Result<Theme, Vec<Diagnostic>> {
        let scene = document_text::parse(src, BTreeMap::new()).map_err(|e| {
            vec![Diagnostic {
                role: None,
                node: None,
                message: e.to_string(),
            }]
        })?;
        Theme::compile(Arc::new(scene), schema, mode)
    }

    /// Parse several documents and compile them as one, each over the
    /// ones before it (see [`crate::layer`]): a base theme's files, then a
    /// theme's own. `sources` are `(label, markup)`; a parse error names
    /// its label.
    ///
    /// # Errors
    /// Every source's parse error, or everything [`Theme::compile`] found.
    pub fn from_sources(
        sources: &[(&str, &str)],
        schema: Schema,
        mode: Option<&str>,
    ) -> Result<Theme, Vec<Diagnostic>> {
        let mut scenes = Vec::new();
        let mut diags = Vec::new();
        for (label, src) in sources {
            match document_text::parse(src, BTreeMap::new()) {
                Ok(s) => scenes.push(s),
                Err(e) => diags.push(Diagnostic {
                    role: None,
                    node: None,
                    message: format!("{label}: {e}"),
                }),
            }
        }
        if !diags.is_empty() {
            return Err(diags);
        }
        Theme::compile(Arc::new(crate::layer::layer(&scenes)), schema, mode)
    }

    /// Compile `scene` for `mode` (`None`: the document's first mode)
    /// against what the host offers.
    ///
    /// Strict: any problem — an undefined token, a field the host does not
    /// have, a binding that does not parse — fails the whole theme, so a
    /// host hot-reloading one keeps the last good theme and shows these.
    ///
    /// # Errors
    /// Every problem found.
    pub fn compile(
        scene: Arc<Scene>,
        schema: Schema,
        mode: Option<&str>,
    ) -> Result<Theme, Vec<Diagnostic>> {
        let mut diags = Vec::new();
        let mode = mode
            .map(str::to_owned)
            .or_else(|| scene.variables.modes.first().cloned())
            .unwrap_or_default();
        if !mode.is_empty()
            && !scene.variables.modes.is_empty()
            && !scene.variables.modes.contains(&mode)
        {
            diags.push(Diagnostic {
                role: None,
                node: None,
                message: format!(
                    "no mode `{mode}` (modes: {})",
                    scene.variables.modes.join(", ")
                ),
            });
        }
        let tokens = resolve_tokens(&scene, &mode, &mut diags);
        let expanded = crate::expand::expand(&scene, &mut diags);

        let mut roles = HashMap::new();
        for &page in expanded.scene.pages() {
            let Some(page_node) = expanded.scene.node(page) else {
                continue;
            };
            // A page's first frame is the role; a page of components alone
            // draws nothing itself.
            let Some(root) = page_node.children.iter().copied().find(|c| {
                expanded
                    .scene
                    .node(*c)
                    .is_some_and(|n| n.kind != NodeKind::Component)
            }) else {
                continue;
            };
            let name = page_node.name.clone();
            if let Some(role) = compile_role(&expanded, root, &name, &tokens, &schema, &mut diags)
                && roles.insert(name.clone(), role).is_some()
            {
                diags.push(Diagnostic {
                    role: Some(name),
                    node: None,
                    message: "two pages with this name".into(),
                });
            }
        }
        if !diags.is_empty() {
            return Err(diags);
        }
        let font = Font::default().for_scene(&scene).into_owned();
        Ok(Theme {
            source: scene,
            mode,
            schema,
            font,
            roles,
            tokens,
        })
    }

    /// The same document in another mode.
    ///
    /// # Errors
    /// As [`Theme::compile`].
    pub fn with_mode(&self, mode: &str) -> Result<Theme, Vec<Diagnostic>> {
        Theme::compile(self.source.clone(), self.schema.clone(), Some(mode))
    }

    #[must_use]
    pub fn mode(&self) -> &str {
        &self.mode
    }

    /// The document's modes.
    #[must_use]
    pub fn modes(&self) -> &[String] {
        &self.source.variables.modes
    }

    /// The roles this theme draws.
    pub fn roles(&self) -> impl Iterator<Item = &str> {
        self.roles.keys().map(String::as_str)
    }

    #[must_use]
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.contains_key(role)
    }

    /// A token's value in this theme's mode.
    #[must_use]
    pub fn token(&self, name: &str) -> Option<&Value> {
        self.tokens.get(name)
    }

    /// The size `role` was drawn at.
    #[must_use]
    pub fn authored_size(&self, role: &str) -> Option<(f64, f64)> {
        self.roles.get(role).map(|r| r.authored)
    }

    /// Whether anything in `role` reads live data (so the host has a live
    /// pass to run for it).
    #[must_use]
    pub fn has_live(&self, role: &str) -> bool {
        self.roles
            .get(role)
            .is_some_and(|r| r.binds.iter().any(|b| b.class == Class::Live))
    }

    /// The host's schema this theme was compiled against.
    #[must_use]
    pub fn schema(&self) -> &Schema {
        &self.schema
    }
}

/// Every variable's value in `mode`, aliases and derivations followed.
fn resolve_tokens(
    scene: &Scene,
    mode: &str,
    diags: &mut Vec<Diagnostic>,
) -> BTreeMap<String, Value> {
    fn resolve(
        name: &str,
        scene: &Scene,
        mode: &str,
        done: &mut BTreeMap<String, Option<Value>>,
        stack: &mut Vec<String>,
    ) -> Result<Value, String> {
        if let Some(v) = done.get(name) {
            return v.clone().ok_or_else(|| format!("`${name}` is invalid"));
        }
        if stack.iter().any(|s| s == name) {
            return Err(format!(
                "`${name}` refers to itself ({} → {name})",
                stack.join(" → ")
            ));
        }
        let var = scene
            .variables
            .vars
            .get(name)
            .ok_or_else(|| format!("no token `${name}`"))?;
        let mut expr =
            theme_expr::parse(var.in_mode(mode)).map_err(|e| format!("`${name}`: {e}"))?;
        stack.push(name.to_owned());
        let mut inner = Ok(());
        expr.inline_tokens(&mut |t| match resolve(t, scene, mode, done, stack) {
            Ok(v) => Some(v),
            Err(e) => {
                if inner.is_ok() {
                    inner = Err(e);
                }
                Some(Value::Missing)
            }
        })
        .map_err(|t| format!("no token `${t}`"))?;
        stack.pop();
        inner?;
        let refs = expr.refs();
        if refs.size || !refs.fields.is_empty() {
            return Err(format!(
                "`${name}` must be a constant; tokens cannot read the box or the host's data"
            ));
        }
        let v = expr.fold().constant().cloned().unwrap_or(Value::Missing);
        done.insert(name.to_owned(), Some(v.clone()));
        Ok(v)
    }

    let mut done = BTreeMap::new();
    for name in scene.variables.vars.keys() {
        if let Err(e) = resolve(name, scene, mode, &mut done, &mut Vec::new()) {
            diags.push(Diagnostic {
                role: None,
                node: None,
                message: e,
            });
            done.insert(name.clone(), None);
        }
    }
    done.into_iter()
        .filter_map(|(k, v)| v.map(|v| (k, v)))
        .collect()
}

fn compile_role(
    expanded: &crate::expand::Expanded,
    root: NodeId,
    role: &str,
    tokens: &BTreeMap<String, Value>,
    schema: &Schema,
    diags: &mut Vec<Diagnostic>,
) -> Option<Role> {
    let source = &expanded.scene;
    // The component's subtree alone, its root at the origin.
    let mut nodes = std::collections::BTreeMap::new();
    for id in source.walk(root) {
        if let Some(n) = source.node(id) {
            nodes.insert(id, n.clone());
        }
    }
    let root_node = nodes.get_mut(&root)?;
    root_node.parent = None;
    root_node.transform.tx = 0.0;
    root_node.transform.ty = 0.0;
    let authored = (root_node.size.width, root_node.size.height);
    let mut scene = Scene {
        root,
        nodes,
        assets: source.assets.clone(),
        variables: document_proto::Variables::default(),
    };

    let mut binds = Vec::new();
    let ids: Vec<NodeId> = scene.walk(root);
    for id in ids {
        let Some(node) = scene.node_mut(id) else {
            continue;
        };
        // Written here, or copied out of a component and parsed there.
        let declared = std::mem::take(&mut node.bind)
            .into_iter()
            .map(|b| crate::expand::Parsed {
                expr: theme_expr::parse(&b.expr).map_err(|e| e.to_string()),
                prop: b.prop,
            });
        let copied =
            expanded
                .parsed
                .get(&id)
                .into_iter()
                .flatten()
                .map(|p| crate::expand::Parsed {
                    prop: p.prop.clone(),
                    expr: p.expr.clone(),
                });
        for b in declared.chain(copied).collect::<Vec<_>>() {
            let mut err = |message: String| {
                diags.push(Diagnostic {
                    role: Some(role.to_owned()),
                    node: Some(node.name.clone()),
                    message,
                });
            };
            let Some(prop) = Prop::named(&b.prop) else {
                err(format!("`bind:{}` is not a bindable property", b.prop));
                continue;
            };
            let mut expr = match b.expr {
                Ok(e) => e,
                Err(e) => {
                    err(format!("bind:{}: {e}", b.prop));
                    continue;
                }
            };
            if let Err(t) = expr.inline_tokens(&mut |t| tokens.get(t).cloned()) {
                err(format!("bind:{}: no token `${t}`", b.prop));
                continue;
            }
            if let Err(f) = expr.resolve_fields(&mut |f| schema.slot(f)) {
                err(format!(
                    "bind:{}: the host has no field `{f}` (fields: {})",
                    b.prop,
                    schema.names().collect::<Vec<_>>().join(", ")
                ));
                continue;
            }
            let expr = expr.fold();
            // An inset has no document equivalent: it is painted per
            // instance, constant or not.
            let painted_only = prop.is_inset();
            if let (Some(v), false) = (expr.constant(), painted_only) {
                apply(node, prop, v);
                continue;
            }
            let refs = expr.refs();
            let class = if refs.slots.iter().any(|s| schema.is_live(*s)) {
                Class::Live
            } else if refs.slots.iter().all(|s| schema.is_layout(*s)) && !painted_only {
                Class::Size
            } else {
                Class::Static
            };
            binds.push(Bound {
                node: id,
                prop,
                expr,
                class,
            });
        }
        prepare(node, binds.iter().filter(|b| b.node == id));
    }
    Some(Role {
        scene,
        root,
        authored,
        binds,
        plans: Mutex::new(HashMap::new()),
    })
}

/// Make a node drawable whatever its bindings come to: a bound fill needs
/// a primitive to patch even when nothing is authored, and a node shown
/// by data has to be in the picture to be shown.
fn prepare<'a>(node: &mut Node, binds: impl Iterator<Item = &'a Bound>) {
    for b in binds {
        match (b.prop, b.class) {
            (Prop::Visible, Class::Static | Class::Live) => node.visible = true,
            (Prop::Fill, _) if node.fills.is_empty() => node
                .fills
                .push(Paint::solid(Color::rgba(0.0, 0.0, 0.0, 0.0))),
            (Prop::Stroke, _) if node.stroke.paints.is_empty() => {
                node.stroke
                    .paints
                    .push(Paint::solid(Color::rgba(0.0, 0.0, 0.0, 0.0)));
            }
            (Prop::Text, _) => {
                if let NodeKind::Text(t) = &mut node.kind
                    && t.characters.is_empty()
                {
                    t.characters = " ".into();
                }
            }
            _ => {}
        }
    }
}

/// Set a node's property to a value (a missing or ill-typed value leaves
/// it as authored).
pub(crate) fn apply(node: &mut Node, prop: Prop, v: &Value) {
    match prop {
        Prop::Visible => {
            if let Some(b) = v.truthy() {
                node.visible = b;
            }
        }
        Prop::X => {
            if let Some(n) = v.num() {
                node.transform.tx = n;
            }
        }
        Prop::Y => {
            if let Some(n) = v.num() {
                node.transform.ty = n;
            }
        }
        Prop::W => {
            if let Some(n) = v.num() {
                node.size.width = n.max(0.0);
            }
        }
        Prop::H => {
            if let Some(n) = v.num() {
                node.size.height = n.max(0.0);
            }
        }
        Prop::Rotation => {
            if let Some(deg) = v.num() {
                let (sin, cos) = deg.to_radians().sin_cos();
                (
                    node.transform.a,
                    node.transform.b,
                    node.transform.c,
                    node.transform.d,
                ) = (cos, -sin, sin, cos);
            }
        }
        Prop::Opacity => {
            if let Some(n) = v.num() {
                node.opacity = n.clamp(0.0, 1.0);
            }
        }
        Prop::Radius => {
            if let Some(n) = v.num() {
                let r = n.max(0.0);
                node.corner_radii = document_proto::CornerRadii {
                    top_left: r,
                    top_right: r,
                    bottom_right: r,
                    bottom_left: r,
                };
            }
        }
        Prop::Fill => {
            // A text node's fill is its ink.
            if let Some(c) = v.color() {
                node.fills = vec![Paint::solid(doc_color(c))];
            }
        }
        Prop::Stroke => {
            if let Some(c) = v.color() {
                node.stroke.paints = vec![Paint::solid(doc_color(c))];
            }
        }
        Prop::Dash => {
            if let Some(n) = v.num() {
                node.stroke.dash_pattern = vec![n.max(0.0), DASH_GAP];
            }
        }
        Prop::InsetTop
        | Prop::InsetBottom
        | Prop::InsetLeft
        | Prop::InsetRight
        | Prop::Scale
        | Prop::ScaleX
        | Prop::ScaleY => {}
        Prop::RadiusTopLeft
        | Prop::RadiusTopRight
        | Prop::RadiusBottomRight
        | Prop::RadiusBottomLeft => {
            if let Some(n) = v.num() {
                set_corner(&mut node.corner_radii, prop, n);
            }
        }
        Prop::Geometry => {
            if let Some(path) = v.as_text() {
                node.fill_geometry = vec![path.to_string()];
            }
        }
        Prop::Size => {
            if let (Some(n), NodeKind::Text(data)) = (v.num(), &mut node.kind) {
                data.font_size = n.max(0.0);
            }
        }
        Prop::Text => {
            if let (Some(t), NodeKind::Text(data)) = (v.as_text(), &mut node.kind) {
                data.characters = t.to_string();
                data.runs.clear();
            }
        }
    }
}

/// Set one corner of `radii` (`prop` a corner radius) to `r`.
pub(crate) fn set_corner(radii: &mut document_proto::CornerRadii, prop: Prop, r: f64) {
    let r = r.max(0.0);
    match prop {
        Prop::RadiusTopLeft => radii.top_left = r,
        Prop::RadiusTopRight => radii.top_right = r,
        Prop::RadiusBottomRight => radii.bottom_right = r,
        Prop::RadiusBottomLeft => radii.bottom_left = r,
        _ => {}
    }
}

/// The gap after a `dash`: longer than any stroke a theme draws, so the
/// first dash is all that shows.
pub(crate) const DASH_GAP: f64 = 1.0e5;

pub(crate) fn doc_color(c: theme_expr::Rgba) -> Color {
    Color::rgba(
        f64::from(c.r),
        f64::from(c.g),
        f64::from(c.b),
        f64::from(c.a),
    )
}
