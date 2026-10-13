//! Canonical controls as real design subtrees.
//!
//! Each builder inserts ordinary nodes (frames, ellipses, rectangles, text)
//! and a [`Control`] whose slots point at them — nothing special in the
//! document. A designer can restyle or redraw any part afterwards; as long
//! as the slots still name the right nodes, the control keeps working.

use std::collections::BTreeMap;

use crate::document::proto::{
    Color, CornerRadii, Node, NodeId, NodeKind, Op, Paint, Scene, Size, Stroke, TextAlign,
    TextData, Transform,
};
use crate::surface::proto::{Binding, ButtonMode, Control, ControlKind, Orientation, SlotRole};

use crate::surface::widgets::theme::{ControlSize, Theme};

/// Inserts nodes and records controls.
pub struct Builder<'a> {
    pub scene: &'a mut Scene,
    pub controls: &'a mut BTreeMap<NodeId, Control>,
    pub theme: &'a Theme,
}

impl Builder<'_> {
    /// Append `node` under `parent`; returns its id.
    pub fn add(&mut self, parent: NodeId, node: Node) -> NodeId {
        let id = node.id;
        self.scene
            .apply(Op::Insert {
                node,
                parent,
                index: u32::MAX,
            })
            .expect("builder parents are nodes it created");
        id
    }

    fn text(
        &self,
        name: &str,
        text: &str,
        size: f64,
        color: Color,
        width: f64,
        align: TextAlign,
    ) -> Node {
        let mut n = Node::new(
            NodeKind::Text(TextData {
                characters: text.to_string(),
                font_size: size,
                align,
                ..TextData::default()
            }),
            name,
        );
        n.fills = vec![Paint::solid(color)];
        n.size = Size {
            width,
            height: (size * 1.4).ceil(),
        };
        n
    }

    fn frame(name: &str, w: f64, h: f64) -> Node {
        let mut n = Node::new(NodeKind::Frame, name);
        n.size = Size {
            width: w,
            height: h,
        };
        n
    }

    fn shape(kind: NodeKind, name: &str, x: f64, y: f64, w: f64, h: f64, fill: Color) -> Node {
        let mut n = Node::new(kind, name);
        n.transform = Transform::translate(x, y);
        n.size = Size {
            width: w,
            height: h,
        };
        n.fills = vec![Paint::solid(fill)];
        n
    }

    fn register(&mut self, id: NodeId, control: Control) {
        self.controls.insert(id, control);
    }

    /// A rotary knob: dial (track arc, value arc, cap, pointer) with a
    /// label above and a readout below.
    pub fn knob(
        &mut self,
        parent: NodeId,
        name: &str,
        size: ControlSize,
        binding: Option<Binding>,
    ) -> NodeId {
        let t = self.theme.clone();
        let d = size.px();
        let label_h = (t.label_size * 1.4).ceil();
        let value_h = (t.value_size * 1.4).ceil();
        let root = self.add(
            parent,
            Self::frame(name, d, label_h + 4.0 + d + 4.0 + value_h),
        );
        let label = self.add(
            root,
            self.text("label", "", t.label_size, t.text_dim, d, TextAlign::Center),
        );

        let mut dial = Self::frame("dial", d, d);
        dial.transform = Transform::translate(0.0, label_h + 4.0);
        let dial = self.add(root, dial);
        let ring = (d * 0.14).max(3.0);
        let mut track = Self::shape(NodeKind::Vector, "track", 0.0, 0.0, d, d, t.track);
        track.fill_geometry = vec![sector_path(
            d * 0.5,
            d * 0.5,
            d * 0.5 - ring,
            d * 0.5,
            -135f64.to_radians(),
            135f64.to_radians(),
        )];
        self.add(dial, track);
        let arc = self.add(
            dial,
            Self::shape(NodeKind::Ellipse, "value-arc", 0.0, 0.0, d, d, t.accent),
        );
        let inset = ring + d * 0.05;
        let cap_d = d - 2.0 * inset;
        let mut cap = Self::shape(NodeKind::Ellipse, "cap", inset, inset, cap_d, cap_d, t.body);
        cap.stroke = Stroke {
            paints: vec![Paint::solid(t.track)],
            weight: 1.0,
            ..Stroke::default()
        };
        self.add(dial, cap);
        let pw = (d * 0.06).max(2.0);
        let mut pointer = Self::shape(
            NodeKind::Rectangle,
            "pointer",
            d * 0.5 - pw * 0.5,
            inset + d * 0.04,
            pw,
            cap_d * 0.34,
            t.pointer,
        );
        pointer.corner_radii = CornerRadii::uniform(pw * 0.5);
        let pointer = self.add(dial, pointer);

        let mut value = self.text("value", "", t.value_size, t.text, d, TextAlign::Center);
        value.transform = Transform::translate(0.0, label_h + 4.0 + d + 4.0);
        let value = self.add(root, value);

        let mut c = Control::new(ControlKind::knob())
            .slot(SlotRole::Label, label.0)
            .slot(SlotRole::ValueArc, arc.0)
            .slot(SlotRole::Needle, pointer.0)
            .slot(SlotRole::Value, value.0);
        c.binding = binding;
        self.register(root, c);
        root
    }

    /// A vertical fader: slot, fill, cap; label above, readout below.
    pub fn fader(
        &mut self,
        parent: NodeId,
        name: &str,
        height: f64,
        binding: Option<Binding>,
    ) -> NodeId {
        let t = self.theme.clone();
        let w = 44.0_f64.max(t.min_touch);
        let label_h = (t.label_size * 1.4).ceil();
        let value_h = (t.value_size * 1.4).ceil();
        let root = self.add(
            parent,
            Self::frame(name, w, label_h + 4.0 + height + 4.0 + value_h),
        );
        let label = self.add(
            root,
            self.text("label", "", t.label_size, t.text_dim, w, TextAlign::Center),
        );
        let mut lane = Self::frame("lane", w, height);
        lane.transform = Transform::translate(0.0, label_h + 4.0);
        let lane = self.add(root, lane);
        let slot_w = 6.0;
        let mut slot = Self::shape(
            NodeKind::Rectangle,
            "slot",
            w * 0.5 - slot_w * 0.5,
            0.0,
            slot_w,
            height,
            t.track,
        );
        slot.corner_radii = CornerRadii::uniform(slot_w * 0.5);
        self.add(lane, slot);
        let mut fill = Self::shape(
            NodeKind::Rectangle,
            "fill",
            w * 0.5 - slot_w * 0.5,
            0.0,
            slot_w,
            height,
            t.accent,
        );
        fill.corner_radii = CornerRadii::uniform(slot_w * 0.5);
        let fill = self.add(lane, fill);
        let cap_h = 18.0;
        let mut cap = Self::shape(NodeKind::Rectangle, "cap", 0.0, 0.0, w, cap_h, t.body);
        cap.corner_radii = CornerRadii::uniform(4.0);
        cap.stroke = Stroke {
            paints: vec![Paint::solid(t.pointer)],
            weight: 1.0,
            ..Stroke::default()
        };
        let cap = self.add(lane, cap);
        let mut value = self.text("value", "", t.value_size, t.text, w, TextAlign::Center);
        value.transform = Transform::translate(0.0, label_h + 4.0 + height + 4.0);
        let value = self.add(root, value);

        let mut c = Control::new(ControlKind::Fader {
            orientation: Orientation::Vertical,
        })
        .slot(SlotRole::Label, label.0)
        .slot(SlotRole::Fill, fill.0)
        .slot(SlotRole::Thumb, cap.0)
        .slot(SlotRole::Value, value.0);
        c.binding = binding;
        self.register(root, c);
        root
    }

    /// A button (toggle / momentary / trigger) with an LED and a label.
    pub fn button(
        &mut self,
        parent: NodeId,
        name: &str,
        size: ControlSize,
        mode: ButtonMode,
        binding: Option<Binding>,
    ) -> NodeId {
        self.switch_like(parent, name, size, ControlKind::Button { mode }, binding)
    }

    /// A stage footswitch tile (tap / hold).
    pub fn footswitch(&mut self, parent: NodeId, name: &str, binding: Option<Binding>) -> NodeId {
        self.switch_like(
            parent,
            name,
            ControlSize::Stage,
            ControlKind::footswitch(),
            binding,
        )
    }

    fn switch_like(
        &mut self,
        parent: NodeId,
        name: &str,
        size: ControlSize,
        kind: ControlKind,
        binding: Option<Binding>,
    ) -> NodeId {
        let t = self.theme.clone();
        let s = size.px().max(t.min_touch);
        let mut root = Self::frame(name, s, s);
        root.fills = vec![Paint::solid(t.body)];
        root.corner_radii = CornerRadii::uniform(t.radius);
        root.stroke = Stroke {
            paints: vec![Paint::solid(t.track)],
            weight: 1.0,
            ..Stroke::default()
        };
        let root = self.add(parent, root);
        let led_d = (s * 0.12).clamp(6.0, 14.0);
        let led = self.add(
            root,
            Self::shape(
                NodeKind::Ellipse,
                "led",
                s * 0.5 - led_d * 0.5,
                s * 0.18,
                led_d,
                led_d,
                t.led,
            ),
        );
        let mut label = self.text(
            "label",
            "",
            t.label_size.max(s * 0.12).round(),
            t.text,
            s,
            TextAlign::Center,
        );
        label.transform = Transform::translate(0.0, s * 0.55);
        let label = self.add(root, label);
        let mut c = Control::new(kind)
            .slot(SlotRole::Led, led.0)
            .slot(SlotRole::Label, label.0);
        c.binding = binding;
        self.register(root, c);
        root
    }

    /// An inline text field for a text param (a name): a well with the
    /// text, a "Name…" placeholder, a caret and a selection highlight.
    pub fn text_field(
        &mut self,
        parent: NodeId,
        name: &str,
        width: f64,
        binding: Option<Binding>,
    ) -> NodeId {
        let t = self.theme.clone();
        let h = t.min_touch.clamp(22.0, 28.0);
        let mut root = Self::frame(name, width, h);
        root.fills = vec![Paint::solid(t.track)];
        root.corner_radii = CornerRadii::uniform(4.0);
        root.stroke = Stroke {
            paints: vec![Paint::solid(t.body)],
            weight: 1.0,
            ..Stroke::default()
        };
        let root = self.add(parent, root);
        let size = t.label_size.max(11.0);
        let y = ((h - size * 1.35) / 2.0).round();
        let sel = self.add(
            root,
            Self::shape(
                NodeKind::Rectangle,
                "selection",
                6.0,
                y,
                1.0,
                size * 1.35,
                Color {
                    a: 0.35,
                    ..t.accent
                },
            ),
        );
        let mut hint = self.text(
            "placeholder",
            "Name…",
            size,
            Color { a: 0.45, ..t.text },
            width - 12.0,
            TextAlign::Left,
        );
        hint.transform = Transform::translate(6.0, y);
        let hint = self.add(root, hint);
        let mut value = self.text("value", "", size, t.text, width - 12.0, TextAlign::Left);
        value.transform = Transform::translate(6.0, y);
        let value = self.add(root, value);
        let caret = self.add(
            root,
            Self::shape(
                NodeKind::Rectangle,
                "caret",
                6.0,
                y,
                1.5,
                size * 1.35,
                t.accent,
            ),
        );
        let mut c = Control::new(ControlKind::TextField {
            click_to_edit: true,
            max_len: 64,
        })
        .slot(SlotRole::Value, value.0)
        .slot(SlotRole::Placeholder, hint.0)
        .slot(SlotRole::TextSelection, sel.0)
        .slot(SlotRole::Caret, caret.0);
        c.binding = binding;
        self.register(root, c);
        root
    }

    /// A vertical level meter with peak-hold; bind to a level stream.
    pub fn meter(
        &mut self,
        parent: NodeId,
        name: &str,
        height: f64,
        binding: Option<Binding>,
    ) -> NodeId {
        let t = self.theme.clone();
        let w = 10.0;
        let mut root = Self::frame(name, w, height);
        root.fills = vec![Paint::solid(t.track)];
        root.corner_radii = CornerRadii::uniform(2.0);
        root.clips_content = true;
        let root = self.add(parent, root);
        let fill = self.add(
            root,
            Self::shape(NodeKind::Rectangle, "fill", 0.0, 0.0, w, height, t.meter),
        );
        let peak = self.add(
            root,
            Self::shape(NodeKind::Rectangle, "peak", 0.0, 0.0, w, 2.0, t.meter_hot),
        );
        let mut c = Control::new(ControlKind::Meter {
            orientation: Orientation::Vertical,
        })
        .slot(SlotRole::Fill, fill.0)
        .slot(SlotRole::PeakHold, peak.0);
        c.binding = binding;
        self.register(root, c);
        root
    }

    /// A text readout of a parameter or stream.
    pub fn readout(
        &mut self,
        parent: NodeId,
        name: &str,
        width: f64,
        binding: Option<Binding>,
    ) -> NodeId {
        let t = self.theme.clone();
        let label_h = (t.label_size * 1.4).ceil();
        let value_h = (t.value_size * 1.6).ceil();
        let root = self.add(parent, Self::frame(name, width, label_h + 2.0 + value_h));
        let label = self.add(
            root,
            self.text(
                "label",
                "",
                t.label_size,
                t.text_dim,
                width,
                TextAlign::Left,
            ),
        );
        let mut value = self.text(
            "value",
            "",
            t.value_size * 1.15,
            t.text,
            width,
            TextAlign::Left,
        );
        value.transform = Transform::translate(0.0, label_h + 2.0);
        let value = self.add(root, value);
        let mut c = Control::new(ControlKind::Readout)
            .slot(SlotRole::Label, label.0)
            .slot(SlotRole::Value, value.0);
        c.binding = binding;
        self.register(root, c);
        root
    }

    /// A static text label (not a control).
    pub fn caption(&mut self, parent: NodeId, text: &str, size: f64, width: f64) -> NodeId {
        let color = self.theme.text;
        self.add(
            parent,
            self.text("caption", text, size, color, width, TextAlign::Left),
        )
    }
}

/// Ring-sector outline; angles in radians, 0 = up, clockwise.
#[must_use]
pub fn sector_path(cx: f64, cy: f64, r_in: f64, r_out: f64, a0: f64, a1: f64) -> String {
    let pt = |r: f64, a: f64| (cx + r * a.sin(), cy - r * a.cos());
    let large = i32::from(a1 - a0 > std::f64::consts::PI);
    let (x0, y0) = pt(r_out, a0);
    let (x1, y1) = pt(r_out, a1);
    let (x2, y2) = pt(r_in, a1);
    let (x3, y3) = pt(r_in, a0);
    format!(
        "M {x0:.3} {y0:.3} A {r_out:.3} {r_out:.3} 0 {large} 1 {x1:.3} {y1:.3} L {x2:.3} {y2:.3} A {r_in:.3} {r_in:.3} 0 {large} 0 {x3:.3} {y3:.3} Z"
    )
}
