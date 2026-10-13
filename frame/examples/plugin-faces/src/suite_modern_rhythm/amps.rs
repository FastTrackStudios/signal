//! Modern Rhythm amps: four heads, each a page of its own in the Head form
//! at the plugin faceplate's aspect (1200 × 545, 2.2 : 1 — the manual's
//! straight-on renders), the control strip across the bottom fifth.
//!
//! - **Hex Rhythm** (the plugin's own screenshot): black tolex, a lattice of
//!   interlocking hexagons over four lit power valves, a cream strip of
//!   black pointer knobs with 0–10 round them.
//! - **Croc Clean**: grey crocodile covering the whole front, a scratched
//!   steel strip, cream pointer knobs in tick rings, a BRIGHT toggle.
//! - **Cube Crunch**: cream tolex, an ivory lattice of tumbling cubes over
//!   the valves, a slate strip, black pointer knobs, BRIGHT.
//! - **Cage Lead**: a perforated black steel cage over glowing valves,
//!   spun-cap knobs on a black strip, a wire handle.
//!
//! All four: a red illuminated rocker for power, an input jack with its
//! cable, and a player's scribble (invented) at the right end.

use document_proto::NodeId;
use surface_kit::materials::{self, Grain, Material, light};
use surface_kit::{Place, draw, motion};
use surface_widgets::Builder;

use crate::delay::parts;
use crate::face::{Face, canvas};
use crate::rigkit::{self as rk, Letter};

use super::PLUGIN;
use super::common::legend;

pub fn faces() -> Vec<Face> {
    vec![hex_rhythm(), croc_clean(), cube_crunch(), cage_lead()]
}

pub const W: f64 = 1200.0;
pub const H: f64 = 545.0;

// ── the cabinet ──────────────────────────────────────────────────────────

/// A deterministic generator for textures.
struct Rng(u64);
impl Rng {
    fn f(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn circle_d(x: f64, y: f64, r: f64, ccw: bool) -> String {
    let k = 0.5523 * r;
    if ccw {
        format!("M {:.2} {y:.2} C {:.2} {:.2} {:.2} {:.2} {x:.2} {:.2} C {:.2} {:.2} {:.2} {:.2} {:.2} {y:.2} C {:.2} {:.2} {:.2} {:.2} {x:.2} {:.2} C {:.2} {:.2} {:.2} {:.2} {:.2} {y:.2} Z ", x + r, x + r, y - k, x + k, y - r, y - r, x - k, y - r, x - r, y - k, x - r, x - r, y + k, x - k, y + r, y + r, x + k, y + r, x + r, y + k, x + r)
    } else {
        format!("M {:.2} {y:.2} C {:.2} {:.2} {:.2} {:.2} {x:.2} {:.2} C {:.2} {:.2} {:.2} {:.2} {:.2} {y:.2} C {:.2} {:.2} {:.2} {:.2} {x:.2} {:.2} C {:.2} {:.2} {:.2} {:.2} {:.2} {y:.2} Z ", x + r, x + r, y + k, x + k, y + r, y + r, x - k, y + r, x - r, y + k, x - r, x - r, y - k, x - k, y - r, y - r, x + k, y - r, x + r, y - k, x + r)
    }
}

/// Leather grain on a covering: fine light and dark flecks.
fn grain(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64), density: f64, (light, dark): (f64, f64), seed: u64) {
    let mut r = Rng(seed | 1);
    for (c, a) in [("#ffffff", light), ("#000000", dark)] {
        let mut d = String::new();
        let n = (w * h * density) as usize;
        for _ in 0..n {
            let (px, py) = (x + r.f() * w, y + r.f() * h);
            let s = 0.6 + r.f() * 1.4;
            d.push_str(&format!("M {px:.1} {py:.1} l {s:.1} {:.1} l {:.1} {:.1} Z ", s * 0.4, -s * 0.7, s * 0.6));
        }
        b.add(parent, draw::with_opacity(draw::path("grain", 0.0, 0.0, W, H, d, draw::solid(c)), a));
    }
}

/// The feet under the cabinet.
fn feet(b: &mut Builder<'_>, root: NodeId, (x0, x1): (f64, f64), y: f64) {
    for x in [x0, x1] {
        let foot = draw::rounded(draw::rect("foot", x - 45.0, y - 4.0, 90.0, H - y + 2.0, draw::linear(180.0, &[(0.0, "#2c2d30"), (1.0, "#0a0a0b")])), 6.0);
        b.add(root, draw::with_effects(foot, vec![draw::drop_shadow("#00000099", 0.0, 3.0, 6.0)]));
    }
}

/// A black moulded carry handle (strap) on top of the cabinet.
fn strap_handle(b: &mut Builder<'_>, root: NodeId, (cx, top, w): (f64, f64, f64), color: &str) {
    for x in [cx - w / 2.0, cx + w / 2.0 - 34.0] {
        let cap = draw::rounded(draw::rect("handle cap", x, top + 12.0, 34.0, 22.0, draw::linear(180.0, &[(0.0, "#9a9b9d"), (1.0, "#3b3c3e")])), 3.0);
        b.add(root, cap);
    }
    let strap = draw::rounded(draw::rect("handle", cx - w / 2.0 + 14.0, top, w - 28.0, 16.0, draw::linear(180.0, &[(0.0, &parts::mixed(color, "#ffffff", 0.25)), (0.5, color), (1.0, &parts::mixed(color, "#000000", 0.5))])), 8.0);
    b.add(root, draw::with_effects(strap, vec![draw::drop_shadow("#000000a0", 0.0, 3.0, 5.0)]));
}

/// Black metal corner protectors at the cabinet's four corners.
fn corners(b: &mut Builder<'_>, root: NodeId, (x, y, w, h): (f64, f64, f64, f64)) {
    for (cx, cy, sx, sy) in [(x, y, 1.0, 1.0), (x + w, y, -1.0, 1.0), (x, y + h, 1.0, -1.0), (x + w, y + h, -1.0, -1.0)] {
        let s = 30.0;
        let (x0, y0) = (if sx > 0.0 { cx - 5.0 } else { cx - s + 5.0 }, if sy > 0.0 { cy - 5.0 } else { cy - s + 5.0 });
        let n = draw::rounded(draw::rect("corner", x0, y0, s, s, draw::linear(160.0, &[(0.0, "#4a4c50"), (0.45, "#1c1d20"), (1.0, "#08080a")])), 9.0);
        b.add(root, draw::with_effects(n, vec![draw::drop_shadow("#000000b0", 0.0, 2.0, 3.0), draw::inner_shadow("#ffffff30", 0.0, 1.0, 1.0)]));
        b.add(root, draw::circle("rivet", cx + 7.0 * sx, cy + 7.0 * sy, 2.4, draw::radial(&[(0.0, "#b9babd"), (1.0, "#1b1c1e")])));
    }
}

/// The cabinet box covered in `m` (tolex, croc, powder…) from the kit's
/// materials, lit from the upper left, its edge rolled, standing on the
/// shelf with a cast shadow.
fn covered(b: &mut Builder<'_>, root: NodeId, (x, y, w, h): (f64, f64, f64, f64), m: &Material, radius: f64) {
    let under = draw::rounded(draw::rect("cabinet", x, y, w, h, draw::solid("#101012")), radius);
    b.add(root, draw::with_effects(under, light::stand(12.0, 1.0)));
    materials::fill(b, root, (x, y, w, h), radius, m);
    for n in light::edge("cabinet edge", (x, y, w, h), radius, 6.0, 1.0, true) {
        b.add(root, n);
    }
    b.add(root, draw::stroked(draw::rounded(draw::rect("cabinet outline", x, y, w, h, draw::solid("#00000000")), radius), "#000000", 1.5));
}

/// Crocodile scales in vector, over a croc-covered box (the material's
/// relief is pixels, which a face written out as markup doesn't carry):
/// a jittered lattice of rounded polygons, each inset from its neighbours
/// by a pale seam, lit on its upper-left edge and shaded on its lower
/// right, the scales running in taller rows as the hide's do.
fn croc_scales(b: &mut Builder<'_>, root: NodeId, (x, y, w, h): (f64, f64, f64, f64), (px, py): (f64, f64), seed: u64) {
    let mut r = Rng(seed | 1);
    let cols = (w / px).ceil() as usize + 2;
    let rows = (h / py).ceil() as usize + 2;
    let mut pts = vec![vec![(0.0, 0.0); cols + 1]; rows + 1];
    for (j, row) in pts.iter_mut().enumerate() {
        for (i, p) in row.iter_mut().enumerate() {
            let stagger = if j % 2 == 1 { px * 0.5 } else { 0.0 };
            *p = (x - px + i as f64 * px + stagger + (r.f() - 0.5) * px * 0.55, y - py + j as f64 * py + (r.f() - 0.5) * py * 0.45);
        }
    }
    let (mut body, mut lit, mut shade) = (String::new(), String::new(), String::new());
    for j in 0..rows {
        for i in 0..cols {
            let q = [pts[j][i], pts[j][i + 1], pts[j + 1][i + 1], pts[j + 1][i]];
            let (cx, cy) = (q.iter().map(|p| p.0).sum::<f64>() / 4.0, q.iter().map(|p| p.1).sum::<f64>() / 4.0);
            let inset = |k: f64, dx: f64, dy: f64| -> String {
                let v: Vec<(f64, f64)> = q.iter().map(|(qx, qy)| (cx + (qx - cx) * k + dx, cy + (qy - cy) * k + dy)).collect();
                // Rounded: corners cut by quadratic curves through the midpoints.
                let mid = |a: (f64, f64), b: (f64, f64)| ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
                let m0 = mid(v[3], v[0]);
                let mut d = format!("M {:.1} {:.1} ", m0.0, m0.1);
                for k in 0..4 {
                    let m = mid(v[k], v[(k + 1) % 4]);
                    d.push_str(&format!("Q {:.1} {:.1} {:.1} {:.1} ", v[k].0, v[k].1, m.0, m.1));
                }
                d.push_str("Z ");
                d
            };
            body.push_str(&inset(0.84, 0.0, 0.0));
            lit.push_str(&inset(0.6, -px * 0.08, -py * 0.08));
            shade.push_str(&inset(0.7, px * 0.07, py * 0.08));
        }
    }
    let mut f = draw::frame("croc scales", x, y, w, h);
    f.clips_content = true;
    let f = b.add(root, f);
    let at = |d: &str| {
        let mut n = draw::path("scales", -x, -y, x + w, y + h, d.to_string(), draw::solid("#000000"));
        n.fills = Vec::new();
        n
    };
    b.add(f, draw::rect("seams", 0.0, 0.0, w, h, draw::solid("#6d6962")));
    let mut n = at(&body);
    n.name = "scales".into();
    n.fills = vec![draw::linear(165.0, &[(0.0, "#58544d"), (0.5, "#403d38"), (1.0, "#2b2926")])];
    b.add(f, n);
    let mut n = at(&shade);
    n.name = "scale shade".into();
    n.fills = vec![draw::solid("#00000040")];
    b.add(f, n);
    let mut n = at(&lit);
    n.name = "scale light".into();
    n.fills = vec![draw::solid("#ffffff12")];
    b.add(f, n);
    b.add(f, draw::rect("room light", 0.0, 0.0, w, h, light::sheen(0.12, 0.3)));
}

// ── valves ───────────────────────────────────────────────────────────────

/// A power valve seen through the front: glass, dark plates, the getter's
/// silvering at the top, and the heater's orange glow (lit and flickering
/// a little, as a filament does).
fn valve(b: &mut Builder<'_>, parent: NodeId, (cx, top, w, h): (f64, f64, f64, f64), glow: f64) {
    let x = cx - w / 2.0;
    b.add(parent, draw::rounded(draw::rect("valve base", x + w * 0.1, top + h - 18.0, w * 0.8, 18.0, draw::linear(180.0, &[(0.0, "#2b2420"), (1.0, "#0c0a09")])), 3.0));
    let glass = draw::rounded(draw::rect("glass", x, top, w, h - 14.0, draw::linear(90.0, &[(0.0, "#ffffff10"), (0.2, "#ffffff2a"), (0.5, "#ffffff08"), (0.85, "#ffffff22"), (1.0, "#ffffff0c")])), w * 0.42);
    b.add(parent, draw::stroked(glass, "#ffffff26", 1.0));
    b.add(parent, draw::rounded(draw::rect("getter", x + 3.0, top + 3.0, w - 6.0, h * 0.14, draw::linear(180.0, &[(0.0, "#c9c3bd55"), (1.0, "#2a262200")])), w * 0.35));
    let (px, pw) = (cx - w * 0.3, w * 0.6);
    b.add(parent, draw::rect("plates", px, top + h * 0.26, pw, h * 0.5, draw::linear(90.0, &[(0.0, "#141210"), (0.5, "#3a342e"), (1.0, "#141210")])));
    for i in 0..3 {
        let gx = px + pw * (0.2 + 0.3 * f64::from(i));
        let g = draw::rect("heater", gx - 1.5, top + h * 0.3, 3.0, h * 0.42, draw::linear(180.0, &[(0.0, "#ffb86a"), (0.5, "#ff7a1a"), (1.0, "#ffb86a")]));
        let id = b.add(parent, draw::emissive(g, draw::hex("#ff7a1a"), 0.9 * glow, 14.0));
        motion::add(b, id, motion::flicker(0.08));
    }
    let halo = draw::circle("glow", cx, top + h * 0.5, w * 0.7, draw::radial(&[(0.0, "#ff8a3a55"), (1.0, "#ff8a3a00")]));
    b.add(parent, draw::with_opacity(halo, glow));
}

/// Four big power valves and four small preamp valves across a chassis
/// window at `(x, y, w, h)`.
fn valve_row(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64), glow: f64) {
    b.add(parent, draw::rect("chassis", x, y, w, h, draw::linear(180.0, &[(0.0, "#0a0908"), (0.7, "#171411"), (1.0, "#0c0a09")])));
    let cx = x + w / 2.0;
    let pitch = w * 0.12;
    for i in 0..4 {
        let vx = cx + (f64::from(i) - 1.5) * pitch;
        valve(b, parent, (vx - pitch * 0.45, y + h * 0.44, pitch * 0.34, h * 0.52), glow * 0.7);
    }
    for i in 0..4 {
        let vx = cx + (f64::from(i) - 1.5) * pitch;
        valve(b, parent, (vx + pitch * 0.05, y + h * 0.1, pitch * 0.5, h * 0.86), glow);
    }
}

// ── grilles ──────────────────────────────────────────────────────────────

/// A lattice of interlocking hexagons: each cell a hexagon with a smaller
/// one inside, joined by three spokes — cut from steel, the gaps showing
/// what is behind.
fn hex_lattice(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64), r: f64, color: &str, weight: f64) {
    let mut lines: Vec<Vec<(f64, f64)>> = Vec::new();
    let dx = r * 3f64.sqrt();
    let dy = r * 1.5;
    let rows = (h / dy).ceil() as i32 + 1;
    let cols = (w / dx).ceil() as i32 + 1;
    for j in 0..=rows {
        for i in -1..=cols {
            let cx = x + f64::from(i) * dx + if j % 2 == 1 { dx / 2.0 } else { 0.0 };
            let cy = y + f64::from(j) * dy;
            let hexp = |rr: f64, rot: f64| -> Vec<(f64, f64)> { (0..=6).map(|k| draw::polar(cx, cy, rr, rot + 60.0 * f64::from(k))).collect() };
            lines.push(hexp(r, 0.0));
            lines.push(hexp(r * 0.48, 30.0));
            for k in 0..3 {
                let a = 60.0 + 120.0 * f64::from(k);
                lines.push(vec![draw::polar(cx, cy, r * 0.48 * 0.866, a), draw::polar(cx, cy, r, a - 30.0)]);
            }
        }
    }
    let mut frame = draw::frame("lattice", x, y, w, h);
    frame.clips_content = true;
    let f = b.add(parent, frame);
    let shifted: Vec<Vec<(f64, f64)>> = lines.iter().map(|l| l.iter().map(|(px, py)| (px - x, py - y)).collect()).collect();
    let n = parts::stroke_path("lattice", (0.0, 0.0, w, h), &shifted, weight, color, false);
    b.add(f, draw::with_effects(n, vec![draw::drop_shadow("#000000c0", 0.0, 2.0, 3.0)]));
}

/// A lattice of tumbling cubes (isometric): hexagon outlines with three
/// edges in from alternate corners.
fn cube_lattice(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64), r: f64, color: &str, weight: f64) {
    let mut lines: Vec<Vec<(f64, f64)>> = Vec::new();
    let dx = r * 3f64.sqrt();
    let dy = r * 1.5;
    let rows = (h / dy).ceil() as i32 + 1;
    let cols = (w / dx).ceil() as i32 + 1;
    for j in 0..=rows {
        for i in -1..=cols {
            let cx = x + f64::from(i) * dx + if j % 2 == 1 { dx / 2.0 } else { 0.0 };
            let cy = y + f64::from(j) * dy;
            lines.push((0..=6).map(|k| draw::polar(cx, cy, r, 60.0 * f64::from(k))).collect());
            for k in 0..3 {
                let a = 120.0 * f64::from(k) + if (i + j) % 2 == 0 { 0.0 } else { 60.0 };
                lines.push(vec![(cx, cy), draw::polar(cx, cy, r, a)]);
                // A second, inset line per face: the lattice's double bars.
                lines.push(vec![draw::polar(cx, cy, r * 0.5, a + 60.0), draw::polar(cx, cy, r * 0.5, a + 60.0 + 60.0)]);
            }
        }
    }
    let mut frame = draw::frame("lattice", x, y, w, h);
    frame.clips_content = true;
    let f = b.add(parent, frame);
    let shifted: Vec<Vec<(f64, f64)>> = lines.iter().map(|l| l.iter().map(|(px, py)| (px - x, py - y)).collect()).collect();
    let n = parts::stroke_path("lattice", (0.0, 0.0, w, h), &shifted, weight, color, false);
    b.add(f, draw::with_effects(n, vec![draw::drop_shadow("#00000099", 0.0, 1.5, 2.0)]));
}

/// A perforated steel sheet: a staggered grid of round holes (cut out, so
/// what is behind shows), with the sheet's own sheen.
fn perforated(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64), pitch: f64, hole: f64) {
    let mut d = format!("M {x:.2} {y:.2} L {:.2} {y:.2} L {:.2} {:.2} L {x:.2} {:.2} Z ", x + w, x + w, y + h, y + h);
    let rows = (h / (pitch * 0.866)) as i32;
    let cols = (w / pitch) as i32;
    for j in 0..rows {
        for i in 0..cols {
            let cx = x + pitch * 0.6 + f64::from(i) * pitch + if j % 2 == 1 { pitch / 2.0 } else { 0.0 };
            let cy = y + pitch * 0.55 + f64::from(j) * pitch * 0.866;
            if cx + hole > x + w - 2.0 {
                continue;
            }
            d.push_str(&circle_d(cx, cy, hole, true));
        }
    }
    let n = draw::path("cage", 0.0, 0.0, W, H, d, draw::linear(170.0, &[(0.0, "#3a3b3e"), (0.5, "#222326"), (1.0, "#141517")]));
    b.add(parent, draw::with_effects(n, vec![draw::drop_shadow("#000000a0", 0.0, 2.0, 4.0)]));
}

// ── the strip ────────────────────────────────────────────────────────────

/// The control strip plate at `(x, y, w, h)` in `paint`, a bevelled edge.
fn strip(b: &mut Builder<'_>, root: NodeId, (x, y, w, h): (f64, f64, f64, f64), paint: (&str, &str), scratches: bool, seed: u64) {
    let n = draw::rect("strip", x, y, w, h, draw::linear(180.0, &[(0.0, paint.0), (1.0, paint.1)]));
    b.add(root, draw::with_effects(draw::stroked(n, "#00000080", 1.0), vec![draw::inner_shadow("#ffffff30", 0.0, 1.5, 1.0), draw::inner_shadow("#00000066", 0.0, -2.0, 3.0), draw::drop_shadow("#00000080", 0.0, 2.0, 4.0)]));
    if scratches {
        let mut r = Rng(seed | 1);
        let mut d = String::new();
        for _ in 0..90 {
            let (px, py) = (x + r.f() * w, y + 4.0 + r.f() * (h - 8.0));
            let (lx, ly) = (6.0 + r.f() * 28.0, (r.f() - 0.5) * 10.0);
            d.push_str(&format!("M {px:.1} {py:.1} l {lx:.1} {ly:.1} l 0.5 0.4 l {:.1} {:.1} Z ", -lx, -ly));
        }
        b.add(root, draw::with_opacity(draw::path("scratches", 0.0, 0.0, W, H, d, draw::solid("#ffffff")), 0.07));
        grain(b, root, (x, y, w, h), 0.02, (0.05, 0.08), seed ^ 0x55);
    }
}

/// What the strip prints round an amp knob.
#[derive(Clone, Copy)]
enum Scale {
    /// 0–10 round it.
    Numerals,
    /// A ring of `n` ticks, every fifth long.
    Ticks(usize),
}

/// The scale printed on the strip round a knob of boss diameter `d` at
/// (`cx`, `cy`) — fixed, in the strip's ink, outside the turning knob.
fn print_scale(b: &mut Builder<'_>, p: NodeId, scale: Scale, (cx, cy): (f64, f64), d: f64, ink: &str) {
    let k = d / 40.0;
    let r_print = d / 2.0 * 1.12 + 4.0 * k;
    let sweep = 300.0;
    match scale {
        Scale::Numerals => {
            let size = (6.2 * k).max(6.0);
            for i in 0..=10 {
                let a = (f64::from(i) / 10.0 - 0.5) * sweep;
                b.add(p, draw::radial_label("numeral", &i.to_string(), (cx, cy), r_print + size * 0.55, a, size, ink));
            }
        }
        Scale::Ticks(n) => {
            let lines: Vec<Vec<(f64, f64)>> = (0..n)
                .map(|i| {
                    let a = (i as f64 / (n - 1) as f64 - 0.5) * sweep;
                    let len = if i % 5 == 0 { 5.5 * k } else { 3.2 * k };
                    vec![draw::polar(cx, cy, r_print, a), draw::polar(cx, cy, r_print + len, a)]
                })
                .collect();
            b.add(p, parts::stroke_path("scale ticks", (0.0, 0.0, W, H), &lines, 0.8 * k, ink, false));
        }
    }
}

/// An amp knob: a `knob.3d` in `preset` + `opts`, its boss `d` across at
/// (`cx`, `cy`), the strip's `scale` round it and its legend cut above.
#[allow(clippy::too_many_arguments)]
fn knob(b: &mut Builder<'_>, p: NodeId, preset: &str, opts: &[&str], scale: Scale, addr: &str, name: &str, (cx, cy): (f64, f64), d: f64, ink: &str, legend_y: f64) {
    print_scale(b, p, scale, (cx, cy), d, ink);
    let mut o: Vec<String> = vec!["ticks=none".into()];
    o.extend(opts.iter().map(ToString::to_string));
    let place = Place::new(name).size(if preset == "amp_pointer" { d * KNOB_SCALE } else { d * 1.2 }).variant(preset).bind(rk::bind(addr)).touch(surface_kit::TOUCH_MIN).ink(surface_kit::Ink::Color(ink.into()));
    let place = Place { options: Some(o), ..place };
    let k = parts::put(b, p, super::knob_id("knob.3d"), &place, cx, cy);
    parts::strip_legend(b, k);
    legend(b, k, &name.to_uppercase(), (cx, legend_y), Letter::new(10.5, 760.0, ink).track(0.06));
}

/// A pointer knob's footprint over its boss diameter: the fin runs out past
/// the boss to the scale.
const KNOB_SCALE: f64 = 1.4;

/// The black pointer knob (Hex Rhythm, Cube Crunch): a black dome crossed
/// by a fin.
const BLACK_POINTER: &[&str] = &["color=#141416", "skirt=none", "cap_r=0.8", "height=0.8", "rough=0.3", "coat=0.5", "fallback_color=#1c1c1e", "pointer=#f2f0ea"];
/// The cream pointer knob (Croc Clean).
const CREAM_POINTER: &[&str] = &["color=#eeece6", "skirt=none", "cap_r=0.8", "height=0.8", "rough=0.42"];
/// Cage Lead's black spun cap, a white index.
const BLACK_SPUN: &[&str] = &["color=#34353a", "pointer=#f2f2f0", "skirt=none", "cap_r=0.96", "index_in=0.45", "fallback_color=#2c2d31"];

/// The input jack with its cable running off to the left and down.
fn jack(b: &mut Builder<'_>, root: NodeId, (cx, cy): (f64, f64)) {
    let cable = [(cx, cy), (cx - 8.0, cy + 30.0), (cx - 40.0, cy + 80.0), (cx - 90.0, cy + 110.0), (cx - 160.0, cy + 122.0)];
    rk::curve(b, root, "cable shadow", (W, H), &cable.iter().map(|(x, y)| (x + 2.0, y + 5.0)).collect::<Vec<_>>(), 12.0, "#00000080");
    rk::curve(b, root, "cable", (W, H), &cable, 10.0, "#161618");
    rk::curve(b, root, "cable sheen", (W, H), &cable.iter().map(|(x, y)| (x - 1.5, y - 1.5)).collect::<Vec<_>>(), 2.0, "#ffffff22");
    b.add(root, draw::circle("jack nut", cx, cy, 15.0, draw::conic(20.0, &[(0.0, "#e9e9e7"), (0.25, "#6f7072"), (0.5, "#d9d9d7"), (0.75, "#5a5b5d"), (1.0, "#e9e9e7")])));
    b.add(root, draw::circle("plug", cx, cy, 9.0, draw::radial(&[(0.0, "#3a3b3e"), (1.0, "#0b0b0c")])));
}

/// The player's scribble: an invented signature-like flourish.
fn scribble(b: &mut Builder<'_>, root: NodeId, (cx, cy): (f64, f64), color: &str) {
    let pts = [
        (cx - 26.0, cy + 12.0), (cx - 24.0, cy - 14.0), (cx - 22.0, cy + 8.0), (cx - 12.0, cy - 20.0), (cx - 8.0, cy + 10.0), (cx - 2.0, cy - 2.0), (cx + 4.0, cy + 8.0),
        (cx + 10.0, cy - 8.0), (cx + 12.0, cy + 9.0), (cx + 20.0, cy - 6.0), (cx + 24.0, cy + 14.0), (cx + 30.0, cy + 2.0),
    ];
    rk::curve(b, root, "scribble", (W, H), &pts, 1.6, color);
    rk::curve(b, root, "scribble", (W, H), &[(cx - 30.0, cy + 18.0), (cx, cy + 20.0), (cx + 34.0, cy + 15.0)], 1.3, color);
}

/// The red power rocker centred on (`cx`, `cy`).
fn power(b: &mut Builder<'_>, root: NodeId, addr: &str, (cx, cy): (f64, f64), h: f64) {
    let p = Place::new("Power").size(h).variant("red").bind(rk::bind(addr)).touch(surface_kit::TOUCH_MIN);
    parts::put(b, root, "switch.lamp_rocker", &p, cx, cy);
}

/// A thin vertical separator printed on the strip.
fn separator(b: &mut Builder<'_>, root: NodeId, x: f64, (y0, y1): (f64, f64), color: &str) {
    parts::vrule(b, root, x, y0, y1, 1.0, color);
}

fn params(ns: &str, knobs: &[(&str, &str, f64)], toggles: &[(&str, &str, bool)]) -> Vec<surface_proto::ParamInfo> {
    let mut v: Vec<surface_proto::ParamInfo> = knobs
        .iter()
        .map(|(a, n, d)| {
            let p = rk::dial10(&format!("{ns}/{a}"), n, *d);
            if matches!(*a, "gain" | "master" | "output") { p.featured() } else { p }
        })
        .collect();
    for (a, n, on) in toggles {
        v.push(rk::toggle(&format!("{ns}/{a}"), n, *on));
    }
    v
}

// ── 05 · Hex Rhythm ──────────────────────────────────────────────────────

/// Hex Rhythm — Head, 1200 × 545 (the plugin's faceplate, 2.2 : 1). Black
/// leather-grain tolex, corner protectors, a black steel hex lattice over
/// the lit valves, a cream strip: GAIN · BASS MID TREBLE · RESONANCE
/// PRESENCE MASTER OUTPUT, black pointer knobs with 0–10.
fn hex_rhythm() -> Face {
    let ns = "mrhex";
    let knobs: [(&str, &str, f64); 8] = [("gain", "Gain", 6.0), ("bass", "Bass", 5.0), ("mid", "Mid", 5.0), ("treble", "Treble", 5.5), ("resonance", "Resonance", 5.0), ("presence", "Presence", 5.0), ("master", "Master", 5.0), ("output", "Output", 5.0)];
    let scene = canvas("Hex Rhythm", |b, root| {
        parts::fit_root(b, root, W, H);
        feet(b, root, (210.0, 1010.0), 510.0);
        strap_handle(b, root, (W / 2.0 + 10.0, 14.0, 300.0), "#1c1d1f");
        let cab = (84.0, 66.0, 1090.0, 446.0);
        covered(b, root, cab, &Material::Tolex { grain: Grain::Levant, color: "#232426".into(), scale: 1.0 }, 10.0);
        grain(b, root, cab, 0.06, (0.05, 0.25), 0x4e11);
        corners(b, root, cab);
        // The grille window: valves behind a hex lattice, a black frame round it.
        let g = (118.0, 100.0, 1022.0, 240.0);
        b.add(root, draw::with_effects(draw::rect("grille frame", g.0 - 6.0, g.1 - 6.0, g.2 + 12.0, g.3 + 12.0, draw::solid("#0b0b0c")), vec![draw::inner_shadow("#000000", 0.0, 2.0, 4.0)]));
        valve_row(b, root, g, 1.0);
        hex_lattice(b, root, g, 38.0, "#2a2b2f", 7.5);
        for (x, y) in [(g.0 + 3.0, g.1 + 3.0), (g.0 + g.2 - 3.0, g.1 + 3.0), (g.0 + 3.0, g.1 + g.3 - 3.0), (g.0 + g.2 - 3.0, g.1 + g.3 - 3.0)] {
            b.add(root, draw::circle("screw", x, y, 3.0, draw::radial(&[(0.0, "#8a8b8d"), (1.0, "#1a1a1c")])));
        }
        // The strip.
        let s = (118.0, 358.0, 1022.0, 124.0);
        strip(b, root, s, ("#ebe6da", "#dcd6c8"), false, 1);
        const INK: &str = "#1a1a1a";
        let xs = [245.0, 330.0, 429.0, 527.0, 623.0, 714.0, 809.0, 905.0];
        for ((a, n, _), x) in knobs.iter().zip(xs) {
            knob(b, root, "amp_pointer", BLACK_POINTER, Scale::Numerals, &format!("{ns}/{a}"), n, (x, 430.0), 34.0, INK, 390.0);
        }
        separator(b, root, 287.0, (386.0, 470.0), "#1a1a1a90");
        separator(b, root, 576.0, (386.0, 470.0), "#1a1a1a90");
        jack(b, root, (156.0, 422.0));
        power(b, root, &format!("{ns}/power"), (988.0, 424.0), 58.0);
        scribble(b, root, (1072.0, 424.0), "#1a1a1a");
    });
    let params = params(ns, &knobs, &[("power", "Power", true)]);
    Face { file: "05-amp-head-hex-rhythm", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

// ── 06 · Croc Clean ──────────────────────────────────────────────────────

/// Croc Clean — Head, 1200 × 545. Grey crocodile over the whole front, a
/// scratched steel strip: BRIGHT, then GAIN BASS MIDDLE TREBLE MASTER
/// OUTPUT on cream pointer knobs inside tick rings.
fn croc_clean() -> Face {
    let ns = "mrcroc";
    let knobs: [(&str, &str, f64); 6] = [("gain", "Gain", 4.0), ("bass", "Bass", 5.0), ("middle", "Middle", 5.0), ("treble", "Treble", 6.0), ("master", "Master", 5.0), ("output", "Output", 5.0)];
    let scene = canvas("Croc Clean", |b, root| {
        parts::fit_root(b, root, W, H);
        feet(b, root, (240.0, 1010.0), 505.0);
        // A straight bar handle with chrome end caps.
        let bar = draw::rounded(draw::rect("handle", 380.0, 22.0, 440.0, 10.0, draw::linear(180.0, &[(0.0, "#3a3b3e"), (1.0, "#0c0c0d")])), 5.0);
        b.add(root, draw::with_effects(bar, vec![draw::drop_shadow("#00000099", 0.0, 3.0, 4.0)]));
        for x in [355.0, 815.0] {
            b.add(root, draw::rounded(draw::rect("handle cap", x, 18.0, 110.0 * 0.3, 20.0, draw::linear(180.0, &[(0.0, "#e2e3e5"), (1.0, "#7c7d80")])), 3.0));
        }
        let cab = (100.0, 40.0, 1070.0, 470.0);
        covered(b, root, cab, &Material::Croc { color: "#4a4741".into(), scale: 1.25 }, 10.0);
        croc_scales(b, root, (cab.0 + 3.0, cab.1 + 3.0, cab.2 - 6.0, cab.3 - 6.0), (15.0, 21.0), 0xc40c);
        corners(b, root, cab);
        // The front's upper edge: a dark piping line.
        b.add(root, draw::stroked(draw::rect("front frame", 140.0, 76.0, 990.0, 290.0, draw::solid("#00000000")), "#1a1917", 3.0));
        let s = (135.0, 364.0, 1000.0, 124.0);
        strip(b, root, s, ("#5a5c5f", "#3a3c3f"), true, 0x5c7a);
        const INK: &str = "#f2f2f0";
        let xs = [357.0, 463.0, 568.0, 673.0, 777.0, 884.0];
        for ((a, n, _), x) in knobs.iter().zip(xs) {
            knob(b, root, "amp_pointer", CREAM_POINTER, Scale::Ticks(21), &format!("{ns}/{a}"), n, (x, 430.0), 40.0, INK, 384.0);
        }
        let t = rk::mini_toggle(b, root, "Bright", &format!("{ns}/bright"), (279.0, 430.0 - 6.0), 14.0, "chrome");
        legend(b, t, "BRIGHT", (279.0, 395.0), Letter::new(10.0, 760.0, INK).track(0.06));
        jack(b, root, (202.0, 426.0));
        power(b, root, &format!("{ns}/power"), (985.0, 424.0), 58.0);
        scribble(b, root, (1072.0, 424.0), "#f2f2f0");
    });
    let params = params(ns, &knobs, &[("bright", "Bright", false), ("power", "Power", true)]);
    Face { file: "06-amp-head-croc-clean", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

// ── 07 · Cube Crunch ─────────────────────────────────────────────────────

/// Cube Crunch — Head, 1200 × 545. Cream tolex with chrome corners and a
/// brown leather handle, an ivory lattice of tumbling cubes over pale
/// valves, a slate strip: BRIGHT, GAIN · BASS MID TREBLE · PRESENCE DEPTH
/// MASTER OUTPUT on black pointer knobs with 0–10.
fn cube_crunch() -> Face {
    let ns = "mrcube";
    let knobs: [(&str, &str, f64); 8] = [("gain", "Gain", 5.0), ("bass", "Bass", 5.0), ("mid", "Mid", 5.0), ("treble", "Treble", 5.0), ("presence", "Presence", 5.0), ("depth", "Depth", 5.0), ("master", "Master", 5.0), ("output", "Output", 5.0)];
    let scene = canvas("Cube Crunch", |b, root| {
        parts::fit_root(b, root, W, H);
        feet(b, root, (200.0, 1030.0), 508.0);
        strap_handle(b, root, (W / 2.0 - 25.0, 10.0, 300.0), "#9c5a3a");
        let cab = (84.0, 62.0, 1090.0, 448.0);
        covered(b, root, cab, &Material::Tolex { grain: Grain::Bronco, color: "#e4dece".into(), scale: 1.0 }, 10.0);
        grain(b, root, cab, 0.05, (0.2, 0.08), 0xc0be);
        // Chrome corners instead of black.
        for (cx, cy) in [(cab.0 + 6.0, cab.1 + 6.0), (cab.0 + cab.2 - 6.0, cab.1 + 6.0), (cab.0 + 6.0, cab.1 + cab.3 - 6.0), (cab.0 + cab.2 - 6.0, cab.1 + cab.3 - 6.0)] {
            b.add(root, draw::with_effects(draw::circle("corner", cx, cy, 11.0, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#ffffff"), (0.5, "#a5a6a8"), (1.0, "#4b4c4e")])), vec![draw::drop_shadow("#00000080", 0.0, 1.0, 2.0)]));
        }
        let g = (120.0, 96.0, 1018.0, 244.0);
        b.add(root, draw::rect("grille back", g.0, g.1, g.2, g.3, draw::solid("#0b0b0b")));
        valve_row(b, root, g, 0.35);
        // The lattice itself, ivory, with its own faint frosting over the valves.
        cube_lattice(b, root, g, 30.0, "#ece9e1", 5.0);
        b.add(root, draw::stroked(draw::rect("grille edge", g.0, g.1, g.2, g.3, draw::solid("#00000000")), "#c9c4b6", 3.0));
        let s = (120.0, 360.0, 1018.0, 124.0);
        strip(b, root, s, ("#44474b", "#2e3134"), true, 0xc7);
        const INK: &str = "#f0f0ee";
        let xs = [301.0, 391.0, 476.0, 561.0, 651.0, 741.0, 815.0, 914.0];
        for ((a, n, _), x) in knobs.iter().zip(xs) {
            knob(b, root, "amp_pointer", BLACK_POINTER, Scale::Numerals, &format!("{ns}/{a}"), n, (x, 428.0), 30.0, INK, 384.0);
        }
        separator(b, root, 346.0, (380.0, 465.0), "#f0f0ee70");
        separator(b, root, 608.0, (380.0, 465.0), "#f0f0ee70");
        let t = rk::mini_toggle(b, root, "Bright", &format!("{ns}/bright"), (227.0, 430.0 - 6.0), 14.0, "chrome");
        legend(b, t, "BRIGHT", (227.0, 392.0), Letter::new(9.5, 760.0, INK).track(0.06));
        jack(b, root, (170.0, 422.0));
        power(b, root, &format!("{ns}/power"), (1000.0, 424.0), 56.0);
        scribble(b, root, (1080.0, 424.0), "#f0f0ee");
    });
    let params = params(ns, &knobs, &[("bright", "Bright", false), ("power", "Power", true)]);
    Face { file: "07-amp-head-cube-crunch", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

// ── 08 · Cage Lead ───────────────────────────────────────────────────────

/// Cage Lead — Head, 1200 × 545. A black perforated steel cage over four
/// glowing valves, a bent-wire handle, a black scratched strip: GAIN BASS
/// MIDDLE TREBLE MASTER OUTPUT on black spun-cap knobs in tick rings.
fn cage_lead() -> Face {
    let ns = "mrcage";
    let knobs: [(&str, &str, f64); 6] = [("gain", "Gain", 7.0), ("bass", "Bass", 5.0), ("middle", "Middle", 5.0), ("treble", "Treble", 5.0), ("master", "Master", 5.0), ("output", "Output", 5.0)];
    let scene = canvas("Cage Lead", |b, root| {
        parts::fit_root(b, root, W, H);
        feet(b, root, (270.0, 1000.0), 505.0);
        // The bent-wire handle.
        let wire = [(450.0, 92.0), (450.0, 30.0), (455.0, 24.0), (770.0, 24.0), (775.0, 30.0), (775.0, 92.0)];
        parts::line(b, root, "handle shadow", &wire.iter().map(|(x, y)| (x + 2.0, y + 3.0)).collect::<Vec<_>>(), 6.0, "#00000080");
        parts::line(b, root, "handle", &wire, 5.0, "#2e2f31");
        parts::line(b, root, "handle sheen", &wire.iter().map(|(x, y)| (x - 1.0, y - 1.0)).collect::<Vec<_>>(), 1.2, "#8a8b8d");
        let cab = (118.0, 90.0, 1005.0, 418.0);
        covered(b, root, cab, &Material::Powder { color: "#232427".into(), coarse: true }, 6.0);
        grain(b, root, cab, 0.08, (0.06, 0.12), 0xca9e);
        let g = (126.0, 98.0, 989.0, 262.0);
        valve_row(b, root, g, 1.0);
        perforated(b, root, g, 17.0, 5.6);
        // A folded lip where the cage meets the strip.
        b.add(root, draw::rect("lip", 118.0, 358.0, 1005.0, 8.0, draw::linear(180.0, &[(0.0, "#3b3c3f"), (1.0, "#0e0f10")])));
        let s = (118.0, 366.0, 1005.0, 132.0);
        strip(b, root, s, ("#2a2b2e", "#17181a"), true, 0xca9e);
        const INK: &str = "#eeeeec";
        let xs = [277.0, 395.0, 513.0, 632.0, 752.0, 871.0];
        for ((a, n, _), x) in knobs.iter().zip(xs) {
            knob(b, root, "spun_metal", BLACK_SPUN, Scale::Ticks(21), &format!("{ns}/{a}"), n, (x, 444.0), 44.0, INK, 400.0);
        }
        jack(b, root, (178.0, 432.0));
        power(b, root, &format!("{ns}/power"), (962.0, 440.0), 56.0);
        scribble(b, root, (1050.0, 440.0), "#eeeeec");
    });
    let params = params(ns, &knobs, &[("power", "Power", true)]);
    Face { file: "08-amp-head-cage-lead", plugin: PLUGIN, scene, params, streams: Vec::new() }
}
