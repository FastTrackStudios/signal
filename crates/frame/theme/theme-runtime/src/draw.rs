//! Painting an instance, and asking what is under the pointer.

use std::sync::Arc;

use document_proto::NodeId;
use kurbo::{Affine, Point, Rect};
use render::anyrender_renderer::{BlendMix, FillPaint};
use render::{PaintPrimitive, Rgba as Prgba};
use render_canvas::{Canvas, paint_filtered};
use theme_expr::{Scope, Value};

use crate::compile::{Bound, DASH_GAP, Prop, Role, Theme};
use crate::plan::{Plan, Seg, Unit, owner};

/// One instance's data: what each schema slot holds for it.
pub trait Fields {
    fn field(&self, slot: u32) -> Value;
}

impl Fields for [Value] {
    fn field(&self, slot: u32) -> Value {
        usize::try_from(slot)
            .ok()
            .and_then(|i| self.get(i))
            .cloned()
            .unwrap_or(Value::Missing)
    }
}

impl Fields for Vec<Value> {
    fn field(&self, slot: u32) -> Value {
        self.as_slice().field(slot)
    }
}

struct InstanceScope<'a> {
    size: (f64, f64),
    parent: (f64, f64),
    fields: &'a dyn Fields,
}

impl Scope for InstanceScope<'_> {
    fn size(&self) -> (f64, f64) {
        self.size
    }

    fn parent_size(&self) -> (f64, f64) {
        self.parent
    }

    fn field(&self, slot: u32) -> Value {
        self.fields.field(slot)
    }
}

/// One instance being painted.
#[derive(Clone, Copy)]
struct Instance<'a> {
    role: &'a Role,
    plan: &'a Plan,
    size: (f64, f64),
    fields: &'a dyn Fields,
}

/// Quantize a size to the layout cache's key (a hundredth of a pixel).
fn key(size: (f64, f64), layout: &[(u32, Value)]) -> crate::plan::PlanKey {
    let q = |v: f64| (v.max(0.0) * 100.0).round() as u64;
    let bits = |v: &Value| match v {
        Value::Num(n) => n.to_bits(),
        Value::Bool(b) => u64::from(*b),
        _ => u64::MAX,
    };
    (
        q(size.0),
        q(size.1),
        layout.iter().map(|(_, v)| bits(v)).collect(),
    )
}

impl Theme {
    fn plan(
        &self,
        role: &str,
        size: (f64, f64),
        fields: &dyn Fields,
    ) -> Option<(&Role, Arc<Plan>)> {
        let r = self.roles.get(role)?;
        let layout: Vec<(u32, Value)> = self
            .schema
            .layout_slots()
            .map(|s| (s, fields.field(s)))
            .collect();
        let mut plans = r
            .plans
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let plan = plans
            .entry(key(size, &layout))
            .or_insert_with(|| Arc::new(Plan::build(r, size, &layout, &self.font)))
            .clone();
        drop(plans);
        Some((r, plan))
    }

    /// Lay `role` out at `size` now, so the first [`Theme::record`] at that
    /// size does not pay for it.
    pub fn prepare(&self, role: &str, size: (f64, f64), fields: &dyn Fields) {
        let _ = self.plan(role, size, fields);
    }

    /// Forget every cached layout (they are rebuilt on use).
    pub fn clear_layouts(&self) {
        for r in self.roles.values() {
            r.plans
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clear();
        }
    }

    /// Paint everything of one instance of `role` that does not read live
    /// data — the picture a host records once and replays — into `canvas`
    /// at `at` (canvas ← instance space). Returns false for an unknown role.
    pub fn record(
        &self,
        role: &str,
        size: (f64, f64),
        fields: &dyn Fields,
        canvas: &mut dyn Canvas,
        at: Affine,
    ) -> bool {
        let Some((r, plan)) = self.plan(role, size, fields) else {
            return false;
        };
        for seg in &plan.record {
            match seg {
                Seg::Picture(rec) => rec.replay(canvas, at),
                Seg::Unit(unit) => self.paint_unit(
                    &Instance {
                        role: r,
                        plan: &plan,
                        size,
                        fields,
                    },
                    unit,
                    canvas,
                    at,
                ),
                Seg::Slot(_) => {}
            }
        }
        true
    }

    /// [`Theme::record`], handing the canvas to `host` at each shown slot
    /// (a node named `%name`), in paint order: what the theme paints
    /// before the slot is under what the host draws there, what it paints
    /// after is over it — a panel's ground under a curve, its bypass scrim
    /// over the curve, its title over the scrim. `host` gets the slot's
    /// name and its box in instance space.
    pub fn record_with<C: Canvas>(
        &self,
        role: &str,
        size: (f64, f64),
        fields: &dyn Fields,
        canvas: &mut C,
        at: Affine,
        host: &mut dyn FnMut(&mut C, &str, Rect),
    ) -> bool {
        let Some((r, plan)) = self.plan(role, size, fields) else {
            return false;
        };
        for seg in &plan.record {
            match seg {
                Seg::Picture(rec) => rec.replay(canvas, at),
                Seg::Unit(unit) => self.paint_unit(
                    &Instance {
                        role: r,
                        plan: &plan,
                        size,
                        fields,
                    },
                    unit,
                    canvas,
                    at,
                ),
                Seg::Slot(k) => {
                    if let Some(t) = plan.slots.get(*k)
                        && let Some(at) = placed(r, &plan, t, size, fields)
                    {
                        host(canvas, &t.name, at);
                    }
                }
            }
        }
        true
    }

    /// Paint the parts of one instance of `role` that read live data, over
    /// its recording. Cheap when there are none ([`Theme::has_live`]).
    pub fn live(
        &self,
        role: &str,
        size: (f64, f64),
        fields: &dyn Fields,
        canvas: &mut dyn Canvas,
        at: Affine,
    ) -> bool {
        let Some((r, plan)) = self.plan(role, size, fields) else {
            return false;
        };
        let instance = Instance {
            role: r,
            plan: &plan,
            size,
            fields,
        };
        for unit in &plan.live {
            self.paint_unit(&instance, unit, canvas, at);
        }
        true
    }

    /// The hit target (a node named `@name`) under `point`, in instance
    /// space: the topmost one shown there for this instance's data.
    #[must_use]
    pub fn hit(
        &self,
        role: &str,
        size: (f64, f64),
        fields: &dyn Fields,
        point: Point,
    ) -> Option<String> {
        let (r, plan) = self.plan(role, size, fields)?;
        plan.targets
            .iter()
            .find(|t| placed(r, &plan, t, size, fields).is_some_and(|at| at.contains(point)))
            .map(|t| t.name.clone())
    }

    /// Where hit target `name` is for this instance (instance space), or
    /// `None` when it is hidden or there is no such target.
    #[must_use]
    pub fn target_rect(
        &self,
        role: &str,
        size: (f64, f64),
        fields: &dyn Fields,
        name: &str,
    ) -> Option<Rect> {
        let (r, plan) = self.plan(role, size, fields)?;
        let t = plan.targets.iter().find(|t| t.name == name)?;
        placed(r, &plan, t, size, fields)
    }

    /// The host's slots for this instance — nodes named `%name`, each
    /// where the theme put it and shown — in paint order: what the host
    /// draws itself (an icon it holds, a curve it computes) in a place the
    /// theme lays out. Drawn by the host after [`Theme::record`], so over
    /// the theme's own picture.
    #[must_use]
    pub fn slots(&self, role: &str, size: (f64, f64), fields: &dyn Fields) -> Vec<(String, Rect)> {
        let Some((r, plan)) = self.plan(role, size, fields) else {
            return Vec::new();
        };
        plan.slots
            .iter()
            .filter_map(|t| placed(r, &plan, t, size, fields).map(|at| (t.name.clone(), at)))
            .collect()
    }

    /// Every hit target's name, topmost first.
    #[must_use]
    pub fn targets(&self, role: &str, size: (f64, f64), fields: &dyn Fields) -> Vec<String> {
        self.plan(role, size, fields)
            .map(|(_, p)| p.targets.iter().map(|t| t.name.clone()).collect())
            .unwrap_or_default()
    }

    fn paint_unit(
        &self,
        instance: &Instance<'_>,
        unit: &Unit,
        canvas: &mut dyn Canvas,
        at: Affine,
    ) {
        let Instance {
            role: r,
            plan,
            size,
            fields,
        } = *instance;
        // A hidden unit (a state variant not in that state) costs only
        // this.
        for &i in &unit.gate {
            let b = &r.binds[i];
            let parent = plan.parents.get(&b.node).copied().unwrap_or(size);
            if b.expr
                .eval(&InstanceScope {
                    size,
                    parent,
                    fields,
                })
                .truthy()
                == Some(false)
            {
                return;
            }
        }
        // Hidden or moved by an ancestor.
        let Some(outer) = moved(r, plan, &unit.outer, size, fields) else {
            return;
        };

        let mut prims = unit.prims.clone();
        let mut hidden: Vec<(u32, u32)> = Vec::new();
        // The moves applied, by the span they moved: a scale pivots on
        // where its node ended up, not where it was laid out.
        let mut moves: Vec<((u32, u32), f64, f64)> = Vec::new();
        for &i in &unit.inner {
            let b = &r.binds[i];
            let parent = plan.parents.get(&b.node).copied().unwrap_or(size);
            let v = b.expr.eval(&InstanceScope {
                size,
                parent,
                fields,
            });
            if v.is_missing() {
                continue;
            }
            let span = plan.order.get(&b.node).copied().unwrap_or((0, 0));
            match b.prop {
                Prop::Visible => {
                    if v.truthy() == Some(false) {
                        hidden.push(span);
                    }
                }
                Prop::X | Prop::Y => {
                    let Some(to) = v.num() else { continue };
                    let d = offset(r, plan, b, to);
                    let (dx, dy) = if b.prop == Prop::X {
                        (d, 0.0)
                    } else {
                        (0.0, d)
                    };
                    moves.push((span, dx, dy));
                    for p in &mut prims {
                        if within(plan, owner(p), span) {
                            shift(p, dx, dy);
                        }
                    }
                }
                _ => {
                    for p in &mut prims {
                        if owner(p) == b.node {
                            patch(p, b, &v);
                        }
                    }
                }
            }
        }
        // Scaled nodes: their primitives wrapped in a group that scales
        // about the node's laid-out origin.
        for &i in &unit.inner {
            let b = &r.binds[i];
            if !matches!(b.prop, Prop::Scale | Prop::ScaleX | Prop::ScaleY) {
                continue;
            }
            let parent = plan.parents.get(&b.node).copied().unwrap_or(size);
            let Some(k) = b
                .expr
                .eval(&InstanceScope {
                    size,
                    parent,
                    fields,
                })
                .num()
            else {
                continue;
            };
            let (kx, ky) = match b.prop {
                Prop::ScaleX => (k, 1.0),
                Prop::ScaleY => (1.0, k),
                _ => (k, k),
            };
            let span = plan.order.get(&b.node).copied().unwrap_or((0, 0));
            let (Some(first), Some(last)) = (
                prims.iter().position(|p| within(plan, owner(p), span)),
                prims.iter().rposition(|p| within(plan, owner(p), span)),
            ) else {
                continue;
            };
            let at = plan.order.get(&b.node).map_or(0, |o| o.0);
            let (mx, my) = moves
                .iter()
                .filter(|((s, e), ..)| *s <= at && at < *e)
                .fold((0.0, 0.0), |(x, y), (_, dx, dy)| (x + dx, y + dy));
            let (ox, oy) = plan
                .boxes
                .get(&b.node)
                .map_or((0.0, 0.0), |lb| (lb.x + mx, lb.y + my));
            let transform = render::anyrender_renderer::AffineTransform2D::from([
                [kx, 0.0, ox - kx * ox],
                [0.0, ky, oy - ky * oy],
            ]);
            prims.insert(last + 1, PaintPrimitive::GroupEnd { node_id: b.node });
            prims.insert(
                first,
                PaintPrimitive::GroupStart {
                    node_id: b.node,
                    transform,
                },
            );
        }
        // Scaled by an ancestor painted elsewhere (a knob's moving parts
        // under the frame that sizes it): about that ancestor's origin,
        // innermost applied first, so the outermost wraps everything.
        for &i in unit.scalers.iter().rev() {
            let b = &r.binds[i];
            let parent = plan.parents.get(&b.node).copied().unwrap_or(size);
            let Some(k) = b
                .expr
                .eval(&InstanceScope {
                    size,
                    parent,
                    fields,
                })
                .num()
            else {
                continue;
            };
            let (kx, ky) = match b.prop {
                Prop::ScaleX => (k, 1.0),
                Prop::ScaleY => (1.0, k),
                _ => (k, k),
            };
            let (ox, oy) = plan
                .boxes
                .get(&b.node)
                .map_or((0.0, 0.0), |lb| (lb.x, lb.y));
            let transform = render::anyrender_renderer::AffineTransform2D::from([
                [kx, 0.0, ox - kx * ox],
                [0.0, ky, oy - ky * oy],
            ]);
            prims.insert(
                0,
                PaintPrimitive::GroupStart {
                    node_id: b.node,
                    transform,
                },
            );
            prims.push(PaintPrimitive::GroupEnd { node_id: b.node });
        }
        let keep = |id: NodeId| {
            let i = plan.order.get(&id).map_or(0, |s| s.0);
            !hidden.iter().any(|(s, e)| *s <= i && i < *e)
        };
        paint_filtered(
            canvas,
            &prims,
            at * Affine::translate(outer),
            &self.font,
            &keep,
        );
    }
}

/// The offset bindings on `node` and its ancestors (`movers`) give it for
/// this instance, or `None` when one of them hides it.
/// Where a target or slot is for this instance: moved by its bindings and
/// its ancestors', sized by its own — or `None` when hidden.
fn placed(
    r: &Role,
    plan: &Plan,
    t: &crate::plan::Target,
    size: (f64, f64),
    fields: &dyn Fields,
) -> Option<Rect> {
    let mut at = t.rect + moved(r, plan, &t.movers, size, fields)?;
    for &i in &t.sizers {
        let b = &r.binds[i];
        let parent = plan.parents.get(&b.node).copied().unwrap_or(size);
        let Some(v) = b
            .expr
            .eval(&InstanceScope {
                size,
                parent,
                fields,
            })
            .num()
        else {
            continue;
        };
        if b.prop == Prop::W {
            at.x1 = at.x0 + v.max(0.0);
        } else {
            at.y1 = at.y0 + v.max(0.0);
        }
    }
    Some(at)
}

fn moved(
    r: &Role,
    plan: &Plan,
    movers: &[usize],
    size: (f64, f64),
    fields: &dyn Fields,
) -> Option<kurbo::Vec2> {
    let mut d = kurbo::Vec2::ZERO;
    for &i in movers {
        let b = &r.binds[i];
        let parent = plan.parents.get(&b.node).copied().unwrap_or(size);
        let v = b.expr.eval(&InstanceScope {
            size,
            parent,
            fields,
        });
        match b.prop {
            Prop::Visible if v.truthy() == Some(false) => return None,
            Prop::X | Prop::Y => {
                let Some(to) = v.num() else { continue };
                let delta = offset(r, plan, b, to);
                if b.prop == Prop::X {
                    d.x += delta;
                } else {
                    d.y += delta;
                }
            }
            _ => {}
        }
    }
    Some(d)
}

/// How far an `x`/`y` binding to `to` (in the parent's space) moves its
/// node from where the layout put it.
fn offset(r: &Role, plan: &Plan, b: &Bound, to: f64) -> f64 {
    let pick = |lb: &render::LayoutBox| if b.prop == Prop::X { lb.x } else { lb.y };
    let laid = plan.boxes.get(&b.node).map_or(0.0, pick);
    let origin = r
        .scene
        .node(b.node)
        .and_then(|n| n.parent)
        .and_then(|p| plan.boxes.get(&p))
        .map_or(0.0, pick);
    origin + to - laid
}

fn within(plan: &Plan, id: NodeId, span: (u32, u32)) -> bool {
    plan.order
        .get(&id)
        .is_some_and(|(i, _)| span.0 <= *i && *i < span.1)
}

fn prgba(c: theme_expr::Rgba) -> Prgba {
    Prgba {
        r: f64::from(c.r),
        g: f64::from(c.g),
        b: f64::from(c.b),
        a: f64::from(c.a),
    }
}

fn shift(p: &mut PaintPrimitive, dx: f64, dy: f64) {
    match p {
        PaintPrimitive::Rect { x, y, .. }
        | PaintPrimitive::Path { x, y, .. }
        | PaintPrimitive::Text { x, y, .. }
        | PaintPrimitive::LayerStart { x, y, .. }
        | PaintPrimitive::ClipStart { x, y, .. }
        | PaintPrimitive::MaskDefStart { x, y, .. }
        | PaintPrimitive::MaskApplyStart { x, y, .. } => {
            *x += dx;
            *y += dy;
        }
        PaintPrimitive::GroupStart { transform, .. } => {
            transform.tx += dx;
            transform.ty += dy;
        }
        _ => {}
    }
}

/// Set one bound paint property on a node's own primitive.
fn patch(p: &mut PaintPrimitive, b: &Bound, v: &Value) {
    match (b.prop, p) {
        (Prop::Fill, PaintPrimitive::Rect { fills, .. } | PaintPrimitive::Path { fills, .. }) => {
            if let Some(c) = v.color() {
                let blend = fills.first().map_or(BlendMix::Normal, |f| f.1.clone());
                *fills = vec![(FillPaint::Solid(prgba(c)), blend)];
            }
        }
        (Prop::Fill, PaintPrimitive::Text { color, .. }) => {
            if let Some(c) = v.color() {
                *color = prgba(c);
            }
        }
        (
            Prop::Stroke,
            PaintPrimitive::Rect {
                stroke: Some(s), ..
            }
            | PaintPrimitive::Path {
                stroke: Some(s), ..
            },
        ) => {
            if let Some(c) = v.color() {
                s.color = prgba(c);
                s.gradient = None;
            }
        }
        (
            Prop::Text,
            PaintPrimitive::Text {
                text, style_runs, ..
            },
        ) => {
            if let Some(t) = v.as_text() {
                *text = t.to_string();
                style_runs.clear();
            }
        }
        (Prop::W, PaintPrimitive::Rect { width, .. } | PaintPrimitive::Text { width, .. }) => {
            if let Some(n) = v.num() {
                *width = n.max(0.0);
            }
        }
        (Prop::H, PaintPrimitive::Rect { height, .. }) => {
            if let Some(n) = v.num() {
                *height = n.max(0.0);
            }
        }
        (
            Prop::Rotation,
            PaintPrimitive::Rect { rotation, .. } | PaintPrimitive::Path { rotation, .. },
        ) => {
            if let Some(deg) = v.num() {
                *rotation = deg.to_radians();
            }
        }
        (
            Prop::Dash,
            PaintPrimitive::Rect {
                stroke: Some(s), ..
            }
            | PaintPrimitive::Path {
                stroke: Some(s), ..
            },
        ) => {
            if let Some(n) = v.num() {
                if n <= 0.01 {
                    // Nothing of it: a zero dash would still draw its caps.
                    s.color.a = 0.0;
                } else {
                    s.dash_pattern = vec![n, DASH_GAP];
                }
            }
        }
        (Prop::InsetTop | Prop::InsetBottom | Prop::InsetLeft | Prop::InsetRight, p) => {
            if let Some(n) = v.num() {
                inset(p, b.prop, n.max(0.0));
            }
        }
        (Prop::Geometry, PaintPrimitive::Path { fill_paths, .. }) => {
            if let Some(path) = v.as_text() {
                *fill_paths = vec![path.to_string()];
            }
        }
        (Prop::Size, PaintPrimitive::Text { font_size, .. }) => {
            if let Some(n) = v.num() {
                *font_size = n.max(0.0);
            }
        }
        (
            Prop::RadiusTopLeft
            | Prop::RadiusTopRight
            | Prop::RadiusBottomRight
            | Prop::RadiusBottomLeft,
            PaintPrimitive::Rect { corner_radii, .. },
        ) => {
            if let Some(n) = v.num() {
                crate::compile::set_corner(corner_radii, b.prop, n);
            }
        }
        (Prop::Radius, PaintPrimitive::Rect { corner_radii, .. }) => {
            if let Some(n) = v.num() {
                let r = n.max(0.0);
                *corner_radii = document_proto::CornerRadii {
                    top_left: r,
                    top_right: r,
                    bottom_right: r,
                    bottom_left: r,
                };
            }
        }
        (
            Prop::Opacity,
            PaintPrimitive::Rect { fills, stroke, .. } | PaintPrimitive::Path { fills, stroke, .. },
        ) => {
            if let Some(k) = v.num() {
                let k = k.clamp(0.0, 1.0);
                for (f, _) in fills.iter_mut() {
                    if let FillPaint::Solid(c) = f {
                        c.a *= k;
                    }
                }
                if let Some(s) = stroke {
                    s.color.a *= k;
                }
            }
        }
        (Prop::Opacity, PaintPrimitive::Text { color, .. }) => {
            if let Some(k) = v.num() {
                color.a *= k.clamp(0.0, 1.0);
            }
        }
        _ => {}
    }
}

/// Shrink a node's own box (its rect, its clip) from one side.
///
/// What it shows stays where it was: a gradient across a rect is kept on
/// the box it was laid across, so a meter's lit part reveals the colours
/// of its whole travel rather than squeezing them into the part lit.
fn inset(p: &mut PaintPrimitive, side: Prop, by: f64) {
    let (PaintPrimitive::Rect {
        x,
        y,
        width,
        height,
        ..
    }
    | PaintPrimitive::ClipStart {
        x,
        y,
        width,
        height,
        ..
    }) = p
    else {
        return;
    };
    let before = (*x, *y, *width, *height);
    match side {
        Prop::InsetTop => {
            let by = by.min(*height);
            *y += by;
            *height -= by;
        }
        Prop::InsetBottom => *height = (*height - by).max(0.0),
        Prop::InsetLeft => {
            let by = by.min(*width);
            *x += by;
            *width -= by;
        }
        Prop::InsetRight => *width = (*width - by).max(0.0),
        _ => {}
    }
    let after = (*x, *y, *width, *height);
    if let PaintPrimitive::Rect { fills, .. } = p {
        for (fill, _) in fills.iter_mut() {
            keep_gradient(fill, before, after);
        }
    }
}

/// Re-express a gradient laid across box `from` as one across box `to`
/// that paints the same colours in the same places.
fn keep_gradient(fill: &mut FillPaint, from: (f64, f64, f64, f64), to: (f64, f64, f64, f64)) {
    let (FillPaint::GradientLinear { transform, .. }
    | FillPaint::GradientRadial { transform, .. }
    | FillPaint::GradientAngular { transform, .. }) = fill
    else {
        return;
    };
    let ((x, y, w, h), (x1, y1, w1, h1)) = (from, to);
    if w1 <= 1e-9 || h1 <= 1e-9 {
        return;
    }
    // Box units of `from` → box units of `to`, after the gradient's own.
    let (ma, md, mx, my) = (w / w1, h / h1, (x - x1) / w1, (y - y1) / h1);
    let t = transform.unwrap_or(render::anyrender_renderer::AffineTransform2D::from([
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
    ]));
    *transform = Some(render::anyrender_renderer::AffineTransform2D::from([
        [ma * t.a, ma * t.b, ma.mul_add(t.tx, mx)],
        [md * t.c, md * t.d, md.mul_add(t.ty, my)],
    ]));
}
