//! Editing collections: selection (click, chord clicks, marquee), group
//! drags, drag modes and wheel rules, removal (drag out, Delete), adding
//! by double-click, popovers that follow their anchors, context menus, and
//! the keyboard routing that ties them together (a text field being edited
//! first, then an open menu, then the selection, then the focused control).

use std::collections::HashMap;

use crate::document::proto::{NodeId, NodeKind};
use crate::surface::proto::{
    Binding, Chord, ClickAction, Command, Condition, Control, ControlKind, DragMode, EditPhase,
    ItemRole, ParamAddr, ParamKind, Placement, Show, Source,
};

use super::repeat::{bind_context, instance_id};
use super::{EngineCall, Runtime};
use crate::surface::runtime::gesture::{Bounds, FINE, Gesture, Input, Key, Modifiers, Output};

/// Pointer travel (px) past which a press is a drag, not a click.
pub const CLICK_SLOP: f64 = 3.0;
/// Where a context menu opens, from the pointer (px right and down).
pub const MENU_OFFSET: f64 = 10.0;

/// A press in progress.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Press {
    pub x: f64,
    pub y: f64,
    pub moved: bool,
    /// Chord-click commands waiting for a release without travel.
    pub deferred: Vec<Command>,
    /// Item of the pressed control (for deferred commands).
    pub item: Option<(NodeId, u32)>,
    /// The press only ran commands: no drag follows.
    pub inert: bool,
}

/// A rubber band being drawn on a canvas (surface coordinates).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Marquee {
    pub canvas: NodeId,
    pub from: (f64, f64),
    pub to: (f64, f64),
    /// Selection kept under it (a Shift-marquee adds).
    pub base: Vec<NodeId>,
}

/// An open context menu.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OpenMenu {
    pub node: NodeId,
    /// Top-left where it opens (surface coordinates, before clamping).
    pub at: (f64, f64),
    /// The item it was opened on (commands' `{n}`, `Remove`).
    pub item: Option<(NodeId, u32)>,
    /// The canvas point it was opened at (for `Add`): (canvas, u, v).
    pub point: Option<(NodeId, f64, f64)>,
    pub active: Option<usize>,
}

/// A group drag: the grabbed handle, and every moved handle's unit point
/// at the start.
pub(crate) type GroupDrag = (NodeId, HashMap<NodeId, (f64, f64)>);

/// A text field being edited.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Edit {
    pub node: NodeId,
    pub addr: ParamAddr,
    pub buffer: String,
    /// Caret and selection anchor, in chars.
    pub caret: usize,
    pub anchor: usize,
    pub original: String,
    pub max_len: usize,
}

/// Interaction state beyond one control's gesture.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Ui {
    /// Selected controls, oldest first (the last is the primary).
    pub selected: Vec<NodeId>,
    /// The control under the pointer (no button held).
    pub hover: Option<NodeId>,
    pub pointer: Option<(f64, f64)>,
    pub press: Option<Press>,
    pub marquee: Option<Marquee>,
    /// Unit points of every handle a group drag moves, at its start.
    pub group: Option<GroupDrag>,
    /// A handle dragged out of its area (let go there, its item goes).
    pub outside: Option<NodeId>,
    /// The last vertical unit a resonance (wheel-mode) drag saw.
    pub wheel_drag: Option<f64>,
    /// The drag mode the last move chose (a release keeps it).
    pub drag_mode: Option<DragMode>,
    pub menus: Vec<OpenMenu>,
    pub edit: Option<Edit>,
    /// Keyboard target for plain keys (the last-pressed control).
    pub focus: Option<NodeId>,
    /// The item context popovers follow.
    pub active: Option<(NodeId, u32)>,
    /// Docked popovers pinned while in use: (x, last touch ms, the item it
    /// was showing — another item's panel goes where that item is).
    pub held: HashMap<NodeId, (f64, f64, Option<(NodeId, u32)>)>,
    /// The docked popover under the pointer, if any.
    pub pointer_in_popover: Option<NodeId>,
    /// The latest input time (ms).
    pub clock: f64,
}

/// How long a docked popover stays put after it was last touched (ms):
/// turning a band's FREQ dial must not slide the panel out from under the
/// hand turning it.
pub const POPOVER_HOLD_MS: f64 = 2000.0;

impl Ui {
    /// Forget what no longer exists (after a rebuild).
    pub fn retain(&mut self, alive: impl Fn(&NodeId) -> bool) {
        self.selected.retain(&alive);
        self.hover = self.hover.filter(&alive);
        self.focus = self.focus.filter(&alive);
        self.outside = None;
        self.group = None;
        self.marquee = None;
        self.menus.retain(|m| alive(&m.node));
        if self.edit.as_ref().is_some_and(|e| !alive(&e.node)) {
            self.edit = None;
        }
    }
}

/// The chord `mods` hold.
#[must_use]
pub fn chord(mods: Modifiers) -> Chord {
    Chord {
        shift: mods.fine,
        command: mods.alt,
        option: mods.option,
    }
}

fn inside(b: &Bounds, x: f64, y: f64) -> bool {
    x >= b.x && y >= b.y && x <= b.x + b.width && y <= b.y + b.height
}

impl Runtime {
    // ── queries ───────────────────────────────────────────────────────────

    /// The selected controls, oldest first.
    #[must_use]
    pub fn selection(&self) -> &[NodeId] {
        &self.ui.selected
    }

    /// The control under the pointer.
    #[must_use]
    pub fn hovered(&self) -> Option<NodeId> {
        self.ui.hover
    }

    /// The text field being edited, with its text so far.
    #[must_use]
    pub fn editing(&self) -> Option<(NodeId, &str)> {
        self.ui.edit.as_ref().map(|e| (e.node, e.buffer.as_str()))
    }

    /// Open context menus, outermost first.
    #[must_use]
    pub fn open_menus(&self) -> Vec<NodeId> {
        self.ui.menus.iter().map(|m| m.node).collect()
    }

    /// A text param's current text.
    #[must_use]
    pub fn text(&self, addr: &ParamAddr) -> Option<&str> {
        self.texts.get(addr).map(String::as_str)
    }

    /// The item (template, index) a node belongs to: its instance's, or —
    /// on a context popover or menu — the item that is bound to now.
    #[must_use]
    pub fn item_of(&self, id: NodeId) -> Option<(NodeId, u32)> {
        self.expansion.item_of(id).or_else(|| {
            let root = self.floating_root(id)?;
            let ctx = self.expansion.contexts.get(&root)?;
            Some((ctx.repeat, ctx.bound?))
        })
    }

    /// Instance roots of every repeat template, by index.
    #[must_use]
    pub fn instances(&self) -> Vec<(NodeId, u32, NodeId)> {
        self.expansion
            .templates
            .iter()
            .flat_map(|(t, tpl)| tpl.instances.iter().map(|(n, root)| (*t, *n, *root)))
            .collect()
    }

    /// Whether item `n` of `template` is in use.
    #[must_use]
    pub fn item_used(&self, template: NodeId, n: u32) -> bool {
        let Some(t) = self.expansion.templates.get(&template) else {
            return false;
        };
        t.used_addr(n)
            .is_none_or(|a| self.real(&a).is_some_and(|v| v >= 0.5))
    }

    /// Whether `cond` holds now.
    #[must_use]
    pub fn holds(&self, cond: &Condition) -> bool {
        self.real(&cond.addr).is_some_and(|v| cond.test(v))
    }

    /// Whether node `id` is drawn: not in a template, not in an unused
    /// item, not under a failing `show_if`, not in a closed popover/menu.
    #[must_use]
    pub fn is_shown(&self, id: NodeId) -> bool {
        if self.expansion.template_nodes.contains(&id) {
            return false;
        }
        if let Some((template, n)) = self.expansion.item_of(id)
            && !self.item_used(template, n)
        {
            return false;
        }
        let scene = &self.surface.scene;
        let mut cur = Some(id);
        while let Some(c) = cur {
            let Some(node) = scene.node(c) else { break };
            if !node.visible {
                return false;
            }
            if let Some(control) = self.surface.controls.get(&c) {
                if control
                    .show_if
                    .as_ref()
                    .is_some_and(|cond| !self.holds(cond))
                {
                    return false;
                }
                match control.kind {
                    ControlKind::Popover { .. } if self.popover_origin(c).is_none() => {
                        return false;
                    }
                    ControlKind::Menu { .. } if !self.ui.menus.iter().any(|m| m.node == c) => {
                        return false;
                    }
                    _ => {}
                }
            }
            cur = node.parent;
        }
        true
    }

    // ── geometry ──────────────────────────────────────────────────────────

    /// A node's layout box (box space).
    pub(crate) fn box_of(&self, id: NodeId) -> Option<Bounds> {
        self.all_boxes
            .get(&id)
            .or_else(|| self.boxes.get(&id))
            .copied()
    }

    /// The top-level frame holding `id` (a popover's stage), and its box.
    pub(crate) fn stage_of(&self, id: NodeId) -> Option<Bounds> {
        let scene = &self.surface.scene;
        let mut cur = id;
        loop {
            let node = scene.node(cur)?;
            let parent = scene.node(node.parent?)?;
            if matches!(parent.kind, NodeKind::Page | NodeKind::Document) {
                return self.box_of(cur);
            }
            cur = parent.id;
        }
    }

    /// The popover or open menu a node sits in (the outermost floating
    /// ancestor, itself included).
    pub(crate) fn floating_root(&self, id: NodeId) -> Option<NodeId> {
        let scene = &self.surface.scene;
        let mut cur = Some(id);
        let mut found = None;
        while let Some(c) = cur {
            if matches!(
                self.surface.controls.get(&c).map(|k| &k.kind),
                Some(ControlKind::Popover { .. } | ControlKind::Menu { .. })
            ) {
                found = Some(c);
            }
            cur = scene.node(c).and_then(|n| n.parent);
        }
        found
    }

    /// How far a node is drawn from where it was laid out: its floating
    /// root's placement less its authored box.
    pub(crate) fn displacement(&self, id: NodeId) -> (f64, f64) {
        let Some(root) = self.floating_root(id) else {
            return (0.0, 0.0);
        };
        let (Some(at), Some(b)) = (self.floating_origin(root), self.box_of(root)) else {
            return (0.0, 0.0);
        };
        (at.0 - b.x, at.1 - b.y)
    }

    /// Where a shown popover or open menu is drawn (top-left).
    pub fn floating_origin(&self, id: NodeId) -> Option<(f64, f64)> {
        match self.surface.controls.get(&id)?.kind {
            ControlKind::Menu { .. } => self.menu_origin(id),
            _ => self.popover_origin(id),
        }
    }

    /// The anchor node of a popover right now (a context popover's: the
    /// active item's instance of it).
    pub(crate) fn popover_anchor(&self, id: NodeId) -> Option<NodeId> {
        let ControlKind::Popover {
            anchor, context, ..
        } = &self.surface.controls.get(&id)?.kind
        else {
            return None;
        };
        let anchor = NodeId((*anchor)?);
        match context {
            // Already bound to an item's instance.
            Some(_) if self.expansion.owner.contains_key(&anchor) => {
                Some(anchor).filter(|a| self.expansion.item_of(*a) == self.ui.active)
            }
            Some(t) => {
                let (template, n) = self.ui.active?;
                (template.0 == *t).then(|| instance_id(anchor, n))
            }
            None => Some(anchor),
        }
    }

    /// Whether popover `id` is up.
    fn popover_up(&self, id: NodeId, show: Show, anchor: NodeId) -> bool {
        if !self.is_live(anchor) {
            return false;
        }
        let dragging = self.captured == Some(anchor);
        let hovered = self.ui.hover == Some(anchor);
        let selected = self.ui.selected.contains(&anchor);
        let over_me = self
            .ui
            .hover
            .is_some_and(|h| h == id || self.floating_root(h) == Some(id));
        match show {
            Show::Always => true,
            Show::Hovered => dragging || hovered,
            Show::Selected => selected,
            Show::Active => {
                let item = self.expansion.item_of(anchor);
                dragging
                    || over_me
                    || (item.is_some() && item == self.ui.active)
                    || (item.is_none() && (hovered || self.ui.selected.last() == Some(&anchor)))
            }
        }
    }

    /// A node that is laid out, drawn and not in an unused item (anchors).
    fn is_live(&self, id: NodeId) -> bool {
        if self.expansion.template_nodes.contains(&id)
            || !self.boxes.contains_key(&id) && !self.all_boxes.contains_key(&id)
        {
            return false;
        }
        self.expansion
            .item_of(id)
            .is_none_or(|(t, n)| self.item_used(t, n))
            && self
                .surface
                .controls
                .get(&id)
                .is_none_or(|c| c.show_if.as_ref().is_none_or(|cond| self.holds(cond)))
    }

    /// Where a shown popover goes (top-left, surface coordinates): beside
    /// its anchor on its preferred side, flipped if that side runs off its
    /// stage, clamped inside it. `None` while it is down.
    #[must_use]
    pub fn popover_origin(&self, id: NodeId) -> Option<(f64, f64)> {
        let control = self.surface.controls.get(&id)?;
        let ControlKind::Popover {
            placement,
            gap,
            show,
            context,
            ..
        } = &control.kind
        else {
            return None;
        };
        if context.is_some()
            && !self
                .expansion
                .contexts
                .get(&id)
                .is_some_and(|c| c.bound.is_some())
        {
            return None;
        }
        let anchor = self.popover_anchor(id)?;
        if !self.popover_up(id, *show, anchor) {
            return None;
        }
        if *placement == Placement::Pinned {
            let t = self.surface.scene.node(id)?.transform;
            return Some((t.tx, t.ty));
        }
        let a = self.anchor_box(anchor)?;
        let size = self.surface.scene.node(id)?.size;
        let stage = self.stage_of(id).unwrap_or(Bounds {
            x: f64::MIN / 4.0,
            y: f64::MIN / 4.0,
            width: f64::MAX / 2.0,
            height: f64::MAX / 2.0,
        });
        if matches!(placement, Placement::DockBottom | Placement::DockTop) {
            // Docked to the anchor's area: a handle's plot, else the stage.
            let area = if self
                .surface
                .controls
                .get(&anchor)
                .is_some_and(|c| matches!(c.kind, ControlKind::Handle { .. }))
            {
                self.handle_area(anchor)
            } else {
                stage
            };
            let (x, y) = place(a, (size.width, size.height), *placement, *gap, area);
            // Held where it was while it is being used.
            let x = match self.ui.held.get(&id) {
                Some(h)
                    if h.2 == self.ui.active
                        && (self.ui.pointer_in_popover == Some(id)
                            || self.ui.clock - h.1 < POPOVER_HOLD_MS) =>
                {
                    h.0
                }
                _ => x,
            };
            return Some((x, y));
        }
        Some(place(a, (size.width, size.height), *placement, *gap, stage))
    }

    /// An anchor's box where it is drawn now (a handle's where its values
    /// put it).
    fn anchor_box(&self, anchor: NodeId) -> Option<Bounds> {
        let b = if self.surface.controls.contains_key(&anchor) {
            self.hit_box(anchor)
        } else {
            None
        };
        let b = b.or_else(|| self.box_of(anchor))?;
        let (dx, dy) = self.displacement(anchor);
        Some(Bounds {
            x: b.x + dx,
            y: b.y + dy,
            ..b
        })
    }

    /// Note the pointer against docked popovers: one under the pointer is
    /// in use, and keeps the x it had when first touched until
    /// [`POPOVER_HOLD_MS`] after the pointer leaves it.
    pub(crate) fn track_docked(&mut self, at: Option<(f64, f64)>, t: f64) {
        self.ui.clock = t;
        let docked: Vec<NodeId> = self
            .surface
            .controls
            .iter()
            .filter(|(_, c)| {
                matches!(
                    c.kind,
                    ControlKind::Popover {
                        placement: Placement::DockBottom | Placement::DockTop,
                        ..
                    }
                )
            })
            .map(|(id, _)| *id)
            .collect();
        let mut under = None;
        for id in docked {
            let Some((x, y)) = self.popover_origin(id) else {
                self.ui.held.remove(&id);
                continue;
            };
            let Some(size) = self.surface.scene.node(id).map(|n| n.size) else {
                continue;
            };
            // On the panel where it's drawn: not a handle drawn over it.
            let inside = at.is_some_and(|(px, py)| {
                px >= x
                    && py >= y
                    && px <= x + size.width
                    && py <= y + size.height
                    && self
                        .hit(px, py)
                        .is_none_or(|h| h == id || self.floating_root(h) == Some(id))
            });
            if inside {
                under = Some(id);
                let active = self.ui.active;
                let x0 = self
                    .ui
                    .held
                    .get(&id)
                    .filter(|h| h.2 == active)
                    .map_or(x, |h| h.0);
                self.ui.held.insert(id, (x0, t, active));
            }
        }
        // A drag that began on the panel keeps it in use until release.
        if under.is_none() && self.captured.is_some() {
            under = self.ui.pointer_in_popover.filter(|id| {
                self.captured
                    .is_some_and(|c| self.floating_root(c) == Some(*id))
            });
        }
        self.ui.pointer_in_popover = under;
        if let Some(id) = under
            && let Some(h) = self.ui.held.get_mut(&id)
        {
            h.1 = t;
        }
    }

    fn menu_origin(&self, id: NodeId) -> Option<(f64, f64)> {
        let m = self.ui.menus.iter().find(|m| m.node == id)?;
        let size = self.surface.scene.node(id)?.size;
        let stage = self.stage_of(id)?;
        let x = m.at.0.min(stage.x + stage.width - size.width).max(stage.x);
        let y =
            m.at.1
                .min(stage.y + stage.height - size.height)
                .max(stage.y);
        Some((x, y))
    }

    // ── hit testing ───────────────────────────────────────────────────────

    /// Controls on the floating layer (open menus, then shown popovers,
    /// topmost first) under a point.
    pub(crate) fn hit_floating(&self, x: f64, y: f64) -> Option<NodeId> {
        self.hit_floating_where(x, y, false)
    }

    /// Whether popover `id` is docked (drawn under its area's handles).
    pub(crate) fn is_docked(&self, id: NodeId) -> bool {
        self.surface.controls.get(&id).is_some_and(|c| {
            matches!(
                c.kind,
                ControlKind::Popover {
                    placement: Placement::DockBottom | Placement::DockTop,
                    ..
                }
            )
        })
    }

    /// Floating controls under a point: menus and ordinary popovers
    /// (`docked == false`), or only docked popovers (`docked == true`), which
    /// sit under the handles of their area.
    pub(crate) fn hit_floating_where(&self, x: f64, y: f64, docked: bool) -> Option<NodeId> {
        let mut roots: Vec<NodeId> = if docked {
            Vec::new()
        } else {
            self.ui.menus.iter().rev().map(|m| m.node).collect()
        };
        let mut pops: Vec<NodeId> = self
            .popover_layers()
            .into_iter()
            .filter(|id| self.is_docked(*id) == docked)
            .collect();
        pops.reverse();
        roots.extend(pops);
        for root in roots {
            let (dx, dy) = self.displacement(root);
            let order = self.surface.scene.walk(root);
            let hit = order.into_iter().rev().find(|id| {
                self.surface.controls.get(id).is_some_and(|c| {
                    // A popover's own box blocks what is under it — not a
                    // label riding a handle.
                    (c.kind.is_input()
                        && !matches!(
                            c.kind,
                            ControlKind::Popover {
                                show: Show::Always,
                                ..
                            }
                        ))
                        && self.hittable(*id, c)
                        && self.box_of(*id).is_some_and(|b| {
                            inside(
                                &Bounds {
                                    x: b.x + dx,
                                    y: b.y + dy,
                                    ..b
                                },
                                x,
                                y,
                            )
                        })
                })
            });
            if hit.is_some() {
                return hit;
            }
        }
        None
    }

    /// Popovers up now, in paint order (labels first, then panels).
    pub(crate) fn popover_layers(&self) -> Vec<NodeId> {
        let order = self.surface.scene.walk(self.surface.scene.root);
        let mut always = Vec::new();
        let mut rest = Vec::new();
        for id in order {
            let Some(c) = self.surface.controls.get(&id) else {
                continue;
            };
            let ControlKind::Popover { show, .. } = c.kind else {
                continue;
            };
            if self.expansion.template_nodes.contains(&id) || self.popover_origin(id).is_none() {
                continue;
            }
            if !self.is_shown_below(id) {
                continue;
            }
            if show == Show::Always {
                always.push(id)
            } else {
                rest.push(id)
            }
        }
        always.extend(rest);
        always
    }

    /// `is_shown` for the node's ancestors (not the node's own popover test).
    fn is_shown_below(&self, id: NodeId) -> bool {
        self.surface
            .scene
            .node(id)
            .and_then(|n| n.parent)
            .is_none_or(|p| self.is_shown(p))
            && self
                .surface
                .controls
                .get(&id)
                .is_none_or(|c| c.show_if.as_ref().is_none_or(|cond| self.holds(cond)))
            && self.surface.scene.node(id).is_some_and(|n| n.visible)
    }

    /// Whether a control takes pointer input now.
    pub(crate) fn hittable(&self, id: NodeId, c: &Control) -> bool {
        if let ControlKind::TextField { click_to_edit, .. } = c.kind
            && !click_to_edit
            && self.ui.edit.as_ref().is_none_or(|e| e.node != id)
        {
            return false;
        }
        self.is_shown(id)
    }

    // ── selection ─────────────────────────────────────────────────────────

    fn selectable(&self, id: NodeId) -> bool {
        self.surface
            .controls
            .get(&id)
            .is_some_and(|c| c.interact.as_ref().is_some_and(|i| i.selectable))
            && self.is_shown(id)
    }

    /// Select exactly `ids`.
    pub fn select(&mut self, ids: &[NodeId]) {
        self.ui.selected = ids
            .iter()
            .copied()
            .filter(|id| self.selectable(*id))
            .collect();
        self.update_active();
    }

    pub fn clear_selection(&mut self) {
        self.ui.selected.clear();
        self.update_active();
    }

    fn click_select(&mut self, id: NodeId, action: &ClickAction) {
        let sel = &mut self.ui.selected;
        match action {
            ClickAction::Select => {
                if let Some(i) = sel.iter().position(|s| *s == id) {
                    sel.remove(i);
                    sel.push(id);
                } else {
                    *sel = vec![id];
                }
            }
            ClickAction::ToggleSelection => {
                if let Some(i) = sel.iter().position(|s| *s == id) {
                    sel.remove(i);
                } else {
                    sel.push(id);
                }
            }
            ClickAction::RangeSelect => {
                let anchor = sel.last().copied();
                let item = |n: NodeId| self.expansion.item_of(n);
                let (Some(a), Some((t, hi))) = (anchor.and_then(item), item(id)) else {
                    *sel = vec![id];
                    return;
                };
                let (lo, hi) = if a.1 <= hi { (a.1, hi) } else { (hi, a.1) };
                let mut picked: Vec<NodeId> = self
                    .surface
                    .controls
                    .iter()
                    .filter(|(cid, c)| {
                        c.interact.as_ref().is_some_and(|i| i.selectable)
                            && self
                                .expansion
                                .item_of(**cid)
                                .is_some_and(|(ct, n)| ct == t && (lo..=hi).contains(&n))
                    })
                    .map(|(cid, _)| *cid)
                    .filter(|cid| self.is_shown(*cid))
                    .collect();
                picked.retain(|p| *p != id);
                picked.push(id);
                self.ui.selected = picked;
            }
            ClickAction::Run { .. } => {}
        }
    }

    /// Which item the context popovers follow: the dragged handle's, the
    /// hovered one's, the last selected one's — held while the pointer is
    /// on a context popover.
    pub(crate) fn update_active(&mut self) {
        let item = |id: Option<NodeId>| {
            id.and_then(|i| self.expansion.item_of(i))
                .filter(|(t, n)| self.item_used(*t, *n))
        };
        let handle = |id: &NodeId| {
            self.surface
                .controls
                .get(id)
                .is_some_and(|c| matches!(c.kind, ControlKind::Handle { .. }))
        };
        let over_context = self
            .ui
            .hover
            .and_then(|h| self.floating_root(h))
            .is_some_and(|r| self.expansion.contexts.contains_key(&r))
            || self.ui.pointer.is_some_and(|(x, y)| {
                self.expansion.contexts.keys().any(|r| {
                    self.floating_origin(*r).is_some_and(|(ox, oy)| {
                        self.surface.scene.node(*r).is_some_and(|n| {
                            inside(
                                &Bounds {
                                    x: ox,
                                    y: oy,
                                    width: n.size.width,
                                    height: n.size.height,
                                },
                                x,
                                y,
                            )
                        })
                    })
                })
            });
        let next = item(self.captured.filter(handle))
            .or_else(|| item(self.ui.hover.filter(handle)))
            .or_else(|| {
                if over_context || self.ui.edit.is_some() {
                    self.ui.active
                } else {
                    None
                }
            })
            .or_else(|| item(self.ui.selected.last().copied()))
            .filter(|(t, n)| self.item_used(*t, *n));
        self.ui.active = next;
        if let Some((template, n)) = next {
            let roots: Vec<NodeId> = self
                .expansion
                .contexts
                .iter()
                .filter(|(_, c)| c.repeat == template)
                .map(|(r, _)| *r)
                .collect();
            for root in roots {
                self.rebind(root, n);
            }
        }
    }

    /// Point context `root` at item `n` (fresh gestures for its controls).
    pub(crate) fn rebind(&mut self, root: NodeId, n: u32) {
        if bind_context(&mut self.surface, &mut self.expansion, root, n) {
            for id in self.surface.scene.walk(root) {
                if let Some(c) = self.surface.controls.get(&id) {
                    self.gestures.insert(id, Gesture::new(c.kind.clone()));
                }
            }
        }
    }

    // ── commands ──────────────────────────────────────────────────────────

    /// Set a param to `real` engineering units (a gesture's end).
    pub(crate) fn set_real(
        &mut self,
        addr: &ParamAddr,
        real: f64,
        t: f64,
        calls: &mut Vec<EngineCall>,
    ) {
        let Some(n) = self.normalized(addr, real) else {
            return;
        };
        let b = Binding::param(addr.0.clone());
        self.send(&b, n, EditPhase::End, t, calls);
    }

    pub(crate) fn set_text(&mut self, addr: &ParamAddr, text: String, calls: &mut Vec<EngineCall>) {
        if self.texts.get(addr) == Some(&text) {
            return;
        }
        self.texts.insert(addr.clone(), text.clone());
        calls.push(EngineCall::SetText {
            addr: addr.clone(),
            text,
        });
    }

    /// Run commands for `item` (`from`: the control they came from).
    pub(crate) fn run(
        &mut self,
        commands: &[Command],
        item: Option<(NodeId, u32)>,
        from: Option<NodeId>,
        t: f64,
    ) -> Vec<EngineCall> {
        let mut calls = Vec::new();
        let resolve = |addr: &ParamAddr| match item {
            Some((_, n)) => ParamAddr::new(crate::surface::proto::substitute(&addr.0, n)),
            None => addr.clone(),
        };
        for c in commands {
            match c {
                Command::Set { addr, value } => {
                    self.set_real(&resolve(addr), *value, t, &mut calls)
                }
                Command::Toggle { addr } => {
                    let a = resolve(addr);
                    let v = self.real(&a).unwrap_or(0.0);
                    self.set_real(&a, if v >= 0.5 { 0.0 } else { 1.0 }, t, &mut calls);
                }
                Command::Cycle { addr } => {
                    let a = resolve(addr);
                    let count = match self.catalog.param(&a).map(|p| &p.kind) {
                        Some(ParamKind::Choice { options }) => options.len(),
                        Some(ParamKind::Toggle) => 2,
                        _ => 0,
                    };
                    if count > 0 {
                        let v = self.real(&a).unwrap_or(0.0).round() as usize;
                        self.set_real(&a, ((v + 1) % count) as f64, t, &mut calls);
                    }
                }
                Command::Invert { addr } => {
                    let a = resolve(addr);
                    if let Some(v) = self.real(&a) {
                        self.set_real(&a, -v, t, &mut calls);
                    }
                }
                Command::Reset { addr } => {
                    let a = resolve(addr);
                    if let Some(p) = self.catalog.param(&a) {
                        let b = Binding::param(a.0.clone());
                        self.send(&b, p.default, EditPhase::End, t, &mut calls);
                    }
                }
                Command::Invoke { id } => calls.push(EngineCall::Invoke { action: id.clone() }),
                Command::Remove => {
                    let mut items: Vec<(NodeId, u32)> = Vec::new();
                    let from_selected = from.is_some_and(|f| self.ui.selected.contains(&f));
                    if from_selected {
                        items.extend(
                            self.ui
                                .selected
                                .iter()
                                .filter_map(|s| self.expansion.item_of(*s)),
                        );
                    }
                    items.extend(item);
                    calls.extend(self.remove_items(&items, t));
                }
                Command::Edit { node } => {
                    let id = match item {
                        Some((_, n)) if self.expansion.template_nodes.contains(&NodeId(*node)) => {
                            instance_id(NodeId(*node), n)
                        }
                        _ => NodeId(*node),
                    };
                    self.begin_edit(id, true);
                }
                Command::Add => {
                    if let Some((canvas, u, v)) = self.ui.menus.first().and_then(|m| m.point) {
                        calls.extend(self.add_at(canvas, u, v, t).0);
                    }
                }
            }
        }
        calls
    }

    /// Remove items: clear their `used` flags.
    pub fn remove_items(&mut self, items: &[(NodeId, u32)], t: f64) -> Vec<EngineCall> {
        let mut calls = Vec::new();
        let mut done: Vec<(NodeId, u32)> = Vec::new();
        for (template, n) in items {
            if done.contains(&(*template, *n)) {
                continue;
            }
            done.push((*template, *n));
            let Some(addr) = self
                .expansion
                .templates
                .get(template)
                .and_then(|tpl| tpl.used_addr(*n))
            else {
                continue;
            };
            self.set_real(&addr, 0.0, t, &mut calls);
        }
        self.ui.selected.retain(|s| {
            self.expansion
                .item_of(*s)
                .is_none_or(|i| !done.contains(&i))
        });
        if self
            .captured
            .is_some_and(|c| self.expansion.item_of(c).is_some_and(|i| done.contains(&i)))
        {
            self.captured = None;
        }
        self.update_active();
        calls
    }

    /// Add an item on `canvas` at unit point (`u`, `v`) per its add rule:
    /// the next free index, reset, marked used, placed, zoned, selected.
    /// Returns the calls and the new item's selectable control.
    pub fn add_at(
        &mut self,
        canvas: NodeId,
        u: f64,
        v: f64,
        t: f64,
    ) -> (Vec<EngineCall>, Option<NodeId>) {
        let mut calls = Vec::new();
        let Some(control) = self.surface.controls.get(&canvas).cloned() else {
            return (calls, None);
        };
        let ControlKind::Canvas {
            x,
            y,
            add: Some(rule),
        } = &control.kind
        else {
            return (calls, None);
        };
        let template = NodeId(rule.repeat);
        let Some(tpl) = self.expansion.templates.get(&template).cloned() else {
            return (calls, None);
        };
        let Some(n) = (tpl.first..tpl.first + tpl.count).find(|n| !self.item_used(template, *n))
        else {
            return (calls, None);
        };
        // A clean slate: every param of the item back to its default.
        if let Some(prefix) = tpl.family_prefix(n) {
            let family: Vec<(ParamAddr, f64, bool)> = self
                .catalog
                .params
                .iter()
                .filter(|p| p.addr.0.starts_with(&prefix))
                .map(|p| (p.addr.clone(), p.default, p.kind == ParamKind::Text))
                .collect();
            for (addr, default, text) in family {
                if text {
                    self.set_text(&addr, String::new(), &mut calls);
                } else {
                    let b = Binding::param(addr.0.clone());
                    self.send(&b, default, EditPhase::End, t, &mut calls);
                }
            }
        }
        let resolve = |a: &ParamAddr| ParamAddr::new(crate::surface::proto::substitute(&a.0, n));
        let y_axis = self.zoomed_axis(&control, y);
        let (xv, yv) = (
            x.from_unit(u.clamp(0.0, 1.0)),
            y_axis.from_unit(v.clamp(0.0, 1.0)),
        );
        let zone = rule.zone(xv, v).cloned();
        if let Some(a) = &rule.x {
            self.set_real(&resolve(a), xv, t, &mut calls);
        }
        if let Some(a) = &rule.y {
            let keep = zone.as_ref().is_none_or(|z| z.keep_y);
            self.set_real(&resolve(a), if keep { yv } else { 0.0 }, t, &mut calls);
        }
        for s in rule
            .set
            .iter()
            .chain(zone.iter().flat_map(|z| z.set.iter()))
        {
            self.set_real(&resolve(&s.addr), s.value, t, &mut calls);
        }
        if let Some(used) = tpl.used_addr(n) {
            self.set_real(&used, 1.0, t, &mut calls);
        }
        // Select the new item's selectable control.
        let new = self
            .surface
            .controls
            .iter()
            .filter(|(id, c)| {
                c.interact.as_ref().is_some_and(|i| i.selectable)
                    && self.expansion.item_of(**id) == Some((template, n))
            })
            .map(|(id, _)| *id)
            .next();
        if let Some(id) = new {
            self.ui.selected = vec![id];
        }
        self.update_active();
        (calls, new)
    }

    // ── pointer ───────────────────────────────────────────────────────────

    /// A press: menus first, then text editing, canvases, selection and
    /// chord clicks, then the control's own gesture.
    pub(crate) fn press(
        &mut self,
        x: f64,
        y: f64,
        mods: Modifiers,
        input: Input,
        t: f64,
    ) -> Vec<EngineCall> {
        let mut calls = Vec::new();
        self.ui.pointer = Some((x, y));
        if !self.ui.menus.is_empty() {
            match self.menu_at(x, y) {
                Some((depth, row)) => {
                    calls.extend(self.menu_activate(depth, row, t));
                    return calls;
                }
                None => {
                    // A press elsewhere only closes the menu.
                    self.ui.menus.clear();
                    return calls;
                }
            }
        }
        let hit = self.hit(x, y);
        if let Some(e) = &self.ui.edit
            && hit != Some(e.node)
        {
            calls.extend(self.commit_edit());
        }
        self.captured = hit;
        self.ui.press = Some(Press {
            x,
            y,
            moved: false,
            deferred: Vec::new(),
            item: hit.and_then(|h| self.item_of(h)),
            inert: false,
        });
        self.ui.outside = None;
        self.ui.group = None;
        self.ui.wheel_drag = None;
        let Some(id) = hit else {
            if !mods.fine {
                self.ui.selected.clear();
            }
            self.update_active();
            return calls;
        };
        self.ui.focus = Some(id);
        let Some(control) = self.surface.controls.get(&id).cloned() else {
            return calls;
        };
        match &control.kind {
            ControlKind::Canvas { .. } => {
                let base = if mods.fine {
                    self.ui.selected.clone()
                } else {
                    Vec::new()
                };
                self.ui.selected = base.clone();
                self.ui.marquee = Some(Marquee {
                    canvas: id,
                    from: (x, y),
                    to: (x, y),
                    base,
                });
                self.update_active();
                return calls;
            }
            ControlKind::TextField { .. } => {
                let (dx, _) = self.displacement(id);
                if self.ui.edit.as_ref().is_none_or(|e| e.node != id) {
                    self.begin_edit(id, false);
                }
                self.place_caret(id, x - dx, mods.fine);
                return calls;
            }
            ControlKind::Popover { .. } | ControlKind::Menu { .. } => return calls,
            _ => {}
        }
        if let Some(interact) = control.interact.clone() {
            let action = interact
                .click_for(chord(mods))
                .cloned()
                .or_else(|| interact.selectable.then_some(ClickAction::Select));
            match action {
                // Runs on a release without travel; a drag is a drag.
                Some(ClickAction::Run { commands }) => {
                    if let Some(p) = &mut self.ui.press {
                        p.deferred = commands;
                    }
                }
                Some(a) if interact.selectable => self.click_select(id, &a),
                _ => {}
            }
            if !interact.press.is_empty()
                && let Some(p) = &mut self.ui.press
            {
                p.deferred = interact.press.clone();
            }
            // A group drag: every selected handle moves with this one.
            if self.ui.selected.len() > 1 && self.ui.selected.contains(&id) {
                let starts: HashMap<NodeId, (f64, f64)> = self
                    .ui
                    .selected
                    .clone()
                    .into_iter()
                    .filter_map(|s| {
                        let c = self.surface.controls.get(&s)?;
                        let (u, v) = self.handle_uv(c)?;
                        Some((s, (u.unwrap_or(0.5), v.unwrap_or(0.5))))
                    })
                    .collect();
                self.ui.group = Some((id, starts));
            }
        }
        self.update_active();
        calls.extend(self.feed(id, input, t));
        calls
    }

    pub(crate) fn drag(&mut self, x: f64, y: f64, input: Input, t: f64) -> Vec<EngineCall> {
        self.ui.pointer = Some((x, y));
        if let Some(p) = &mut self.ui.press
            && !p.moved
            && (x - p.x).hypot(y - p.y) > CLICK_SLOP
        {
            p.moved = true;
        }
        let Some(id) = self.captured else {
            self.ui.hover = self.hit(x, y);
            if let Some(depth) = self.menu_at(x, y) {
                self.menu_hover(depth.0, depth.1);
            }
            self.update_active();
            return Vec::new();
        };
        if let Some(m) = &mut self.ui.marquee {
            m.to = (x, y);
            let (x0, x1) = (m.from.0.min(x), m.from.0.max(x));
            let (y0, y1) = (m.from.1.min(y), m.from.1.max(y));
            let base = m.base.clone();
            let inside_band: Vec<NodeId> = self
                .surface
                .controls
                .keys()
                .copied()
                .filter(|cid| self.selectable(*cid))
                .filter(|cid| {
                    self.hit_box(*cid).is_some_and(|b| {
                        let (cx, cy) = (b.x + b.width / 2.0, b.y + b.height / 2.0);
                        cx >= x0 && cx <= x1 && cy >= y0 && cy <= y1
                    })
                })
                .collect();
            let mut sel = base;
            for c in inside_band {
                if !sel.contains(&c) {
                    sel.push(c);
                }
            }
            self.ui.selected = sel;
            return Vec::new();
        }
        if self.ui.press.as_ref().is_some_and(|p| p.inert) {
            return Vec::new();
        }
        // Dragged out of its area: its item goes on release.
        if let Some(c) = self.surface.controls.get(&id)
            && c.interact.as_ref().is_some_and(|i| i.removable)
            && matches!(c.kind, ControlKind::Handle { .. })
        {
            let (lx, ly) = self.local(id, x, y);
            let area = self.handle_area(id);
            self.ui.outside = (!inside(&area, lx, ly)).then_some(id);
        }
        self.feed(id, input, t)
    }

    pub(crate) fn release(&mut self, input: Input, t: f64) -> Vec<EngineCall> {
        let mut calls = Vec::new();
        let press = self.ui.press.take();
        self.ui.marquee = None;
        let Some(id) = self.captured else {
            return calls;
        };
        let inert = press.as_ref().is_some_and(|p| p.inert);
        if !inert {
            calls.extend(self.feed(id, input, t));
        }
        if let Some(p) = press
            && !p.moved
            && !p.deferred.is_empty()
        {
            let inside_now = match input {
                Input::Up { x, y, .. } => {
                    let (dx, dy) = self.displacement(id);
                    self.hit_box(id).is_none_or(|b| inside(&b, x - dx, y - dy))
                }
                _ => false,
            };
            if inside_now {
                calls.extend(self.run(&p.deferred, p.item, Some(id), t));
            }
        }
        if self.ui.outside.take() == Some(id)
            && let Some(item) = self.expansion.item_of(id)
        {
            calls.extend(self.remove_items(&[item], t));
        }
        self.ui.group = None;
        self.ui.wheel_drag = None;
        self.captured = None;
        self.update_active();
        calls
    }

    /// A double-click on the captured control: its `double_click`
    /// commands, a canvas's add, else the gesture's own (a reset).
    pub(crate) fn double_click(&mut self, t: f64) -> Vec<EngineCall> {
        let Some(id) = self.captured else {
            return Vec::new();
        };
        let Some(control) = self.surface.controls.get(&id).cloned() else {
            return Vec::new();
        };
        if let ControlKind::Canvas { .. } = control.kind {
            self.ui.marquee = None;
            let Some(p) = self.ui.press.clone() else {
                return Vec::new();
            };
            let Some(b) = self.box_of(id) else {
                return Vec::new();
            };
            let (lx, ly) = self.local(id, p.x, p.y);
            let (u, v) = (
                (lx - b.x) / b.width.max(1.0),
                1.0 - (ly - b.y) / b.height.max(1.0),
            );
            let (mut calls, new) = self.add_at(id, u, v, t);
            // The new item is under the hand: keep dragging it.
            if let Some(h) = new {
                self.captured = Some(h);
                if let Some(pr) = &mut self.ui.press {
                    pr.item = self.expansion.item_of(h);
                }
                calls.extend(self.feed(
                    h,
                    Input::Down {
                        x: p.x,
                        y: p.y,
                        t,
                        mods: Modifiers::default(),
                    },
                    t,
                ));
            }
            return calls;
        }
        if let ControlKind::TextField { .. } = control.kind {
            if let Some(e) = &mut self.ui.edit {
                e.anchor = 0;
                e.caret = e.buffer.chars().count();
            }
            return Vec::new();
        }
        if let Some(i) = &control.interact
            && !i.double_click.is_empty()
        {
            let item = self.item_of(id);
            let cmds = i.double_click.clone();
            // The press that made it is not a drag.
            if let Some(p) = &mut self.ui.press {
                p.inert = true;
                p.deferred.clear();
            }
            let mut calls = self.feed(id, Input::Cancel { t }, t);
            calls.extend(self.run(&cmds, item, Some(id), t));
            return calls;
        }
        self.feed(id, Input::DoubleClick, t)
    }

    /// A handle's input with its interaction rules: wheel rules, drag
    /// modes, a group drag.
    pub(crate) fn feed_handle(
        &mut self,
        id: NodeId,
        control: &Control,
        input: Input,
        uv: (f64, f64),
        area: Bounds,
        t: f64,
    ) -> Vec<EngineCall> {
        let interact = control.interact.clone().unwrap_or_default();
        let mut calls = Vec::new();
        if let Input::Wheel { notches, mods } = input {
            let c = chord(mods);
            let unfine = Chord { shift: false, ..c };
            let rule = interact
                .wheel
                .iter()
                .find(|r| {
                    (r.chord == c || r.chord == unfine)
                        && r.when.as_ref().is_none_or(|w| self.holds(w))
                })
                .cloned();
            if let Some(r) = rule {
                self.nudge(&r.target, notches, r.step, mods.fine, t, &mut calls);
                return calls;
            }
        }
        let mods = match input {
            Input::Move { mods, .. } | Input::Down { mods, .. } => Some(mods),
            _ => None,
        };
        // The first rule for the chord (Shift, fine, may ride along) whose
        // condition holds; a release keeps the mode of the last move.
        let mode = mods.map_or(self.ui.drag_mode.unwrap_or(DragMode::Free), |m| {
            let c = chord(m);
            let unfine = Chord { shift: false, ..c };
            interact
                .drag
                .iter()
                .find(|r| {
                    (r.chord == c || r.chord == unfine)
                        && r.when.as_ref().is_none_or(|w| self.holds(w))
                })
                .map_or(DragMode::Free, |r| r.mode)
        });
        self.ui.drag_mode = if matches!(input, Input::Up { .. } | Input::Cancel { .. }) {
            None
        } else {
            Some(mode)
        };
        // With chord rules, Cmd/Ctrl is a chord, not the gesture's reset.
        let input = match input {
            Input::Down { x, y, t, mods }
                if !interact.click.is_empty() || !interact.drag.is_empty() =>
            {
                Input::Down {
                    x,
                    y,
                    t,
                    mods: Modifiers { alt: false, ..mods },
                }
            }
            other => other,
        };
        if matches!(input, Input::Down { .. }) {
            self.ui.wheel_drag = Some(uv.1);
        }
        let outputs = self
            .gestures
            .entry(id)
            .or_insert_with(|| Gesture::new(control.kind.clone()))
            .point(input, uv, area);
        let start = self
            .ui
            .group
            .as_ref()
            .and_then(|(_, s)| s.get(&id).copied());
        for o in outputs {
            let Output::SetXy {
                x: nu,
                y: nv,
                phase,
            } = o
            else {
                self.handle_output(control, o, t, &mut calls);
                continue;
            };
            let from = start.unwrap_or(uv);
            let (nu, nv) = match mode {
                DragMode::Free => (nu, nv),
                DragMode::XOnly => (nu, uv.1),
                DragMode::YOnly => (uv.0, nv),
                DragMode::Wheel { per_px } => {
                    let last = self.ui.wheel_drag.unwrap_or(nv);
                    self.ui.wheel_drag = Some(nv);
                    // The gesture's travel is already fine-scaled by Shift.
                    let dy_px = (nv - last) * area.height;
                    if let ControlKind::Handle { wheel: Some(b), .. } = &control.kind
                        && let Source::Param { addr } = &b.source
                        && dy_px != 0.0
                    {
                        let now = self.values.get(addr).map_or(0.0, |v| b.to_position(*v));
                        let b = b.clone();
                        self.send(
                            &b,
                            (now + dy_px * per_px).clamp(0.0, 1.0),
                            EditPhase::Change,
                            t,
                            &mut calls,
                        );
                    }
                    (uv.0, uv.1)
                }
            };
            if !matches!(mode, DragMode::Wheel { .. }) {
                self.ui.wheel_drag = None;
            }
            self.handle_output(
                control,
                Output::SetXy {
                    x: nu,
                    y: nv,
                    phase,
                },
                t,
                &mut calls,
            );
            // The rest of the group follows by the same travel.
            if let Some((grabbed, starts)) = self.ui.group.clone()
                && grabbed == id
            {
                let (du, dv) = (nu - from.0, nv - from.1);
                for (other, (su, sv)) in starts {
                    if other == id {
                        continue;
                    }
                    if let Some(oc) = self.surface.controls.get(&other).cloned() {
                        let out = Output::SetXy {
                            x: (su + du).clamp(0.0, 1.0),
                            y: (sv + dv).clamp(0.0, 1.0),
                            phase,
                        };
                        self.handle_output(&oc, out, t, &mut calls);
                    }
                }
            }
        }
        calls
    }

    /// Turn `addr` by `notches` × `step` position units (a stepped param:
    /// whole detents).
    fn nudge(
        &mut self,
        addr: &ParamAddr,
        notches: f64,
        step: f64,
        fine: bool,
        t: f64,
        calls: &mut Vec<EngineCall>,
    ) {
        let Some(p) = self.catalog.param(addr) else {
            return;
        };
        let now = self.values.get(addr).copied().unwrap_or(p.default);
        let next = match p.range.steps.filter(|n| *n >= 2) {
            Some(n) => {
                let k = f64::from(n - 1);
                (((now * k).round() + notches.signum() * notches.abs().round().max(1.0))
                    .clamp(0.0, k))
                    / k
            }
            None => (now + notches * step * if fine { FINE } else { 1.0 }).clamp(0.0, 1.0),
        };
        let b = Binding::param(addr.0.clone());
        self.send(&b, next, EditPhase::End, t, calls);
    }

    // ── context menus ─────────────────────────────────────────────────────

    /// A right-click at (`x`, `y`): opens the menu of the control there
    /// (selecting it), or of the canvas. Returns whether one opened.
    pub fn context_click(&mut self, x: f64, y: f64, t: f64) -> Vec<EngineCall> {
        let mut calls = Vec::new();
        if let Some(e) = &self.ui.edit
            && self.hit(x, y) != Some(e.node)
        {
            calls.extend(self.commit_edit());
        }
        self.ui.menus.clear();
        self.ui.pointer = Some((x, y));
        let Some(id) = self.hit(x, y) else {
            return calls;
        };
        let Some(control) = self.surface.controls.get(&id).cloned() else {
            return calls;
        };
        let Some(menu) = control.interact.as_ref().and_then(|i| i.menu) else {
            return calls;
        };
        let item = self.expansion.item_of(id);
        let menu = match item {
            Some((_, n)) if self.expansion.template_nodes.contains(&NodeId(menu)) => {
                instance_id(NodeId(menu), n)
            }
            _ => NodeId(menu),
        };
        let point = match control.kind {
            ControlKind::Canvas { .. } => {
                self.ui.selected.clear();
                let b = self.box_of(id).unwrap_or_default();
                let (lx, ly) = self.local(id, x, y);
                Some((
                    id,
                    (lx - b.x) / b.width.max(1.0),
                    1.0 - (ly - b.y) / b.height.max(1.0),
                ))
            }
            _ => {
                if control.interact.as_ref().is_some_and(|i| i.selectable)
                    && !self.ui.selected.contains(&id)
                {
                    self.ui.selected = vec![id];
                }
                None
            }
        };
        if let Some((_, n)) = item
            && self.expansion.contexts.contains_key(&menu)
        {
            self.rebind_menu(menu, n);
        }
        self.ui.menus.push(OpenMenu {
            node: menu,
            at: (x + MENU_OFFSET, y + MENU_OFFSET),
            item,
            point,
            active: None,
        });
        self.update_active();
        let _ = t;
        calls
    }

    /// Bind a context menu (and its submenus) to item `n`.
    fn rebind_menu(&mut self, menu: NodeId, n: u32) {
        self.rebind(menu, n);
        let subs: Vec<NodeId> = match self.surface.controls.get(&menu).map(|c| &c.kind) {
            Some(ControlKind::Menu { items, .. }) => {
                items.iter().filter_map(|i| i.submenu).map(NodeId).collect()
            }
            _ => Vec::new(),
        };
        for s in subs {
            if self.expansion.contexts.contains_key(&s) {
                self.rebind(s, n);
            }
        }
    }

    fn menu_items(&self, menu: NodeId) -> Vec<crate::surface::proto::MenuItem> {
        match self.surface.controls.get(&menu).map(|c| &c.kind) {
            Some(ControlKind::Menu { items, .. }) => items.clone(),
            _ => Vec::new(),
        }
    }

    /// The (menu depth, row) under a point.
    fn menu_at(&self, x: f64, y: f64) -> Option<(usize, Option<usize>)> {
        for (depth, m) in self.ui.menus.iter().enumerate().rev() {
            let Some((ox, oy)) = self.menu_origin(m.node) else {
                continue;
            };
            let Some(node) = self.surface.scene.node(m.node) else {
                continue;
            };
            if !inside(
                &Bounds {
                    x: ox,
                    y: oy,
                    width: node.size.width,
                    height: node.size.height,
                },
                x,
                y,
            ) {
                continue;
            }
            let (dx, dy) = self.displacement(m.node);
            let row = self.menu_items(m.node).iter().position(|it| {
                it.selectable()
                    && self.box_of(NodeId(it.row)).is_some_and(|b| {
                        inside(
                            &Bounds {
                                x: b.x + dx,
                                y: b.y + dy,
                                ..b
                            },
                            x,
                            y,
                        )
                    })
            });
            return Some((depth, row));
        }
        None
    }

    fn menu_hover(&mut self, depth: usize, row: Option<usize>) {
        self.ui.menus.truncate(depth + 1);
        if let Some(m) = self.ui.menus.get_mut(depth) {
            m.active = row;
        }
        if let Some(r) = row {
            self.open_submenu(depth, r);
        }
    }

    /// Open row `row`'s submenu of menu `depth` beside it.
    fn open_submenu(&mut self, depth: usize, row: usize) {
        let Some(m) = self.ui.menus.get(depth).cloned() else {
            return;
        };
        let items = self.menu_items(m.node);
        let Some(sub) = items.get(row).and_then(|i| i.submenu) else {
            return;
        };
        let sub = NodeId(sub);
        let (dx, dy) = self.displacement(m.node);
        let Some(rb) = self.box_of(NodeId(items[row].row)) else {
            return;
        };
        let Some(menu_node) = self.surface.scene.node(m.node) else {
            return;
        };
        let (ox, _) = self.menu_origin(m.node).unwrap_or((rb.x + dx, rb.y + dy));
        let w = self.surface.scene.node(sub).map_or(0.0, |n| n.size.width);
        let stage = self.stage_of(m.node).unwrap_or_default();
        let right = ox + menu_node.size.width - 2.0;
        let x = if right + w > stage.x + stage.width {
            ox - w + 2.0
        } else {
            right
        };
        self.ui.menus.truncate(depth + 1);
        if let Some((_, n)) = m.item
            && self.expansion.contexts.contains_key(&sub)
        {
            self.rebind(sub, n);
        }
        self.ui.menus.push(OpenMenu {
            node: sub,
            at: (x, rb.y + dy - 4.0),
            item: m.item,
            point: m.point,
            active: None,
        });
    }

    fn menu_activate(&mut self, depth: usize, row: Option<usize>, t: f64) -> Vec<EngineCall> {
        let Some(row) = row else { return Vec::new() };
        let Some(m) = self.ui.menus.get(depth).cloned() else {
            return Vec::new();
        };
        let items = self.menu_items(m.node);
        let Some(item) = items.get(row) else {
            return Vec::new();
        };
        if item.submenu.is_some() && item.commands.is_empty() {
            self.menu_hover(depth, Some(row));
            return Vec::new();
        }
        let commands = item.commands.clone();
        // Commands resolve against the item (already bound for context
        // menus: resolve once more is harmless).
        let point = self.ui.menus.first().and_then(|m| m.point);
        let calls = {
            let saved = self.ui.menus.clone();
            self.ui.menus = vec![OpenMenu {
                point,
                ..saved[0].clone()
            }];
            self.run(&commands, m.item, None, t)
        };
        self.ui.menus.clear();
        self.update_active();
        calls
    }

    fn menu_key(&mut self, key: Key, t: f64) -> Vec<EngineCall> {
        let depth = self.ui.menus.len() - 1;
        let m = self.ui.menus[depth].clone();
        let items = self.menu_items(m.node);
        let selectable: Vec<usize> = items
            .iter()
            .enumerate()
            .filter(|(_, i)| i.role == ItemRole::Action)
            .map(|(i, _)| i)
            .collect();
        let step = |dir: isize| {
            if selectable.is_empty() {
                return None;
            }
            let pos = m
                .active
                .and_then(|a| selectable.iter().position(|s| *s == a));
            let len = selectable.len() as isize;
            let next = match pos {
                Some(p) => (p as isize + dir).rem_euclid(len),
                None if dir > 0 => 0,
                None => len - 1,
            };
            Some(selectable[next as usize])
        };
        match key {
            Key::Down => self.ui.menus[depth].active = step(1),
            Key::Up => self.ui.menus[depth].active = step(-1),
            Key::Right => {
                if let Some(a) = m.active {
                    self.open_submenu(depth, a);
                    if let Some(sub) = self.ui.menus.get(depth + 1).cloned() {
                        let first = self
                            .menu_items(sub.node)
                            .iter()
                            .position(|i| i.role == ItemRole::Action);
                        self.ui.menus[depth + 1].active = first;
                    }
                }
            }
            Key::Left | Key::Escape => {
                self.ui.menus.pop();
            }
            Key::Enter | Key::Space => {
                if let Some(a) = m.active {
                    return self.menu_activate(depth, Some(a), t);
                }
            }
            _ => {}
        }
        Vec::new()
    }

    // ── keyboard ──────────────────────────────────────────────────────────

    /// A key: to the text field being edited, else an open menu, else the
    /// selection (Delete / Backspace remove, Escape clears), else the
    /// focused control. `None` when nothing took it (the host may use it).
    pub fn key(&mut self, key: Key, mods: Modifiers, t: f64) -> Option<Vec<EngineCall>> {
        if self.ui.edit.is_some() {
            return Some(self.edit_key(key, mods));
        }
        if !self.ui.menus.is_empty() {
            return Some(self.menu_key(key, t));
        }
        match key {
            Key::Delete | Key::Backspace if !self.ui.selected.is_empty() => {
                let items: Vec<(NodeId, u32)> = self
                    .ui
                    .selected
                    .iter()
                    .filter(|s| {
                        self.surface
                            .controls
                            .get(s)
                            .is_some_and(|c| c.interact.as_ref().is_some_and(|i| i.removable))
                    })
                    .filter_map(|s| self.expansion.item_of(*s))
                    .collect();
                if items.is_empty() {
                    return None;
                }
                return Some(self.remove_items(&items, t));
            }
            Key::Escape if !self.ui.selected.is_empty() => {
                self.clear_selection();
                return Some(Vec::new());
            }
            // A letter the selection's key rules name: for each selected item.
            Key::Char(c) if !mods.alt && !self.ui.selected.is_empty() => {
                let mut calls = Vec::new();
                let mut any = false;
                for s in self.ui.selected.clone() {
                    let Some(rule) = self
                        .surface
                        .controls
                        .get(&s)
                        .and_then(|ctl| ctl.interact.as_ref())
                        .and_then(|i| {
                            i.keys
                                .iter()
                                .find(|k| k.key.to_lowercase() == c.to_lowercase().to_string())
                                .cloned()
                        })
                    else {
                        continue;
                    };
                    any = true;
                    let item = self.item_of(s);
                    calls.extend(self.run(&rule.commands, item, None, t));
                }
                if any {
                    return Some(calls);
                }
            }
            _ => {}
        }
        let target = self.ui.selected.last().copied().or(self.ui.focus)?;
        if !self.surface.controls.contains_key(&target)
            || matches!(key, Key::Escape | Key::Tab | Key::Char(_))
        {
            return None;
        }
        Some(self.feed(target, Input::Key { key, mods }, t))
    }
}

/// Place a `size` box beside `a` on `side` with `gap`, flipped to the
/// opposite side if it runs off `stage`, then clamped inside it.
#[must_use]
pub fn place(
    a: Bounds,
    (w, h): (f64, f64),
    side: Placement,
    gap: f64,
    stage: Bounds,
) -> (f64, f64) {
    let (cx, cy) = (a.x + a.width / 2.0, a.y + a.height / 2.0);
    let (sx0, sy0, sx1, sy1) = (
        stage.x,
        stage.y,
        stage.x + stage.width,
        stage.y + stage.height,
    );
    let (x, y) = match side {
        Placement::Above | Placement::Below => {
            let above = a.y - gap - h;
            let below = a.y + a.height + gap;
            let y = match side {
                Placement::Above if above < sy0 && below + h <= sy1 => below,
                Placement::Above => above,
                _ if below + h > sy1 && above >= sy0 => above,
                _ => below,
            };
            (cx - w / 2.0, y)
        }
        Placement::DockBottom => (cx - w / 2.0, sy1 - h - gap),
        Placement::DockTop => (cx - w / 2.0, sy0 + gap),
        // Not placed: it stays where it is drawn (see `popover_origin`).
        Placement::Pinned => (a.x, a.y),
        Placement::Left | Placement::Right => {
            let left = a.x - gap - w;
            let right = a.x + a.width + gap;
            let x = match side {
                Placement::Left if left < sx0 && right + w <= sx1 => right,
                Placement::Left => left,
                _ if right + w > sx1 && left >= sx0 => left,
                _ => right,
            };
            (x, cy - h / 2.0)
        }
    };
    (x.min(sx1 - w).max(sx0), y.min(sy1 - h).max(sy0))
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAGE: Bounds = Bounds {
        x: 0.0,
        y: 0.0,
        width: 400.0,
        height: 300.0,
    };

    #[test]
    fn popovers_flip_and_clamp() {
        let a = Bounds {
            x: 190.0,
            y: 140.0,
            width: 20.0,
            height: 20.0,
        };
        assert_eq!(
            place(a, (100.0, 50.0), Placement::Above, 4.0, STAGE),
            (150.0, 86.0)
        );
        assert_eq!(
            place(a, (100.0, 50.0), Placement::Below, 4.0, STAGE),
            (150.0, 164.0)
        );
        // Near the top: flips below.
        let top = Bounds { y: 10.0, ..a };
        assert_eq!(
            place(top, (100.0, 50.0), Placement::Above, 4.0, STAGE).1,
            34.0
        );
        // Near the left edge: clamped in.
        let left = Bounds { x: 0.0, ..a };
        assert_eq!(
            place(left, (100.0, 50.0), Placement::Above, 4.0, STAGE).0,
            0.0
        );
        // Right of an anchor at the right edge: flips left.
        let right = Bounds { x: 380.0, ..a };
        assert_eq!(
            place(right, (100.0, 50.0), Placement::Right, 4.0, STAGE).0,
            276.0
        );
    }

    #[test]
    fn chords_from_modifiers() {
        assert_eq!(
            chord(Modifiers {
                fine: true,
                alt: false,
                option: true
            }),
            Chord::OPTION_SHIFT
        );
        assert_eq!(chord(Modifiers::default()), Chord::NONE);
    }
}
