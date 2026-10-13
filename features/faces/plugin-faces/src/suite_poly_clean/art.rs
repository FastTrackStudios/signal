//! The heads' sculpted fronts, drawn procedurally (a seeded generator, so
//! every build draws the same art):
//!
//! - [`hammered_glass`] — violet hammered glass lit from below by the
//!   tubes (the lead head);
//! - [`relief_white`] — a white plaster relief: moulded frame, a shield
//!   with a bead strand, C-scrolls and acanthus sprays, a haloed cherub at
//!   one end and a horned, bat-winged one at the other (the rhythm head);
//! - [`floral_black`] — a black floral relief: roses, five-petal flowers,
//!   leaves and wheat (the acoustic head);
//! - [`flourish`] — the maker's scribbled mark on a faceplate.

use std::f64::consts::{PI, TAU};

use frame::document::proto::{Node, NodeId};
use frame::surface::kit::{draw, motion};
use frame::surface::widgets::Builder;

use crate::reverb::ui;

/// A small deterministic generator (an LCG), so the art is the same on
/// every build.
pub struct Rng(u64);

impl Rng {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self(seed)
    }
    /// 0..1.
    pub fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 11) as f64) / ((1_u64 << 53) as f64)
    }
    /// `lo..hi`.
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next()
    }
}

/// A closed shape through `pts` (parent coordinates) as a vector whose box
/// is the points' extent — so its gradients light this shape alone.
fn shape(name: &str, pts: &[(f64, f64)], fill: frame::document::proto::Paint) -> Node {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for (x, y) in pts {
        x0 = x0.min(*x);
        y0 = y0.min(*y);
        x1 = x1.max(*x);
        y1 = y1.max(*y);
    }
    let local: Vec<(f64, f64)> = pts.iter().map(|(x, y)| (x - x0, y - y0)).collect();
    draw::path(
        name,
        x0,
        y0,
        (x1 - x0).max(1.0),
        (y1 - y0).max(1.0),
        draw::polygon(&local),
        fill,
    )
}

/// An ellipse's outline, turned `rot` degrees about its centre.
fn oval(cx: f64, cy: f64, rx: f64, ry: f64, rot: f64) -> Vec<(f64, f64)> {
    let (s, c) = rot.to_radians().sin_cos();
    (0..28)
        .map(|i| {
            let a = TAU * f64::from(i) / 28.0;
            let (x, y) = (rx * a.cos(), ry * a.sin());
            (cx + x * c - y * s, cy + x * s + y * c)
        })
        .collect()
}

/// A pointed leaf from `(x, y)` along `deg` (0 = up), `len` long, `wid` wide.
fn leaf(x: f64, y: f64, len: f64, wid: f64, deg: f64) -> Vec<(f64, f64)> {
    let a = deg.to_radians();
    let (ux, uy) = (a.sin(), -a.cos());
    let (nx, ny) = (-uy, ux);
    let mut pts = Vec::new();
    for i in 0..=12 {
        let t = f64::from(i) / 12.0;
        let half = wid / 2.0 * (PI * t).sin().powf(0.8);
        pts.push((x + ux * len * t + nx * half, y + uy * len * t + ny * half));
    }
    for i in (0..=12).rev() {
        let t = f64::from(i) / 12.0;
        let half = wid / 2.0 * (PI * t).sin().powf(0.8);
        pts.push((x + ux * len * t - nx * half, y + uy * len * t - ny * half));
    }
    pts
}

/// An acanthus leaf: [`leaf`] with a scalloped, lobed edge.
fn acanthus(x: f64, y: f64, len: f64, wid: f64, deg: f64) -> Vec<(f64, f64)> {
    let a = deg.to_radians();
    let (ux, uy) = (a.sin(), -a.cos());
    let (nx, ny) = (-uy, ux);
    let half =
        |t: f64| wid / 2.0 * (PI * t).sin().powf(0.7) * (0.72 + 0.28 * (t * 7.0 * PI).cos().abs());
    let mut pts = Vec::new();
    for i in 0..=24 {
        let t = f64::from(i) / 24.0;
        pts.push((
            x + ux * len * t + nx * half(t),
            y + uy * len * t + ny * half(t),
        ));
    }
    for i in (0..=24).rev() {
        let t = f64::from(i) / 24.0;
        pts.push((
            x + ux * len * t - nx * half(t),
            y + uy * len * t - ny * half(t),
        ));
    }
    pts
}

// ── the lead head's hammered glass ───────────────────────────────────────

/// Hammered violet glass filling `w × h` of `parent`, lit from behind by
/// the tubes: magenta along the top, deep violet through the middle, four
/// tall pale flames of valve light rising from a lavender glow along the
/// bottom — and over it all the glass itself, a packed field of shallow
/// hammer dimples (each a soft cup: its upper rim in shadow, its lower
/// lip catching the light behind) under a frosted grain.
pub fn hammered_glass(b: &mut Builder<'_>, parent: NodeId, w: f64, h: f64) {
    b.add(
        parent,
        draw::rect(
            "glass",
            0.0,
            0.0,
            w,
            h,
            draw::linear(
                180.0,
                &[
                    (0.0, "#c43ccc"),
                    (0.12, "#8d2fb0"),
                    (0.38, "#58288e"),
                    (0.66, "#4a3597"),
                    (0.88, "#6f55cc"),
                    (1.0, "#b89cff"),
                ],
            ),
        ),
    );
    // Deeper at the ends, as the light falls off away from the valves.
    b.add(
        parent,
        draw::rect(
            "falloff",
            0.0,
            0.0,
            w,
            h,
            draw::linear(
                90.0,
                &[
                    (0.0, "#1a0a3a66"),
                    (0.25, "#1a0a3a00"),
                    (0.75, "#1a0a3a00"),
                    (1.0, "#1a0a3a66"),
                ],
            ),
        ),
    );
    // The valves' light: a flame per valve, rising from the bottom.
    let mut rng = Rng::new(7);
    for i in 0..4 {
        let cx = w * (0.355 + 0.1 * f64::from(i)) + rng.range(-8.0, 8.0);
        let fw = rng.range(130.0, 160.0);
        let top = h * rng.range(0.3, 0.4);
        let col = draw::rect(
            "valve light",
            cx - fw / 2.0,
            top,
            fw,
            (h - top) * 1.3,
            draw::radial_at(
                0.5,
                0.62,
                0.5,
                &[
                    (0.0, "#f8f2ffee"),
                    (0.3, "#dcc8ffa0"),
                    (0.65, "#a78bff30"),
                    (1.0, "#9d7cff00"),
                ],
            ),
        );
        let col = b.add(parent, col);
        // The flame's hot core, higher and narrower.
        b.add(
            parent,
            draw::rect(
                "valve core",
                cx - fw * 0.28,
                top + (h - top) * 0.12,
                fw * 0.56,
                (h - top) * 0.9,
                draw::radial_at(
                    0.5,
                    0.55,
                    0.5,
                    &[(0.0, "#fbf8ffb0"), (0.5, "#e6d8ff40"), (1.0, "#e6d8ff00")],
                ),
            ),
        );
        motion::add(b, col, motion::flicker(0.05));
    }
    b.add(
        parent,
        draw::rect(
            "valve glow",
            0.0,
            h * 0.6,
            w,
            h * 0.4,
            draw::linear(
                180.0,
                &[(0.0, "#c2a6ff00"), (0.75, "#c9b0ff55"), (1.0, "#e6d9ffc0")],
            ),
        ),
    );

    // The dimples: overlapping hammer cups on a jittered grid, each shaded
    // as a hollow lit from behind — its upper rim in shadow, its lower lip
    // bright — so the glass reads as beaten, not tiled.
    let (cw, ch) = (44.0, 34.0);
    let mut rng = Rng::new(42);
    let rows = (h / ch).ceil() as i32 + 1;
    let cols = (w / cw).ceil() as i32 + 1;
    for j in 0..rows {
        for i in 0..cols {
            let off = if j % 2 == 0 { 0.0 } else { cw / 2.0 };
            let (cx, cy) = (
                f64::from(i) * cw + off + rng.range(-cw * 0.3, cw * 0.3),
                f64::from(j) * ch + rng.range(-ch * 0.3, ch * 0.3),
            );
            let (rx, ry) = (cw * rng.range(0.55, 0.8), ch * rng.range(0.55, 0.8));
            let k = rng.range(0.6, 1.0);
            let cup = draw::linear(
                180.0 + rng.range(-20.0, 20.0),
                &[
                    (0.0, &format!("#12053a{:02x}", (90.0 * k) as u8)),
                    (0.45, "#12053a10"),
                    (0.75, "#f4e8ff08"),
                    (1.0, &format!("#f4e8ff{:02x}", (70.0 * k) as u8)),
                ],
            );
            b.add(
                parent,
                shape("dimple", &oval(cx, cy, rx, ry, rng.range(-25.0, 25.0)), cup),
            );
        }
    }
    // Frost: a fine speckle of lit and dark grains over the glass.
    let (mut lit, mut dark) = (String::new(), String::new());
    for _ in 0..2600 {
        let (x, y) = (rng.range(0.0, w), rng.range(0.0, h));
        let r = rng.range(0.5, 1.3);
        let d = format!("M {:.1} {:.1} h {r:.1} v {r:.1} h -{r:.1} Z ", x, y);
        if rng.next() < 0.5 {
            lit.push_str(&d)
        } else {
            dark.push_str(&d)
        }
    }
    b.add(
        parent,
        draw::path("frost lit", 0.0, 0.0, w, h, lit, draw::solid("#f6ecff26")),
    );
    b.add(
        parent,
        draw::path("frost dark", 0.0, 0.0, w, h, dark, draw::solid("#12053a2e")),
    );
    // The hammered surface's own ripple and the frosting on the glass.
    let full = (0.0, 0.0, w, h);
    frame::surface::kit::materials::texture::relief(
        b,
        parent,
        full,
        0.0,
        frame::surface::kit::materials::texture::Relief::Hammered { cells: 10 },
        0.35,
        1.2,
    );
    frame::surface::kit::materials::texture::relief(
        b,
        parent,
        full,
        0.0,
        frame::surface::kit::materials::texture::Relief::Powder { coarse: true },
        0.4,
        0.8,
    );
    // Glass sheen and the shadow the tolex casts into the opening.
    b.add(
        parent,
        draw::rect(
            "sheen",
            0.0,
            0.0,
            w,
            h,
            draw::linear(
                110.0,
                &[(0.0, "#ffffff00"), (0.4, "#ffffff0c"), (0.6, "#ffffff00")],
            ),
        ),
    );
    b.add(
        parent,
        draw::rect(
            "top shade",
            0.0,
            0.0,
            w,
            14.0,
            draw::linear(180.0, &[(0.0, "#2a0a3a99"), (1.0, "#2a0a3a00")]),
        ),
    );
    b.add(
        parent,
        draw::rect(
            "left shade",
            0.0,
            0.0,
            10.0,
            h,
            draw::linear(90.0, &[(0.0, "#2a0a3a80"), (1.0, "#2a0a3a00")]),
        ),
    );
}

// ── the rhythm head's white relief ───────────────────────────────────────

fn plaster(n: Node, depth: f64) -> Node {
    draw::with_effects(
        n,
        vec![
            draw::drop_shadow("#0000007a", depth * 0.6, depth * 1.2, depth * 1.5),
            draw::inner_shadow("#00000045", -depth * 0.35, -depth * 0.6, depth * 0.7),
            draw::inner_shadow("#ffffffe6", depth * 0.25, depth * 0.35, depth * 0.3),
        ],
    )
}

const WHITE: &str = "#eeeeec";

fn white_fill() -> frame::document::proto::Paint {
    draw::radial_at(
        0.35,
        0.3,
        0.9,
        &[(0.0, "#fbfbfa"), (0.6, "#ececea"), (1.0, "#d2d2d0")],
    )
}

/// A spiral scroll about (`cx`, `cy`): `turns` turns from `r0` out to `r1`,
/// `dir` ±1 for its hand.
fn scroll(cx: f64, cy: f64, r0: f64, r1: f64, turns: f64, start: f64, dir: f64) -> Vec<(f64, f64)> {
    (0..=90)
        .map(|i| {
            let t = f64::from(i) / 90.0;
            let a = start + dir * t * turns * TAU;
            let r = r0 + (r1 - r0) * t;
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

/// A spray of leaflets along a quadratic curve `p0 → p2` bowed through `p1`.
#[allow(clippy::too_many_arguments)]
fn spray(
    b: &mut Builder<'_>,
    parent: NodeId,
    (p0, p1, p2): ((f64, f64), (f64, f64), (f64, f64)),
    n: usize,
    size: f64,
    rng: &mut Rng,
    depth: f64,
    name: &str,
) {
    let q = |t: f64| {
        let u = 1.0 - t;
        (
            u * u * p0.0 + 2.0 * u * t * p1.0 + t * t * p2.0,
            u * u * p0.1 + 2.0 * u * t * p1.1 + t * t * p2.1,
        )
    };
    let stem: Vec<(f64, f64)> = (0..=24).map(|i| q(f64::from(i) / 24.0)).collect();
    b.add(
        parent,
        draw::with_effects(
            ui::line_node("stem", &stem, size * 0.18, "#dcdcda"),
            vec![draw::drop_shadow(
                "#00000040",
                depth * 0.4,
                depth * 0.8,
                depth,
            )],
        ),
    );
    let mut d = String::new();
    for k in 0..n {
        let t = (k as f64 + 0.5) / n as f64;
        let (x, y) = q(t);
        let (x2, y2) = q((t + 0.02).min(1.0));
        let along = (x2 - x).atan2(-(y2 - y)).to_degrees();
        let side = if k % 2 == 0 { 1.0 } else { -1.0 };
        let s = size * (1.0 - 0.45 * t) * rng.range(0.8, 1.15);
        d.push_str(&draw::polygon(&acanthus(
            x,
            y,
            s,
            s * 0.55,
            along + side * rng.range(35.0, 60.0),
        )));
        d.push(' ');
    }
    let (w, h) = b
        .scene
        .node(parent)
        .map_or((0.0, 0.0), |p| (p.size.width, p.size.height));
    b.add(
        parent,
        plaster(
            draw::path(name, 0.0, 0.0, w, h, d, draw::solid(WHITE)),
            depth,
        ),
    );
}

/// A small five-petal rosette.
fn rosette(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64, r: f64, dark: bool) {
    for i in 0..5 {
        let a = f64::from(i) * 72.0;
        let (px, py) = draw::polar(cx, cy, r * 0.55, a);
        let n = shape(
            "petal",
            &oval(px, py, r * 0.5, r * 0.36, a - 90.0),
            if dark { black_fill() } else { white_fill() },
        );
        b.add(
            parent,
            if dark {
                lacquer(n, r * 0.15)
            } else {
                plaster(n, r * 0.15)
            },
        );
    }
    let n = shape(
        "heart",
        &oval(cx, cy, r * 0.3, r * 0.3, 0.0),
        if dark { black_fill() } else { white_fill() },
    );
    b.add(
        parent,
        if dark {
            lacquer(n, r * 0.1)
        } else {
            plaster(n, r * 0.1)
        },
    );
}

/// A cherub: head, curls, a raised arm, a drape, and (`devil`) horns and a
/// bat wing — or a halo and a feathered wing. `dir` ±1 faces it.
fn cherub(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64, s: f64, dir: f64, devil: bool) {
    // The wing, behind.
    if devil {
        // A bat wing: a smooth leading edge from the shoulder out to the
        // tip, a scalloped trailing edge between its fingers.
        let (sx, sy) = (cx - dir * s * 0.12, cy - s * 0.12);
        let tips: Vec<(f64, f64)> = (0..5)
            .map(|i| {
                let a = (-60.0 + 34.0 * f64::from(i)).to_radians();
                let r = s * (1.15 - 0.1 * f64::from(i));
                (sx - dir * r * a.cos(), sy + r * a.sin())
            })
            .collect();
        let mut pts = vec![(sx, sy)];
        // Leading edge: bowed up.
        for k in 1..=8 {
            let t = f64::from(k) / 8.0;
            let (x, y) = (
                sx + (tips[0].0 - sx) * t,
                sy + (tips[0].1 - sy) * t - s * 0.18 * (PI * t).sin(),
            );
            pts.push((x, y));
        }
        // Trailing edge: a scallop between each pair of finger tips.
        for w2 in tips.windows(2) {
            let (a, c) = (w2[0], w2[1]);
            for k in 1..=6 {
                let t = f64::from(k) / 6.0;
                let (x, y) = (a.0 + (c.0 - a.0) * t, a.1 + (c.1 - a.1) * t);
                let pull = s * 0.14 * (PI * t).sin();
                pts.push((x + (sx - x) * pull / s, y + (sy - y) * pull / s));
            }
        }
        pts.push((cx, cy + s * 0.3));
        b.add(parent, plaster(shape("bat wing", &pts, white_fill()), 3.0));
        let ribs: Vec<Vec<(f64, f64)>> = tips.iter().map(|t| vec![(sx, sy), *t]).collect();
        b.add(
            parent,
            plaster(
                ui::strokes("wing bones", &ribs, s * 0.035, draw::solid(WHITE)),
                1.5,
            ),
        );
    } else {
        for i in 0..5 {
            let a = -dir * (100.0 + 14.0 * f64::from(i));
            let len = s * (1.0 - 0.1 * f64::from(i));
            b.add(
                parent,
                plaster(
                    shape(
                        "feather",
                        &leaf(cx - dir * s * 0.15, cy - s * 0.05, len, s * 0.3, a),
                        white_fill(),
                    ),
                    2.0,
                ),
            );
        }
    }
    // Body and drape.
    b.add(
        parent,
        plaster(
            shape(
                "body",
                &oval(cx, cy + s * 0.35, s * 0.36, s * 0.5, dir * 15.0),
                white_fill(),
            ),
            3.0,
        ),
    );
    b.add(
        parent,
        plaster(
            shape(
                "drape",
                &leaf(
                    cx - dir * s * 0.1,
                    cy + s * 0.9,
                    s * 1.1,
                    s * 0.5,
                    -dir * 20.0,
                ),
                white_fill(),
            ),
            2.5,
        ),
    );
    b.add(
        parent,
        plaster(
            shape(
                "leg",
                &oval(
                    cx + dir * s * 0.15,
                    cy + s * 0.85,
                    s * 0.14,
                    s * 0.3,
                    -dir * 25.0,
                ),
                white_fill(),
            ),
            2.5,
        ),
    );
    // The raised arm, reaching toward the centre.
    b.add(
        parent,
        plaster(
            shape(
                "arm",
                &leaf(
                    cx + dir * s * 0.2,
                    cy + s * 0.05,
                    s * 0.8,
                    s * 0.2,
                    dir * 55.0,
                ),
                white_fill(),
            ),
            2.5,
        ),
    );
    // Head with curls.
    let (hx, hy) = (cx + dir * s * 0.05, cy - s * 0.32);
    b.add(
        parent,
        plaster(
            shape("head", &oval(hx, hy, s * 0.3, s * 0.3, 0.0), white_fill()),
            3.0,
        ),
    );
    for i in 0..6 {
        let (x, y) = draw::polar(hx, hy, s * 0.26, -80.0 + 32.0 * f64::from(i));
        b.add(
            parent,
            plaster(
                shape("curl", &oval(x, y, s * 0.08, s * 0.08, 0.0), white_fill()),
                1.2,
            ),
        );
    }
    if devil {
        for sx in [-1.0, 1.0] {
            b.add(
                parent,
                plaster(
                    shape(
                        "horn",
                        &leaf(
                            hx + sx * s * 0.14,
                            hy - s * 0.2,
                            s * 0.26,
                            s * 0.1,
                            sx * 25.0,
                        ),
                        white_fill(),
                    ),
                    1.5,
                ),
            );
        }
    } else {
        let mut pts = oval(
            hx - dir * s * 0.05,
            hy - s * 0.4,
            s * 0.3,
            s * 0.08,
            -dir * 10.0,
        );
        pts.push(pts[0]);
        let ring = ui::strokes("halo", &[pts], s * 0.05, draw::solid(WHITE));
        b.add(parent, plaster(ring, 1.5));
    }
}

/// A white plaster relief filling `w × h` of `parent`.
pub fn relief_white(b: &mut Builder<'_>, parent: NodeId, w: f64, h: f64) {
    b.add(
        parent,
        draw::rect(
            "ground",
            0.0,
            0.0,
            w,
            h,
            draw::linear(180.0, &[(0.0, "#d9d9d7"), (1.0, "#e6e6e4")]),
        ),
    );
    // The moulded frame: a stepped bevel, then the recessed field.
    let m = 14.0;
    let outer = draw::rect(
        "moulding",
        m * 0.4,
        m * 0.4,
        w - m * 0.8,
        h - m * 0.8,
        draw::solid("#e9e9e7"),
    );
    b.add(
        parent,
        draw::with_effects(
            outer,
            vec![
                draw::inner_shadow("#ffffff", 1.5, 1.5, 1.0),
                draw::inner_shadow("#0000004d", -1.5, -1.5, 2.0),
            ],
        ),
    );
    let mut field = draw::frame("field", m, m, w - 2.0 * m, h - 2.0 * m);
    field.fills = vec![draw::linear(180.0, &[(0.0, "#dededc"), (1.0, "#e8e8e6")])];
    field.effects = vec![draw::inner_shadow("#00000059", 2.0, 4.0, 6.0)];
    field.clips_content = true;
    let field = b.add(parent, field);
    let (fw, fh) = (w - 2.0 * m, h - 2.0 * m);
    let (cx, cy) = (fw / 2.0, fh * 0.52);
    let mut rng = Rng::new(11);

    // Sprays of acanthus sweeping out from the shield, mirrored.
    for dir in [-1.0, 1.0] {
        for (k, (dx0, dy0, dx1, dy1, dx2, dy2)) in [
            (40.0, -30.0, 150.0, -110.0, 290.0, -55.0),
            (40.0, 30.0, 160.0, 110.0, 300.0, 45.0),
            (60.0, -5.0, 170.0, -40.0, 250.0, 5.0),
            (290.0, -55.0, 370.0, -100.0, 460.0, -25.0),
            (300.0, 45.0, 390.0, 95.0, 480.0, 20.0),
            (240.0, 0.0, 330.0, -10.0, 420.0, 15.0),
        ]
        .iter()
        .enumerate()
        {
            spray(
                b,
                field,
                (
                    (cx + dir * dx0, cy + dy0),
                    (cx + dir * dx1, cy + dy1),
                    (cx + dir * dx2, cy + dy2),
                ),
                16 + 2 * k,
                30.0,
                &mut rng,
                2.4,
                "acanthus",
            );
        }
        // The C-scrolls either side of the shield.
        for (x, y, r, turns, start) in [
            (150.0, 30.0, 40.0, 1.4, 0.0),
            (290.0, -30.0, 28.0, 1.2, PI),
            (95.0, -50.0, 20.0, 1.1, PI / 2.0),
            (400.0, 10.0, 24.0, 1.3, 0.0),
        ] {
            let pts = scroll(cx + dir * x, cy + y, 3.0, r, turns, start, dir);
            b.add(
                field,
                plaster(ui::line_node("scroll", &pts, 10.0, WHITE), 3.0),
            );
        }
        for (x, y, r) in [
            (220.0, -60.0, 11.0),
            (360.0, 55.0, 10.0),
            (120.0, 70.0, 9.0),
            (250.0, 62.0, 9.5),
            (420.0, -55.0, 9.0),
            (200.0, 20.0, 8.0),
            (330.0, -75.0, 8.0),
            (440.0, 70.0, 8.0),
        ] {
            rosette(b, field, cx + dir * x, cy + y, r, false);
        }
    }
    // The shield, its bead strand and the rosette crowning it.
    let (top, bot) = (cy - 72.0, cy + 82.0);
    let mut shield = vec![(cx, top + 10.0)];
    for i in 0..=30 {
        let u = f64::from(i) / 30.0;
        let half = 46.0 * (1.0 - u.powf(2.5)).powf(0.6) + 4.0;
        shield.push((cx - half, top + (bot - top) * u));
    }
    for i in (0..=30).rev() {
        let u = f64::from(i) / 30.0;
        let half = 46.0 * (1.0 - u.powf(2.5)).powf(0.6) + 4.0;
        shield.push((cx + half, top + (bot - top) * u));
    }
    b.add(field, plaster(shape("shield", &shield, white_fill()), 4.0));
    b.add(
        field,
        plaster(
            shape(
                "shield boss",
                &oval(cx, cy + 5.0, 26.0, 40.0, 0.0),
                white_fill(),
            ),
            2.0,
        ),
    );
    for i in 0..13 {
        let t = f64::from(i) / 12.0;
        let a = PI * (0.1 + 0.8 * t);
        let (x, y) = (cx - 34.0 * a.cos(), cy - 30.0 + 38.0 * a.sin());
        b.add(
            field,
            plaster(shape("bead", &oval(x, y, 3.2, 3.2, 0.0), white_fill()), 1.0),
        );
    }
    rosette(b, field, cx, cy - 78.0, 12.0, false);
    // The cherubs at either end, reaching in.
    cherub(b, field, fw * 0.075, fh * 0.46, 84.0, 1.0, false);
    cherub(b, field, fw * 0.925, fh * 0.46, 84.0, -1.0, true);
}

// ── the acoustic head's black floral relief ──────────────────────────────

fn black_fill() -> frame::document::proto::Paint {
    draw::radial_at(
        0.32,
        0.25,
        0.95,
        &[(0.0, "#56565a"), (0.5, "#2a2a2c"), (1.0, "#101011")],
    )
}

fn lacquer(n: Node, depth: f64) -> Node {
    draw::with_effects(
        n,
        vec![
            draw::drop_shadow("#000000e0", depth * 0.5, depth, depth * 1.6),
            draw::inner_shadow("#ffffff40", depth * 0.25, depth * 0.35, depth * 0.3),
            draw::inner_shadow("#00000080", -depth * 0.3, -depth * 0.5, depth * 0.8),
        ],
    )
}

/// A crescent petal round (`cx`, `cy`): an arc band from `a0` through
/// `span` degrees at radius `r`, `t` thick in its middle, tapering to
/// points at its ends and cupped (its inner edge pulled in).
fn crescent(cx: f64, cy: f64, r: f64, t: f64, a0: f64, span: f64) -> Vec<(f64, f64)> {
    let n = 18;
    let mut pts = Vec::new();
    for i in 0..=n {
        let u = f64::from(i) / f64::from(n);
        pts.push(draw::polar(
            cx,
            cy,
            r + t * 0.15 * (PI * u).sin(),
            a0 + span * u,
        ));
    }
    for i in (0..=n).rev() {
        let u = f64::from(i) / f64::from(n);
        pts.push(draw::polar(cx, cy, r - t * (PI * u).sin(), a0 + span * u));
    }
    pts
}

/// A rose: a tight spiral bud wrapped in crescent petals, each ring wider
/// and more open, the outer petals rolled back at their lips.
fn rose(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64, r: f64, rng: &mut Rng) {
    // The outer, open petals: broad cups behind everything.
    let off = rng.range(0.0, 72.0);
    for i in 0..5 {
        let a = off + 72.0 * f64::from(i);
        let (px, py) = draw::polar(cx, cy, r * 0.55, a);
        b.add(
            parent,
            lacquer(
                shape(
                    "outer petal",
                    &oval(px, py, r * 0.5, r * 0.42, a + 90.0),
                    black_fill(),
                ),
                (r * 0.07).max(2.0),
            ),
        );
        b.add(
            parent,
            lacquer(
                shape(
                    "petal lip",
                    &crescent(px, py, r * 0.42, r * 0.1, a - 70.0, 140.0),
                    black_fill(),
                ),
                (r * 0.03).max(1.0),
            ),
        );
    }
    // The wrapped petals: crescents spiralling inward.
    for ring in 0..5 {
        let k = f64::from(ring);
        let rr = r * (0.62 - 0.11 * k);
        let n = 3;
        let off = rng.range(0.0, 120.0) + k * 47.0;
        for i in 0..n {
            let a = off + 360.0 * f64::from(i) / f64::from(n);
            let c = crescent(
                cx + rng.range(-1.0, 1.0),
                cy + rng.range(-1.0, 1.0),
                rr,
                rr * 0.34,
                a,
                170.0 + k * 10.0,
            );
            b.add(
                parent,
                lacquer(shape("petal", &c, black_fill()), (rr * 0.08).max(1.2)),
            );
        }
    }
    let bud = scroll(cx, cy, 1.0, r * 0.14, 1.4, rng.range(0.0, TAU), 1.0);
    b.add(
        parent,
        lacquer(
            ui::line_node("bud", &bud, (r * 0.04).max(1.2), "#3a3a3d"),
            1.2,
        ),
    );
}

/// A heart, as the body has it: a lopsided muscle, its vessels rising from
/// the top, a vein running down its face.
fn heart(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64, s: f64, rot: f64) {
    for (dx, dy, w, l, a) in [
        (-0.12, -0.55, 0.13, 0.42, -10.0),
        (0.08, -0.6, 0.12, 0.5, 8.0),
        (0.28, -0.45, 0.1, 0.36, 30.0),
    ] {
        let (x, y) = (cx + dx * s, cy + dy * s);
        b.add(
            parent,
            lacquer(
                shape(
                    "vessel",
                    &oval(x, y, w * s, l * s * 0.5, rot + a),
                    black_fill(),
                ),
                s * 0.04,
            ),
        );
    }
    b.add(
        parent,
        lacquer(
            shape(
                "atrium",
                &oval(cx - s * 0.22, cy - s * 0.18, s * 0.32, s * 0.28, rot - 20.0),
                black_fill(),
            ),
            s * 0.05,
        ),
    );
    b.add(
        parent,
        lacquer(
            shape(
                "ventricle",
                &leaf(cx + s * 0.05, cy - s * 0.3, s * 1.05, s * 0.85, rot + 170.0),
                black_fill(),
            ),
            s * 0.07,
        ),
    );
    let vein = [
        (cx + s * 0.02, cy - s * 0.2),
        (cx + s * 0.1, cy + s * 0.1),
        (cx + s * 0.05, cy + s * 0.4),
        (cx + s * 0.14, cy + s * 0.55),
    ];
    b.add(parent, ui::line_node("vein", &vein, s * 0.03, "#3c3c40"));
}

/// A five-petal flower.
fn blossom(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64, r: f64, rot: f64) {
    for i in 0..5 {
        let a = rot + f64::from(i) * 72.0;
        b.add(
            parent,
            lacquer(
                shape("petal", &leaf(cx, cy, r, r * 0.7, a), black_fill()),
                r * 0.06,
            ),
        );
        // The petal's crease.
        let (ex, ey) = draw::polar(cx, cy, r * 0.7, a);
        b.add(
            parent,
            ui::line_node("crease", &[(cx, cy), (ex, ey)], 0.9, "#0b0b0c"),
        );
    }
    rosette(b, parent, cx, cy, r * 0.22, true);
}

/// A stalk of wheat from (`x`, `y`) along `deg`.
fn wheat(b: &mut Builder<'_>, parent: NodeId, x: f64, y: f64, len: f64, deg: f64) {
    let a = deg.to_radians();
    let (ux, uy) = (a.sin(), -a.cos());
    b.add(
        parent,
        lacquer(
            ui::line_node(
                "straw",
                &[(x, y), (x + ux * len, y + uy * len)],
                3.0,
                "#2a2a2c",
            ),
            1.5,
        ),
    );
    for k in 0..14 {
        let t = 0.3 + 0.7 * f64::from(k) / 14.0;
        let (gx, gy) = (x + ux * len * t, y + uy * len * t);
        for side in [-1.0, 1.0] {
            b.add(
                parent,
                lacquer(
                    shape(
                        "grain",
                        &oval(
                            gx + side * ux.abs() * 0.0,
                            gy,
                            len * 0.06,
                            len * 0.028,
                            deg + 90.0 + side * 35.0,
                        ),
                        black_fill(),
                    ),
                    1.2,
                ),
            );
        }
    }
}

/// A black floral relief filling `w × h` of `parent`: packed so no ground
/// shows — leaves behind, then sweeping wheat, blossoms, hearts, roses.
pub fn floral_black(b: &mut Builder<'_>, parent: NodeId, w: f64, h: f64) {
    b.add(
        parent,
        draw::rect(
            "ground",
            0.0,
            0.0,
            w,
            h,
            draw::linear(180.0, &[(0.0, "#1b1b1c"), (1.0, "#121213")]),
        ),
    );
    let mut rng = Rng::new(23);
    // Leaves everywhere behind, in two layers.
    for layer in 0..2 {
        for _ in 0..170 {
            let (x, y) = (rng.range(-30.0, w + 30.0), rng.range(-20.0, h + 20.0));
            let len = rng.range(40.0, 84.0) * if layer == 0 { 1.2 } else { 1.0 };
            let deg = rng.range(0.0, 360.0);
            let pts = acanthus(x, y, len, len * 0.5, deg);
            b.add(
                parent,
                lacquer(shape("leaf", &pts, black_fill()), 2.0 + f64::from(layer)),
            );
            let a = deg.to_radians();
            let rib = [(x, y), (x + a.sin() * len * 0.85, y - a.cos() * len * 0.85)];
            b.add(parent, ui::line_node("rib", &rib, 0.9, "#0a0a0a"));
        }
    }
    for (x, y, len, deg) in [
        (0.12, 0.62, 180.0, 62.0),
        (0.36, 0.98, 210.0, 68.0),
        (0.72, 0.28, 170.0, -66.0),
        (0.6, 1.05, 190.0, 55.0),
        (0.0, 0.95, 150.0, 42.0),
        (0.9, 0.95, 160.0, -60.0),
        (0.2, 0.05, 150.0, 115.0),
    ] {
        wheat(b, parent, w * x, h * y, len, deg);
    }
    for (x, y, r, rot) in [
        (0.14, 0.74, 54.0, 10.0),
        (0.47, 0.5, 50.0, 30.0),
        (0.63, 0.2, 48.0, -10.0),
        (0.955, 0.5, 42.0, 20.0),
        (0.3, 0.14, 28.0, 5.0),
        (0.87, 0.9, 30.0, 40.0),
        (0.73, 0.88, 24.0, 12.0),
        (0.53, 0.06, 26.0, 50.0),
        (0.25, 0.95, 28.0, 8.0),
        (0.42, 0.18, 22.0, 20.0),
    ] {
        blossom(b, parent, w * x, h * y, r, rot);
    }
    heart(b, parent, w * 0.245, h * 0.62, 58.0, -12.0);
    heart(b, parent, w * 0.585, h * 0.64, 70.0, 10.0);
    heart(b, parent, w * 0.735, h * 0.22, 56.0, 18.0);
    rose(b, parent, w * 0.065, h * 0.22, 58.0, &mut rng);
    rose(b, parent, w * 0.97, h * 0.1, 44.0, &mut rng);
    rose(b, parent, w * 0.33, h * 0.42, 86.0, &mut rng);
    rose(b, parent, w * 0.84, h * 0.6, 90.0, &mut rng);
    rose(b, parent, w * 0.5, h * 0.9, 34.0, &mut rng);
    // The shadow the tolex casts into the opening.
    b.add(
        parent,
        draw::rect(
            "top shade",
            0.0,
            0.0,
            w,
            16.0,
            draw::linear(180.0, &[(0.0, "#000000c0"), (1.0, "#00000000")]),
        ),
    );
    b.add(
        parent,
        draw::rect(
            "left shade",
            0.0,
            0.0,
            12.0,
            h,
            draw::linear(90.0, &[(0.0, "#000000a0"), (1.0, "#00000000")]),
        ),
    );
}

/// The maker's mark: a quick looped scribble `w` wide from (`x`, `cy`).
pub fn flourish(b: &mut Builder<'_>, parent: NodeId, x: f64, cy: f64, w: f64, ink: &str) {
    let mut pts = Vec::new();
    for i in 0..=60 {
        let t = f64::from(i) / 60.0;
        let a = t * TAU * 1.5;
        pts.push((
            x + w * 0.05 + w * 0.3 * t + w * 0.06 * a.cos(),
            cy - w * 0.12 * a.sin() + w * 0.05 * t,
        ));
    }
    for i in 0..=30 {
        let t = f64::from(i) / 30.0;
        pts.push((
            x + w * 0.35 + w * 0.2 * t,
            cy + w * 0.03 - w * 0.25 * (PI * t).sin(),
        ));
    }
    for i in 0..=20 {
        let t = f64::from(i) / 20.0;
        pts.push((x + w * 0.55 + w * 0.45 * t, cy - w * 0.02 * t));
    }
    b.add(parent, ui::line_node("mark", &pts, 1.1, ink));
    b.add(
        parent,
        ui::line_node(
            "mark stroke",
            &[(x + w * 0.25, cy + w * 0.12), (x + w * 0.55, cy - w * 0.22)],
            1.0,
            ink,
        ),
    );
}
