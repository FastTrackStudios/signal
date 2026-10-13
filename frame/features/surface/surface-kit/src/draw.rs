//! Drawing helpers shared by every kit family: paints (solid, angled linear,
//! radial with an offset hot-spot, conic), node constructors, and the circle
//! geometry hardware faces are made of (rings, arcs, lobed edges, polygons,
//! points on a radius).
//!
//! Angles follow the kit's convention everywhere: **degrees, 0 = straight
//! up, clockwise** — the way a knob's travel and a CSS `linear-gradient`
//! angle are both described.

use document_proto::{
    BlendMode, Color, CornerRadii, Effect, EffectKind, Emission, GradientStop, Node, NodeKind, Paint, PaintKind, Size, Stroke,
    StrokeAlign, TextAlign, TextData, Transform,
};

// ── colour ───────────────────────────────────────────────────────────────

/// `#rgb`, `#rrggbb` or `#rrggbbaa`. Panics on malformed input — kit colours
/// are literals.
#[must_use]
pub fn hex(s: &str) -> Color {
    let h = s.trim_start_matches('#');
    let p = |i: usize, n: usize| u8::from_str_radix(&h[i..i + n], 16).expect("hex colour");
    let (r, g, b, a) = match h.len() {
        3 => (p(0, 1) * 17, p(1, 1) * 17, p(2, 1) * 17, 255),
        6 => (p(0, 2), p(2, 2), p(4, 2), 255),
        8 => (p(0, 2), p(2, 2), p(4, 2), p(6, 2)),
        _ => panic!("bad hex colour {s}"),
    };
    Color { r: f64::from(r) / 255.0, g: f64::from(g) / 255.0, b: f64::from(b) / 255.0, a: f64::from(a) / 255.0 }
}

#[must_use]
pub fn with_alpha(c: Color, a: f64) -> Color {
    Color { a, ..c }
}

/// Mix `a` toward `b` by `t` (0..1).
#[must_use]
pub fn mix(a: Color, b: Color, t: f64) -> Color {
    Color { r: a.r + (b.r - a.r) * t, g: a.g + (b.g - a.g) * t, b: a.b + (b.b - a.b) * t, a: a.a + (b.a - a.a) * t }
}

// ── paints ───────────────────────────────────────────────────────────────

#[must_use]
pub fn solid(c: &str) -> Paint {
    Paint::solid(hex(c))
}

fn stops(s: &[(f64, &str)]) -> Vec<GradientStop> {
    s.iter().map(|(p, c)| GradientStop { position: *p, color: hex(c) }).collect()
}

fn paint(kind: PaintKind) -> Paint {
    Paint { kind, opacity: 1.0, blend: BlendMode::Normal, visible: true }
}

/// A linear gradient at `angle_deg` (CSS convention: 0 = to top, 90 = to
/// the right, 180 = to bottom), spanning the node's box corner to corner
/// along that direction.
#[must_use]
pub fn linear(angle_deg: f64, s: &[(f64, &str)]) -> Paint {
    let a = angle_deg.to_radians();
    let (dx, dy) = (a.sin(), -a.cos());
    let len = dx.abs() + dy.abs();
    let (sx, sy) = (0.5 - dx * len / 2.0, 0.5 - dy * len / 2.0);
    let (ex, ey) = (0.5 + dx * len / 2.0, 0.5 + dy * len / 2.0);
    // The renderer runs a linear gradient from T(0, .5) to T(1, .5) in the
    // node's unit square; build T so those land on start and end.
    let (ax, ay) = (ex - sx, ey - sy);
    let t = Transform { a: ax, b: -ay, tx: sx + 0.5 * ay, c: ay, d: ax, ty: sy - 0.5 * ax };
    paint(PaintKind::LinearGradient { stops: stops(s), transform: Some(t) })
}

/// A radial gradient centred at (`cx`, `cy`) in the node's unit square with
/// radius `r` (fraction of the box): the off-centre hot-spot of a lit dome.
#[must_use]
pub fn radial_at(cx: f64, cy: f64, r: f64, s: &[(f64, &str)]) -> Paint {
    // Unit-square radial: centre T(.5,.5), radius along T's columns.
    let t = Transform { a: 2.0 * r, b: 0.0, tx: cx - r, c: 0.0, d: 2.0 * r, ty: cy - r };
    paint(PaintKind::RadialGradient { stops: stops(s), transform: Some(t) })
}

/// A centred radial gradient filling the box.
#[must_use]
pub fn radial(s: &[(f64, &str)]) -> Paint {
    paint(PaintKind::RadialGradient { stops: stops(s), transform: None })
}

/// A conic (sweep) gradient around the box centre, starting at `start_deg`
/// (0 = up, clockwise) — brushed-metal caps and anisotropic sheen.
#[must_use]
pub fn conic(start_deg: f64, s: &[(f64, &str)]) -> Paint {
    // The renderer's sweep starts at +x; kit angles start at up.
    let a = (start_deg - 90.0).to_radians();
    let (c, sn) = (a.cos(), a.sin());
    let t = Transform { a: c, b: -sn, tx: 0.5 - 0.5 * c + 0.5 * sn, c: sn, d: c, ty: 0.5 - 0.5 * sn - 0.5 * c };
    paint(PaintKind::AngularGradient { stops: stops(s), transform: Some(t) })
}

// ── effects ──────────────────────────────────────────────────────────────

#[must_use]
pub fn drop_shadow(color: &str, dx: f64, dy: f64, radius: f64) -> Effect {
    Effect { kind: EffectKind::DropShadow, visible: true, color: hex(color), offset_x: dx, offset_y: dy, radius, spread: 0.0 }
}

#[must_use]
pub fn inner_shadow(color: &str, dx: f64, dy: f64, radius: f64) -> Effect {
    Effect { kind: EffectKind::InnerShadow, visible: true, color: hex(color), offset_x: dx, offset_y: dy, radius, spread: 0.0 }
}

// ── nodes ────────────────────────────────────────────────────────────────

fn placed(kind: NodeKind, name: &str, x: f64, y: f64, w: f64, h: f64) -> Node {
    let mut n = Node::new(kind, name);
    n.transform = Transform::translate(x, y);
    n.size = Size { width: w, height: h };
    n
}

#[must_use]
pub fn frame(name: &str, x: f64, y: f64, w: f64, h: f64) -> Node {
    placed(NodeKind::Frame, name, x, y, w, h)
}

#[must_use]
pub fn rect(name: &str, x: f64, y: f64, w: f64, h: f64, fill: Paint) -> Node {
    let mut n = placed(NodeKind::Rectangle, name, x, y, w, h);
    n.fills = vec![fill];
    n
}

/// A circle of radius `r` centred at (`cx`, `cy`).
#[must_use]
pub fn circle(name: &str, cx: f64, cy: f64, r: f64, fill: Paint) -> Node {
    let mut n = placed(NodeKind::Ellipse, name, cx - r, cy - r, 2.0 * r, 2.0 * r);
    n.fills = vec![fill];
    n
}

/// A filled path (SVG path data in the node's own coordinates, box `w × h`).
#[must_use]
pub fn path(name: &str, x: f64, y: f64, w: f64, h: f64, d: String, fill: Paint) -> Node {
    let mut n = placed(NodeKind::Vector, name, x, y, w, h);
    n.fill_geometry = vec![d];
    n.fills = vec![fill];
    n
}

#[must_use]
pub fn text(name: &str, s: &str, x: f64, y: f64, w: f64, size: f64, color: &str, align: TextAlign) -> Node {
    let mut n = placed(
        NodeKind::Text(TextData { characters: s.to_string(), font_size: size, align, ..TextData::default() }),
        name,
        x,
        y,
        w,
        (size * 1.35).ceil(),
    );
    n.fills = vec![solid(color)];
    n
}

// ── lettering ────────────────────────────────────────────────────────────

/// Families every frame renderer bundles (see `render_fonts::BUNDLED`).
/// Any other name falls back to Inter; a document can bring more as font
/// assets.
pub mod family {
    /// The UI sans (the default when no family is set).
    pub const INTER: &str = "Inter";
    /// Condensed display sans: model names, big legends, amp badges.
    pub const OSWALD: &str = "Oswald";
    /// Technical sans with a width axis (50–125): condensed to extended.
    pub const SAIRA: &str = "Saira";
    /// Wide engraved caps: brushed-metal panels, hi-fi faceplates.
    pub const MICHROMA: &str = "Michroma";
    /// Geometric tech caps (400–900): synth and modern-metal logos.
    pub const ORBITRON: &str = "Orbitron";
    /// Flowing script (400–700): signature logos.
    pub const DANCING_SCRIPT: &str = "Dancing Script";
    /// Fat retro brush script: surf, funk and 50s logos.
    pub const PACIFICO: &str = "Pacifico";
    /// Marker handwriting: scribbled labels, tape strips.
    pub const CAVEAT_BRUSH: &str = "Caveat Brush";
    /// Heavy brush marker: punk, funk, graffiti logos.
    pub const KNEWAVE: &str = "Knewave";
    /// High-contrast serif (400–900): boutique and valve-era plates.
    pub const PLAYFAIR_DISPLAY: &str = "Playfair Display";
    /// 7-segment digits (300/400/700, italics): tuners, BPM, time.
    pub const DSEG7: &str = "DSEG7 Classic";
    /// 14-segment alphanumerics (400/700, italic): preset names.
    pub const DSEG14: &str = "DSEG14 Classic";
    /// Dot matrix (100–900): VFD and LED-matrix displays.
    pub const DOTO: &str = "Doto";
    /// Pixel terminal: CRT readouts.
    pub const VT323: &str = "VT323";
    /// Monospace (100–800): readouts that must not jitter.
    pub const JETBRAINS_MONO: &str = "JetBrains Mono";
    /// Blackletter (bold): metal and gothic logos.
    pub const UNIFRAKTUR_COOK: &str = "UnifrakturCook";
    /// Condensed gothic display: metal logos.
    pub const PIRATA_ONE: &str = "Pirata One";
}

/// How text is lettered: family, weight, italic and width. Unset fields
/// keep the renderer's defaults (Inter, regular, upright, normal width).
///
/// ```ignore
/// let logo = draw::lettered(draw::text("logo", "Blackout", x, y, w, 42.0, "#f2f2f0", TextAlign::Center),
///     &Lettering::family(family::PIRATA_ONE).weight(400.0));
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Lettering {
    pub family: Option<String>,
    /// 100–900.
    pub weight: Option<f64>,
    pub italic: bool,
    /// Percent of normal width: 75 condensed, 125 extended (exact on
    /// families with a width axis, like Saira).
    pub width: Option<f64>,
}

impl Lettering {
    #[must_use]
    pub fn family(family: &str) -> Self {
        Self { family: Some(family.to_string()), ..Self::default() }
    }

    #[must_use]
    pub fn weight(mut self, weight: f64) -> Self {
        self.weight = Some(weight);
        self
    }

    #[must_use]
    pub fn bold(self) -> Self {
        self.weight(700.0)
    }

    #[must_use]
    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    #[must_use]
    pub fn width(mut self, percent: f64) -> Self {
        self.width = Some(percent);
        self
    }

    /// Set these on a text node (other nodes are left alone). Runs keep
    /// their own settings and inherit the rest.
    pub fn apply(&self, n: &mut Node) {
        if let NodeKind::Text(t) = &mut n.kind {
            if self.family.is_some() {
                t.font_family.clone_from(&self.family);
            }
            if self.weight.is_some() {
                t.font_weight = self.weight;
            }
            if self.italic {
                t.font_style = Some("italic".into());
            }
            if self.width.is_some() {
                t.font_width = self.width;
            }
        }
    }
}

/// `n` lettered as `l` (see [`Lettering::apply`]).
#[must_use]
pub fn lettered(mut n: Node, l: &Lettering) -> Node {
    l.apply(&mut n);
    n
}

/// [`text`] in a family, weight, italic and width.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn styled_text(name: &str, s: &str, x: f64, y: f64, w: f64, size: f64, color: &str, align: TextAlign, l: &Lettering) -> Node {
    lettered(text(name, s, x, y, w, size, color, align), l)
}

/// Centred text whose box is centred on (`cx`, `cy`).
#[must_use]
pub fn label_at(name: &str, s: &str, cx: f64, cy: f64, size: f64, color: &str) -> Node {
    let w = (s.chars().count() as f64 * size * 0.62).max(size * 1.4) + 6.0;
    text(name, s, cx - w / 2.0, cy - size * 0.68, w, size, color, TextAlign::Center)
}

/// Rough set width of `s` at `size` (bold, untracked) — for placing labels.
/// Pass the string as it will show (upper-cased, if it will be).
#[must_use]
pub fn text_width(s: &str, size: f64) -> f64 {
    s.chars().map(|c| if c.is_uppercase() { 0.68 } else { 0.58 }).sum::<f64>() * size
}

/// Smallest type a printed scale is set at, px: the numerals and position
/// words round a knob or switch stay readable however small the knob is.
pub const SCALE_TYPE_MIN: f64 = 9.0;

/// Where a label printed round a shaft sits so that its box clears radius
/// `r` (the end of its tick or dot, plus a gap): the box of `w × h` has its
/// centre on the ray at `deg`, pushed outward by the box's half-extent in
/// that direction. Returns `(centre, distance of the centre from the shaft)`.
#[must_use]
pub fn radial_centre(cx: f64, cy: f64, r: f64, deg: f64, w: f64, h: f64) -> ((f64, f64), f64) {
    let (s, c) = deg.to_radians().sin_cos();
    let rc = r + 0.5 * (w * s.abs() + h * c.abs());
    (polar(cx, cy, rc, deg), rc)
}

/// The box (`x0, y0, x1, y1`, relative to the shaft) a [`radial_label`]
/// of `s` at `size` covers — for sizing the frame around a scale.
#[must_use]
pub fn radial_extent(s: &str, r: f64, deg: f64, size: f64) -> (f64, f64, f64, f64) {
    let (w, h) = (text_width(s, size), size * 0.9);
    let ((x, y), _) = radial_centre(0.0, 0.0, r, deg, w, h);
    (x - w / 2.0, y - h / 2.0, x + w / 2.0, y + h / 2.0)
}

/// A scale label round a shaft at (`cx`, `cy`) whose box clears radius `r`
/// at `deg` (see [`radial_centre`]). On the sides it is anchored at its
/// inner edge — set left on the right, right on the left — so a word set
/// wider than estimated grows away from the knob, never back over its tick.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn radial_label(name: &str, s: &str, (cx, cy): (f64, f64), r: f64, deg: f64, size: f64, color: &str) -> Node {
    let (w, h) = (text_width(s, size), size * 0.9);
    let ((x, y), _) = radial_centre(cx, cy, r, deg, w, h);
    let bw = w * 1.4 + size;
    let sin = deg.to_radians().sin();
    let (bx, align) = if sin > 0.3 {
        (x - w / 2.0, TextAlign::Left)
    } else if sin < -0.3 {
        (x + w / 2.0 - bw, TextAlign::Right)
    } else {
        (x - bw / 2.0, TextAlign::Center)
    };
    text(name, s, bx, y - size * 0.68, bw, size, color, align)
}

#[must_use]
pub fn stroked(mut n: Node, color: &str, weight: f64) -> Node {
    n.stroke = Stroke { paints: vec![solid(color)], weight, align: StrokeAlign::Inside, ..Stroke::default() };
    n
}

#[must_use]
pub fn rounded(mut n: Node, r: f64) -> Node {
    n.corner_radii = CornerRadii::uniform(r);
    n
}

#[must_use]
pub fn with_effects(mut n: Node, effects: Vec<Effect>) -> Node {
    n.effects = effects;
    n
}

#[must_use]
pub fn with_opacity(mut n: Node, o: f64) -> Node {
    n.opacity = o;
    n
}

/// Make `n` give off light: `color` blooming `radius` px at `intensity`
/// (1 = the node's own brightness). Optional by design — renderers without
/// an effects stage draw a soft glow or nothing — and it scales with the
/// node's opacity, so put it on the node an `Led` slot dims.
#[must_use]
pub fn emissive(mut n: Node, color: Color, intensity: f64, radius: f64) -> Node {
    n.emission = Some(Emission { color: Color { a: 1.0, ..color }, intensity, radius });
    n
}

/// Rotate `n` by `deg` (clockwise) about its own box centre.
#[must_use]
pub fn rotated(mut n: Node, deg: f64) -> Node {
    let (w, h) = (n.size.width, n.size.height);
    let (x, y) = (n.transform.tx, n.transform.ty);
    let a = deg.to_radians();
    let (s, c) = a.sin_cos();
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    // Place the rotated box so its centre stays at (cx, cy).
    let tx = cx - (c * w / 2.0 - s * h / 2.0);
    let ty = cy - (s * w / 2.0 + c * h / 2.0);
    n.transform = Transform { a: c, b: -s, tx, c: s, d: c, ty };
    n
}

// ── geometry ─────────────────────────────────────────────────────────────

/// The point at `deg` (0 = up, clockwise) on radius `r` around (`cx`, `cy`).
#[must_use]
pub fn polar(cx: f64, cy: f64, r: f64, deg: f64) -> (f64, f64) {
    let a = deg.to_radians();
    (cx + r * a.sin(), cy - r * a.cos())
}

/// A ring sector between radii `r_in..r_out`, from `a0` to `a1` degrees.
#[must_use]
pub fn sector(cx: f64, cy: f64, r_in: f64, r_out: f64, a0: f64, a1: f64) -> String {
    let (x0, y0) = polar(cx, cy, r_out, a0);
    let (x1, y1) = polar(cx, cy, r_out, a1);
    let (x2, y2) = polar(cx, cy, r_in, a1);
    let (x3, y3) = polar(cx, cy, r_in, a0);
    let large = i32::from(a1 - a0 > 180.0);
    format!(
        "M {x0:.3} {y0:.3} A {r_out:.3} {r_out:.3} 0 {large} 1 {x1:.3} {y1:.3} L {x2:.3} {y2:.3} A {r_in:.3} {r_in:.3} 0 {large} 0 {x3:.3} {y3:.3} Z"
    )
}

/// A closed polygon through `points`.
#[must_use]
pub fn polygon(points: &[(f64, f64)]) -> String {
    let mut d = String::new();
    for (i, (x, y)) in points.iter().enumerate() {
        d.push_str(&format!("{} {x:.3} {y:.3} ", if i == 0 { "M" } else { "L" }));
    }
    d.push('Z');
    d
}

/// A scalloped / geared outline: `lobes` bumps of `depth` on radius `r`
/// (the Pultec skirt, a 1073 cap, a knurled grip when `lobes` is large).
#[must_use]
pub fn lobed(cx: f64, cy: f64, r: f64, lobes: u32, depth: f64) -> String {
    let n = (lobes.max(3) * 12) as usize;
    let pts: Vec<(f64, f64)> = (0..n)
        .map(|i| {
            let t = i as f64 / n as f64;
            let rr = r - depth * 0.5 + depth * 0.5 * (t * f64::from(lobes) * std::f64::consts::TAU).cos();
            polar(cx, cy, rr, t * 360.0)
        })
        .collect();
    polygon(&pts)
}

/// A regular polygon (hex nut, chicken-head base) of `sides` on radius `r`.
#[must_use]
pub fn regular(cx: f64, cy: f64, r: f64, sides: u32, rot_deg: f64) -> String {
    let pts: Vec<(f64, f64)> =
        (0..sides).map(|i| polar(cx, cy, r, rot_deg + 360.0 * f64::from(i) / f64::from(sides))).collect();
    polygon(&pts)
}
