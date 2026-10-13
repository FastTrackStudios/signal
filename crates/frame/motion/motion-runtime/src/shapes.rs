//! Shape providers: geometry computed from numbers.
//!
//! A `Target::Shape { provider, part, inputs }` motion replaces its node's
//! outline with `provider(inputs + value)` each time an input changes (the
//! result is memoised). Coordinates are the node's own box, so a provider
//! node usually spans the artwork it redraws. Builtins:
//!
//! | provider | parts | inputs |
//! |---|---|---|
//! | `coil` | `back`, `front`, `light` (the lit edge of the front turns) | `x0`, `x1`, `y`, `r` (half height), `pitch` (5.5), `bend` (px sideways at the middle), `ripple` (px, second mode), `squeeze` (px along the axis), `phase` (turns) |
//! | `reverb.room` | `back`, `outline`, `floor`, `edges`, `boards`, `speaker`, `cone`, `direct`, `rays`, `echoes`, `bounces`, `taps`, `tap_heads` | `size`, `decay`, `diffusion` (0..1) |
//! | `wave` | — | `cycles` (2), `amp` (0..1 of the half height, 0.8), `phase` (turns), `shape` (0 sine, 1 triangle, 2 saw, 3 square) |
//!
//! A host registers its own (an engine's exact room model) with
//! [`ShapeRegistry::register`]; a registration under a builtin's name
//! overrides it.

use std::collections::BTreeMap;
use std::f64::consts::{PI, TAU};
use std::fmt::Write as _;

/// What a provider is asked for.
#[derive(Debug)]
pub struct ShapeCtx<'a> {
    pub part: &'a str,
    /// The node's box.
    pub width: f64,
    pub height: f64,
    /// Named inputs (plus `value`, the motion's output).
    pub args: &'a [(&'a str, f64)],
}

impl ShapeCtx<'_> {
    /// Input `name`, or `default`.
    #[must_use]
    pub fn get(&self, name: &str, default: f64) -> f64 {
        self.args.iter().find(|(k, _)| *k == name).map_or(default, |(_, v)| *v)
    }
}

/// Outlines (SVG path data, node-local) for a context; `None` = unknown part.
pub type ShapeFn = fn(&ShapeCtx<'_>) -> Option<Vec<String>>;

/// Providers by name.
#[derive(Debug, Clone, Default)]
pub struct ShapeRegistry {
    map: BTreeMap<String, ShapeFn>,
}

impl ShapeRegistry {
    /// The builtin providers.
    #[must_use]
    pub fn builtin() -> Self {
        let mut r = Self::default();
        r.register("coil", coil);
        r.register("reverb.room", room);
        r.register("wave", wave);
        r
    }

    pub fn register(&mut self, name: impl Into<String>, f: ShapeFn) {
        self.map.insert(name.into(), f);
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<ShapeFn> {
        self.map.get(name).copied()
    }

    /// Registered names.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.map.keys().map(String::as_str)
    }
}

/// The builtin provider names (for lint without a runtime).
pub const BUILTIN: &[&str] = &["coil", "reverb.room", "wave"];

// ── path helpers ─────────────────────────────────────────────────────────

fn polylines(lines: &[Vec<(f64, f64)>]) -> String {
    let mut d = String::new();
    for line in lines {
        for (i, (x, y)) in line.iter().enumerate() {
            let _ = write!(d, "{} {x:.3} {y:.3} ", if i == 0 { "M" } else { "L" });
        }
    }
    d.truncate(d.trim_end().len());
    d
}

fn closed(pts: &[(f64, f64)]) -> String {
    let mut d = polylines(&[pts.to_vec()]);
    d.push_str(" Z");
    d
}

fn circles(cs: &[(f64, f64, f64)]) -> String {
    let mut d = String::new();
    for (cx, cy, r) in cs {
        let _ = write!(d, "M {:.3} {cy:.3} A {r:.3} {r:.3} 0 1 0 {:.3} {cy:.3} A {r:.3} {r:.3} 0 1 0 {:.3} {cy:.3} Z ", cx - r, cx + r, cx - r);
    }
    d.truncate(d.trim_end().len());
    d
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn lerp2(a: (f64, f64), b: (f64, f64), t: f64) -> (f64, f64) {
    (lerp(a.0, b.0, t), lerp(a.1, b.1, t))
}

fn fract(x: f64) -> f64 {
    x - x.floor()
}

/// A fixed pseudo-random 0..1 for `i`.
fn jitter(i: f64) -> f64 {
    fract(((i * 12.9898).sin() * 43_758.545_3).abs())
}

// ── coil ─────────────────────────────────────────────────────────────────

/// A spring seen from the side: loops of a helix between `x0..x1` at `y`,
/// displaced by its modes (`bend`: the fundamental, sideways; `ripple`: the
/// second, travelling with `phase`; `squeeze`: along the axis). The ends
/// stay on their hooks.
fn coil(c: &ShapeCtx<'_>) -> Option<Vec<String>> {
    let x0 = c.get("x0", 0.0);
    let x1 = c.get("x1", c.width);
    let y = c.get("y", c.height / 2.0);
    let r = c.get("r", c.height / 2.0 * 0.8);
    let pitch = c.get("pitch", 5.5).max(0.5);
    let bend = c.get("bend", 0.0);
    let ripple = c.get("ripple", 0.0);
    let squeeze = c.get("squeeze", 0.0);
    let phase = c.get("phase", 0.0);
    let n = ((x1 - x0) / pitch).max(0.0) as usize;
    let span = (x1 - x0).max(1e-9);
    let (from, to, lift) = match c.part {
        "back" => (PI, TAU, 0.0),
        "front" => (0.0, PI, 0.0),
        "light" => (0.0, PI, -0.45),
        _ => return None,
    };
    let mut lines = Vec::with_capacity(n);
    for i in 0..n {
        let cx0 = x0 + pitch * (i as f64 + 0.5);
        let u = (cx0 - x0) / span;
        let env = (PI * u).sin();
        let cx = cx0 + squeeze * (TAU * u).sin() * env;
        let dy = bend * env + ripple * (TAU * (2.0 * u - phase)).sin() * env;
        let pts: Vec<(f64, f64)> = (0..=10)
            .map(|k| {
                let t = from + (to - from) * f64::from(k) / 10.0;
                (cx + pitch * 0.55 * t.cos() + pitch * 0.35 * t.sin(), y + dy + r * t.sin() + lift)
            })
            .collect();
        lines.push(pts);
    }
    Some(vec![polylines(&lines)])
}

// ── reverb.room ──────────────────────────────────────────────────────────

/// The room box's geometry for `size`: front opening, back wall.
struct Room {
    front: (f64, f64, f64, f64),
    back: (f64, f64, f64, f64),
    src: (f64, f64),
    ear: (f64, f64),
}

impl Room {
    fn new(h: f64, size: f64) -> Self {
        let front = (22.0, 14.0, 318.0, h - 14.0);
        let (cx, cy) = ((front.0 + front.2) / 2.0, (front.1 + front.3) / 2.0 - 2.0);
        // Bigger rooms recede further: the back wall shrinks toward the
        // vanishing point.
        let k = 1.0 - 0.75 * (0.2 + 0.8 * size.clamp(0.0, 1.0));
        let at = |x: f64, y: f64| (cx + (x - cx) * k, cy + (y - cy) * k);
        let (b0, b1) = (at(front.0, front.1), at(front.2, front.3));
        let back = (b0.0, b0.1, b1.0, b1.1);
        let src = (lerp(back.0, back.2, 0.2), back.3 - 10.0 * k.max(0.4) / 0.58);
        let ear = (front.0 + (front.2 - front.0) * 0.6, front.3 - 16.0);
        Self { front, back, src, ear }
    }

    /// Reflection point `i` on the walls (left, ceiling, right, back in
    /// turn), placed by fixed irrational steps so it's stable per `i`.
    fn bounce(&self, i: usize) -> (f64, f64) {
        let (f, b) = (self.front, self.back);
        let fi = i as f64;
        let a = 0.15 + 0.7 * fract(0.31 + fi * 0.618_034);
        let depth = 0.2 + 0.6 * fract(0.17 + fi * 0.381_966);
        match i % 4 {
            0 => lerp2((f.0, lerp(f.1, f.3, a)), (b.0, lerp(b.1, b.3, a)), depth),
            1 => lerp2((lerp(f.0, f.2, a), f.1), (lerp(b.0, b.2, a), b.1), depth),
            2 => lerp2((f.2, lerp(f.1, f.3, a)), (b.2, lerp(b.1, b.3, a)), depth),
            _ => (lerp(b.0, b.2, a), lerp(b.1, b.3, 0.15 + 0.5 * depth)),
        }
    }
}

/// How many first reflections `diffusion` (0..1) draws.
fn reflections(diffusion: f64) -> usize {
    2 + (diffusion.clamp(0.0, 1.0) * 6.0).round() as usize
}

/// The early-reflection taps for the diagram: (ms, level 0..1), the
/// direct sound first. Spacing grows with `size`, density with
/// `diffusion`, and levels fall slower with `decay`.
#[must_use]
pub fn room_taps(size: f64, decay: f64, diffusion: f64) -> Vec<(f64, f64)> {
    let n = 5 + (diffusion.clamp(0.0, 1.0) * 10.0).round() as usize;
    let spacing = 2.5 + 8.0 * size.clamp(0.0, 1.0);
    let tau = 12.0 + 70.0 * decay.clamp(0.0, 1.0);
    let mut taps = vec![(0.0, 1.0)];
    for j in 1..n {
        let fj = j as f64;
        let t = spacing * fj.powf(1.12) * (0.8 + 0.4 * jitter(fj + 3.0));
        if t > 78.0 {
            break;
        }
        let a = (-t / tau).exp() * (0.7 + 0.3 * jitter(fj + 51.0));
        taps.push((t, a.clamp(0.02, 1.0)));
    }
    taps
}

/// A studio room in perspective, redrawn from its params: the back wall
/// recedes with size, the reflection paths re-trace off the walls (their
/// count follows diffusion, the second-order ones follow decay), and the
/// tap diagram beside it spreads with size and lengthens with decay.
fn room(c: &ShapeCtx<'_>) -> Option<Vec<String>> {
    let (w, h) = (c.width, c.height);
    let size = c.get("size", 0.45);
    let decay = c.get("decay", 0.4);
    let diffusion = c.get("diffusion", 0.6);
    let r = Room::new(h, size);
    let (f, b) = (r.front, r.back);
    let n = reflections(diffusion);
    let d = match c.part {
        "back" => closed(&[(b.0, b.1), (b.2, b.1), (b.2, b.3), (b.0, b.3)]),
        "outline" => polylines(&[vec![(b.0, b.1), (b.2, b.1), (b.2, b.3), (b.0, b.3), (b.0, b.1)]]),
        "floor" => closed(&[(b.0, b.3), (b.2, b.3), (f.2, f.3), (f.0, f.3)]),
        "edges" => polylines(&[
            vec![(f.0, f.1), (b.0, b.1)],
            vec![(f.2, f.1), (b.2, b.1)],
            vec![(f.0, f.3), (b.0, b.3)],
            vec![(f.2, f.3), (b.2, b.3)],
        ]),
        "boards" => polylines(
            &(1..8)
                .map(|i| {
                    let t = f64::from(i) / 8.0;
                    vec![(lerp(b.0, b.2, t), b.3), (lerp(f.0, f.2, t), f.3)]
                })
                .collect::<Vec<_>>(),
        ),
        "speaker" => closed(&[(r.src.0 - 7.0, r.src.1 - 12.0), (r.src.0 + 7.0, r.src.1 - 12.0), (r.src.0 + 7.0, r.src.1 + 8.0), (r.src.0 - 7.0, r.src.1 + 8.0)]),
        "cone" => circles(&[(r.src.0, r.src.1 + 1.0, 4.5)]),
        "direct" => polylines(&[vec![r.src, r.ear]]),
        "rays" => polylines(&(0..n).map(|i| vec![r.src, r.bounce(i), r.ear]).collect::<Vec<_>>()),
        "echoes" => {
            let m = (decay.clamp(0.0, 1.0) * n as f64).round() as usize;
            if m == 0 {
                String::from("M 0 0")
            } else {
                polylines(&(0..m).map(|i| vec![r.src, r.bounce(i), r.bounce((i + 2) % n), r.ear]).collect::<Vec<_>>())
            }
        }
        "bounces" => circles(&(0..n).map(|i| {
            let p = r.bounce(i);
            (p.0, p.1, 2.2)
        }).collect::<Vec<_>>()),
        "taps" | "tap_heads" => {
            let (x0, x1, base, top) = (352.0, w - 20.0, h - 30.0, 26.0);
            let taps = room_taps(size, decay, diffusion);
            let at = |ms: f64, a: f64| (x0 + (x1 - x0) * ms / 80.0, base - (base - top) * a);
            if c.part == "taps" {
                polylines(&taps.iter().map(|(ms, a)| vec![(at(*ms, 0.0).0, base), at(*ms, *a)]).collect::<Vec<_>>())
            } else {
                circles(&taps.iter().map(|(ms, a)| {
                    let p = at(*ms, *a);
                    (p.0, p.1, 1.8)
                }).collect::<Vec<_>>())
            }
        }
        _ => return None,
    };
    Some(vec![d])
}

// ── wave ─────────────────────────────────────────────────────────────────

/// A periodic wave across the box: `cycles`, `amp` of the half height,
/// `phase` in turns, `shape` 0 sine / 1 triangle / 2 saw / 3 square.
fn wave(c: &ShapeCtx<'_>) -> Option<Vec<String>> {
    let (w, h) = (c.width, c.height);
    let cycles = c.get("cycles", 2.0);
    let amp = c.get("amp", 0.8) * h / 2.0;
    let phase = c.get("phase", 0.0);
    let shape = c.get("shape", 0.0).round() as i32;
    let n = ((w / 2.0).ceil() as usize).clamp(16, 400);
    let f = |p: f64| {
        let p = fract(p);
        match shape {
            1 => 1.0 - 4.0 * (p - 0.5).abs(),
            2 => 2.0 * p - 1.0,
            3 => {
                if p < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            _ => (TAU * p).sin(),
        }
    };
    let pts: Vec<(f64, f64)> = (0..=n)
        .map(|i| {
            let u = i as f64 / n as f64;
            (u * w, h / 2.0 - amp * f(u * cycles + phase))
        })
        .collect();
    Some(vec![polylines(&[pts])])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx<'a>(part: &'a str, args: &'a [(&'a str, f64)]) -> ShapeCtx<'a> {
        ShapeCtx { part, width: 560.0, height: 140.0, args }
    }

    #[test]
    fn coil_ends_stay_put_and_the_middle_bends() {
        let still = coil(&ctx("front", &[("x0", 100.0), ("x1", 300.0), ("y", 50.0), ("r", 10.0)])).unwrap();
        let bent = coil(&ctx("front", &[("x0", 100.0), ("x1", 300.0), ("y", 50.0), ("r", 10.0), ("bend", 6.0)])).unwrap();
        assert_ne!(still, bent);
        let ys = |d: &str| d.split_whitespace().filter_map(|t| t.parse::<f64>().ok()).skip(1).step_by(2).collect::<Vec<_>>();
        let (s, b) = (ys(&still[0]), ys(&bent[0]));
        assert!((s[0] - b[0]).abs() < 0.5, "first loop barely moves");
        let mid = s.len() / 2;
        assert!((b[mid] - s[mid] - 6.0).abs() < 0.2, "middle bends by `bend`: {} vs {}", b[mid], s[mid]);
        assert!(coil(&ctx("nope", &[])).is_none());
    }

    #[test]
    fn room_follows_its_params() {
        let back = |size: f64| room(&ctx("back", &[("size", size)])).unwrap()[0].clone();
        assert_ne!(back(0.1), back(0.9));
        let count = |part: &str, diffusion: f64| room(&ctx(part, &[("diffusion", diffusion)])).unwrap()[0].matches('M').count();
        assert_eq!(count("rays", 0.0), 2);
        assert_eq!(count("rays", 1.0), 8);
        assert_eq!(count("bounces", 0.5), 5);
        let taps = |decay: f64| room_taps(0.5, decay, 0.6);
        let (short, long) = (taps(0.0), taps(1.0));
        assert_eq!(short.len(), long.len());
        assert!(long.iter().zip(&short).skip(1).all(|(l, s)| l.1 > s.1), "longer decay, higher taps");
        assert!(room_taps(1.0, 0.5, 0.6).last().unwrap().0 > room_taps(0.0, 0.5, 0.6).last().unwrap().0 || room_taps(1.0, 0.5, 0.6).len() < room_taps(0.0, 0.5, 0.6).len());
    }

    #[test]
    fn registry_overrides_builtins() {
        fn mine(_: &ShapeCtx<'_>) -> Option<Vec<String>> {
            Some(vec!["M0 0".into()])
        }
        let mut r = ShapeRegistry::builtin();
        assert_eq!(r.names().collect::<Vec<_>>(), BUILTIN);
        r.register("coil", mine);
        assert_eq!(r.get("coil").unwrap()(&ctx("front", &[])).unwrap(), ["M0 0"]);
    }
}
