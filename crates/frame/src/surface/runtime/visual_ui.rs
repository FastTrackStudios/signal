//! Ops for the interaction slots: selection rings, hover states, on/off
//! pairs, colour tints, a text field's caret and selection, a canvas's
//! marquee, a menu's highlight.

use crate::document::proto::{Color, Node, Op, PaintKind, Prop, Size, Transform};
use crate::surface::proto::SlotRole;

use crate::surface::runtime::runtime::text::EditView;

/// The interaction part of what a control shows.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Interaction {
    pub selected: bool,
    pub hovered: bool,
    /// The colour its `Tint` slots take.
    pub tint: Option<Color>,
    /// A text field being edited.
    pub edit: Option<EditView>,
    /// Its text is empty (a text param with no text): `Value` hides,
    /// `Placeholder` shows.
    pub empty: bool,
    /// A canvas's marquee, in the marquee node's parent's space.
    pub marquee: Option<(f64, f64, f64, f64)>,
    /// A menu's active row (`y`, `height`) in the highlight's parent space.
    pub highlight: Option<(f64, f64)>,
}

fn set(id: crate::document::proto::NodeId, prop: Prop) -> Op {
    Op::Set { id, prop }
}

/// Ops for `node` playing interaction slot `role` (`on`: the control reads on).
pub(crate) fn slot_ops(ops: &mut Vec<Op>, node: &Node, role: SlotRole, on: bool, s: &Interaction) {
    let show = |ops: &mut Vec<Op>, visible: bool| {
        if !visible {
            ops.push(set(node.id, Prop::Visible(false)));
        }
    };
    match role {
        SlotRole::Selected => show(ops, s.selected),
        SlotRole::Hovered => show(ops, s.hovered),
        SlotRole::On => show(ops, on),
        SlotRole::Off => show(ops, !on),
        SlotRole::Placeholder => show(
            ops,
            s.empty && s.edit.as_ref().is_none_or(|e| e.text.is_empty()),
        ),
        SlotRole::Tint => {
            if let Some(c) = s.tint {
                tint_ops(ops, node, c);
            }
        }
        SlotRole::Caret => match &s.edit {
            Some(e) => {
                let t = Transform {
                    tx: node.transform.tx + e.caret_x,
                    ..node.transform
                };
                ops.push(set(node.id, Prop::Transform(t)));
            }
            None => show(ops, false),
        },
        SlotRole::TextSelection => match s.edit.as_ref().and_then(|e| e.selection) {
            Some((x0, x1)) => {
                ops.push(set(
                    node.id,
                    Prop::Transform(Transform {
                        tx: node.transform.tx + x0,
                        ..node.transform
                    }),
                ));
                ops.push(set(
                    node.id,
                    Prop::Size(Size {
                        width: (x1 - x0).max(1.0),
                        height: node.size.height,
                    }),
                ));
            }
            None => show(ops, false),
        },
        SlotRole::Marquee => match s.marquee {
            Some((x, y, w, h)) if w > 5.0 || h > 5.0 => {
                ops.push(set(node.id, Prop::Transform(Transform::translate(x, y))));
                ops.push(set(
                    node.id,
                    Prop::Size(Size {
                        width: w,
                        height: h,
                    }),
                ));
                ops.push(set(node.id, Prop::Visible(true)));
            }
            _ => show(ops, false),
        },
        SlotRole::Highlight => match s.highlight {
            Some((y, h)) => {
                ops.push(set(
                    node.id,
                    Prop::Transform(Transform {
                        ty: y,
                        ..node.transform
                    }),
                ));
                ops.push(set(
                    node.id,
                    Prop::Size(Size {
                        width: node.size.width,
                        height: h,
                    }),
                ));
                ops.push(set(node.id, Prop::Visible(true)));
            }
            None => show(ops, false),
        },
        _ => {}
    }
}

/// Recolour a node's solid fills, solid stroke and light to `c`, keeping
/// each paint's alpha.
fn tint_ops(ops: &mut Vec<Op>, node: &Node, c: Color) {
    let recolor = |paints: &[crate::document::proto::Paint]| {
        paints
            .iter()
            .map(|p| match &p.kind {
                PaintKind::Solid { color } => crate::document::proto::Paint {
                    kind: PaintKind::Solid {
                        color: Color { a: color.a, ..c },
                    },
                    ..p.clone()
                },
                _ => p.clone(),
            })
            .collect::<Vec<_>>()
    };
    if node
        .fills
        .iter()
        .any(|p| matches!(p.kind, PaintKind::Solid { .. }))
    {
        ops.push(set(node.id, Prop::Fills(recolor(&node.fills))));
    }
    if node
        .stroke
        .paints
        .iter()
        .any(|p| matches!(p.kind, PaintKind::Solid { .. }))
    {
        ops.push(set(
            node.id,
            Prop::Stroke(crate::document::proto::Stroke {
                paints: recolor(&node.stroke.paints),
                ..node.stroke.clone()
            }),
        ));
    }
    if let Some(e) = &node.emission {
        ops.push(set(
            node.id,
            Prop::Emission(Some(crate::document::proto::Emission {
                color: Color { a: 1.0, ..c },
                ..e.clone()
            })),
        ));
    }
}

/// `#rrggbb` stops, evenly spaced, sampled at `t` (0..1).
#[must_use]
pub fn ramp(stops: &[String], t: f64) -> Option<Color> {
    let parse = |s: &str| -> Option<Color> {
        let h = s.trim_start_matches('#');
        let p = |i: usize| {
            u8::from_str_radix(h.get(i..i + 2)?, 16)
                .ok()
                .map(|v| f64::from(v) / 255.0)
        };
        Some(Color {
            r: p(0)?,
            g: p(2)?,
            b: p(4)?,
            a: 1.0,
        })
    };
    let colors: Vec<Color> = stops.iter().filter_map(|s| parse(s)).collect();
    match colors.len() {
        0 => None,
        1 => Some(colors[0]),
        n => {
            let x = t.clamp(0.0, 1.0) * (n - 1) as f64;
            let i = (x.floor() as usize).min(n - 2);
            let f = x - i as f64;
            let (a, b) = (colors[i], colors[i + 1]);
            Some(Color {
                r: a.r + (b.r - a.r) * f,
                g: a.g + (b.g - a.g) * f,
                b: a.b + (b.b - a.b) * f,
                a: 1.0,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ramps_interpolate() {
        let stops = vec!["#000000".to_string(), "#ffffff".to_string()];
        let mid = ramp(&stops, 0.5).unwrap();
        assert!((mid.r - 0.5).abs() < 1e-9);
        assert_eq!(ramp(&stops, 2.0).unwrap().g, 1.0);
        assert!(ramp(&[], 0.5).is_none());
    }
}
