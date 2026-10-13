//! A role laid out for one size.
//!
//! The component was drawn at one size; the host shows it at many. Every
//! node's box is solved top-down:
//!
//! 1. A node's children follow its box changing by their constraints —
//!    `resize-x` / `resize-y` (left/right/center/stretch/scale) — or, in
//!    an auto-layout container, by the flow: padding, gap, alignment,
//!    `grow`, min/max. Hidden children leave the flow.
//! 2. Bindings that read only the box (`bind:visible="w >= 60"`,
//!    `bind:h="h - 120"`) are evaluated as each node is placed, before its
//!    own children, so a section collapsed at this width is gone from the
//!    flow and what follows it closes up.
//!
//! The result is written back into a copy of the scene as plain absolute
//! geometry (auto layout cleared), so the renderer only paints.

use std::collections::HashMap;

use document_proto::{
    AutoLayout, CrossAxisAlign, LayoutDirection, MainAxisAlign, NodeId, ResizeX, ResizeY, Scene,
};
use theme_expr::{Scope, Value};

use crate::compile::{Bound, Class, Prop, apply};

/// The box a size binding sees: the instance's, and the node's parent's.
struct BoxScope<'a> {
    size: (f64, f64),
    parent: (f64, f64),
    /// The layout fields' values, by slot.
    layout: &'a [(u32, Value)],
}

impl Scope for BoxScope<'_> {
    fn size(&self) -> (f64, f64) {
        self.size
    }

    fn parent_size(&self) -> (f64, f64) {
        self.parent
    }

    fn field(&self, slot: u32) -> Value {
        self.layout
            .iter()
            .find(|(s, _)| *s == slot)
            .map_or(Value::Missing, |(_, v)| v.clone())
    }
}

/// Lay `scene` (a role's subtree, rooted at `root`) out at `size`.
/// Returns each node's parent's laid-out size, which the later passes'
/// `pw`/`ph` read.
pub(crate) fn lay_out(
    scene: &mut Scene,
    root: NodeId,
    size: (f64, f64),
    layout: &[(u32, Value)],
    binds: &[Bound],
) -> HashMap<NodeId, (f64, f64)> {
    let mut by_node: HashMap<NodeId, Vec<&Bound>> = HashMap::new();
    for b in binds.iter().filter(|b| b.class == Class::Size) {
        by_node.entry(b.node).or_default().push(b);
    }
    let mut parents = HashMap::new();
    let mut cx = Cx {
        scene,
        size,
        layout,
        by_node,
        parents: &mut parents,
    };
    // The root's box is the instance's, whatever it was drawn at.
    cx.place(root, size);
    parents.insert(root, size);
    parents
}

struct Cx<'a, 'b> {
    scene: &'a mut Scene,
    size: (f64, f64),
    layout: &'a [(u32, Value)],
    by_node: HashMap<NodeId, Vec<&'b Bound>>,
    parents: &'a mut HashMap<NodeId, (f64, f64)>,
}

impl Cx<'_, '_> {
    /// Whether `id` has a size binding on `prop` (which then wins over
    /// its constraint on that axis).
    fn binds(&self, id: NodeId, prop: Prop) -> bool {
        self.by_node
            .get(&id)
            .is_some_and(|bs| bs.iter().any(|b| b.prop == prop))
    }

    /// Apply `id`'s size bindings, with `parent` as its parent's box.
    fn size_binds(&mut self, id: NodeId, parent: (f64, f64)) {
        let Some(binds) = self.by_node.get(&id) else {
            return;
        };
        let scope = BoxScope {
            size: self.size,
            parent,
            layout: self.layout,
        };
        let values: Vec<(Prop, Value)> = binds
            .iter()
            .map(|b| (b.prop, b.expr.eval(&scope)))
            .collect();
        if let Some(n) = self.scene.node_mut(id) {
            for (prop, v) in &values {
                apply(n, *prop, v);
            }
        }
    }

    /// `id` now has the box `new` (its `size` is still the authored one):
    /// resize it and place its children.
    fn place(&mut self, id: NodeId, new: (f64, f64)) {
        let Some(node) = self.scene.node_mut(id) else {
            return;
        };
        let old = (node.size.width, node.size.height);
        node.size.width = new.0;
        node.size.height = new.1;
        let layout = node.layout.take();
        let children = node.children.clone();

        // Size bindings first: what is hidden at this size leaves the flow.
        for &c in &children {
            self.parents.insert(c, new);
            self.size_binds(c, new);
        }

        match layout {
            Some(al) => self.flow(&al, &children, new),
            None => {
                for &c in &children {
                    let bound = [Prop::X, Prop::W, Prop::Y, Prop::H].map(|p| self.binds(c, p));
                    let Some(child) = self.scene.node_mut(c) else {
                        continue;
                    };
                    let (x, w) = follow_x(
                        child.layout_child.resize_x,
                        old.0,
                        new.0,
                        child.transform.tx,
                        child.size.width,
                    );
                    let (y, h) = follow_y(
                        child.layout_child.resize_y,
                        old.1,
                        new.1,
                        child.transform.ty,
                        child.size.height,
                    );
                    // A bound axis is where the binding put it.
                    if !bound[0] {
                        child.transform.tx = x;
                    }
                    if !bound[2] {
                        child.transform.ty = y;
                    }
                    let w = if bound[1] { child.size.width } else { w };
                    let h = if bound[3] { child.size.height } else { h };
                    self.place(c, (w, h));
                }
            }
        }
    }

    /// Auto layout: place `children` in a box of `size`.
    fn flow(&mut self, al: &AutoLayout, children: &[NodeId], size: (f64, f64)) {
        let row = al.direction == LayoutDirection::Horizontal;
        let (main_size, cross_size) = if row { size } else { (size.1, size.0) };
        let (pad_main0, pad_main1, pad_cross0, pad_cross1) = if row {
            (
                al.padding_left,
                al.padding_right,
                al.padding_top,
                al.padding_bottom,
            )
        } else {
            (
                al.padding_top,
                al.padding_bottom,
                al.padding_left,
                al.padding_right,
            )
        };
        let inner_cross = (cross_size - pad_cross0 - pad_cross1).max(0.0);

        // In flow: visible and not absolute.
        let flowing: Vec<NodeId> = children
            .iter()
            .copied()
            .filter(|c| {
                self.scene
                    .node(*c)
                    .is_some_and(|n| n.visible && !n.layout_child.absolute)
            })
            .collect();
        let main_of = |n: &document_proto::Node| if row { n.size.width } else { n.size.height };

        let gaps = al.gap * (flowing.len().saturating_sub(1)) as f64;
        let fixed: f64 = flowing
            .iter()
            .filter_map(|c| self.scene.node(*c))
            .map(main_of)
            .sum();
        let free = main_size - pad_main0 - pad_main1 - gaps - fixed;
        let grow_total: f64 = flowing
            .iter()
            .filter_map(|c| self.scene.node(*c))
            .filter_map(|n| n.layout_child.grow)
            .filter(|g| *g > 0.0)
            .sum();

        // Each child's main size: its own, plus its share of what is free.
        let mut mains = Vec::with_capacity(flowing.len());
        let mut used = 0.0;
        for &c in &flowing {
            let Some(n) = self.scene.node(c) else {
                continue;
            };
            let lc = &n.layout_child;
            let mut m = main_of(n);
            if free > 0.0
                && grow_total > 0.0
                && let Some(g) = lc.grow.filter(|g| *g > 0.0)
            {
                m += free * g / grow_total;
            }
            let (lo, hi) = if row {
                (lc.min_width, lc.max_width)
            } else {
                (lc.min_height, lc.max_height)
            };
            m = m.max(lo.unwrap_or(0.0)).min(hi.unwrap_or(f64::INFINITY));
            used += m;
            mains.push(m);
        }
        let leftover = (main_size - pad_main0 - pad_main1 - gaps - used).max(0.0);
        let (mut cursor, between) = match al.main_align.unwrap_or(MainAxisAlign::Start) {
            MainAxisAlign::Start => (pad_main0, al.gap),
            MainAxisAlign::Center => (pad_main0 + leftover / 2.0, al.gap),
            MainAxisAlign::End => (pad_main0 + leftover, al.gap),
            MainAxisAlign::SpaceBetween if flowing.len() > 1 => {
                (pad_main0, al.gap + leftover / (flowing.len() - 1) as f64)
            }
            MainAxisAlign::SpaceBetween => (pad_main0, al.gap),
        };

        for (&c, &m) in flowing.iter().zip(&mains) {
            let Some(n) = self.scene.node_mut(c) else {
                continue;
            };
            let own_cross = if row { n.size.height } else { n.size.width };
            let align = n.layout_child.align_self.map_or_else(
                || match al.cross_align.unwrap_or(CrossAxisAlign::Start) {
                    CrossAxisAlign::Start | CrossAxisAlign::Baseline => Cross::Start,
                    CrossAxisAlign::Center => Cross::Center,
                    CrossAxisAlign::End => Cross::End,
                },
                |a| match a {
                    document_proto::AlignSelf::Start => Cross::Start,
                    document_proto::AlignSelf::Center => Cross::Center,
                    document_proto::AlignSelf::End => Cross::End,
                    document_proto::AlignSelf::Stretch => Cross::Stretch,
                },
            );
            let (cross_at, cross) = match align {
                Cross::Start => (pad_cross0, own_cross),
                Cross::Center => (pad_cross0 + (inner_cross - own_cross) / 2.0, own_cross),
                Cross::End => (pad_cross0 + inner_cross - own_cross, own_cross),
                Cross::Stretch => (pad_cross0, inner_cross),
            };
            let (x, y, w, h) = if row {
                (cursor, cross_at, m, cross)
            } else {
                (cross_at, cursor, cross, m)
            };
            n.transform.tx = x;
            n.transform.ty = y;
            cursor += m + between;
            self.place(c, (w, h));
        }

        // Out of flow: absolute children keep their place, by constraints.
        for &c in children {
            let Some(n) = self.scene.node(c) else {
                continue;
            };
            if n.layout_child.absolute || !n.visible {
                let b = (n.size.width, n.size.height);
                self.place(c, b);
            }
        }
    }
}

enum Cross {
    Start,
    Center,
    End,
    Stretch,
}

/// Where a child sits, and how wide, when its parent goes from `old` to
/// `new` wide (see [`ResizeX`]). `None` is Figma's default, left: in a
/// theme nothing moves unless it says how — unlike `Scene::resize_width`,
/// which keeps a face's unconstrained knobs centred in proportion.
fn follow_x(rule: Option<ResizeX>, old: f64, new: f64, x: f64, w: f64) -> (f64, f64) {
    if old <= 0.0 || (new - old).abs() < 1e-9 {
        return (x, w);
    }
    let d = new - old;
    match rule {
        None | Some(ResizeX::Left) => (x, w),
        Some(ResizeX::Right) => (x + d, w),
        Some(ResizeX::Center) => (x + d / 2.0, w),
        Some(ResizeX::Stretch) => (x, (w + d).max(0.0)),
        Some(ResizeX::Scale) => (x * new / old, w * new / old),
    }
}

/// [`follow_x`] on the vertical axis.
fn follow_y(rule: Option<ResizeY>, old: f64, new: f64, y: f64, h: f64) -> (f64, f64) {
    let x_rule = rule.map(|r| match r {
        ResizeY::Top => ResizeX::Left,
        ResizeY::Bottom => ResizeX::Right,
        ResizeY::Center => ResizeX::Center,
        ResizeY::Stretch => ResizeX::Stretch,
        ResizeY::Scale => ResizeX::Scale,
    });
    follow_x(x_rule, old, new, y, h)
}
