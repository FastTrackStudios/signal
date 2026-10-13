//! Hand lettering as strokes: a single-stroke capital alphabet (plus digits
//! and a little punctuation) drawn the way a paint marker letters a
//! boutique pedal — round-capped lines, a wobble, letters that bounce on
//! the baseline. The renderer only carries one typeface, so lettering that
//! must *look* handwritten is drawn, not set.
//!
//! [`hand`] returns one vector node (all the word's strokes). `slant`
//! shears it into a brush script for amp logos.

use crate::document::proto::{Node, NodeKind, Size, Stroke, StrokeCap, StrokeJoin, Transform};

use crate::surface::kit::draw;

/// One glyph: its advance (em fractions) and its strokes (x right, y down,
/// the cap height 0..1).
fn glyph(c: char) -> (f64, Vec<Vec<(f64, f64)>>) {
    let o = |cx: f64, rx: f64| -> Vec<(f64, f64)> {
        (0..=20)
            .map(|i| {
                let t = f64::from(i) / 20.0 * std::f64::consts::TAU;
                (cx + rx * t.sin(), 0.5 - 0.5 * t.cos())
            })
            .collect()
    };
    let v = |pts: &[(f64, f64)]| pts.to_vec();
    match c {
        'A' => (
            0.66,
            vec![
                v(&[(0.0, 1.0), (0.3, 0.0), (0.6, 1.0)]),
                v(&[(0.12, 0.64), (0.48, 0.64)]),
            ],
        ),
        'B' => (
            0.62,
            vec![
                v(&[
                    (0.0, 1.0),
                    (0.0, 0.0),
                    (0.38, 0.0),
                    (0.5, 0.1),
                    (0.5, 0.35),
                    (0.38, 0.47),
                    (0.0, 0.47),
                ]),
                v(&[
                    (0.38, 0.47),
                    (0.55, 0.6),
                    (0.55, 0.88),
                    (0.42, 1.0),
                    (0.0, 1.0),
                ]),
            ],
        ),
        'C' => (
            0.64,
            vec![v(&[
                (0.58, 0.14),
                (0.44, 0.0),
                (0.18, 0.0),
                (0.0, 0.22),
                (0.0, 0.78),
                (0.18, 1.0),
                (0.44, 1.0),
                (0.6, 0.86),
            ])],
        ),
        'D' => (
            0.64,
            vec![v(&[
                (0.0, 0.0),
                (0.0, 1.0),
                (0.3, 1.0),
                (0.56, 0.8),
                (0.56, 0.2),
                (0.3, 0.0),
                (0.0, 0.0),
            ])],
        ),
        'E' => (
            0.58,
            vec![
                v(&[(0.52, 0.0), (0.0, 0.0), (0.0, 1.0), (0.54, 1.0)]),
                v(&[(0.0, 0.5), (0.4, 0.5)]),
            ],
        ),
        'F' => (
            0.56,
            vec![
                v(&[(0.52, 0.0), (0.0, 0.0), (0.0, 1.0)]),
                v(&[(0.0, 0.5), (0.4, 0.5)]),
            ],
        ),
        'G' => (
            0.66,
            vec![v(&[
                (0.58, 0.14),
                (0.44, 0.0),
                (0.18, 0.0),
                (0.0, 0.22),
                (0.0, 0.78),
                (0.18, 1.0),
                (0.44, 1.0),
                (0.6, 0.84),
                (0.6, 0.56),
                (0.34, 0.56),
            ])],
        ),
        'H' => (
            0.64,
            vec![
                v(&[(0.0, 0.0), (0.0, 1.0)]),
                v(&[(0.56, 0.0), (0.56, 1.0)]),
                v(&[(0.0, 0.5), (0.56, 0.5)]),
            ],
        ),
        'I' => (0.24, vec![v(&[(0.08, 0.0), (0.08, 1.0)])]),
        'i' => (
            0.22,
            vec![
                v(&[(0.07, 0.4), (0.07, 1.0)]),
                v(&[(0.07, 0.12), (0.08, 0.14)]),
            ],
        ),
        'J' => (
            0.56,
            vec![v(&[
                (0.5, 0.0),
                (0.5, 0.78),
                (0.36, 1.0),
                (0.14, 1.0),
                (0.0, 0.82),
            ])],
        ),
        'K' => (
            0.62,
            vec![
                v(&[(0.0, 0.0), (0.0, 1.0)]),
                v(&[(0.54, 0.0), (0.0, 0.58)]),
                v(&[(0.18, 0.44), (0.58, 1.0)]),
            ],
        ),
        'L' => (0.54, vec![v(&[(0.0, 0.0), (0.0, 1.0), (0.5, 1.0)])]),
        'M' => (
            0.76,
            vec![v(&[
                (0.0, 1.0),
                (0.0, 0.0),
                (0.34, 0.62),
                (0.68, 0.0),
                (0.68, 1.0),
            ])],
        ),
        'N' => (
            0.64,
            vec![v(&[(0.0, 1.0), (0.0, 0.0), (0.56, 1.0), (0.56, 0.0)])],
        ),
        'O' | '0' => (0.68, vec![o(0.3, 0.3)]),
        'P' => (
            0.6,
            vec![v(&[
                (0.0, 1.0),
                (0.0, 0.0),
                (0.4, 0.0),
                (0.54, 0.12),
                (0.54, 0.4),
                (0.4, 0.52),
                (0.0, 0.52),
            ])],
        ),
        'Q' => (0.7, vec![o(0.3, 0.3), v(&[(0.36, 0.72), (0.64, 1.02)])]),
        'R' => (
            0.62,
            vec![
                v(&[
                    (0.0, 1.0),
                    (0.0, 0.0),
                    (0.4, 0.0),
                    (0.54, 0.12),
                    (0.54, 0.4),
                    (0.4, 0.52),
                    (0.0, 0.52),
                ]),
                v(&[(0.24, 0.52), (0.58, 1.0)]),
            ],
        ),
        'S' => (
            0.62,
            vec![v(&[
                (0.56, 0.12),
                (0.42, 0.0),
                (0.14, 0.0),
                (0.0, 0.14),
                (0.0, 0.34),
                (0.14, 0.46),
                (0.42, 0.54),
                (0.56, 0.66),
                (0.56, 0.86),
                (0.42, 1.0),
                (0.12, 1.0),
                (0.0, 0.88),
            ])],
        ),
        'T' => (
            0.62,
            vec![v(&[(0.0, 0.0), (0.6, 0.0)]), v(&[(0.3, 0.0), (0.3, 1.0)])],
        ),
        'U' => (
            0.66,
            vec![v(&[
                (0.0, 0.0),
                (0.0, 0.8),
                (0.16, 1.0),
                (0.42, 1.0),
                (0.58, 0.8),
                (0.58, 0.0),
            ])],
        ),
        'V' => (0.66, vec![v(&[(0.0, 0.0), (0.3, 1.0), (0.6, 0.0)])]),
        'W' => (
            0.84,
            vec![v(&[
                (0.0, 0.0),
                (0.18, 1.0),
                (0.38, 0.3),
                (0.58, 1.0),
                (0.76, 0.0),
            ])],
        ),
        'X' => (
            0.62,
            vec![v(&[(0.0, 0.0), (0.56, 1.0)]), v(&[(0.56, 0.0), (0.0, 1.0)])],
        ),
        'Y' => (
            0.64,
            vec![
                v(&[(0.0, 0.0), (0.3, 0.5), (0.6, 0.0)]),
                v(&[(0.3, 0.5), (0.3, 1.0)]),
            ],
        ),
        'Z' => (
            0.62,
            vec![v(&[(0.0, 0.0), (0.56, 0.0), (0.0, 1.0), (0.58, 1.0)])],
        ),
        '1' => (0.4, vec![v(&[(0.04, 0.18), (0.22, 0.0), (0.22, 1.0)])]),
        '2' => (
            0.6,
            vec![v(&[
                (0.0, 0.2),
                (0.12, 0.04),
                (0.3, 0.0),
                (0.46, 0.06),
                (0.54, 0.22),
                (0.48, 0.42),
                (0.0, 1.0),
                (0.56, 1.0),
            ])],
        ),
        '3' => (
            0.6,
            vec![
                v(&[
                    (0.0, 0.1),
                    (0.16, 0.0),
                    (0.4, 0.0),
                    (0.52, 0.12),
                    (0.52, 0.34),
                    (0.36, 0.48),
                    (0.18, 0.48),
                ]),
                v(&[
                    (0.36, 0.48),
                    (0.56, 0.62),
                    (0.56, 0.86),
                    (0.42, 1.0),
                    (0.14, 1.0),
                    (0.0, 0.9),
                ]),
            ],
        ),
        '4' => (
            0.62,
            vec![v(&[(0.42, 1.0), (0.42, 0.0), (0.0, 0.68), (0.6, 0.68)])],
        ),
        '5' => (
            0.6,
            vec![v(&[
                (0.54, 0.0),
                (0.06, 0.0),
                (0.02, 0.44),
                (0.3, 0.38),
                (0.5, 0.48),
                (0.56, 0.7),
                (0.46, 0.94),
                (0.26, 1.0),
                (0.0, 0.92),
            ])],
        ),
        '6' => (
            0.6,
            vec![v(&[
                (0.5, 0.06),
                (0.3, 0.0),
                (0.1, 0.12),
                (0.0, 0.45),
                (0.0, 0.8),
                (0.16, 1.0),
                (0.42, 1.0),
                (0.56, 0.82),
                (0.56, 0.62),
                (0.42, 0.46),
                (0.16, 0.46),
                (0.0, 0.6),
            ])],
        ),
        '7' => (0.58, vec![v(&[(0.0, 0.0), (0.56, 0.0), (0.2, 1.0)])]),
        '8' => (
            0.6,
            vec![v(&[
                (0.28, 0.46),
                (0.08, 0.36),
                (0.06, 0.12),
                (0.28, 0.0),
                (0.5, 0.12),
                (0.48, 0.36),
                (0.28, 0.46),
                (0.02, 0.6),
                (0.02, 0.88),
                (0.28, 1.0),
                (0.54, 0.88),
                (0.54, 0.6),
                (0.28, 0.46),
            ])],
        ),
        '9' => (
            0.6,
            vec![v(&[
                (0.56, 0.4),
                (0.42, 0.54),
                (0.14, 0.54),
                (0.0, 0.38),
                (0.0, 0.18),
                (0.14, 0.0),
                (0.42, 0.0),
                (0.56, 0.18),
                (0.56, 0.55),
                (0.46, 0.88),
                (0.26, 1.0),
                (0.06, 0.94),
            ])],
        ),
        '-' => (0.46, vec![v(&[(0.04, 0.56), (0.38, 0.54)])]),
        '.' => (0.2, vec![v(&[(0.05, 0.96), (0.07, 0.98)])]),
        '/' => (0.46, vec![v(&[(0.0, 1.0), (0.4, 0.0)])]),
        '&' => (
            0.66,
            vec![v(&[
                (0.62, 1.0),
                (0.1, 0.36),
                (0.1, 0.12),
                (0.26, 0.0),
                (0.42, 0.12),
                (0.4, 0.3),
                (0.0, 0.66),
                (0.02, 0.9),
                (0.2, 1.0),
                (0.42, 0.94),
                (0.6, 0.66),
            ])],
        ),
        _ => (0.36, Vec::new()),
    }
}

/// A small deterministic wobble source.
struct Wobble(u64);

impl Wobble {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) as f64 / f64::from(u32::MAX >> 1)) - 0.5
    }
}

/// How lettering is drawn.
#[derive(Clone, Copy, Debug)]
pub struct Hand {
    /// Cap height, px.
    pub size: f64,
    /// Stroke width as a fraction of the cap height.
    pub weight: f64,
    /// Shear (0 = upright; 0.25 a brush script's lean).
    pub slant: f64,
    /// Wobble as a fraction of the cap height (0 = ruler-straight).
    pub wobble: f64,
    /// Extra space between letters, em.
    pub tracking: f64,
}

impl Hand {
    /// Paint-marker capitals at `size` px.
    #[must_use]
    pub const fn marker(size: f64) -> Self {
        Self {
            size,
            weight: 0.12,
            slant: 0.0,
            wobble: 0.075,
            tracking: 0.12,
        }
    }

    /// A slanted brush script (an amp's logo).
    #[must_use]
    pub const fn script(size: f64) -> Self {
        Self {
            size,
            weight: 0.12,
            slant: 0.28,
            wobble: 0.02,
            tracking: 0.02,
        }
    }
}

/// The lettering's strokes and its set width (px), laid out from (0, 0) at
/// the top of the capitals. Lower case letters are capitals except `i`,
/// which a hand letters small (GAiN).
#[must_use]
pub fn layout(s: &str, h: &Hand) -> (Vec<Vec<(f64, f64)>>, f64) {
    let seed = s.bytes().fold(0x9e37_79b9_u64, |a, c| {
        a.wrapping_mul(31).wrapping_add(u64::from(c))
    });
    let mut w = Wobble(seed);
    let mut x = 0.0;
    let mut lines = Vec::new();
    for c in s.chars() {
        let c = if c == 'i' { c } else { c.to_ascii_uppercase() };
        let (adv, strokes) = glyph(c);
        // Each letter bounces a little and leans its own way.
        let (bounce, lean, scale) = (
            w.next() * h.wobble * 1.8,
            w.next() * 0.1,
            1.0 + w.next() * 0.14,
        );
        for s in strokes {
            let pts = s
                .iter()
                .map(|(gx, gy)| {
                    let (jx, jy) = (w.next() * h.wobble, w.next() * h.wobble);
                    let gy = (gy - 0.5) * scale + 0.5;
                    let y = (gy + bounce + jy) * h.size;
                    let xx =
                        x + (gx * scale + jx + lean * (0.5 - gy)) * h.size + h.slant * (h.size - y);
                    (xx, y)
                })
                .collect();
            lines.push(pts);
        }
        x += (adv + h.tracking) * h.size;
    }
    (lines, (x - h.tracking * h.size).max(0.0))
}

/// Hand lettering `s` in `color`, centred on (`cx`, `cy`) (the middle of
/// the capitals): one stroked vector.
#[must_use]
pub fn hand(name: &str, s: &str, cx: f64, cy: f64, h: &Hand, color: &str) -> Node {
    let (lines, width) = layout(s, h);
    let pad = h.size * (h.weight + h.wobble * 2.0);
    let (ox, oy) = (cx - width / 2.0 - h.slant * h.size * 0.5, cy - h.size / 2.0);
    // Two passes a hair apart: the marker's dry, uneven edge.
    let mut d = String::new();
    let mut w = Wobble(0x5eed ^ s.len() as u64);
    for (pass, jit) in [(0, 0.0), (1, h.size * h.wobble * 0.6)] {
        for line in &lines {
            let (ox, oy) = if pass == 0 {
                (0.0, 0.0)
            } else {
                (w.next() * jit * 2.0, w.next() * jit * 2.0)
            };
            for (i, (x, y)) in line.iter().enumerate() {
                d.push_str(&format!(
                    "{} {:.2} {:.2} ",
                    if i == 0 { "M" } else { "L" },
                    x + pad + ox,
                    y + pad + oy
                ));
            }
        }
    }
    let mut n = Node::new(NodeKind::Vector, name);
    n.transform = Transform::translate(ox - pad, oy - pad);
    n.size = Size {
        width: width + h.slant * h.size + 2.0 * pad,
        height: h.size + 2.0 * pad,
    };
    n.fill_geometry = vec![d];
    n.stroke = Stroke {
        paints: vec![draw::solid(color)],
        weight: (h.size * h.weight).max(0.8),
        cap: StrokeCap::Round,
        join: StrokeJoin::Round,
        ..Stroke::default()
    };
    n
}

/// The set width of `s`, px.
#[must_use]
pub fn width(s: &str, h: &Hand) -> f64 {
    layout(s, h).1
}
