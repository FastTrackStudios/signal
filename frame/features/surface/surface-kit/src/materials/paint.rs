//! The material paints: [`Material`] says what a surface is made of,
//! [`fill`] covers a box with it. Each is a base paint lit by the kit's
//! light ([`super::light`]), a texture (a pixel relief tile or a few vector
//! paths), and a sheen — a handful of nodes whatever the size.
//!
//! | material | how | nodes |
//! |---|---|---|
//! | tolex, leather, croc, powder, hammertone, rubber | base + relief tiles + sheen | 3 + tiles |
//! | brushed | base + streak tiles + anisotropic band | 4 + tiles |
//! | spun | conic sheen + ring paths | 4 |
//! | wood | base + figure paths + fibre tiles | 5 + tiles |
//! | grille cloth, perforated, expanded | colour tiles + recess | 2 + tiles |
//! | diamond, chevron grille | cloth tiles + embossed vector strands | 5 + tiles |
//! | chrome, nickel, plastic | gradient stack + hotspot | 3–4 |

use document_proto::NodeId;
use surface_widgets::Builder;

use super::light;
use super::texture::{self, Relief, Weave};
use crate::controls::parts::{alpha, fixed, lin, shade, tint};
use crate::draw;

/// Tolex grains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grain {
    /// Fine pebble (Levant) — Fender, Mesa.
    Levant,
    /// Coarse pebble (Bronco) — Dumble, Marshall.
    Bronco,
    /// Wrinkled (Elephant) — Marshall, Vox.
    Elephant,
    /// Smooth vinyl with a fine tooth.
    Fine,
}

/// Grille cloths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cloth {
    /// Marshall-style basketweave: blocks of threads alternating direction.
    Basket,
    /// Fender wheat: a gold twill.
    Wheat,
    /// Fender oxblood: a red-brown twill with a light thread.
    Oxblood,
    /// Fine black speaker cloth.
    Black,
    /// Silver-blue sparkle cloth (blackface).
    Silver,
}

/// Grain direction for wood and brushed metal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Horizontal,
    Vertical,
}

/// What a surface is made of.
#[derive(Debug, Clone, PartialEq)]
pub enum Material {
    /// Vinyl tolex; `scale` 1 = the grain's natural size.
    Tolex { grain: Grain, color: String, scale: f64 },
    Leather { color: String },
    Croc { color: String, scale: f64 },
    /// Powder coat; `coarse` for a textured (wrinkle) coat.
    Powder { color: String, coarse: bool },
    /// Hammertone (hammered) paint.
    Hammertone { color: String, scale: f64 },
    Brushed { color: String, dir: Dir },
    /// Spun (concentric) metal, for round faces.
    Spun { color: String },
    Wood { color: String, dir: Dir },
    GrilleCloth { cloth: Cloth },
    /// Round-hole perforated sheet; `pitch` px between holes.
    Perforated { color: String, pitch: f64 },
    /// Expanded-metal mesh; `pitch` px per diamond.
    Expanded { color: String, pitch: f64 },
    /// Piping laid in diagonals over cloth (the Vox / JMX diamond).
    DiamondGrille { cloth: String, stripes: Vec<String>, pitch: f64 },
    /// Bars in stacked chevrons (V bands) over a dark baffle.
    ChevronGrille { bars: String, pitch: f64 },
    Chrome,
    Nickel,
    /// Glossy moulded plastic.
    Plastic { color: String, gloss: f64 },
    Rubber { color: String },
}

impl Material {
    /// A short name (`tolex`, `powder`, …) for node names and lookups.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Tolex { .. } => "tolex",
            Self::Leather { .. } => "leather",
            Self::Croc { .. } => "croc",
            Self::Powder { .. } => "powder",
            Self::Hammertone { .. } => "hammertone",
            Self::Brushed { .. } => "brushed",
            Self::Spun { .. } => "spun",
            Self::Wood { .. } => "wood",
            Self::GrilleCloth { .. } => "grille cloth",
            Self::Perforated { .. } => "perforated",
            Self::Expanded { .. } => "expanded",
            Self::DiamondGrille { .. } => "diamond grille",
            Self::ChevronGrille { .. } => "chevron grille",
            Self::Chrome => "chrome",
            Self::Nickel => "nickel",
            Self::Plastic { .. } => "plastic",
            Self::Rubber { .. } => "rubber",
        }
    }

    /// Parse a material spec as written in `Place::options`
    /// (`tolex`, `tolex:bronco`, `cloth:wheat`, `wood:vertical`, …) with
    /// `color` as its paint where it takes one.
    #[must_use]
    pub fn parse(spec: &str, color: Option<&str>) -> Option<Self> {
        let (kind, sub) = spec.split_once(':').unwrap_or((spec, ""));
        let c = |d: &str| color.unwrap_or(d).to_string();
        let dir = if sub == "vertical" { Dir::Vertical } else { Dir::Horizontal };
        Some(match kind {
            "tolex" => Self::Tolex { grain: grain(sub), color: c("#1c1b1a"), scale: 1.0 },
            "leather" => Self::Leather { color: c("#6e4128") },
            "croc" => Self::Croc { color: c("#2a2522"), scale: 1.0 },
            "powder" => Self::Powder { color: c("#3a3c40"), coarse: sub == "coarse" },
            "hammertone" | "hammered" => Self::Hammertone { color: c("#6f7a80"), scale: 1.0 },
            "brushed" => Self::Brushed { color: c("#b9bbbd"), dir },
            "spun" => Self::Spun { color: c("#c3c5c7") },
            "wood" => Self::Wood { color: c("#6b3f22"), dir },
            "cloth" | "grille" => Self::GrilleCloth { cloth: cloth(sub) },
            "perforated" => Self::Perforated { color: c("#b9bcbf"), pitch: 7.0 },
            "expanded" => Self::Expanded { color: c("#8d9196"), pitch: 16.0 },
            "diamond" => Self::DiamondGrille { cloth: c("#1a1717"), stripes: vec!["#b8324a".into(), "#e6e2d6".into(), "#5f9e86".into()], pitch: 90.0 },
            "chevron" => Self::ChevronGrille { bars: c("#e8e2d2"), pitch: 18.0 },
            "chrome" => Self::Chrome,
            "nickel" => Self::Nickel,
            "plastic" | "gloss" => Self::Plastic { color: c("#141414"), gloss: 1.0 },
            "rubber" => Self::Rubber { color: c("#1a1a1b") },
            "satin" => Self::Plastic { color: c("#3a3c40"), gloss: 0.35 },
            _ => return None,
        })
    }
}

/// A tolex grain by name (`levant`, `bronco`, `elephant`, `fine`).
#[must_use]
pub fn grain(s: &str) -> Grain {
    match s {
        "bronco" => Grain::Bronco,
        "elephant" => Grain::Elephant,
        "fine" => Grain::Fine,
        _ => Grain::Levant,
    }
}

/// A grille cloth by name (`basket`, `wheat`, `oxblood`, `black`, `silver`).
#[must_use]
pub fn cloth(s: &str) -> Cloth {
    match s {
        "wheat" => Cloth::Wheat,
        "oxblood" => Cloth::Oxblood,
        "black" => Cloth::Black,
        "silver" => Cloth::Silver,
        _ => Cloth::Basket,
    }
}

/// Relative luminance of a `#hex` colour, 0..1.
#[must_use]
pub fn luma(c: &str) -> f64 {
    let c = draw::hex(c);
    0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b
}

/// A relief's strength for a paint: a dark paint shows its highlights, a
/// light one its shadows, at about the same visible contrast.
fn gain_for(color: &str, base: f64) -> f64 {
    base * (0.75 + 0.5 * (0.5 - luma(color)).abs())
}

/// Cover `(x, y, w, h)` under `parent` with `m`, clipped to a box rounded by
/// `radius`. Returns the material's frame (clipped; put nothing else in it).
pub fn fill(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64), radius: f64, m: &Material) -> NodeId {
    let mut f = draw::rounded(draw::frame(&format!("material {}", m.name()), x, y, w, h), radius);
    f.clips_content = true;
    let f = b.add(parent, fixed(f));
    let full = (0.0, 0.0, w, h);
    let base = |b: &mut Builder<'_>, c: &str, lift: f64, drop: f64| {
        let paint = lin(light::sheen_angle(), &[(0.0, tint(c, lift)), (0.5, c.to_string()), (1.0, shade(c, drop))]);
        b.add(f, fixed(draw::rect("base", 0.0, 0.0, w, h, paint)));
    };
    let sheen = |b: &mut Builder<'_>, lit: f64, dark: f64| {
        b.add(f, fixed(draw::rect("sheen", 0.0, 0.0, w, h, light::sheen(lit, dark))));
    };
    match m {
        Material::Tolex { grain, color, scale } => {
            base(b, color, 0.05, 0.18);
            let (r, g) = match grain {
                Grain::Levant => (Relief::Pebble { cells: 48 }, 0.5),
                Grain::Bronco => (Relief::Bronco { cells: 32 }, 0.55),
                Grain::Elephant => (Relief::Wrinkle { cells: 6 }, 0.5),
                Grain::Fine => (Relief::Tooth, 0.14),
            };
            texture::relief(b, f, full, 0.0, r, gain_for(color, g), *scale);
            sheen(b, 0.1, 0.2);
        }
        Material::Leather { color } => {
            base(b, color, 0.1, 0.25);
            texture::relief(b, f, full, 0.0, Relief::Leather, gain_for(color, 0.5), 1.0);
            // Leather has a soft waxy gloss.
            b.add(f, fixed(draw::rect("gloss", 0.0, 0.0, w, h, draw::radial_at(0.3, 0.2, 0.9, &[(0.0, "#ffffff24"), (1.0, "#ffffff00")]))));
            sheen(b, 0.06, 0.2);
        }
        Material::Croc { color, scale } => {
            base(b, color, 0.08, 0.2);
            texture::relief(b, f, full, 0.0, Relief::Scales { cells: 7 }, gain_for(color, 0.6), *scale);
            sheen(b, 0.12, 0.22);
        }
        Material::Powder { color, coarse } => {
            base(b, color, 0.06, 0.12);
            texture::relief(b, f, full, 0.0, Relief::Powder { coarse: *coarse }, gain_for(color, if *coarse { 0.3 } else { 0.2 }), 1.0);
            sheen(b, 0.1, 0.14);
        }
        Material::Hammertone { color, scale } => {
            // Metallic flake in the paint: a brighter, cooler sheen.
            base(b, color, 0.18, 0.28);
            texture::relief(b, f, full, 0.0, Relief::Hammered { cells: 14 }, gain_for(color, 0.55), *scale);
            b.add(f, fixed(draw::rect("flake", 0.0, 0.0, w, h, draw::radial_at(0.32, 0.25, 0.85, &[(0.0, "#ffffff38"), (1.0, "#ffffff00")]))));
            sheen(b, 0.08, 0.18);
        }
        Material::Brushed { color, dir } => brushed(b, f, w, h, color, *dir),
        Material::Spun { color } => spun(b, f, w, h, color),
        Material::Wood { color, dir } => wood(b, f, w, h, color, *dir),
        Material::GrilleCloth { cloth } => {
            texture::weave(b, f, full, 0.0, &cloth_weave(*cloth));
            recess(b, f, w, h);
        }
        Material::Perforated { color, pitch } => {
            texture::weave(b, f, full, 0.0, &Weave::Perforated { metal: color.clone(), hole: pitch * 0.62, pitch: *pitch });
            sheen(b, 0.14, 0.2);
            recess(b, f, w, h);
        }
        Material::Expanded { color, pitch } => {
            texture::weave(b, f, full, 0.0, &Weave::Expanded { metal: color.clone(), pitch: *pitch });
            sheen(b, 0.12, 0.2);
            recess(b, f, w, h);
        }
        Material::DiamondGrille { cloth, stripes, pitch } => {
            texture::weave(b, f, full, 0.0, &Weave::Plain { thread: tint(cloth, 0.12), gap: shade(cloth, 0.5) });
            diamonds(b, f, w, h, stripes, *pitch);
            recess(b, f, w, h);
        }
        Material::ChevronGrille { bars, pitch } => {
            texture::weave(b, f, full, 0.0, &Weave::Plain { thread: "#1d1d1f".into(), gap: "#060607".into() });
            chevrons(b, f, w, h, bars, *pitch);
            recess(b, f, w, h);
        }
        Material::Chrome => polished(b, f, w, h, ["#fbfcfd", "#d9dde2", "#8b939c", "#2c3036", "#6c737b", "#d5d9de"]),
        Material::Nickel => polished(b, f, w, h, ["#f3f0e8", "#d8d3c8", "#a39d91", "#5d5850", "#8f897e", "#d2ccc0"]),
        Material::Plastic { color, gloss } => {
            base(b, color, 0.1 * gloss + 0.04, 0.3);
            // A window's reflection: a soft bright band up-left, a crisp hotspot.
            let (hx, hy) = light::hotspot();
            b.add(f, fixed(draw::rect("gloss", 0.0, 0.0, w, h, draw::radial_at(hx, hy * 0.6, 0.55, &[(0.0, &alpha("#ffffff", 0.38 * gloss)), (0.5, &alpha("#ffffff", 0.08 * gloss)), (1.0, "#ffffff00")]))));
            texture::relief(b, f, full, 0.0, Relief::Tooth, 0.05, 1.0);
            sheen(b, 0.12 * gloss, 0.15);
        }
        Material::Rubber { color } => {
            base(b, color, 0.04, 0.1);
            texture::relief(b, f, full, 0.0, Relief::Tooth, gain_for(color, 0.14), 0.6);
            sheen(b, 0.04, 0.1);
        }
    }
    f
}

fn cloth_weave(c: Cloth) -> Weave {
    match c {
        Cloth::Basket => Weave::Basket { warp: "#d4c7a4".into(), weft: "#b8aa86".into(), gap: "#2a241b".into(), threads: 4 },
        Cloth::Wheat => Weave::Twill { a: "#c9ab6c".into(), b: "#8e7240".into(), gap: "#3a2c18".into(), rib: 3 },
        Cloth::Oxblood => Weave::Twill { a: "#6b2a24".into(), b: "#b8905c".into(), gap: "#1e0c0a".into(), rib: 3 },
        Cloth::Black => Weave::Plain { thread: "#2a2a2c".into(), gap: "#070708".into() },
        Cloth::Silver => Weave::Basket { warp: "#c9ccd2".into(), weft: "#8f95a0".into(), gap: "#202226".into(), threads: 2 },
    }
}

/// A cloth or sheet set into a baffle: its edges in shadow.
fn recess(b: &mut Builder<'_>, f: NodeId, w: f64, h: f64) {
    let edge = (w.min(h) * 0.02).clamp(2.0, 8.0);
    for n in light::edge("recess", (0.0, 0.0, w, h), 0.0, edge, 1.0, false) {
        b.add(f, n);
    }
}

/// Brushed metal: streak tiles along the grain, a base lit across it, and
/// the anisotropic band a brushed sheet shows across its grain.
fn brushed(b: &mut Builder<'_>, f: NodeId, w: f64, h: f64, color: &str, dir: Dir) {
    let across = if dir == Dir::Horizontal { 180.0 } else { 90.0 };
    let paint = lin(across, &[(0.0, tint(color, 0.14)), (0.45, color.to_string()), (0.55, shade(color, 0.04)), (1.0, shade(color, 0.2))]);
    b.add(f, fixed(draw::rect("base", 0.0, 0.0, w, h, paint)));
    let g = gain_for(color, 0.22);
    if dir == Dir::Horizontal {
        texture::relief(b, f, (0.0, 0.0, w, h), 0.0, Relief::Brushed, g, 1.0);
    } else {
        // The streaks run along x in the tile: turn the frame that holds them.
        let holder = b.add(f, fixed(draw::rotated(draw::frame("grain", (w - h) / 2.0, (h - w) / 2.0, h, w), 90.0)));
        texture::relief(b, holder, (0.0, 0.0, h, w), 0.0, Relief::Brushed, g, 1.0);
    }
    let band = lin(
        light::sheen_angle() - 90.0,
        &[(0.0, "#ffffff00".into()), (0.3, alpha("#ffffff", 0.16)), (0.42, "#ffffff00".into()), (0.7, "#00000000".into()), (0.8, alpha("#000000", 0.08)), (1.0, "#00000000".into())],
    );
    b.add(f, fixed(draw::rect("anisotropy", 0.0, 0.0, w, h, band)));
}

/// A ring (annulus) path between `r0` and `r1`.
#[must_use]
pub fn ring(cx: f64, cy: f64, r0: f64, r1: f64) -> String {
    let c = |r: f64, sweep: u8| {
        format!("M {:.3} {cy:.3} A {r:.3} {r:.3} 0 1 {sweep} {:.3} {cy:.3} A {r:.3} {r:.3} 0 1 {sweep} {:.3} {cy:.3} Z ", cx - r, cx + r, cx - r)
    };
    format!("{}{}", c(r1, 1), c(r0, 0))
}

/// Spun metal: concentric machining rings under a four-lobed conic sheen
/// (a spun face reflects the light as a bow-tie across its centre, lined up
/// with the light).
fn spun(b: &mut Builder<'_>, f: NodeId, w: f64, h: f64, color: &str) {
    b.add(f, fixed(draw::rect("base", 0.0, 0.0, w, h, draw::solid(color))));
    let (lt, dk) = (tint(color, 0.55), shade(color, 0.35));
    let a = light::AZIMUTH;
    let stops = [(0.0, lt.clone()), (0.125, dk.clone()), (0.25, color.to_string()), (0.375, dk.clone()), (0.5, lt.clone()), (0.625, dk.clone()), (0.75, color.to_string()), (0.875, dk), (1.0, lt)];
    let refs: Vec<(f64, &str)> = stops.iter().map(|(p, s)| (*p, s.as_str())).collect();
    b.add(f, fixed(draw::rect("sheen", 0.0, 0.0, w, h, draw::conic(a, &refs))));
    let (cx, cy) = (w / 2.0, h / 2.0);
    let rmax = cx.hypot(cy);
    let mut dl = String::new();
    let mut dd = String::new();
    let mut rng = texture::Rng::new(5);
    let mut r = 1.2;
    while r < rmax {
        let t = 0.25 + rng.f() * 0.35;
        let d = ring(cx, cy, r, r + t);
        if rng.f() > 0.5 { dl.push_str(&d) } else { dd.push_str(&d) }
        r += 0.8 + rng.f() * 1.1;
    }
    b.add(f, fixed(draw::path("rings", 0.0, 0.0, w, h, dl, draw::solid("#ffffff1f"))));
    b.add(f, fixed(draw::path("rings", 0.0, 0.0, w, h, dd, draw::solid("#0000001f"))));
}

/// Wood: the figure (wide, wavering bands of late wood, a knot's swirl)
/// as two vector paths, fine fibre as a streak tile, and a lacquer sheen.
fn wood(b: &mut Builder<'_>, f: NodeId, w: f64, h: f64, color: &str, dir: Dir) {
    let (lw, lh) = if dir == Dir::Horizontal { (w, h) } else { (h, w) };
    let holder = if dir == Dir::Horizontal {
        f
    } else {
        b.add(f, fixed(draw::rotated(draw::frame("grain", (w - h) / 2.0, (h - w) / 2.0, h, w), 90.0)))
    };
    let paint = lin(180.0, &[(0.0, tint(color, 0.08)), (0.5, color.to_string()), (1.0, shade(color, 0.12))]);
    b.add(holder, fixed(draw::rect("base", 0.0, 0.0, lw, lh, paint)));
    let mut rng = texture::Rng::new(0x3000d);
    let mut dark = String::new();
    let mut light_bands = String::new();
    let mut y = rng.f() * 6.0;
    let step = 20.0;
    while y < lh + 10.0 {
        let amp = 1.5 + rng.f() * 5.0;
        let freq = (0.4 + rng.f() * 0.8) / lw * std::f64::consts::TAU;
        let ph = rng.f() * 6.0;
        let t = 0.6 + rng.f() * 2.2;
        let n = ((lw / step).ceil() as usize).max(2);
        let wave = |x: f64, k: f64| y + amp * (x * freq + ph).sin() + k * (x * freq * 3.1 + ph * 2.0).sin();
        let mut d = format!("M 0 {:.2} ", wave(0.0, 0.8));
        for i in 1..=n {
            let x = lw * i as f64 / n as f64;
            d.push_str(&format!("L {x:.2} {:.2} ", wave(x, 0.8)));
        }
        for i in (0..=n).rev() {
            let x = lw * i as f64 / n as f64;
            d.push_str(&format!("L {x:.2} {:.2} ", wave(x, 0.8) + t));
        }
        d.push_str("Z ");
        if rng.f() > 0.3 { dark.push_str(&d) } else { light_bands.push_str(&d) }
        y += 2.5 + rng.f() * 7.0;
    }
    b.add(holder, fixed(draw::path("figure", 0.0, 0.0, lw, lh, dark, draw::solid(&alpha(&shade(color, 0.55), 0.35)))));
    b.add(holder, fixed(draw::path("figure", 0.0, 0.0, lw, lh, light_bands, draw::solid(&alpha(&tint(color, 0.35), 0.18)))));
    texture::relief(b, holder, (0.0, 0.0, lw, lh), 0.0, Relief::Fibre, gain_for(color, 0.3), 1.0);
    b.add(f, fixed(draw::rect("lacquer", 0.0, 0.0, w, h, light::sheen(0.16, 0.2))));
}

/// Polished metal: a studio's reflection as hard gradient bands (sky, the
/// bright window, the dark horizon, the floor), and a crisp hotspot.
fn polished(b: &mut Builder<'_>, f: NodeId, w: f64, h: f64, s: [&str; 6]) {
    let refl = draw::linear(170.0, &[(0.0, s[1]), (0.18, s[0]), (0.36, s[1]), (0.5, s[2]), (0.56, s[3]), (0.72, s[4]), (1.0, s[5])]);
    b.add(f, fixed(draw::rect("reflection", 0.0, 0.0, w, h, refl)));
    let (hx, hy) = light::hotspot();
    b.add(f, fixed(draw::rect("hotspot", 0.0, 0.0, w, h, draw::radial_at(hx, hy * 0.5, 0.3, &[(0.0, "#ffffffc0"), (1.0, "#ffffff00")]))));
    for n in light::edge("edge", (0.0, 0.0, w, h), 0.0, 1.2, 0.8, true) {
        b.add(f, n);
    }
}

/// Parallel strands of width `sw` along `dir` (unit), spaced `pitch`
/// across, covering `w × h` — one path.
fn strands(w: f64, h: f64, pitch: f64, sw: f64, slope: f64, offset: f64) -> String {
    // Lines y = slope·x + c for c across the range that crosses the box.
    let (c0, c1) = if slope > 0.0 { (-slope * w - pitch, h + pitch) } else { (-pitch, h - slope * w + pitch) };
    let mut d = String::new();
    let half = sw / 2.0 * (1.0 + slope * slope).sqrt();
    let mut c = c0 + offset.rem_euclid(pitch) - pitch;
    while c < c1 {
        d.push_str(&draw::polygon(&[(0.0, c - half), (w, slope * w + c - half), (w, slope * w + c + half), (0.0, c + half)]));
        d.push(' ');
        c += pitch;
    }
    d
}

/// Embossed strands: a shadow copy (away from the light), the strand, a lit
/// copy (toward it) — depth without a blur.
fn emboss(b: &mut Builder<'_>, f: NodeId, w: f64, h: f64, d: &str, color: &str, depth: f64) {
    let (sx, sy) = light::shadow_offset(depth);
    let (lx, ly) = light::toward_light();
    b.add(f, fixed(draw::path("strand shadow", sx, sy, w, h, d.to_string(), draw::solid("#0000008c"))));
    b.add(f, fixed(draw::path("strand", 0.0, 0.0, w, h, d.to_string(), draw::solid(color))));
    let lit = draw::path("strand lit", lx * depth * 0.25, ly * depth * 0.25, w, h, d.to_string(), draw::solid(&alpha(&tint(color, 0.6), 0.35)));
    b.add(f, fixed(lit));
}

/// The diamond: stitched piping crossing on the diagonals, in the given
/// thread colours laid side by side.
fn diamonds(b: &mut Builder<'_>, f: NodeId, w: f64, h: f64, stripes: &[String], pitch: f64) {
    let slope = 1.3;
    let sw = 1.3;
    for (i, c) in stripes.iter().enumerate() {
        let off = i as f64 * 3.8;
        let mut d = strands(w, h, pitch, sw, slope, off);
        d.push_str(&strands(w, h, pitch, sw, -slope, off + pitch / 2.0));
        // Stitches: the thread shows in dashes. Cut them with a comb of gaps.
        emboss(b, f, w, h, &d, c, 1.2);
    }
    // The stitching: short gaps across every stripe, as cloth over the thread.
    let mut gaps = String::new();
    let mut y = 0.0;
    while y < h {
        gaps.push_str(&format!("M 0 {y:.2} H {w:.2} v 1.2 H 0 Z "));
        y += 4.5;
    }
    b.add(f, fixed(draw::path("stitch gaps", 0.0, 0.0, w, h, gaps, draw::solid("#0d0c0c80"))));
}

/// Stacked chevrons (V bands pointing up), embossed.
fn chevrons(b: &mut Builder<'_>, f: NodeId, w: f64, h: f64, bars: &str, pitch: f64) {
    let bw = pitch * 0.42;
    let span = pitch * 2.2;
    let mut d = String::new();
    let mut x0 = -span;
    while x0 < w + span {
        let mut y = -pitch;
        while y < h + pitch * 2.0 {
            // One V: from (x0, y + span/2) up to (x0 + span/2, y) down to (x0 + span, y + span/2).
            let (a, m, e) = ((x0, y + span / 2.0), (x0 + span / 2.0, y), (x0 + span, y + span / 2.0));
            d.push_str(&draw::polygon(&[a, m, e, (e.0, e.1 + bw), (m.0, m.1 + bw), (a.0, a.1 + bw)]));
            d.push(' ');
            y += pitch;
        }
        x0 += span;
    }
    emboss(b, f, w, h, &d, bars, 2.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs_parse() {
        assert!(matches!(Material::parse("tolex:bronco", Some("#101010")), Some(Material::Tolex { grain: Grain::Bronco, .. })));
        assert!(matches!(Material::parse("cloth:oxblood", None), Some(Material::GrilleCloth { cloth: Cloth::Oxblood })));
        assert!(matches!(Material::parse("wood:vertical", None), Some(Material::Wood { dir: Dir::Vertical, .. })));
        assert!(Material::parse("plaid", None).is_none());
    }
}
