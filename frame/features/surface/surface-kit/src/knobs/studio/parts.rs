//! The scaffolding the photo-matched knobs share: which scale a placed knob
//! prints and how far it turns, the square frame round the shaft sized to
//! hold that scale, the printed marks and numerals, the legend, and the
//! control wired to the turning body.
//!
//! ```text
//! <root control=Knob{sweep}>        side × (side + legend)  (tight: touch²)
//!   knob   (square, centred on the shaft)
//!     scale marks, numerals          printed on the panel         (fixed)
//!     … the style's tiers, body (← Needle), light …
//!   legend ← Label
//! ```

use document_proto::{Node, NodeId, NodeKind, TextAlign, TextCase};
use surface_proto::{ControlKind, SlotRole};
use surface_widgets::Builder;

use crate::controls::parts::text_w;
use crate::draw;
use crate::registry::Place;

/// How a printed scale marks each position.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Mark {
    /// A round dot (console pots, the class-A module).
    Dot,
    /// Numerals alone (the passive EQ's panel).
    Bare,
}

/// A printed scale: its labels (first = start of travel) and the travel.
pub(super) struct Scale {
    pub labels: Vec<String>,
    pub sweep: f64,
}

impl Scale {
    /// What `p` asks for: `options` make the knob stepped (printed, turning
    /// ≤ 60° a detent); an authored `scale` or `sweep` wins; else `default`
    /// over `travel`.
    pub fn of(p: &Place, default: &[&str], travel: f64) -> Self {
        let stepped = p.options.as_ref().filter(|o| !o.is_empty());
        let sweep = p.sweep.unwrap_or_else(|| stepped.map_or(travel, |o| crate::registry::detent_sweep(o.len())));
        let labels = p.scale.clone().or_else(|| stepped.cloned()).unwrap_or_else(|| default.iter().map(ToString::to_string).collect());
        Self { labels, sweep }
    }

    fn at(&self, i: usize) -> f64 {
        let n = self.labels.len();
        let t = if n < 2 { 0.5 } else { i as f64 / (n - 1) as f64 };
        (t - 0.5) * self.sweep
    }

    /// `(half, below)`: the half-side a square round the shaft needs to
    /// hold the numerals set clear of radius `clear`, and how far below the
    /// shaft the lowest one reaches.
    pub fn reach(&self, clear: f64, size: f64) -> (f64, f64) {
        let ext: Vec<(f64, f64, f64, f64)> =
            self.labels.iter().enumerate().filter(|(_, s)| !s.is_empty()).map(|(i, s)| draw::radial_extent(s, clear, self.at(i), size)).collect();
        let half = ext.iter().map(|e| e.0.abs().max(e.1.abs()).max(e.2.abs()).max(e.3.abs())).fold(0.0, f64::max);
        let below = ext.iter().map(|e| e.3).fold(0.0, f64::max);
        (half, below)
    }

    /// Print the scale round the shaft at `c` in `knob`: a mark at radius
    /// `r` for each position, the numerals clear of `clear`. Nothing for an
    /// empty scale.
    #[allow(clippy::too_many_arguments)]
    pub fn print(&self, b: &mut Builder<'_>, knob: NodeId, side: f64, c: f64, mark: (Mark, f64, f64), clear: f64, size: f64, ink: &str) {
        let (kind, r, m) = mark;
        let mut d = String::new();
        for i in 0..self.labels.len() {
            let a = self.at(i);
            match kind {
                Mark::Dot => {
                    let (x, y) = draw::polar(c, c, r, a);
                    d.push_str(&dot(x, y, m / 2.0));
                }
                Mark::Bare => {}
            }
        }
        if !d.is_empty() {
            b.add(knob, draw::path("scale marks", 0.0, 0.0, side, side, d, draw::solid(ink)));
        }
        for (i, s) in self.labels.iter().enumerate() {
            if s.is_empty() {
                continue;
            }
            let mut t = draw::radial_label("numeral", s, (c, c), clear, self.at(i), size, ink);
            if let NodeKind::Text(td) = &mut t.kind {
                td.font_weight = Some(650.0);
            }
            b.add(knob, t);
        }
    }
}

/// A dot of radius `r` at (`x`, `y`) as path data.
pub(super) fn dot(x: f64, y: f64, r: f64) -> String {
    format!("M {:.3} {y:.3} A {r:.3} {r:.3} 0 1 0 {:.3} {y:.3} A {r:.3} {r:.3} 0 1 0 {:.3} {y:.3} Z ", x - r, x + r, x - r)
}

/// A caption centred on (`cx`, `cy`): a legend under a knob.
pub(super) fn caption(name: &str, s: &str, cx: f64, cy: f64, size: f64, ink: &str, case: TextCase) -> Node {
    let w = text_w(s, size) + size * 3.0;
    let mut n = draw::text(name, s, cx - w / 2.0, cy - size * 0.68, w, size, ink, TextAlign::Center);
    if let NodeKind::Text(t) = &mut n.kind {
        t.font_weight = Some(700.0);
        t.text_case = case;
        t.letter_spacing = size * 0.06;
    }
    n
}

/// Ridges round the band `r0..r1` of a disc centred at `c`: a knurl.
pub(super) fn knurl(c: f64, r0: f64, r1: f64, n: u32, w: f64) -> String {
    let mut d = String::new();
    for i in 0..n {
        let a = 360.0 * f64::from(i) / f64::from(n);
        let (x0, y0) = draw::polar(c, c, r0, a);
        let (x1, y1) = draw::polar(c, c, r1, a);
        let (nx, ny) = (a.to_radians().cos() * w / 2.0, a.to_radians().sin() * w / 2.0);
        d.push_str(&draw::polygon(&[(x0 - nx, y0 - ny), (x0 + nx, y0 + ny), (x1 + nx, y1 + ny), (x1 - nx, y1 - ny)]));
        d.push(' ');
    }
    d
}

/// A ring between radii `r0` and `r1` round `c`: the inner circle runs the
/// other way, so the hole shows under either fill rule.
pub(super) fn ring(c: f64, r0: f64, r1: f64) -> String {
    let (x0, x1) = (c - r0, c + r0);
    format!("{}M {x0:.3} {c:.3} A {r0:.3} {r0:.3} 0 1 1 {x1:.3} {c:.3} A {r0:.3} {r0:.3} 0 1 1 {x0:.3} {c:.3} Z", dot(c, c, r1))
}

/// A bar `w` wide from radius `r0` to `r1`, pointing up from `c`: an index.
pub(super) fn bar(c: f64, r0: f64, r1: f64, w: f64) -> Node {
    draw::rounded(draw::rect("index", c - w / 2.0, c - r1, w, r1 - r0, draw::solid("#ffffff")), w / 2.0)
}

/// The frame of a single knob of body diameter `d`, with its scale printed.
pub(super) struct Frame {
    pub root: NodeId,
    pub knob: NodeId,
    pub side: f64,
    pub c: f64,
    /// Where the legend's centre goes, in the root's space.
    pub legend_y: f64,
}

/// Open a knob's root and square `knob` frame, sized for `scale` printed
/// with `mark` and numerals clear of `clear` at `size`, `extent` the
/// farthest the knob itself reaches from the shaft.
#[allow(clippy::too_many_arguments)]
pub(super) fn open(b: &mut Builder<'_>, parent: NodeId, p: &Place, scale: &Scale, mark: (Mark, f64, f64), clear: f64, size: f64, extent: f64, ink: &str) -> Frame {
    let k = extent / 32.0;
    let (reach, below) = scale.reach(clear, size);
    let half = (extent + 4.0 * k).max(reach + 3.0 * k);
    let side = 2.0 * half;
    let c = half;
    let legend_size = (9.0 * k).max(8.0);
    let legend_y = c + below.max(extent) + 4.0 * k + legend_size * 0.6;
    let root = b.add(parent, draw::frame(&p.name, 0.0, 0.0, side, side.max(legend_y + legend_size)));
    let knob = b.add(root, draw::frame("knob", 0.0, 0.0, side, side));
    scale.print(b, knob, side, c, mark, clear, size, ink);
    Frame { root, knob, side, c, legend_y }
}

/// Finish a single knob: its legend (`Label`), the control turning `body`,
/// and the tight box when `p.touch` asks for one (`d` = the body's edge).
pub(super) fn close(b: &mut Builder<'_>, p: &Place, f: &Frame, body: NodeId, sweep: f64, d: f64, ink: &str) -> NodeId {
    let k = d / 64.0;
    let size = (9.0 * k).max(8.0);
    let legend = b.add(f.root, caption("legend", p.legend.as_deref().unwrap_or(""), f.c, f.legend_y, size, ink, p.case()));
    let ctl = p.control(ControlKind::Knob { sweep_degrees: sweep }).slot(SlotRole::Needle, body.0).slot(SlotRole::Label, legend.0);
    b.controls.insert(f.root, ctl);
    if let Some(t) = p.touch {
        crate::registry::tighten(b, f.root, (f.c, f.c), d, t);
    }
    f.root
}
