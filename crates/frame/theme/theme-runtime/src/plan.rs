//! A role at one size, ready to paint: its primitives split by when they
//! change.
//!
//! - **Pictures**: runs of primitives no binding touches, recorded once.
//! - **Units**: a bound node's primitives (with whatever structure
//!   encloses them — groups, clips, layers), patched with an instance's
//!   values and painted. A node bound only in paint (`fill`, `text`) is a
//!   unit by itself; one bound in place (`visible`, `x`, `y`) takes its
//!   whole subtree, which moves or vanishes with it.
//!
//! Record units read static data and are painted by [`Theme::record`]
//! between the pictures, in paint order. Live units read live data and are
//! painted by [`Theme::live`] over the recording.
//!
//! [`Theme::record`]: crate::Theme::record
//! [`Theme::live`]: crate::Theme::live

use std::collections::{HashMap, HashSet};

use document_proto::{NodeId, Scene};
use kurbo::{Affine, Rect};
use render::{LayoutBox, PaintPrimitive};
use render_canvas::{Font, Recording, paint_filtered};

use crate::compile::{Class, Prop, Role};
use crate::layout::lay_out;

/// A layout's cache key: the box, and the layout fields' values (as bits:
/// a layout is reused only for exactly the values it was built with).
pub(crate) type PlanKey = (u64, u64, Vec<u64>);

pub(crate) struct Plan {
    /// Each node's pre-order span `[start, end)` within the role.
    pub(crate) order: HashMap<NodeId, (u32, u32)>,
    /// Each node's parent's laid-out size (`pw`, `ph`).
    pub(crate) parents: HashMap<NodeId, (f64, f64)>,
    /// Each node's laid-out box, instance-local.
    pub(crate) boxes: HashMap<NodeId, LayoutBox>,
    pub(crate) record: Vec<Seg>,
    pub(crate) live: Vec<Unit>,
    pub(crate) targets: Vec<Target>,
    /// The host's slots (nodes named `%name`), in paint order.
    pub(crate) slots: Vec<Target>,
}

pub(crate) enum Seg {
    Picture(Recording),
    Unit(Unit),
    /// Where a host slot falls in paint order (an index into
    /// [`Plan::slots`]): the host draws there, and the theme goes on over
    /// it.
    Slot(usize),
}

pub(crate) struct Unit {
    /// The root's own `visible` bindings: checked before anything is
    /// cloned, so a hidden state variant costs one evaluation.
    pub(crate) gate: Vec<usize>,
    /// The unit's primitives, wrapped in the structure that encloses them.
    pub(crate) prims: Vec<PaintPrimitive>,
    /// Bindings on the unit's nodes (indices into the role's binds).
    pub(crate) inner: Vec<usize>,
    /// Bindings on its ancestors that hide or move it.
    pub(crate) outer: Vec<usize>,
    /// `scale` bindings on its ancestors, outermost first: they scale the
    /// whole unit about each ancestor's origin.
    pub(crate) scalers: Vec<usize>,
}

/// A hit target (a node named `@name`) or a host slot (`%name`).
pub(crate) struct Target {
    pub(crate) node: NodeId,
    pub(crate) name: String,
    pub(crate) rect: Rect,
    /// `visible`/`x`/`y` bindings on it or its ancestors.
    pub(crate) movers: Vec<usize>,
    /// Its own `w`/`h` bindings that read host data (a size binding on
    /// the box alone is in `rect` already).
    pub(crate) sizers: Vec<usize>,
}

impl Plan {
    pub(crate) fn build(role: &Role, size: (f64, f64), layout: &[(u32, theme_expr::Value)], font: &Font) -> Plan {
        let mut scene: Scene = role.scene.clone();
        let parents = lay_out(&mut scene, role.root, size, layout, &role.binds);
        let prims = render::build_scene_primitives(&scene, role.root);
        let boxes = render::build_scene_layout_boxes(&scene, role.root);

        // Pre-order spans.
        let mut order = HashMap::new();
        number(&scene, role.root, &mut 0, &mut order);
        let span = |id: NodeId| order.get(&id).copied().unwrap_or((u32::MAX, u32::MAX));
        let inside = |id: NodeId, of: NodeId| {
            let (s, e) = span(of);
            let (i, _) = span(id);
            s <= i && i < e
        };

        // Which nodes are units, and of which kind. Pre-order, so an outer
        // unit is chosen before anything inside it.
        let binds = &role.binds;
        let mut by_node: HashMap<NodeId, Vec<usize>> = HashMap::new();
        for (i, b) in binds.iter().enumerate().filter(|(_, b)| b.class != Class::Size) {
            by_node.entry(b.node).or_default().push(i);
        }
        let mut nodes_in_order: Vec<NodeId> = by_node.keys().copied().collect();
        nodes_in_order.sort_by_key(|id| span(*id).0);

        let mut live_roots: Vec<NodeId> = Vec::new();
        for &id in &nodes_in_order {
            if live_roots.iter().any(|r| inside(id, *r)) {
                continue;
            }
            if by_node[&id].iter().any(|&i| binds[i].class == Class::Live) {
                live_roots.push(id);
            }
        }
        // Paint order survives the split. The live pass paints over the
        // recording, so a static node drawn AFTER a live one it overlaps
        // — a knob's centre cap over its face that lights on hover — would
        // end up under it. Such a node is painted live too, and so is
        // whatever then follows it.
        let mut extent: HashMap<NodeId, kurbo::Rect> = HashMap::new();
        for id in scene.walk(role.root).into_iter().rev() {
            let own = boxes.get(&id).map(|b| kurbo::Rect::new(b.x, b.y, b.x + b.width, b.y + b.height));
            let kids = scene.node(id).map(|n| n.children.clone()).unwrap_or_default();
            let all = kids.iter().filter_map(|k| extent.get(k)).copied().chain(own).reduce(|a, b| a.union(b));
            if let Some(r) = all {
                extent.insert(id, r);
            }
        }
        // Within a pixel counts: two shapes sharing an edge share the
        // antialiased pixels along it, and those blend in paint order too.
        let overlaps = |a: &kurbo::Rect, b: &kurbo::Rect| {
            a.x0 < b.x1 + 1.0 && b.x0 < a.x1 + 1.0 && a.y0 < b.y1 + 1.0 && b.y0 < a.y1 + 1.0
        };
        loop {
            let mut grown = false;
            for parent in scene.walk(role.root) {
                let Some(kids) = scene.node(parent).map(|n| n.children.clone()) else { continue };
                let mut lit: Vec<kurbo::Rect> = Vec::new();
                for kid in kids {
                    let Some(r) = extent.get(&kid).copied() else { continue };
                    let holds_live = live_roots.iter().any(|l| inside(*l, kid) || inside(kid, *l));
                    if holds_live {
                        lit.push(r);
                    } else if lit.iter().any(|l| overlaps(l, &r)) {
                        live_roots.retain(|l| !inside(*l, kid));
                        live_roots.push(kid);
                        lit.push(r);
                        grown = true;
                    }
                }
            }
            if !grown {
                break;
            }
        }
        live_roots.sort_by_key(|id| span(*id).0);
        let in_live = |id: NodeId| live_roots.iter().any(|r| inside(id, *r));

        // Record units: static-bound nodes outside the live subtrees.
        let mut subtree_units: Vec<NodeId> = Vec::new();
        let mut node_units: Vec<NodeId> = Vec::new();
        for &id in &nodes_in_order {
            if in_live(id) || subtree_units.iter().any(|r| inside(id, *r)) {
                continue;
            }
            // Placed (or, inset, a clip that reveals its children): the subtree.
            let places = by_node[&id].iter().any(|&i| matches!(binds[i].prop, Prop::Visible | Prop::X | Prop::Y) || binds[i].prop.is_inset());
            if places {
                // Absorbs any node-only units already chosen inside it.
                node_units.retain(|n| !inside(*n, id));
                subtree_units.push(id);
            } else {
                node_units.push(id);
            }
        }

        let owner_span = |p: &PaintPrimitive| span(owner(p)).0;
        let in_subtree = |p: &PaintPrimitive, root: NodeId| {
            let (s, e) = span(root);
            let i = owner_span(p);
            s <= i && i < e
        };
        let binds_within = |root: NodeId, whole: bool, live: bool| -> Vec<usize> {
            binds
                .iter()
                .enumerate()
                .filter(|(_, b)| b.class != Class::Size)
                .filter(|(_, b)| if whole { inside(b.node, root) } else { b.node == root })
                .filter(|(_, b)| live || !in_live(b.node))
                .map(|(i, _)| i)
                .collect()
        };
        let gate_of = |root: NodeId| -> Vec<usize> {
            binds
                .iter()
                .enumerate()
                .filter(|(_, b)| b.node == root && b.prop == Prop::Visible && b.class != Class::Size)
                .map(|(i, _)| i)
                .collect()
        };
        let movers_of = |node: NodeId, strict: bool| -> Vec<usize> {
            binds
                .iter()
                .enumerate()
                .filter(|(_, b)| b.class != Class::Size && matches!(b.prop, Prop::Visible | Prop::X | Prop::Y))
                .filter(|(_, b)| inside(node, b.node) && !(strict && b.node == node))
                .map(|(i, _)| i)
                .collect()
        };

        let scalers_of = |node: NodeId| -> Vec<usize> {
            let mut found: Vec<usize> = binds
                .iter()
                .enumerate()
                .filter(|(_, b)| matches!(b.prop, Prop::Scale | Prop::ScaleX | Prop::ScaleY))
                .filter(|(_, b)| b.node != node && inside(node, b.node))
                .map(|(i, _)| i)
                .collect();
            found.sort_by_key(|&i| span(binds[i].node).0);
            found
        };
        // ── live units ──
        let live = live_roots
            .iter()
            .map(|&root| {
                let range = range_of(&prims, |p| in_subtree(p, root));
                Unit {
                    gate: gate_of(root),
                    prims: wrap(&prims, range, |_| true),
                    inner: binds_within(root, true, true),
                    outer: movers_of(root, true),
                    scalers: scalers_of(root),
                }
            })
            .collect();

        // ── hit targets and slots ──
        let (scene, boxes_ref) = (&scene, &boxes);
        let named = |prefix: char| {
            scene.walk(role.root).into_iter().filter_map(move |id| {
                let n = scene.node(id)?;
                let name = n.name.strip_prefix(prefix)?;
                // Hidden at this size, by itself or an ancestor.
                let mut up = Some(id);
                while let Some(a) = up.and_then(|a| scene.node(a)) {
                    if !a.visible {
                        return None;
                    }
                    up = a.parent;
                }
                let b = boxes_ref.get(&id)?;
                Some(Target {
                    node: id,
                    name: name.to_owned(),
                    rect: Rect::new(b.x, b.y, b.x + b.width, b.y + b.height),
                    movers: movers_of(id, false),
                    sizers: binds
                        .iter()
                        .enumerate()
                        .filter(|(_, b)| b.node == id && b.class != Class::Size && matches!(b.prop, Prop::W | Prop::H))
                        .map(|(i, _)| i)
                        .collect(),
                })
            })
        };
        // Topmost first: what paints last is hit first.
        let targets: Vec<Target> = named('@').collect::<Vec<_>>().into_iter().rev().collect();
        let slots: Vec<Target> = named('%').collect();

        // ── the record pass, in paint order ──
        // The slots, by where they start in paint order: each splits the
        // recording there. One inside a unit (a placed subtree) falls
        // after the whole unit.
        let mut slot_at: Vec<(u32, usize)> = slots.iter().enumerate().map(|(k, t)| (span(t.node).0, k)).collect();
        slot_at.sort_unstable();
        let mut next_slot = 0;
        let mut record = Vec::new();
        let mut run: HashSet<NodeId> = HashSet::new();
        let mut i = 0;
        let flush = |run: &mut HashSet<NodeId>, record: &mut Vec<Seg>| {
            if run.is_empty() {
                return;
            }
            let mut rec = Recording::new();
            let keep = std::mem::take(run);
            paint_filtered(&mut rec, &prims, Affine::IDENTITY, font, &|id| keep.contains(&id));
            record.push(Seg::Picture(rec));
        };
        while i < prims.len() {
            let p = &prims[i];
            let o = owner(p);
            while let Some(&(start, k)) = slot_at.get(next_slot)
                && start <= owner_span(p)
            {
                flush(&mut run, &mut record);
                record.push(Seg::Slot(k));
                next_slot += 1;
            }
            if !is_leaf(p) || in_live(o) {
                i += 1;
                continue;
            }
            if let Some(&root) = subtree_units.iter().find(|r| in_subtree(p, **r)) {
                flush(&mut run, &mut record);
                let range = range_of(&prims, |q| in_subtree(q, root));
                record.push(Seg::Unit(Unit {
                    gate: gate_of(root),
                    prims: wrap(&prims, range.clone(), |q| !in_live(owner(q))),
                    inner: binds_within(root, true, false),
                    outer: movers_of(root, true),
                    scalers: scalers_of(root),
                }));
                i = range.end;
                continue;
            }
            if node_units.contains(&o) {
                flush(&mut run, &mut record);
                let range = range_of(&prims, |q| is_leaf(q) && owner(q) == o);
                record.push(Seg::Unit(Unit {
                    gate: gate_of(o),
                    prims: wrap(&prims, range.clone(), |_| true),
                    inner: binds_within(o, false, false),
                    outer: movers_of(o, true),
                    scalers: scalers_of(o),
                }));
                i = range.end;
                continue;
            }
            run.insert(o);
            i += 1;
        }
        flush(&mut run, &mut record);
        for &(_, k) in slot_at.get(next_slot..).unwrap_or_default() {
            record.push(Seg::Slot(k));
        }

        Plan { order, parents, boxes, record, live, targets, slots }
    }
}

fn number(scene: &Scene, id: NodeId, next: &mut u32, out: &mut HashMap<NodeId, (u32, u32)>) {
    let start = *next;
    *next += 1;
    if let Some(n) = scene.node(id) {
        for &c in &n.children {
            number(scene, c, next, out);
        }
    }
    out.insert(id, (start, *next));
}

/// The node a primitive belongs to.
pub(crate) fn owner(p: &PaintPrimitive) -> NodeId {
    match p {
        PaintPrimitive::GroupStart { node_id, .. }
        | PaintPrimitive::GroupEnd { node_id }
        | PaintPrimitive::LayerStart { node_id, .. }
        | PaintPrimitive::LayerEnd { node_id }
        | PaintPrimitive::ClipStart { node_id, .. }
        | PaintPrimitive::ClipEnd { node_id }
        | PaintPrimitive::MaskDefStart { node_id, .. }
        | PaintPrimitive::MaskDefEnd { node_id }
        | PaintPrimitive::MaskApplyStart { node_id, .. }
        | PaintPrimitive::MaskApplyEnd { node_id }
        | PaintPrimitive::Rect { node_id, .. }
        | PaintPrimitive::Path { node_id, .. }
        | PaintPrimitive::Text { node_id, .. } => *node_id,
    }
}

pub(crate) fn is_leaf(p: &PaintPrimitive) -> bool {
    matches!(p, PaintPrimitive::Rect { .. } | PaintPrimitive::Path { .. } | PaintPrimitive::Text { .. })
}

fn closer(p: &PaintPrimitive) -> Option<PaintPrimitive> {
    Some(match p {
        PaintPrimitive::GroupStart { node_id, .. } => PaintPrimitive::GroupEnd { node_id: *node_id },
        PaintPrimitive::LayerStart { node_id, .. } => PaintPrimitive::LayerEnd { node_id: *node_id },
        PaintPrimitive::ClipStart { node_id, .. } => PaintPrimitive::ClipEnd { node_id: *node_id },
        PaintPrimitive::MaskDefStart { node_id, .. } => PaintPrimitive::MaskDefEnd { node_id: *node_id },
        PaintPrimitive::MaskApplyStart { node_id, .. } => PaintPrimitive::MaskApplyEnd { node_id: *node_id },
        _ => return None,
    })
}

const fn is_close(p: &PaintPrimitive) -> bool {
    matches!(
        p,
        PaintPrimitive::GroupEnd { .. }
            | PaintPrimitive::LayerEnd { .. }
            | PaintPrimitive::ClipEnd { .. }
            | PaintPrimitive::MaskDefEnd { .. }
            | PaintPrimitive::MaskApplyEnd { .. }
    )
}

/// The smallest index range holding every primitive `pick` says yes to.
fn range_of(prims: &[PaintPrimitive], pick: impl Fn(&PaintPrimitive) -> bool) -> std::ops::Range<usize> {
    let first = prims.iter().position(&pick).unwrap_or(0);
    let last = prims.iter().rposition(&pick).map_or(first, |i| i + 1);
    first..last
}

/// `range`'s primitives (those `keep` says yes to), opened by the
/// structure enclosing the range and closed after it.
fn wrap(prims: &[PaintPrimitive], range: std::ops::Range<usize>, keep: impl Fn(&PaintPrimitive) -> bool) -> Vec<PaintPrimitive> {
    let mut open: Vec<&PaintPrimitive> = Vec::new();
    for p in &prims[..range.start] {
        if closer(p).is_some() {
            open.push(p);
        } else if is_close(p) {
            open.pop();
        }
    }
    let mut out: Vec<PaintPrimitive> = open.iter().map(|p| (*p).clone()).collect();
    out.extend(prims[range].iter().filter(|p| keep(p)).cloned());
    out.extend(open.iter().rev().filter_map(|p| closer(p)));
    out
}

