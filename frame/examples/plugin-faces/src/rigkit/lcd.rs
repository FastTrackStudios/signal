//! Real displays: segment and dot-matrix readouts bound to params.
//!
//! A readout is a `Readout` control whose `Value` is set in a display
//! family — `DSEG14 Classic` (14-segment alphanumerics), `DSEG7 Classic`
//! (7-segment digits), `Doto` (dot matrix) — over the unlit segments'
//! ghosts, so it reads as a lit display at any value.

use document_proto::{NodeId, TextAlign};
use surface_kit::draw::{self, family};
use surface_proto::{Binding, Control, ControlKind, SlotRole, ValueFormat};
use surface_widgets::Builder;

use super::lettering::{Letter, cap_middle, text_in};
use crate::delay::parts;

/// A 14-segment display face.
#[must_use]
pub const fn seg14(size: f64, color: &str) -> Letter<'_> {
    Letter::new(size, 400.0, color).family(family::DSEG14).track(0.08)
}

/// A 7-segment display face.
#[must_use]
pub const fn seg7(size: f64, color: &str) -> Letter<'_> {
    Letter::new(size, 400.0, color).family(family::DSEG7).track(0.08)
}

/// A dot-matrix display face.
#[must_use]
pub const fn dots(size: f64, color: &str) -> Letter<'_> {
    Letter::new(size, 600.0, color).family(family::DOTO).track(0.04)
}

/// Every segment of a DSEG family, lit: the unlit ghost of one cell.
#[must_use]
pub fn all_segments(l: &Letter<'_>) -> char {
    if l.family == family::DSEG7 { '8' } else { '~' }
}

/// A static line of display lettering in a box `w` wide at `x`, its caps
/// centred on `cy`, aligned in the box (the ghosts and fixed legends of a
/// display), lit when `glow` > 0.
pub fn line(b: &mut Builder<'_>, parent: NodeId, s: &str, (x, cy, w): (f64, f64, f64), l: Letter<'_>, align: TextAlign, glow: f64) -> NodeId {
    let mut n = text_in(s, (x, cy - cap_middle(&l), w), &l, align);
    n.name = format!("display {s}");
    if glow > 0.0 {
        n = parts::emit(n, l.color, glow, l.size * 0.8);
    }
    b.add(parent, n)
}

/// A readout of `binding` in `l`, in a box `w` wide at `x` with its caps
/// centred on `cy`, aligned in the box; `ghost` (the unlit segments'
/// colour) prints `cells` all-segment cells under it, aligned the same.
#[allow(clippy::too_many_arguments)]
pub fn readout(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    binding: Binding,
    (x, cy, w): (f64, f64, f64),
    l: Letter<'_>,
    align: TextAlign,
    ghost: Option<(&str, usize)>,
    format: ValueFormat,
) -> NodeId {
    let h = (l.size * 1.6).ceil();
    let root = b.add(parent, draw::frame(name, x, cy - h / 2.0, w, h));
    let top = h / 2.0 - cap_middle(&l);
    if let Some((color, cells)) = ghost {
        let s: String = std::iter::repeat_n(all_segments(&l), cells).collect();
        let mut g = text_in(&s, (0.0, top, w), &l.color(color), align);
        g.name = "unlit segments".into();
        b.add(root, g);
    }
    let mut t = text_in("", (0.0, top, w), &l, align);
    t.name = "value".into();
    let value = b.add(root, parts::emit(t, l.color, 0.8, l.size * 0.7));
    let mut c = Control::new(ControlKind::Readout).slot(SlotRole::Value, value.0);
    c.binding = Some(binding);
    c.format = Some(format);
    b.controls.insert(root, c);
    root
}
