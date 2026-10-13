//! Pixel textures, generated in code: small tileable noise, cell and weave
//! tiles encoded as PNG assets and laid over a surface as one repeating
//! image fill. No photographs — every tile is a function of its parameters and
//! a seed, so a build is reproducible and an asset is stored once per
//! document (assets are content-addressed).
//!
//! Two kinds of tile:
//!
//! - **relief** ([`Relief`]): a height field (pebbles, dimples, wrinkles,
//!   pores, brush streaks) shaded by the kit's light ([`super::light`]) and
//!   stored as *white where lit, black where shaded*, with alpha the
//!   strength — an overlay that keeps the paint's own hue underneath;
//! - **colour** ([`Weave`]): a periodic pattern in its own colours (grille
//!   cloth, perforation, expanded mesh) that *is* the surface.
//!
//! A surface is covered by one rect whose image paint is a `Tile`, scaled to
//! the tile's design size: the renderer repeats it ([`TileFill::Repeat`],
//! the default). [`TileFill::Grid`] instead lays a clipped frame of
//! `cols × rows` rects, one per tile (capped at [`MAX_TILES`]; past that the
//! tile is drawn larger), for a renderer that can't repeat an image. Either
//! way every tile shares one asset, which the renderer decodes once.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use document_proto::{Asset, AssetId, BlendMode, ImageScaleMode, NodeId, Op, Paint, PaintKind, Transform};
use surface_widgets::Builder;

use crate::controls::parts::fixed;
use crate::draw;

/// Most image rects one textured surface may use ([`TileFill::Grid`]).
pub const MAX_TILES: usize = 48;

/// How a texture tile covers a surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TileFill {
    /// One rect with a repeating (`Tile`) image fill: one node whatever the
    /// size, and the tile always at its design size.
    #[default]
    Repeat,
    /// A grid of rects, one per tile, for renderers that don't repeat
    /// images (the tile grows past [`MAX_TILES`] rects).
    Grid,
}

// ── deterministic randomness ────────────────────────────────────────────

/// SplitMix64: small, fast, and the same everywhere.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self(seed ^ 0x9e37_79b9_7f4a_7c15)
    }

    pub fn u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in `0..1`.
    pub fn f(&mut self) -> f64 {
        (self.u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn hash2(x: i64, y: i64, seed: u64) -> u64 {
    let mut r = Rng::new(seed ^ (x as u64).wrapping_mul(0x51_7cc1_b727_220a) ^ (y as u64).wrapping_mul(0x2545_f491_4f6c_dd1d));
    r.u64()
}

fn unit(h: u64) -> f64 {
    (h >> 11) as f64 / (1u64 << 53) as f64
}

// ── periodic fields (all wrap at the tile's edge) ───────────────────────

/// A field over a `n × n` tile, row-major.
#[derive(Debug, Clone)]
pub struct Field {
    pub n: usize,
    pub v: Vec<f64>,
}

impl Field {
    #[must_use]
    pub fn new(n: usize) -> Self {
        Self { n, v: vec![0.0; n * n] }
    }

    #[must_use]
    pub fn at(&self, x: i64, y: i64) -> f64 {
        let n = self.n as i64;
        self.v[(y.rem_euclid(n) * n + x.rem_euclid(n)) as usize]
    }

    /// Add `other · k`.
    pub fn add(&mut self, other: &Self, k: f64) {
        for (a, b) in self.v.iter_mut().zip(&other.v) {
            *a += b * k;
        }
    }

    /// Rescale to `-1..1` about the mean.
    pub fn normalise(&mut self) {
        let mean = self.v.iter().sum::<f64>() / self.v.len() as f64;
        let dev = self.v.iter().map(|v| (v - mean).abs()).fold(0.0, f64::max).max(1e-9);
        for v in &mut self.v {
            *v = (*v - mean) / dev;
        }
    }
}

fn smooth(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

/// Periodic value noise with `cells` lattice cells across the tile (x and y
/// independently, for streaks: `cx` ≪ `cy` stretches features along x).
#[must_use]
pub fn value_noise(n: usize, cx: usize, cy: usize, seed: u64) -> Field {
    let mut f = Field::new(n);
    let (cx, cy) = (cx.max(1) as i64, cy.max(1) as i64);
    for y in 0..n {
        for x in 0..n {
            let fx = x as f64 / n as f64 * cx as f64;
            let fy = y as f64 / n as f64 * cy as f64;
            let (ix, iy) = (fx.floor() as i64, fy.floor() as i64);
            let (tx, ty) = (smooth(fx - ix as f64), smooth(fy - iy as f64));
            let g = |i: i64, j: i64| unit(hash2(i.rem_euclid(cx), j.rem_euclid(cy), seed));
            let a = g(ix, iy) + (g(ix + 1, iy) - g(ix, iy)) * tx;
            let b = g(ix, iy + 1) + (g(ix + 1, iy + 1) - g(ix, iy + 1)) * tx;
            f.v[y * n + x] = a + (b - a) * ty;
        }
    }
    f
}

/// Fractal value noise: `octaves` of [`value_noise`], each twice as fine.
#[must_use]
pub fn fbm(n: usize, cells: usize, octaves: u32, seed: u64) -> Field {
    let mut f = Field::new(n);
    let mut amp = 1.0;
    for o in 0..octaves {
        let c = cells << o;
        if c > n {
            break;
        }
        f.add(&value_noise(n, c, c, seed.wrapping_add(u64::from(o) * 7919)), amp);
        amp *= 0.5;
    }
    f.normalise();
    f
}

/// Periodic cellular (Worley) noise: `(F1, F2)` distances, in cell units,
/// for `cells × cells` jittered points across the tile.
#[must_use]
pub fn cellular(n: usize, cells: usize, jitter: f64, seed: u64) -> (Field, Field) {
    let c = cells.max(1) as i64;
    let pts: Vec<(f64, f64)> = (0..c * c)
        .map(|i| {
            let h = hash2(i % c, i / c, seed);
            let (a, b) = (unit(h), unit(h.rotate_left(29)));
            (0.5 + (a - 0.5) * jitter, 0.5 + (b - 0.5) * jitter)
        })
        .collect();
    let (mut f1, mut f2) = (Field::new(n), Field::new(n));
    for y in 0..n {
        for x in 0..n {
            let fx = x as f64 / n as f64 * c as f64;
            let fy = y as f64 / n as f64 * c as f64;
            let (ix, iy) = (fx.floor() as i64, fy.floor() as i64);
            let (mut d1, mut d2) = (9.0_f64, 9.0_f64);
            for j in -1..=1 {
                for i in -1..=1 {
                    let (gx, gy) = (ix + i, iy + j);
                    let p = pts[(gy.rem_euclid(c) * c + gx.rem_euclid(c)) as usize];
                    let d = (gx as f64 + p.0 - fx).hypot(gy as f64 + p.1 - fy);
                    if d < d1 {
                        d2 = d1;
                        d1 = d;
                    } else if d < d2 {
                        d2 = d;
                    }
                }
            }
            f1.v[y * n + x] = d1;
            f2.v[y * n + x] = d2;
        }
    }
    (f1, f2)
}

/// Shade a height field under the kit's light: the slope toward the light
/// is lit (+), away from it shaded (−). `relief` scales the slope.
#[must_use]
pub fn shade(h: &Field, relief: f64) -> Field {
    let (lx, ly) = super::light::toward_light();
    let mut out = Field::new(h.n);
    for y in 0..h.n as i64 {
        for x in 0..h.n as i64 {
            let dx = (h.at(x + 1, y) - h.at(x - 1, y)) * 0.5;
            let dy = (h.at(x, y + 1) - h.at(x, y - 1)) * 0.5;
            // A surface rising toward the light faces it.
            out.v[(y as usize) * h.n + x as usize] = -(dx * lx + dy * ly) * relief;
        }
    }
    out
}

// ── the tiles ────────────────────────────────────────────────────────────

/// A relief texture: a height field, shaded by the kit's light.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Relief {
    /// Tolex pebble (Levant): small rounded bumps, packed.
    Pebble { cells: usize },
    /// Coarse tolex (Bronco): larger, flatter-topped grains with a fine tooth.
    Bronco { cells: usize },
    /// Elephant tolex: ridged wrinkles.
    Wrinkle { cells: usize },
    /// A smooth vinyl or rubber: only a fine tooth.
    Tooth,
    /// Leather: soft mottling and pores.
    Leather,
    /// Croc: domed scales between creased seams.
    Scales { cells: usize },
    /// Powder coat: a sparkle of fine specks over faint orange peel.
    Powder { coarse: bool },
    /// Hammertone: shallow round dimples, hammered into the paint.
    Hammered { cells: usize },
    /// Brushed metal: streaks along x.
    Brushed,
    /// Wood fibre: long, faint streaks along x.
    Fibre,
}

impl Relief {
    fn key(self) -> String {
        format!("{self:?}")
    }

    fn field(self, n: usize) -> Field {
        match self {
            Self::Pebble { cells } => {
                let (f1, _) = cellular(n, cells, 0.9, 11);
                // A bump per cell: high in the middle, falling to the seams.
                let mut h = Field { n, v: f1.v.iter().map(|d| -(d * d)).collect() };
                h.normalise();
                let mut out = shade(&h, 2.2);
                out.add(&fbm(n, n / 8, 2, 12), 0.25);
                out
            }
            Self::Bronco { cells } => {
                let (f1, f2) = cellular(n, cells, 1.0, 21);
                let mut h = Field { n, v: f1.v.iter().zip(&f2.v).map(|(a, b)| ((b - a) * 3.0).min(1.0)).collect() };
                h.normalise();
                let mut out = shade(&h, 1.6);
                out.add(&fbm(n, n / 4, 1, 22), 0.35);
                out
            }
            Self::Wrinkle { cells } => {
                let mut r = fbm(n, cells, 3, 31);
                for v in &mut r.v {
                    *v = 1.0 - v.abs() * 2.0;
                }
                r.normalise();
                let mut out = shade(&r, 3.0);
                out.add(&fbm(n, n / 6, 2, 32), 0.2);
                out
            }
            Self::Tooth => fbm(n, n / 2, 1, 41),
            Self::Leather => {
                // Soft, irregular pebbling with creases between, and pores.
                let (f1, f2) = cellular(n, 30, 1.0, 51);
                let mut h = Field { n, v: f1.v.iter().zip(&f2.v).map(|(a, b)| ((b - a) * 2.5).min(1.0).sqrt()).collect() };
                h.add(&fbm(n, 12, 3, 52), 0.5);
                h.normalise();
                let mut out = shade(&h, 1.8);
                let (p1, _) = cellular(n, n / 8, 0.9, 53);
                out.add(&Field { n, v: p1.v.iter().map(|d| if *d < 0.14 { -0.8 } else { 0.0 }).collect() }, 0.3);
                out.add(&fbm(n, 3, 2, 54), 0.12);
                out
            }
            Self::Scales { cells } => {
                let (f1, f2) = cellular(n, cells, 0.8, 61);
                let mut h = Field { n, v: f1.v.iter().zip(&f2.v).map(|(a, b)| ((b - a) * 2.2).min(1.0).sqrt()).collect() };
                h.normalise();
                let mut out = shade(&h, 3.5);
                // The seams themselves are dark creases.
                let seams = Field { n, v: f1.v.iter().zip(&f2.v).map(|(a, b)| if b - a < 0.06 { -1.0 } else { 0.0 }).collect() };
                out.add(&seams, 0.6);
                out
            }
            Self::Powder { coarse } => {
                let mut rng = Rng::new(if coarse { 71 } else { 72 });
                let mut out = Field::new(n);
                for v in &mut out.v {
                    let r = rng.f();
                    *v = if r > 0.93 { 1.0 } else if r < 0.1 { -0.8 } else { (rng.f() - 0.5) * 0.5 };
                }
                if coarse {
                    let (f1, _) = cellular(n, n / 3, 1.0, 73);
                    out.add(&Field { n, v: f1.v.iter().map(|d| if *d < 0.2 { 0.6 } else { 0.0 }).collect() }, 1.0);
                }
                out.add(&shade(&fbm(n, 6, 3, 74), 5.0), 0.4);
                out
            }
            Self::Hammered { cells } => {
                let (f1, _) = cellular(n, cells, 0.85, 81);
                // A dimple per cell: lowest in the middle, a sharp rim at the seams.
                let mut h = Field { n, v: f1.v.iter().map(|d| d.powf(1.5)).collect() };
                h.normalise();
                let mut out = shade(&h, 4.0);
                out.add(&fbm(n, n / 16, 2, 82), 0.25);
                out
            }
            Self::Brushed => {
                let mut out = value_noise(n, 2, n, 91);
                out.add(&value_noise(n, 5, n / 2, 92), 0.6);
                out.add(&value_noise(n, 1, n / 8, 93), 0.5);
                let mut rng = Rng::new(94);
                for y in 0..n {
                    // The odd deeper scratch.
                    if rng.f() > 0.9 {
                        let k = (rng.f() - 0.5) * 1.6;
                        for x in 0..n {
                            out.v[y * n + x] += k;
                        }
                    }
                }
                out
            }
            Self::Fibre => {
                let mut out = value_noise(n, 3, n / 2, 101);
                out.add(&value_noise(n, 8, n / 4, 102), 0.5);
                out
            }
        }
    }

    /// The tile's pixels across (a power of two up to 256) and how many
    /// design px it covers.
    fn tile(self) -> (usize, f64) {
        match self {
            Self::Brushed | Self::Fibre => (256, 256.0),
            Self::Scales { .. } => (256, 192.0),
            Self::Leather => (192, 144.0),
            _ => (192, 128.0),
        }
    }
}

/// A colour texture: a periodic pattern in its own colours.
#[derive(Debug, Clone, PartialEq)]
pub enum Weave {
    /// Basketweave grille cloth: blocks of `threads` parallel threads
    /// alternating in direction, in two thread colours.
    Basket { warp: String, weft: String, gap: String, threads: usize },
    /// A twill (wheat, oxblood): diagonal ribs of `rib` px, two tones.
    Twill { a: String, b: String, gap: String, rib: usize },
    /// A fine plain weave (black, silver cloth).
    Plain { thread: String, gap: String },
    /// Round perforations of diameter `hole` on a staggered `pitch`.
    Perforated { metal: String, hole: f64, pitch: f64 },
    /// Expanded metal: a diamond mesh of strands.
    Expanded { metal: String, pitch: f64 },
}

impl Weave {
    fn key(&self) -> String {
        format!("{self:?}")
    }

    /// Pixels across and the design px they cover (a whole number of repeats).
    fn tile(&self) -> (usize, f64) {
        match self {
            Self::Perforated { pitch, .. } | Self::Expanded { pitch, .. } => {
                let reps = (96.0 / pitch).round().max(1.0);
                // The expanded diamond repeats every pitch across, 2 × pitch/√3 down; keep it square.
                let design = pitch * reps;
                ((design * 2.0).round() as usize, design)
            }
            _ => (192, 96.0),
        }
    }

    fn rgba(&self, n: usize, design: f64) -> Vec<[f64; 4]> {
        let px = design / n as f64; // design px per pixel
        let rgba = |c: &str| {
            let c = draw::hex(c);
            [c.r, c.g, c.b, c.a]
        };
        let lerp = |a: [f64; 4], b: [f64; 4], t: f64| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t, a[3] + (b[3] - a[3]) * t];
        let mut out = vec![[0.0; 4]; n * n];
        let jitter = fbm(n, 24, 2, 7);
        match self {
            Self::Basket { warp, weft, gap, threads } => {
                let (wa, wb, g) = (rgba(warp), rgba(weft), rgba(gap));
                let t = (*threads).max(2);
                // Threads 2 design px wide; a block is `t` threads square.
                let tw = 2.0;
                let block = tw * t as f64;
                for y in 0..n {
                    for x in 0..n {
                        let (dx, dy) = (x as f64 * px, y as f64 * px);
                        let (bx, by) = ((dx / block) as i64, (dy / block) as i64);
                        let horiz = (bx + by) % 2 == 0;
                        // Across the threads: a round profile, dark in the gap.
                        let across = if horiz { dy } else { dx } / tw;
                        let along = if horiz { dx } else { dy } / tw;
                        let prof = (across.fract() * std::f64::consts::PI).sin();
                        let base = if horiz { wa } else { wb };
                        let lit = prof * (0.85 + 0.15 * (along * 1.7).sin());
                        let j = jitter.v[y * n + x] * 0.08;
                        let c = lerp(g, base, (lit + j).clamp(0.0, 1.0).powf(0.7));
                        out[y * n + x] = c;
                    }
                }
            }
            Self::Twill { a, b, gap, rib } => {
                let (ca, cb, g) = (rgba(a), rgba(b), rgba(gap));
                let r = (*rib).max(2) as f64;
                for y in 0..n {
                    for x in 0..n {
                        let (dx, dy) = (x as f64 * px, y as f64 * px);
                        // Diagonal ribs, each made of short floats of thread.
                        let d = (dx + dy) / r;
                        let prof = (d.fract() * std::f64::consts::PI).sin();
                        let float = ((dx - dy) / 1.6).rem_euclid(1.0);
                        let tone = if (d.floor() as i64).rem_euclid(4) == 0 { cb } else { ca };
                        let lit = prof * (0.75 + 0.25 * (float * std::f64::consts::PI).sin());
                        let j = jitter.v[y * n + x] * 0.1;
                        out[y * n + x] = lerp(g, tone, (lit + j).clamp(0.0, 1.0).powf(0.6));
                    }
                }
            }
            Self::Plain { thread, gap } => {
                let (c, g) = (rgba(thread), rgba(gap));
                for y in 0..n {
                    for x in 0..n {
                        let (dx, dy) = (x as f64 * px / 1.5, y as f64 * px / 1.5);
                        let over = ((dx.floor() + dy.floor()) as i64) % 2 == 0;
                        let prof = if over { (dy.fract() * std::f64::consts::PI).sin() } else { (dx.fract() * std::f64::consts::PI).sin() };
                        let j = jitter.v[y * n + x] * 0.12;
                        out[y * n + x] = lerp(g, c, (prof * 0.9 + j).clamp(0.0, 1.0));
                    }
                }
            }
            Self::Perforated { metal, hole, pitch } => {
                let m = rgba(metal);
                let (lx, ly) = super::light::toward_light();
                let rows = (design / (pitch * 0.866)).round().max(1.0);
                let row_h = design / rows;
                for y in 0..n {
                    for x in 0..n {
                        let (dx, dy) = ((x as f64 + 0.5) * px, (y as f64 + 0.5) * px);
                        let row = (dy / row_h).floor();
                        let off = if (row as i64) % 2 == 0 { 0.0 } else { pitch / 2.0 };
                        let cx = ((dx - off) / pitch).floor() * pitch + off + pitch / 2.0;
                        let cy = row * row_h + row_h / 2.0;
                        let (ox, oy) = (dx - cx, dy - cy);
                        let d = ox.hypot(oy);
                        let r = hole / 2.0;
                        let aa = (px * 0.7).max(0.3);
                        let t = ((d - r) / aa + 0.5).clamp(0.0, 1.0);
                        // The hole's far wall (away from the light) catches it.
                        let wall = if d < r && d > r - 0.9 { ((-(ox * lx + oy * ly) / d.max(1e-6)).max(0.0)) * 0.55 } else { 0.0 };
                        let inside = [0.02 + wall * m[0], 0.02 + wall * m[1], 0.02 + wall * m[2], 0.92];
                        out[y * n + x] = lerp(inside, m, t);
                    }
                }
            }
            Self::Expanded { metal, pitch } => {
                let m = rgba(metal);
                let (w, h) = (*pitch, *pitch * 0.5);
                for y in 0..n {
                    for x in 0..n {
                        let (dx, dy) = ((x as f64 + 0.5) * px, (y as f64 + 0.5) * px);
                        // Distance to the diamond lattice's strands.
                        let u = (dx / w + dy / h).rem_euclid(1.0);
                        let v = (dx / w - dy / h).rem_euclid(1.0);
                        let (du, dv) = ((u - 0.5).abs(), (v - 0.5).abs());
                        let s = du.max(dv);
                        let strand = ((s - 0.36) / 0.06).clamp(0.0, 1.0);
                        // Strands are tilted: the face toward the light is lit.
                        let face = if du > dv { if u > 0.5 { 1.0 } else { 0.55 } } else if v > 0.5 { 0.8 } else { 0.45 };
                        let lit = [m[0] * face, m[1] * face, m[2] * face, 1.0];
                        out[y * n + x] = lerp([0.0, 0.0, 0.0, 0.9], lit, strand);
                    }
                }
            }
        }
        out
    }
}

// ── PNG ──────────────────────────────────────────────────────────────────

fn crc32(bytes: &[u8]) -> u32 {
    static TABLE: LazyLock<[u32; 256]> = LazyLock::new(|| {
        let mut t = [0u32; 256];
        for (i, e) in t.iter_mut().enumerate() {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 == 1 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
            }
            *e = c;
        }
        t
    });
    let mut c = 0xffff_ffffu32;
    for b in bytes {
        c = TABLE[((c ^ u32::from(*b)) & 0xff) as usize] ^ (c >> 8);
    }
    c ^ 0xffff_ffff
}

/// Encode 8-bit pixels (`channels`: 2 = grey + alpha, 4 = RGBA) as a PNG.
#[must_use]
pub fn png(w: usize, h: usize, channels: usize, px: &[u8]) -> Vec<u8> {
    let color_type = if channels == 2 { 4u8 } else { 6u8 };
    let mut raw = Vec::with_capacity((w * channels + 1) * h);
    for row in px.chunks(w * channels) {
        raw.push(0); // filter: none
        raw.extend_from_slice(row);
    }
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut chunk = |kind: &[u8; 4], data: &[u8]| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc32(&body).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, color_type, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &miniz_oxide::deflate::compress_to_vec_zlib(&raw, 9));
    chunk(b"IEND", &[]);
    out
}

/// PNG bytes per texture key, so a surface built many times encodes once.
type Tile = (Vec<u8>, f64);

static TILES: LazyLock<Mutex<HashMap<String, Tile>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

fn cached(key: String, make: impl FnOnce() -> Tile) -> Tile {
    if let Some(hit) = TILES.lock().ok().and_then(|m| m.get(&key).cloned()) {
        return hit;
    }
    let made = make();
    if let Ok(mut m) = TILES.lock() {
        m.insert(key, made.clone());
    }
    made
}

/// The relief tile as PNG bytes and the design px it covers. `gain` scales
/// its strength (alpha), `scale` its size on the surface.
#[must_use]
pub fn relief_png(r: Relief, gain: f64) -> (Vec<u8>, f64) {
    cached(format!("{}|{gain:.3}", r.key()), || {
        let (n, design) = r.tile();
        let f = r.field(n);
        let mut px = Vec::with_capacity(n * n * 2);
        for v in &f.v {
            let a = (v.abs() * gain).clamp(0.0, 1.0);
            px.push(if *v > 0.0 { 255 } else { 0 });
            px.push((a * 255.0).round() as u8);
        }
        (png(n, n, 2, &px), design)
    })
}

/// The colour tile as PNG bytes and the design px it covers.
#[must_use]
pub fn weave_png(w: &Weave) -> (Vec<u8>, f64) {
    cached(w.key(), || {
        let (n, design) = w.tile();
        let n = n.clamp(16, 320);
        let mut px = Vec::with_capacity(n * n * 4);
        for c in w.rgba(n, design) {
            for v in c {
                px.push((v.clamp(0.0, 1.0) * 255.0).round() as u8);
            }
        }
        (png(n, n, 4, &px), design)
    })
}

/// Store `bytes` in the document's asset table (deduplicated by content).
pub fn put_png(b: &mut Builder<'_>, bytes: Vec<u8>) -> AssetId {
    let asset = Asset::new("image/png", bytes);
    let id = asset.id.clone();
    if b.scene.asset(&id).is_none() {
        b.scene.apply(Op::PutAsset { asset }).expect("putting an asset cannot fail");
    }
    id
}

/// An image paint stretching the asset over its node's box.
#[must_use]
pub fn image_paint(asset: AssetId, opacity: f64) -> Paint {
    Paint {
        kind: PaintKind::Image { asset, source_ref: None, scale_mode: ImageScaleMode::Stretch, transform: None },
        opacity,
        blend: BlendMode::Normal,
        visible: true,
    }
}

/// Cover `(x, y, w, h)` under `parent` with `asset` as tiles of `tile`
/// design px, rounded by `radius` — as one repeating fill
/// ([`TileFill::Repeat`]). Returns the textured node.
pub fn tiles(b: &mut Builder<'_>, parent: NodeId, rect: (f64, f64, f64, f64), radius: f64, asset: &AssetId, tile: f64, opacity: f64) -> NodeId {
    tiles_with(b, parent, rect, radius, asset, tile, opacity, TileFill::default())
}

/// [`tiles`], laid as `fill` says. [`TileFill::Grid`] grows the tile so no
/// more than [`MAX_TILES`] rects are used and returns their clipping frame;
/// an asset that isn't a PNG falls back to it.
#[allow(clippy::too_many_arguments)]
pub fn tiles_with(
    b: &mut Builder<'_>,
    parent: NodeId,
    (x, y, w, h): (f64, f64, f64, f64),
    radius: f64,
    asset: &AssetId,
    tile: f64,
    opacity: f64,
    fill: TileFill,
) -> NodeId {
    let t = tile.max(8.0);
    let px = b.scene.asset(asset).and_then(|a| png_size(&a.bytes));
    if let (TileFill::Repeat, Some((pw, ph))) = (fill, px) {
        // The tile's pixels scaled to its design size, repeated.
        let (sx, sy) = (t / f64::from(pw.max(1)), t / f64::from(ph.max(1)));
        let paint = Paint {
            kind: PaintKind::Image {
                asset: asset.clone(),
                source_ref: None,
                scale_mode: ImageScaleMode::Tile,
                transform: Some(Transform { a: sx, b: 0.0, c: 0.0, d: sy, tx: 0.0, ty: 0.0 }),
            },
            opacity,
            blend: BlendMode::Normal,
            visible: true,
        };
        return b.add(parent, fixed(draw::rounded(draw::rect("texture", x, y, w, h, paint), radius)));
    }
    let mut t = t;
    while ((w / t).ceil() * (h / t).ceil()) as usize > MAX_TILES {
        t *= 1.25;
    }
    let mut clip = draw::rounded(draw::frame("texture", x, y, w, h), radius);
    clip.clips_content = true;
    let clip = b.add(parent, fixed(clip));
    let (cols, rows) = ((w / t).ceil() as usize, (h / t).ceil() as usize);
    for j in 0..rows {
        for i in 0..cols {
            let mut n = draw::rect("tile", i as f64 * t, j as f64 * t, t, t, image_paint(asset.clone(), opacity));
            n.layout_child.absolute = true;
            b.add(clip, n);
        }
    }
    clip
}

/// A PNG's pixel size, from its header.
fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let be = |i: usize| Some(u32::from_be_bytes(bytes.get(i..i + 4)?.try_into().ok()?));
    (bytes.starts_with(b"\x89PNG\r\n\x1a\n") && bytes.get(12..16) == Some(b"IHDR")).then(|| Some((be(16)?, be(20)?)))?
}

/// [`tiles`] of a relief texture: `gain` its strength, `scale` its size.
pub fn relief(b: &mut Builder<'_>, parent: NodeId, rect: (f64, f64, f64, f64), radius: f64, r: Relief, gain: f64, scale: f64) -> NodeId {
    let (bytes, design) = relief_png(r, 1.0);
    let id = put_png(b, bytes);
    tiles(b, parent, rect, radius, &id, design * scale, gain.clamp(0.0, 1.0))
}

/// [`tiles`] of a colour texture.
pub fn weave(b: &mut Builder<'_>, parent: NodeId, rect: (f64, f64, f64, f64), radius: f64, w: &Weave) -> NodeId {
    let (bytes, design) = weave_png(w);
    let id = put_png(b, bytes);
    tiles(b, parent, rect, radius, &id, design, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_wrap_at_the_tile_edge() {
        let f = fbm(64, 8, 3, 1);
        // Neighbours across the seam differ no more than inside the tile.
        let seam: f64 = (0..64).map(|y| (f.at(63, y) - f.at(0, y)).abs()).fold(0.0, f64::max);
        let inner: f64 = (0..64).map(|y| (f.at(31, y) - f.at(32, y)).abs()).fold(0.0, f64::max);
        assert!(seam < inner * 2.5 + 0.05, "seam {seam} vs inner {inner}");
    }

    #[test]
    fn a_png_has_a_valid_signature_and_chunks() {
        let p = png(2, 2, 2, &[0, 0, 255, 255, 0, 128, 255, 64]);
        assert_eq!(&p[..8], &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
        assert_eq!(&p[12..16], b"IHDR");
        assert_eq!(crc32(b"IEND"), 0xae42_6082);
    }

    #[test]
    fn png_size_reads_the_header() {
        assert_eq!(png_size(&png(3, 5, 2, &[0; 30])), Some((3, 5)));
        assert_eq!(png_size(b"GIF89a"), None);
    }

    #[test]
    fn tiles_are_small() {
        for r in [Relief::Pebble { cells: 40 }, Relief::Hammered { cells: 16 }, Relief::Brushed, Relief::Powder { coarse: false }] {
            let (bytes, _) = relief_png(r, 1.0);
            assert!(bytes.len() < 140_000, "{r:?}: {} bytes", bytes.len());
        }
    }
}
