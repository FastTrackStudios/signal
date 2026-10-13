//! Drawing the interaction layer on the render copy: unused items and
//! failing `show_if`s hidden, a handle dragged out dimmed, context text
//! resolved, closed popovers and menus hidden, open ones lifted above
//! everything at their placement, menu check marks.

use document_proto::{NodeId, NodeKind, Op, Prop, Scene, Transform};
use surface_proto::{Binding, Control, ControlKind, ParamKind, Source, substitute};

use super::Runtime;
use crate::visual::ControlState;
use crate::visual_ui::{Interaction, ramp};

fn set(id: NodeId, prop: Prop) -> Op {
    Op::Set { id, prop }
}

impl Runtime {
    /// What each control shows right now: its values, and its selection,
    /// hover, tint, text and editing state.
    #[must_use]
    pub fn state(&self, id: NodeId, control: &Control, t: f64) -> ControlState {
        let mut s = self.base_state(id, control, t);
        s.ui = self.interaction(id, control, s.position);
        if let Some(Binding { source: Source::Param { addr }, .. }) = &control.binding
            && self.catalog.param(addr).is_some_and(|p| p.kind == ParamKind::Text)
        {
            let text = self.texts.get(addr).cloned().unwrap_or_default();
            s.ui.empty = text.is_empty();
            s.value_text = Some(text);
            s.label_text = self.catalog.param(addr).map(|p| p.short_name.clone().unwrap_or_else(|| p.name.clone()));
        }
        s
    }

    fn interaction(&self, id: NodeId, control: &Control, position: f64) -> Interaction {
        let mut i = Interaction {
            selected: self.ui.selected.contains(&id),
            hovered: self.ui.hover == Some(id) || self.captured == Some(id),
            edit: self.edit_view(id),
            ..Interaction::default()
        };
        if !control.tint.is_empty() {
            // A handle's tint follows its x; a curve's its `tint` input
            // across its x axis; anything else its position.
            let curve_at = match &control.kind {
                ControlKind::Curve { inputs, x, .. } => inputs.iter().find(|i| i.name == "tint").and_then(|i| match &i.source {
                    surface_proto::InputSource::Param { addr } => self.real(addr).map(|v| x.to_unit(v)),
                    surface_proto::InputSource::Const { value } => Some(x.to_unit(*value)),
                    surface_proto::InputSource::Stream { .. } => None,
                }),
                _ => None,
            };
            let at = match (self.handle_uv(control), curve_at) {
                (Some((Some(u), _)), _) => u,
                (_, Some(u)) => u,
                _ => position,
            };
            i.tint = ramp(&control.tint, at);
        }
        match &control.kind {
            ControlKind::Canvas { .. } => {
                if let (Some(m), Some(b)) = (self.ui.marquee.as_ref().filter(|m| m.canvas == id), self.box_of(id)) {
                    let (x0, x1) = (m.from.0.min(m.to.0), m.from.0.max(m.to.0));
                    let (y0, y1) = (m.from.1.min(m.to.1), m.from.1.max(m.to.1));
                    i.marquee = Some((x0 - b.x, y0 - b.y, x1 - x0, y1 - y0));
                }
            }
            ControlKind::Menu { items, .. } => {
                let active = self.ui.menus.iter().find(|m| m.node == id).and_then(|m| m.active);
                if let (Some(row), Some(mb)) = (active.and_then(|a| items.get(a)), self.box_of(id))
                    && let Some(rb) = self.box_of(NodeId(row.row))
                {
                    i.highlight = Some((rb.y - mb.y, rb.height));
                }
            }
            _ => {}
        }
        i
    }

    /// Whether a control is drawn at all (worth driving).
    pub(crate) fn drawn(&self, id: NodeId) -> bool {
        if self.expansion.template_nodes.contains(&id) {
            return false;
        }
        if let Some((t, n)) = self.expansion.item_of(id)
            && !self.item_used(t, n)
        {
            return false;
        }
        match self.floating_root(id) {
            Some(root) => self.floating_origin(root).is_some(),
            None => true,
        }
    }

    /// The interaction layer's ops on the render copy.
    pub(crate) fn frame_ui(&self, scene: &mut Scene) {
        let mut ops: Vec<Op> = Vec::new();
        // Unused items are not drawn.
        for tpl in self.expansion.templates.iter().map(|(id, t)| (*id, t)) {
            for (n, root) in &tpl.1.instances {
                if !self.item_used(tpl.0, *n) {
                    ops.push(set(*root, Prop::Visible(false)));
                }
            }
        }
        for (id, control) in &self.surface.controls {
            if control.show_if.as_ref().is_some_and(|c| !self.holds(c)) {
                ops.push(set(*id, Prop::Visible(false)));
            }
        }
        // A handle dragged out of its area fades: let go and it's gone.
        if let Some(id) = self.ui.outside
            && let Some(n) = scene.node(id)
        {
            ops.push(set(id, Prop::Opacity(n.opacity * 0.35)));
        }
        // Context popovers and menus print their item's index.
        for ctx in self.expansion.contexts.values() {
            let Some(n) = ctx.bound else { continue };
            for (node, template) in &ctx.texts {
                if let Some(NodeKind::Text(data)) = scene.node(*node).map(|x| &x.kind) {
                    let mut data = data.clone();
                    data.characters = substitute(template, n);
                    if let Some(first) = data.runs.first().cloned() {
                        data.runs = vec![document_proto::TextRun { text: data.characters.clone(), ..first }];
                    }
                    ops.push(set(*node, Prop::Kind(NodeKind::Text(data))));
                }
            }
        }
        // Menus: closed ones hidden, check marks per their conditions.
        for (id, control) in &self.surface.controls {
            let ControlKind::Menu { items, .. } = &control.kind else { continue };
            if !self.ui.menus.iter().any(|m| m.node == *id) {
                ops.push(set(*id, Prop::Visible(false)));
                continue;
            }
            for item in items {
                if let Some(check) = item.check {
                    let on = item.checked.as_ref().is_some_and(|c| self.holds(c));
                    if !on {
                        ops.push(set(NodeId(check), Prop::Visible(false)));
                    }
                }
            }
        }
        // Popovers: down ones hidden; up ones lifted over everything.
        let up = self.popover_layers();
        for (id, control) in &self.surface.controls {
            if matches!(control.kind, ControlKind::Popover { .. }) && !up.contains(id) {
                ops.push(set(*id, Prop::Visible(false)));
            }
        }
        let _ = scene.transact(ops);
        let floating: Vec<NodeId> = up.into_iter().chain(self.ui.menus.iter().map(|m| m.node)).collect();
        for id in floating {
            if self.is_docked(id) {
                self.dock(scene, id);
            } else {
                self.lift(scene, id);
            }
        }
    }

    /// Place docked popover `id` in its anchor's area, under that area's
    /// handles: a band panel is drawn over the curves but never over a band.
    fn dock(&self, scene: &mut Scene, id: NodeId) {
        let Some((x, y)) = self.floating_origin(id) else { return };
        let anchor = self.popover_anchor(id);
        // The area: the anchor's parent (a handle's plot), and the layer it
        // is drawn in (a repeat instance) — the panel goes in the area's
        // container, before the first child holding a handle.
        let layer = anchor.and_then(|a| scene.node(a)?.parent);
        let Some(container) = layer.and_then(|l| scene.node(l)?.parent) else {
            self.lift(scene, id);
            return;
        };
        let Some(cbox) = self.box_of(container) else {
            self.lift(scene, id);
            return;
        };
        let has_handle = |root: NodeId| {
            scene.walk(root).into_iter().any(|n| self.surface.controls.get(&n).is_some_and(|c| matches!(c.kind, ControlKind::Handle { .. })))
        };
        let children = scene.node(container).map(|n| n.children.clone()).unwrap_or_default();
        let index = children.iter().position(|c| *c != id && has_handle(*c)).unwrap_or(children.len());
        let Some(node) = scene.node(id) else { return };
        let t = Transform { tx: x - cbox.x, ty: y - cbox.y, ..node.transform };
        let _ = scene.transact([
            Op::Move { id, parent: container, index: u32::try_from(index).unwrap_or(u32::MAX) },
            set(id, Prop::Transform(t)),
            set(id, Prop::Visible(true)),
        ]);
    }

    /// Move floating node `id` to the top of its stage, at its placement.
    fn lift(&self, scene: &mut Scene, id: NodeId) {
        let Some((x, y)) = self.floating_origin(id) else { return };
        let Some(stage_box) = self.stage_of(id) else { return };
        let stage = {
            let mut cur = id;
            loop {
                let Some(parent) = scene.node(cur).and_then(|n| n.parent) else { break cur };
                if scene.node(parent).is_some_and(|p| matches!(p.kind, NodeKind::Page | NodeKind::Document)) {
                    break cur;
                }
                cur = parent;
            }
        };
        let Some(node) = scene.node(id) else { return };
        let t = Transform { tx: x - stage_box.x, ty: y - stage_box.y, ..node.transform };
        let _ = scene.transact([
            Op::Move { id, parent: stage, index: u32::MAX },
            set(id, Prop::Transform(t)),
            set(id, Prop::Visible(true)),
        ]);
    }
}
