//! `knob.3d`: a knob rendered per pixel on the GPU — render's built-in
//! `knob.3d` shader (a distance-field cap, skirt and insert, sphere-traced
//! and lit by the kit's light) over a `knob.render` knob, which is its
//! vector fallback and its interaction.
//!
//! ```text
//! <root control=Knob{sweep}>
//!   knob
//!     scale dots, numerals            (fixed, panel ink — both paths)
//!     cap  shader=knob.3d             (the knob's box less its ring)
//!       … knob.render's tiers, body ← Needle, lit overlays (the fallback)
//!   legend ← Label
//! ```
//!
//! On a GPU renderer (`--fx gpu`) the shader replaces the `cap` frame's
//! subtree: the ribs, knurl, beak and pointer turn with the control's
//! position (`value`) while the key light, the highlights and the shadow
//! stay put. Everywhere else (`--fx off`/`fallback`, SVG) the vector
//! `knob.render` draws, its `body` the turning `Needle`.
//!
//! Variant = a [`preset`] (`pedal_chicken`, `teardrop`, `fluted_cream`,
//! `knurled_dome`, `spun_metal`, `bakelite_chicken`, `amp_pointer`,
//! `gold_knurl`, `white_knurl`, `stepped_cream`, `amp_skirted`, `scalloped`, and
//! `knob.render`'s caps `flat`, `domed`, `fluted`, `knurled`,
//! `chicken_head`, `pointer`). Options override it:
//!
//! | option | does |
//! |---|---|
//! | `color=`, `skirt_color=`, `pointer=`, `insert_color=` | colours (`#hex`) |
//! | `metal=`, `rough=`, `coat=`, `aniso=` | the cap's material (0..1) |
//! | `spec=`, `shadow=` | specular and shadow strength (0..2) |
//! | `ribs=` | flutes or knurl teeth |
//! | `tilt=` | view tilt, degrees (0 = straight down) |
//! | `ticks=`, `legend=top` | as `knob.render` |
//! | `profile=` | the cap's shape: `flat`, `domed`, `fluted`, `knurled`, `chicken_head`, `teardrop`, `wing`, `stepped`, `insert` |
//! | `skirt=` | `none`, `plain`, `flared`, `knurled`, `metal`, `dished` (the fallback's nearest too) |
//! | `index=` | the pointer: `none`, `line`, `dot`, `full`, `groove` |
//! | `cap_r=`, `height=`, `dome=`, `edge=`, `rib_depth=`, `knurl_band=`, `diamond=` | the cap's proportions (knob units: the footprint's radius is 1) |
//! | `step_r=`, `step_h=`, `skirt_r=`, `skirt_h=`, `skirt_teeth=`, `insert_r=`, `insert_h=` | the step, skirt and insert |
//! | `index_w=`, `index_in=`, `index_out=` | the pointer's width and its ends (cap radii) |
//! | `skirt_metal=`, `skirt_rough=`, `skirt_coat=`, `insert_metal=`, `insert_rough=` | the skirt's and insert's material |
//! | `fallback=`, `material=`, `fallback_color=` | the vector fallback's cap, material and colour |
//!
//! [`Look3d`] is the same from Rust; [`Look3d::shader`] its `ShaderFx`.

use std::collections::BTreeMap;

use document_proto::{NodeId, ShaderFx, ShaderInput};
use surface_widgets::Builder;

use super::knob::{self, Cap, KnobMaterial, Look, Skirt, Ticks};
use crate::registry::{ComponentDef, Family, Place};

/// The cap's shape, as the shader's `p0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    Flat = 0,
    Domed = 1,
    Fluted = 2,
    Knurled = 3,
    ChickenHead = 4,
    /// A teardrop cap, its point the index.
    Teardrop = 5,
    /// A dome crossed by a fin (an amp's pointer knob).
    Wing = 6,
    /// A wide cylinder with a smaller one on top.
    Stepped = 7,
    /// A fluted body with a teardrop insert on top (the pedal knob).
    Insert = 8,
}

/// The skirt under the cap, as the shader's `p2`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkirtKind {
    None = 0,
    Plain = 1,
    Flared = 2,
    Knurled = 3,
    Metal = 4,
    Dished = 5,
}

/// The pointer, as the shader's `p3`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pointer {
    None = 0,
    /// A painted line on the top.
    Line = 1,
    Dot = 2,
    /// A line on the top and down the side.
    Full = 3,
    /// An engraved, filled groove.
    Groove = 4,
}

/// A surface: colour (`#hex`) and PBR-ish parameters (0..1).
#[derive(Debug, Clone, PartialEq)]
pub struct Finish {
    pub color: String,
    pub metal: f64,
    pub rough: f64,
    pub coat: f64,
    pub aniso: f64,
}

impl Finish {
    #[must_use]
    pub fn plastic(color: &str, rough: f64) -> Self {
        Self { color: color.into(), metal: 0.0, rough, coat: 0.0, aniso: 0.0 }
    }

    #[must_use]
    pub fn metal(color: &str, rough: f64) -> Self {
        Self { color: color.into(), metal: 1.0, rough, coat: 0.0, aniso: 0.0 }
    }

    #[must_use]
    pub fn coat(mut self, coat: f64) -> Self {
        self.coat = coat;
        self
    }

    #[must_use]
    pub fn aniso(mut self, aniso: f64) -> Self {
        self.aniso = aniso;
        self
    }
}

/// Everything a GPU knob varies by (knob units: the footprint's radius is 1).
#[derive(Debug, Clone, PartialEq)]
pub struct Look3d {
    pub profile: Profile,
    /// Flutes or knurl teeth (0 = the profile's default).
    pub ribs: f64,
    pub cap: Finish,
    pub cap_r: f64,
    pub height: f64,
    /// A spherical crown this high above the top edge.
    pub dome: f64,
    /// The top edge's rounding radius.
    pub bevel: f64,
    pub rib_depth: f64,
    /// The part of the side (from the panel) knurled, 0..1.
    pub knurl_band: f64,
    /// Diamond (crossed) knurl instead of straight.
    pub diamond: bool,
    /// `Stepped`: the top step's radius and height.
    pub step: (f64, f64),
    pub skirt: SkirtKind,
    pub skirt_finish: Finish,
    /// Skirt radius and height.
    pub skirt_size: (f64, f64),
    pub skirt_teeth: f64,
    /// `Insert`: the teardrop's finish, size and height.
    pub insert: Finish,
    pub insert_size: (f64, f64),
    pub pointer: Pointer,
    pub pointer_color: String,
    /// Pointer width, inner and outer end (cap radii).
    pub pointer_size: (f64, f64, f64),
    pub specular: f64,
    pub shadow: f64,
    pub occlusion: f64,
    /// View tilt, degrees.
    pub tilt: f64,
    /// The vector knob drawn where the shader doesn't run.
    pub fallback: Look,
}

/// `#rrggbb` → linear-free sRGB 0..1 (grey on a bad string).
fn rgb(hex: &str) -> [f64; 3] {
    let h = hex.trim_start_matches('#');
    let byte = |i: usize| h.get(i..i + 2).and_then(|s| u8::from_str_radix(s, 16).ok()).map_or(0.5, |v| f64::from(v) / 255.0);
    [byte(0), byte(2), byte(4)]
}

impl Look3d {
    /// A plain black plastic flat knob; presets start here.
    #[must_use]
    pub fn base(fallback: Cap) -> Self {
        Self {
            profile: Profile::Flat,
            ribs: 0.0,
            cap: Finish::plastic("#1c1c1e", 0.26).coat(0.3),
            cap_r: 0.72,
            height: 0.5,
            dome: 0.0,
            bevel: 0.07,
            rib_depth: 0.045,
            knurl_band: 1.0,
            diamond: false,
            step: (0.5, 0.14),
            skirt: SkirtKind::None,
            skirt_finish: Finish::plastic("#151517", 0.4),
            skirt_size: (1.0, 0.16),
            skirt_teeth: 48.0,
            insert: Finish::metal("#d4d6d9", 0.2),
            insert_size: (0.62, 0.09),
            pointer: Pointer::Line,
            pointer_color: "#f2f0ea".into(),
            pointer_size: (0.075, 0.3, 0.92),
            specular: 1.0,
            shadow: 1.0,
            occlusion: 1.0,
            tilt: 30.0,
            fallback: Look::of(fallback),
        }
    }

    /// The shader and its parameters, for a knob turning through `sweep`°
    /// whose footprint is `radius` × half its box.
    #[must_use]
    pub fn shader(&self, sweep: f64, radius: f64) -> ShaderFx {
        let [r, g, b] = rgb(&self.cap.color);
        let params: BTreeMap<String, f64> = [
            ("strength", self.specular),
            ("speed", self.cap.aniso),
            ("amount", self.cap.metal),
            ("scale", self.bevel),
            ("angle", sweep),
            ("elevation", self.tilt),
            ("softness", self.cap.rough),
            ("radius", radius),
            ("tint_r", r),
            ("tint_g", g),
            ("tint_b", b),
            ("tint_a", self.cap.coat),
            ("p0", f64::from(self.profile as u8)),
            ("p1", self.ribs),
            ("p2", f64::from(self.skirt as u8)),
            ("p3", f64::from(self.pointer as u8)),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
        let [sr, sg, sb] = rgb(&self.skirt_finish.color);
        let [ir, ig, ib] = rgb(&self.insert.color);
        let [pr, pg, pb] = rgb(&self.pointer_color);
        let s = &self.skirt_finish;
        let inputs = vec![
            ShaderInput::constant(
                "shape",
                vec![self.cap_r, self.height, self.dome, self.rib_depth, self.knurl_band, self.step.0, self.step.1, f64::from(u8::from(self.diamond)), self.skirt_teeth],
            ),
            ShaderInput::constant("skirt", vec![sr, sg, sb, s.metal, s.rough, s.coat, self.skirt_size.0, self.skirt_size.1]),
            ShaderInput::constant("insert", vec![ir, ig, ib, self.insert.metal, self.insert.rough, self.insert.aniso, self.insert_size.0, self.insert_size.1]),
            ShaderInput::constant("pointer", vec![pr, pg, pb, self.pointer_size.0, self.pointer_size.1, self.pointer_size.2]),
            ShaderInput::constant("light", vec![self.shadow, self.occlusion, 1.0, 1.0, 1.0]),
        ];
        ShaderFx { shader: "knob.3d".into(), params, inputs }
    }

    /// The look a `Place` asks for: its preset (variant), then options.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn from_place(p: &Place) -> Self {
        let mut l = preset(p.variant.as_deref().unwrap_or("flat"));
        let num = |k: &str| p.opt(k).and_then(|v| v.parse::<f64>().ok());
        let hex = |k: &str| p.opt(k).filter(|c| c.starts_with('#')).map(str::to_string);
        // The fallback's cap and material first, so the options below
        // repaint it.
        if let Some(cap) = p.opt("fallback") {
            l.fallback.cap = Cap::parse(cap);
        }
        if let Some(m) = p.opt("material") {
            l.fallback.material = match m {
                "rubber" => KnobMaterial::Rubber,
                "metal" | "spun" => KnobMaterial::Metal,
                "chrome" => KnobMaterial::Chrome,
                "bakelite" => KnobMaterial::Bakelite,
                _ => KnobMaterial::Plastic,
            };
        }
        // The fallback's bevel (flute and knurl depth with fx off).
        if let Some(b) = num("bevel") {
            l.fallback.bevel = b.clamp(0.0, 2.0);
        }
        // The shape.
        if let Some(s) = p.opt("profile") {
            l.profile = match s {
                "domed" => Profile::Domed,
                "fluted" => Profile::Fluted,
                "knurled" => Profile::Knurled,
                "chicken_head" => Profile::ChickenHead,
                "teardrop" => Profile::Teardrop,
                "wing" => Profile::Wing,
                "stepped" => Profile::Stepped,
                "insert" => Profile::Insert,
                _ => Profile::Flat,
            };
        }
        if let Some(s) = p.opt("skirt") {
            (l.skirt, l.fallback.skirt) = match s {
                "plain" => (SkirtKind::Plain, Skirt::Plain),
                "flared" => (SkirtKind::Flared, Skirt::Flared),
                "knurled" => (SkirtKind::Knurled, Skirt::Ridged),
                "metal" => (SkirtKind::Metal, Skirt::Chrome),
                "dished" => (SkirtKind::Dished, Skirt::Plain),
                _ => (SkirtKind::None, Skirt::None),
            };
        }
        if let Some(s) = p.opt("index") {
            l.pointer = match s {
                "line" => Pointer::Line,
                "dot" => Pointer::Dot,
                "full" => Pointer::Full,
                "groove" => Pointer::Groove,
                _ => Pointer::None,
            };
        }
        if let Some(v) = p.opt("diamond") {
            l.diamond = v == "true" || v == "1";
        }
        let set = |v: &mut f64, k: &str, lo: f64, hi: f64| {
            if let Some(n) = num(k) {
                *v = n.clamp(lo, hi);
            }
        };
        set(&mut l.cap_r, "cap_r", 0.1, 1.0);
        set(&mut l.height, "height", 0.05, 1.5);
        set(&mut l.dome, "dome", 0.0, 0.6);
        set(&mut l.bevel, "edge", 0.0, 0.4);
        set(&mut l.rib_depth, "rib_depth", 0.0, 0.3);
        set(&mut l.knurl_band, "knurl_band", 0.0, 1.0);
        set(&mut l.step.0, "step_r", 0.05, 1.0);
        set(&mut l.step.1, "step_h", 0.0, 1.0);
        set(&mut l.skirt_size.0, "skirt_r", 0.1, 1.0);
        set(&mut l.skirt_size.1, "skirt_h", 0.0, 1.0);
        set(&mut l.skirt_teeth, "skirt_teeth", 0.0, 200.0);
        set(&mut l.insert_size.0, "insert_r", 0.05, 1.0);
        set(&mut l.insert_size.1, "insert_h", 0.0, 0.5);
        set(&mut l.pointer_size.0, "index_w", 0.0, 0.5);
        set(&mut l.pointer_size.1, "index_in", 0.0, 1.5);
        set(&mut l.pointer_size.2, "index_out", 0.0, 1.5);
        set(&mut l.skirt_finish.metal, "skirt_metal", 0.0, 1.0);
        set(&mut l.skirt_finish.rough, "skirt_rough", 0.02, 1.0);
        set(&mut l.skirt_finish.coat, "skirt_coat", 0.0, 1.0);
        set(&mut l.insert.metal, "insert_metal", 0.0, 1.0);
        set(&mut l.insert.rough, "insert_rough", 0.02, 1.0);
        if let Some(c) = hex("color") {
            l.fallback.color.clone_from(&c);
            l.cap.color = c;
        }
        if let Some(c) = hex("skirt_color") {
            l.fallback.skirt_color.clone_from(&c);
            l.skirt_finish.color = c;
        }
        if let Some(c) = hex("pointer") {
            l.fallback.pointer.clone_from(&c);
            l.pointer_color = c;
        }
        if let Some(c) = hex("insert_color") {
            l.insert.color = c;
        }
        if let Some(c) = hex("fallback_color") {
            l.fallback.color = c;
        }
        l.cap.metal = num("metal").unwrap_or(l.cap.metal).clamp(0.0, 1.0);
        l.cap.rough = num("rough").unwrap_or(l.cap.rough).clamp(0.02, 1.0);
        l.cap.coat = num("coat").unwrap_or(l.cap.coat).clamp(0.0, 1.0);
        l.cap.aniso = num("aniso").unwrap_or(l.cap.aniso).clamp(0.0, 1.0);
        l.specular = num("spec").unwrap_or(l.specular).clamp(0.0, 2.0);
        l.shadow = num("shadow").unwrap_or(l.shadow).clamp(0.0, 2.0);
        l.fallback.specular = l.fallback.specular.min(l.specular.max(0.0));
        l.fallback.shadow = l.shadow;
        l.ribs = num("ribs").unwrap_or(l.ribs).clamp(0.0, 200.0);
        l.tilt = num("tilt").unwrap_or(l.tilt).clamp(0.0, 50.0);
        l.fallback.ticks = match p.opt("ticks") {
            Some("lines") => Ticks::Lines,
            Some("none") => Ticks::None,
            _ => l.fallback.ticks,
        };
        l.fallback.legend_top = p.opt("legend") == Some("top");
        l
    }
}

/// The presets, by name: the reference knobs, then `knob.render`'s caps.
pub const PRESETS: [&str; 18] = [
    "pedal_chicken",
    "teardrop",
    "fluted_cream",
    "knurled_dome",
    "spun_metal",
    "bakelite_chicken",
    "amp_pointer",
    "gold_knurl",
    "white_knurl",
    "stepped_cream",
    "amp_skirted",
    "scalloped",
    "flat",
    "domed",
    "fluted",
    "knurled",
    "chicken_head",
    "pointer",
];

/// A preset look (unknown names: `flat`).
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn preset(name: &str) -> Look3d {
    let fb = |cap: Cap, f: &dyn Fn(&mut Look)| {
        let mut l = Look::of(cap);
        f(&mut l);
        l
    };
    match name {
        // Cory Wong pedals: a black fluted body on a flared skirt, a
        // polished teardrop insert on top as the index.
        "pedal_chicken" => Look3d {
            profile: Profile::Insert,
            ribs: 14.0,
            cap: Finish::plastic("#121214", 0.2).coat(0.6),
            cap_r: 0.64,
            height: 0.6,
            rib_depth: 0.05,
            bevel: 0.05,
            skirt: SkirtKind::Flared,
            skirt_finish: Finish::plastic("#111113", 0.18).coat(0.6),
            skirt_size: (1.0, 0.2),
            insert: Finish::metal("#e2e4e7", 0.12),
            insert_size: (0.64, 0.1),
            pointer: Pointer::None,
            fallback: fb(Cap::Pointer, &|l| {
                l.color = "#d9dde1".into();
                l.material = KnobMaterial::Chrome;
                l.skirt = Skirt::Ridged;
                l.skirt_color = "#141416".into();
            }),
            ..Look3d::base(Cap::Pointer)
        },
        // A satin aluminium teardrop on a black body with a knurled skirt.
        "teardrop" => Look3d {
            profile: Profile::Insert,
            ribs: 18.0,
            cap: Finish::plastic("#17171a", 0.34).coat(0.3),
            cap_r: 0.64,
            height: 0.56,
            rib_depth: 0.035,
            skirt: SkirtKind::Knurled,
            skirt_finish: Finish::plastic("#141416", 0.4),
            skirt_size: (1.0, 0.18),
            skirt_teeth: 56.0,
            insert: Finish::metal("#c9ccd1", 0.3).aniso(0.6),
            insert_size: (0.66, 0.08),
            pointer: Pointer::None,
            fallback: fb(Cap::Pointer, &|l| {
                l.color = "#c4c6c8".into();
                l.material = KnobMaterial::Metal;
                l.skirt_color = "#141416".into();
            }),
            ..Look3d::base(Cap::Pointer)
        },
        // Cream "jedi" flutes, a dark dot for the index.
        "fluted_cream" => Look3d {
            profile: Profile::Fluted,
            ribs: 10.0,
            cap: Finish::plastic("#dccba8", 0.42),
            cap_r: 0.96,
            height: 0.56,
            rib_depth: 0.1,
            bevel: 0.08,
            dome: 0.04,
            pointer: Pointer::Dot,
            pointer_color: "#2a2723".into(),
            pointer_size: (0.07, 0.62, 0.62),
            fallback: fb(Cap::Fluted, &|l| {
                l.color = "#e9dfc7".into();
                l.pointer = "#2a2723".into();
            }),
            ..Look3d::base(Cap::Fluted)
        },
        // The Halfman OD: a glossy oxblood dome over a short knurled band.
        "knurled_dome" => Look3d {
            profile: Profile::Knurled,
            ribs: 72.0,
            cap: Finish::plastic("#3a1d1b", 0.3).coat(0.9),
            cap_r: 0.96,
            height: 0.46,
            dome: 0.3,
            bevel: 0.12,
            rib_depth: 0.03,
            knurl_band: 0.32,
            pointer: Pointer::None,
            fallback: fb(Cap::Domed, &|l| {
                l.color = "#3a1d1b".into();
                l.material = KnobMaterial::Bakelite;
            }),
            ..Look3d::base(Cap::Domed)
        },
        // A spun aluminium cap in a thin black skirt, a black index.
        "spun_metal" => Look3d {
            profile: Profile::Flat,
            cap: Finish::metal("#c9cbce", 0.3).aniso(0.85),
            cap_r: 0.84,
            height: 0.42,
            dome: 0.035,
            bevel: 0.06,
            skirt: SkirtKind::Plain,
            skirt_finish: Finish::plastic("#101012", 0.35).coat(0.4),
            skirt_size: (1.0, 0.2),
            pointer: Pointer::Line,
            pointer_color: "#141414".into(),
            pointer_size: (0.06, 0.1, 0.95),
            fallback: fb(Cap::Flat, &|l| {
                l.material = KnobMaterial::Metal;
                l.color = "#c4c6c8".into();
                l.pointer = "#1a1a1a".into();
                l.skirt_color = "#101012".into();
            }),
            ..Look3d::base(Cap::Flat)
        },
        // Glossy black bakelite chicken head, a white line down the beak.
        "bakelite_chicken" => Look3d {
            profile: Profile::ChickenHead,
            cap: Finish::plastic("#0e0d0d", 0.16).coat(1.0),
            cap_r: 0.8,
            height: 0.5,
            bevel: 0.12,
            skirt: SkirtKind::Flared,
            skirt_finish: Finish::plastic("#0e0d0d", 0.16).coat(1.0),
            skirt_size: (0.66, 0.16),
            pointer: Pointer::Line,
            pointer_color: "#efeae0".into(),
            pointer_size: (0.045, 0.45, 1.18),
            fallback: fb(Cap::ChickenHead, &|l| {
                l.material = KnobMaterial::Bakelite;
                l.color = "#161515".into();
            }),
            ..Look3d::base(Cap::ChickenHead)
        },
        // Nolly's amp knob: a white dome crossed by a pointer fin.
        "amp_pointer" => Look3d {
            profile: Profile::Wing,
            cap: Finish::plastic("#e8e7e3", 0.45),
            cap_r: 0.8,
            height: 0.72,
            pointer: Pointer::None,
            skirt: SkirtKind::Plain,
            skirt_finish: Finish::plastic("#2a2a2c", 0.5),
            skirt_size: (0.78, 0.08),
            fallback: fb(Cap::Pointer, &|l| {
                l.color = "#e8e7e3".into();
                l.skirt = Skirt::None;
            }),
            ..Look3d::base(Cap::Pointer)
        },
        // THX pedals: polished gold, a diamond-knurled side, a dot index.
        "gold_knurl" => Look3d {
            profile: Profile::Knurled,
            ribs: 64.0,
            diamond: true,
            cap: Finish::metal("#e0b575", 0.2),
            cap_r: 0.96,
            height: 0.62,
            dome: 0.07,
            bevel: 0.1,
            rib_depth: 0.035,
            knurl_band: 0.66,
            pointer: Pointer::Dot,
            pointer_color: "#1a1714".into(),
            pointer_size: (0.07, 0.62, 0.62),
            fallback: fb(Cap::Knurled, &|l| {
                l.material = KnobMaterial::Metal;
                l.color = "#c9a266".into();
                l.pointer = "#1a1714".into();
            }),
            ..Look3d::base(Cap::Knurled)
        },
        // The Clean Machine: a cream-white cap, diamond knurl, dot index.
        "white_knurl" => Look3d {
            profile: Profile::Knurled,
            ribs: 64.0,
            diamond: true,
            cap: Finish::plastic("#e2e0da", 0.4),
            cap_r: 0.96,
            height: 0.64,
            dome: 0.1,
            bevel: 0.12,
            rib_depth: 0.035,
            knurl_band: 0.66,
            pointer: Pointer::Dot,
            pointer_color: "#1d1d1d".into(),
            pointer_size: (0.065, 0.6, 0.6),
            fallback: fb(Cap::Knurled, &|l| {
                l.color = "#ecebe6".into();
                l.pointer = "#1d1d1d".into();
            }),
            ..Look3d::base(Cap::Knurled)
        },
        // Gravity Tank: a cream cylinder with a smaller one on top.
        "stepped_cream" => Look3d {
            profile: Profile::Stepped,
            cap: Finish::plastic("#d9c6a2", 0.5),
            cap_r: 0.96,
            height: 0.4,
            bevel: 0.06,
            step: (0.62, 0.22),
            pointer: Pointer::Dot,
            pointer_color: "#2a2520".into(),
            pointer_size: (0.05, 0.86, 0.86),
            fallback: fb(Cap::Flat, &|l| {
                l.color = "#e4d6ba".into();
                l.pointer = "#2a2520".into();
                l.skirt = Skirt::None;
            }),
            ..Look3d::base(Cap::Flat)
        },
        // A black amp knob: a ribbed cap on a wide skirt (the numbers are
        // the printed scale's), a white index.
        "amp_skirted" => Look3d {
            profile: Profile::Knurled,
            ribs: 20.0,
            cap: Finish::plastic("#141415", 0.2).coat(0.6),
            cap_r: 0.56,
            height: 0.62,
            dome: 0.05,
            bevel: 0.06,
            rib_depth: 0.05,
            skirt: SkirtKind::Flared,
            skirt_finish: Finish::plastic("#141415", 0.2).coat(0.6),
            skirt_size: (1.0, 0.16),
            pointer: Pointer::Full,
            pointer_size: (0.06, 0.2, 1.0),
            fallback: fb(Cap::Knurled, &|l| {
                l.skirt = Skirt::Flared;
            }),
            ..Look3d::base(Cap::Knurled)
        },
        // The Asato pedals: a tall cylinder of rounded scallops under a
        // flat top whose edge follows them, a painted line for the index.
        "scalloped" => Look3d {
            profile: Profile::Fluted,
            ribs: 18.0,
            cap: Finish::plastic("#4f6f70", 0.34).coat(0.35),
            cap_r: 0.96,
            height: 0.66,
            dome: 0.03,
            bevel: 0.1,
            rib_depth: 0.07,
            pointer: Pointer::Line,
            pointer_color: "#eef0ea".into(),
            pointer_size: (0.085, 0.18, 0.58),
            fallback: fb(Cap::Fluted, &|l| {
                l.color = "#56777a".into();
                l.pointer = "#eef0ea".into();
            }),
            ..Look3d::base(Cap::Fluted)
        },
        // `knob.render`'s caps, with its default colours.
        "domed" => Look3d { profile: Profile::Domed, cap_r: 1.0, cap: Finish::plastic("#19191b", 0.32).coat(0.5), dome: 0.2, bevel: 0.14, ..Look3d::base(Cap::Domed) },
        "fluted" => Look3d { profile: Profile::Fluted, cap_r: 1.0, ribs: 8.0, rib_depth: 0.1, cap: Finish::plastic("#1b1b1d", 0.26).coat(0.3), ..Look3d::base(Cap::Fluted) },
        "knurled" => Look3d { profile: Profile::Knurled, cap_r: 1.0, ribs: 60.0, rib_depth: 0.03, cap: Finish::plastic("#2a2b2d", 0.3).coat(0.3), ..Look3d::base(Cap::Knurled) },
        "chicken_head" => Look3d {
            profile: Profile::ChickenHead,
            cap: Finish::plastic("#161617", 0.36).coat(0.4),
            cap_r: 0.8,
            bevel: 0.1,
            pointer_size: (0.05, 0.45, 1.18),
            ..Look3d::base(Cap::ChickenHead)
        },
        "pointer" => Look3d {
            profile: Profile::Teardrop,
            cap: Finish::plastic("#ece8df", 0.42),
            cap_r: 0.8,
            height: 0.46,
            dome: 0.06,
            bevel: 0.12,
            skirt: SkirtKind::Knurled,
            skirt_finish: Finish::plastic("#18181a", 0.4),
            skirt_size: (1.0, 0.16),
            pointer: Pointer::None,
            ..Look3d::base(Cap::Pointer)
        },
        _ => Look3d {
            skirt: SkirtKind::Plain,
            cap_r: 0.8,
            skirt_size: (1.0, 0.14),
            skirt_finish: Finish::plastic("#141416", 0.4),
            ..Look3d::base(Cap::Flat)
        },
    }
}

/// Place a GPU knob: `knob.render`'s knob with its standing parts under the
/// `knob.3d` shader.
pub fn build(b: &mut Builder<'_>, parent: NodeId, p: &Place, look: &Look3d) -> NodeId {
    let (side, r) = knob::cap_box(p);
    let fx = look.shader(knob::travel(p), r / (side / 2.0));
    knob::build_with(b, parent, p, &look.fallback, Some(fx))
}

fn render(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    build(b, parent, p, &Look3d::from_place(p))
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![ComponentDef {
        id: "knob.3d",
        family: Family::Knob,
        title: "3D knob",
        description: "A knob rendered per pixel on the GPU like a plugin's product render: a lathe-turned or moulded cap (flutes, knurl, dome, chicken head, teardrop, wing, stepped), a skirt, a pointer or insert, with PBR-ish plastic, bakelite (clearcoat), polished and spun (anisotropic) metal, occlusion in the grooves and a soft shadow on the panel. The ribs turn with the value under a fixed upper-left light. Variant = preset: `pedal_chicken`, `teardrop`, `fluted_cream`, `knurled_dome`, `spun_metal`, `bakelite_chicken`, `amp_pointer`, `gold_knurl`, `white_knurl`, `stepped_cream`, `amp_skirted`, `scalloped`, or a cap `flat|domed|fluted|knurled|chicken_head|pointer`. Options: `color=`/`skirt_color=`/`pointer=`/`insert_color=` (#hex), `metal=`/`rough=`/`coat=`/`aniso=` (0..1), `spec=`/`shadow=` (0..2), `ribs=`, `tilt=` (degrees), `ticks=`, `legend=top`; the shape: `profile=`, `skirt=`, `index=` (none|line|dot|full|groove), `cap_r=`/`height=`/`dome=`/`edge=`/`rib_depth=`/`knurl_band=`/`diamond=`, `step_r=`/`step_h=`, `skirt_r=`/`skirt_h=`/`skirt_teeth=`, `insert_r=`/`insert_h=`, `index_w=`/`index_in=`/`index_out=`, `skirt_metal=`/`skirt_rough=`/`skirt_coat=`, `insert_metal=`/`insert_rough=`; the fallback: `fallback=` (cap), `material=`, `fallback_color=`. Without the GPU stage it draws as `knob.render`. Size = knob diameter.",
        variants: &PRESETS,
        default_size: 48.0,
        binds: "param",
        build: render,
    }]
}
