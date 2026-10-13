//! Display lettering, set as real text from the font book.
//!
//! Logos and model names are most of what makes a unit read as *that*
//! maker's: a wide engraved sans, geometric tech caps, a blackletter. Each
//! is a text node in a bundled family (`draw::family::*`) at its real
//! weight, italic and width; the book measures it so it can be centred on
//! its caps, fitted to a width, stretched or turned.

use document_proto::{NodeId, TextAlign, Transform};
use render_fonts::{Chain, FontBook, FontQuery};
use surface_kit::draw::{self, Lettering, family};
use surface_widgets::Builder;

/// How a line of display lettering is set.
#[derive(Clone, Copy)]
pub struct Letter<'a> {
    pub size: f64,
    pub weight: f64,
    pub italic: bool,
    /// Letter spacing, em.
    pub track: f64,
    pub color: &'a str,
    /// Horizontal scale: < 1 condenses, > 1 extends (a family with a
    /// width axis sets it as its width; any other is scaled).
    pub stretch: f64,
    /// A bundled family (`draw::family::*`).
    pub family: &'a str,
}

impl<'a> Letter<'a> {
    #[must_use]
    pub const fn new(size: f64, weight: f64, color: &'a str) -> Self {
        Self { size, weight, italic: false, track: 0.0, color, stretch: 1.0, family: family::INTER }
    }
    #[must_use]
    pub const fn italic(mut self) -> Self {
        self.italic = true;
        self
    }
    #[must_use]
    pub const fn track(mut self, t: f64) -> Self {
        self.track = t;
        self
    }
    #[must_use]
    pub const fn stretch(mut self, s: f64) -> Self {
        self.stretch = s;
        self
    }
    #[must_use]
    pub const fn color(mut self, c: &'a str) -> Self {
        self.color = c;
        self
    }
    #[must_use]
    pub const fn family(mut self, f: &'a str) -> Self {
        self.family = f;
        self
    }
    #[must_use]
    pub const fn size(mut self, s: f64) -> Self {
        self.size = s;
        self
    }
    #[must_use]
    pub const fn weight(mut self, w: f64) -> Self {
        self.weight = w;
        self
    }

    /// Whether the family draws width on an axis (Saira): the stretch is
    /// then its width, not a scale.
    fn axis_width(&self) -> bool {
        self.family == family::SAIRA
    }

    fn width_pct(&self) -> f64 {
        if self.axis_width() { (self.stretch * 100.0).clamp(50.0, 125.0) } else { 100.0 }
    }

    /// The horizontal scale left to the node's transform.
    fn scale(&self) -> f64 {
        if self.axis_width() { 1.0 } else { self.stretch }
    }

    fn chain(&self) -> Chain {
        FontBook::bundled().chain(&FontQuery::family(self.family).weight(self.weight as f32).italic(self.italic).width(self.width_pct() as f32))
    }

    fn lettering(&self) -> Lettering {
        let mut l = Lettering::family(self.family).weight(self.weight);
        if self.italic {
            l = l.italic();
        }
        if self.axis_width() {
            l = l.width(self.width_pct());
        }
        l
    }
}

/// What the book says of a line: its set width (no trailing space, before
/// any scale), the face's ascent and cap height at the size.
struct Measure {
    width: f64,
    ascent: f64,
    cap: f64,
}

fn measure(s: &str, l: &Letter<'_>) -> Measure {
    let chain = l.chain();
    let spacing = l.track * l.size;
    let n = s.chars().filter(|c| *c != '\n').count();
    let width = chain.advance(s, l.size, spacing) - if n > 0 { spacing } else { 0.0 };
    let (ascent, cap) = chain.primary().map_or((l.size * 0.8, l.size * 0.7), |f| {
        let cap = f.font().and_then(|font| {
            use skrifa::MetadataProvider;
            font.metrics(skrifa::instance::Size::new(l.size as f32), f.location()).cap_height
        });
        (f.ascent(l.size), cap.map_or(l.size * 0.7, f64::from))
    });
    Measure { width, ascent, cap }
}

/// The renderer's text anchor: glyphs start 1 px in and 1 px down.
const ANCHOR: f64 = 1.0;

/// The text node for `s` (its box at the local origin), and the local
/// point its caps' middle-left sits at.
fn node(s: &str, l: &Letter<'_>, m: &Measure) -> (document_proto::Node, (f64, f64)) {
    let spacing = l.track * l.size;
    let w = m.width + spacing + 2.0 * ANCHOR + 1.0;
    let mut n = draw::styled_text("lettering", s, 0.0, 0.0, w, l.size, l.color, TextAlign::Left, &l.lettering());
    if let document_proto::NodeKind::Text(t) = &mut n.kind {
        t.letter_spacing = spacing;
    }
    n.name = format!("lettering {s}");
    n.size.height = (m.ascent + l.size * 0.3 + 2.0 * ANCHOR).ceil();
    (n, (ANCHOR, ANCHOR + m.ascent - m.cap / 2.0))
}

/// How far under a text node's top the middle of its caps falls, set in `l`.
#[must_use]
pub fn cap_middle(l: &Letter<'_>) -> f64 {
    let m = measure("0", l);
    ANCHOR + m.ascent - m.cap / 2.0
}

/// A text node in `l`'s family, weight and spacing (no transform): what a
/// readout's `Value` slot or a static line of lettering is made of.
#[must_use]
pub fn text_in(s: &str, (x, y, w): (f64, f64, f64), l: &Letter<'_>, align: TextAlign) -> document_proto::Node {
    let m = measure("0", l);
    let mut n = draw::styled_text("text", s, x, y, w, l.size, l.color, align, &l.lettering());
    if let document_proto::NodeKind::Text(t) = &mut n.kind {
        t.letter_spacing = l.track * l.size;
    }
    n.size.height = (m.ascent + l.size * 0.3 + 2.0 * ANCHOR).ceil();
    n
}

/// The set width of `s` in `l` (scaled by its stretch).
#[must_use]
pub fn set_width(s: &str, l: &Letter<'_>) -> f64 {
    measure(s, l).width * l.scale()
}

/// Display lettering centred on (`cx`, `cy`) — `cy` the middle of the caps.
pub fn letter(b: &mut Builder<'_>, parent: NodeId, s: &str, (cx, cy): (f64, f64), l: Letter<'_>) -> NodeId {
    letter_aligned(b, parent, s, (cx, cy), l, TextAlign::Center)
}

/// Display lettering turned by `deg` (clockwise) about its own centre,
/// which lands on (`cx`, `cy`) — a legend printed along a knob's slot,
/// a vertical label.
pub fn letter_turned(b: &mut Builder<'_>, parent: NodeId, s: &str, (cx, cy): (f64, f64), deg: f64, l: Letter<'_>) -> NodeId {
    // Renderers set a text run level, so a turned line is drawn as the
    // family's own glyph outlines, turned (the node keeps its characters).
    let m = measure(s, &l);
    let (d, w) = outline(s, &l);
    let r = w.max(l.size) / 2.0 + 2.0;
    let (sn, cs) = deg.to_radians().sin_cos();
    // The caps' middle in the outline's space: baseline 0.78·size, cap up.
    let (x0, y0) = (w / 2.0, l.size * 0.78 - m.cap / 2.0);
    let turn = |x: f64, y: f64| {
        let (dx, dy) = (x - x0, y - y0);
        (r + cs * dx - sn * dy, r + sn * dx + cs * dy)
    };
    let d = map_path(&d, turn);
    let mut n = draw::text("lettering", s, cx - r, cy - r, 2.0 * r, l.size, l.color, TextAlign::Center);
    l.lettering().apply(&mut n);
    n.name = format!("lettering {s}");
    n.size.height = 2.0 * r;
    n.fill_geometry = vec![d];
    b.add(parent, n)
}

/// Every coordinate pair of an absolute path mapped through `f`.
fn map_path(d: &str, f: impl Fn(f64, f64) -> (f64, f64)) -> String {
    let mut out = String::new();
    let mut nums: Vec<f64> = Vec::new();
    let flush = |nums: &mut Vec<f64>, out: &mut String| {
        for pair in nums.chunks(2) {
            if let [x, y] = pair {
                let (u, v) = f(*x, *y);
                out.push_str(&format!("{u:.2} {v:.2} "));
            }
        }
        nums.clear();
    };
    for t in d.split_whitespace() {
        if let Ok(v) = t.parse::<f64>() {
            nums.push(v);
        } else {
            flush(&mut nums, &mut out);
            out.push_str(t);
            out.push(' ');
        }
    }
    flush(&mut nums, &mut out);
    out
}

/// Display lettering sized so it sets `width` wide, centred on (`cx`, `cy`);
/// returns the node and the size it was set at.
pub fn letter_fit(b: &mut Builder<'_>, parent: NodeId, s: &str, (cx, cy): (f64, f64), width: f64, l: Letter<'_>) -> (NodeId, f64) {
    let probe = Letter { size: 100.0, ..l };
    let w = set_width(s, &probe);
    let size = if w > 0.0 { 100.0 * width / w } else { l.size };
    (letter(b, parent, s, (cx, cy), Letter { size, ..l }), size)
}

/// Display lettering anchored at `x` by `align`, the caps centred on `cy`.
pub fn letter_aligned(b: &mut Builder<'_>, parent: NodeId, s: &str, (x, cy): (f64, f64), l: Letter<'_>, align: TextAlign) -> NodeId {
    let m = measure(s, &l);
    let k = l.scale();
    let w = m.width * k;
    let x0 = match align {
        TextAlign::Center => x - w / 2.0,
        TextAlign::Right => x - w,
        _ => x,
    };
    if (k - 1.0).abs() > 1e-6 {
        // Renderers don't scale a text run: a stretched line is drawn as
        // the family's own glyph outlines, stretched (characters kept).
        let (d, ow) = outline(s, &l);
        let top = cy - (l.size * 0.78 - m.cap / 2.0);
        let mut n = draw::text("lettering", s, x0, top, ow.max(1.0), l.size, l.color, TextAlign::Left);
        l.lettering().apply(&mut n);
        n.name = format!("lettering {s}");
        n.size.height = l.size;
        n.fill_geometry = vec![d];
        return b.add(parent, n);
    }
    let (mut n, (lx, ly)) = node(s, &l, &m);
    n.transform = Transform::translate(x0 - lx, cy - ly);
    b.add(parent, n)
}

/// `s` as outlines (for lettering drawn as a shape — cut hollow, stroked):
/// the path in a box whose top-left is the origin, the baseline at
/// `0.78 × size`, stretched by the letter's scale; and the set width.
#[must_use]
pub fn outline(s: &str, l: &Letter<'_>) -> (String, f64) {
    let spacing = l.track * l.size;
    let (path, adv) = l.chain().line(s, l.size, spacing);
    let (k, base) = (l.scale(), l.size * 0.78);
    // Tokenise kurbo's SVG (command letters may touch their numbers).
    let svg = path.to_svg();
    let mut toks: Vec<String> = Vec::new();
    let mut tok = String::new();
    for ch in svg.chars() {
        if ch.is_ascii_alphabetic() && ch != 'e' {
            if !tok.is_empty() {
                toks.push(std::mem::take(&mut tok));
            }
            toks.push(ch.to_string());
        } else if ch == ' ' || ch == ',' {
            if !tok.is_empty() {
                toks.push(std::mem::take(&mut tok));
            }
        } else {
            tok.push(ch);
        }
    }
    if !tok.is_empty() {
        toks.push(tok);
    }
    let mut out = String::new();
    let mut nums: Vec<f64> = Vec::new();
    let flush = |nums: &mut Vec<f64>, out: &mut String| {
        for pair in nums.chunks(2) {
            if let [x, y] = pair {
                out.push_str(&format!("{:.2} {:.2} ", x * k, y + base));
            }
        }
        nums.clear();
    };
    for t in toks {
        if let Ok(v) = t.parse::<f64>() {
            nums.push(v);
        } else {
            flush(&mut nums, &mut out);
            out.push_str(&t);
            out.push(' ');
        }
    }
    flush(&mut nums, &mut out);
    let n = s.chars().count();
    (out, (adv - if n > 0 { spacing } else { 0.0 }) * k)
}
