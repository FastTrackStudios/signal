//! `knob.render`: the generic rendered knob — a 3D knob as a plugin's
//! product render shows it, from straight above: a cap of some shape on an
//! optional skirt, lit by the kit's light, standing off the panel.
//!
//! ```text
//! <root control=Knob{sweep}>
//!   knob   (square, centred on the shaft)
//!     scale dots, numerals               (fixed, panel ink)
//!     occlusion, skirt + its shadows, wall   (fixed: round tiers, lit)
//!     body  ← Needle: outline (lobes, teeth, wing), face disc, ribs, pointer
//!     face light, rim light, flute light, specular   (fixed overlays)
//!   legend ← Label (under the knob, or over it with `legend=top`)
//! ```
//!
//! What turns is painted rotation-invariantly (flat, or radial about the
//! shaft); every lit paint — the sheen across the face, the rim catching
//! the light on its upper-left arc, the gloss hotspot — is a fixed overlay
//! above the body, so the hotspot stays put while the pointer turns.
//!
//! Variant = cap shape: `flat`, `domed`, `fluted`, `knurled`,
//! `chicken_head`, `pointer`. Options:
//!
//! | option | values | default |
//! |---|---|---|
//! | `material=` | `plastic`, `rubber`, `metal` (spun), `chrome`, `bakelite` | `plastic` |
//! | `color=` | the cap's colour | per shape |
//! | `skirt=` | `none`, `plain`, `flared`, `ridged`, `chrome` | per shape |
//! | `skirt_color=` | the skirt's colour | the cap's, darker |
//! | `pointer=` | the pointer's colour | white / black on a light cap |
//! | `bevel=` | 0..2 — the height of the cap's edge | 1 |
//! | `spec=` | 0..2 — gloss hotspot strength | per material |
//! | `shadow=` | 0..2 — occlusion and cast shadow | 1 |
//! | `ticks=` | `dots`, `lines`, `none` — the printed ring | `dots` |
//! | `legend=top` | the legend over the knob | under |
//!
//! [`Look`] is the same set as a struct, and [`build`] places one from Rust.

use crate::document::proto::{Node, NodeId, NodeKind, TextAlign, TextCase};
use crate::surface::proto::{ControlKind, SlotRole};
use crate::surface::widgets::Builder;

use super::light;
use super::paint::{luma, ring};
use crate::surface::kit::controls::parts::{PANEL_INK, alpha, lin, rad, shade, tint};
use crate::surface::kit::draw;
use crate::surface::kit::registry::{ComponentDef, Family, Place};

/// The cap's shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cap {
    /// A flat-topped cylinder with a bevelled edge.
    Flat,
    /// A dome.
    Domed,
    /// Big soft flutes round a flat top (the "jedi" / MXR knob).
    Fluted,
    /// Fine straight knurling round a flat top.
    Knurled,
    /// A chicken-head: a round boss with a long tapered beak.
    ChickenHead,
    /// A teardrop pointer cap on a skirt.
    Pointer,
}

impl Cap {
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s {
            "domed" => Self::Domed,
            "fluted" => Self::Fluted,
            "knurled" => Self::Knurled,
            "chicken_head" => Self::ChickenHead,
            "pointer" => Self::Pointer,
            _ => Self::Flat,
        }
    }
}

/// The skirt under the cap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skirt {
    None,
    /// A plain disc a little wider than the cap.
    Plain,
    /// A wide flared skirt (a Boss / MXR skirt).
    Flared,
    /// A knurled skirt: teeth round its edge.
    Ridged,
    /// A polished metal skirt.
    Chrome,
}

/// What the cap is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnobMaterial {
    Plastic,
    Rubber,
    /// Machined metal: a spun face.
    Metal,
    Chrome,
    /// Glossy phenolic, a little warmer and deeper than plastic.
    Bakelite,
}

/// Printed ring round the travel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ticks {
    Dots,
    Lines,
    None,
}

/// Everything a rendered knob varies by.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub cap: Cap,
    pub skirt: Skirt,
    pub material: KnobMaterial,
    pub color: String,
    pub skirt_color: String,
    pub pointer: String,
    /// Height of the cap's edge, 0..2 (1 = default).
    pub bevel: f64,
    /// Gloss hotspot strength, 0..2.
    pub specular: f64,
    /// Occlusion and cast-shadow strength, 0..2.
    pub shadow: f64,
    pub ticks: Ticks,
    pub legend_top: bool,
}

impl Look {
    /// The defaults for a cap shape.
    #[must_use]
    pub fn of(cap: Cap) -> Self {
        let (skirt, color) = match cap {
            Cap::Flat => (Skirt::Plain, "#1d1d1f"),
            Cap::Domed => (Skirt::None, "#19191b"),
            Cap::Fluted => (Skirt::None, "#1b1b1d"),
            Cap::Knurled => (Skirt::None, "#2a2b2d"),
            Cap::ChickenHead => (Skirt::None, "#161617"),
            Cap::Pointer => (Skirt::Ridged, "#ece8df"),
        };
        let pointer = if luma(color) > 0.5 {
            "#1a1a1a"
        } else {
            "#f2f0ea"
        };
        Self {
            cap,
            skirt,
            material: KnobMaterial::Plastic,
            color: color.into(),
            skirt_color: if cap == Cap::Pointer {
                "#18181a".into()
            } else {
                shade(color, 0.25)
            },
            pointer: pointer.into(),
            bevel: 1.0,
            specular: 1.0,
            shadow: 1.0,
            ticks: Ticks::Dots,
            legend_top: false,
        }
    }

    /// The look a `Place` asks for (variant = cap, options as documented).
    #[must_use]
    pub fn from_place(p: &Place) -> Self {
        let mut l = Self::of(Cap::parse(p.variant.as_deref().unwrap_or("flat")));
        let num = |k: &str| p.opt(k).and_then(|v| v.parse::<f64>().ok());
        if let Some(m) = p.opt("material") {
            l.material = match m {
                "rubber" => KnobMaterial::Rubber,
                "metal" | "spun" => KnobMaterial::Metal,
                "chrome" => KnobMaterial::Chrome,
                "bakelite" => KnobMaterial::Bakelite,
                _ => KnobMaterial::Plastic,
            };
            if matches!(l.material, KnobMaterial::Metal) && p.opt("color").is_none() {
                l.color = "#c4c6c8".into();
                l.pointer = "#1a1a1a".into();
            }
            if matches!(l.material, KnobMaterial::Chrome) && p.opt("color").is_none() {
                l.color = "#d9dde1".into();
                l.pointer = "#1a1a1a".into();
            }
            if matches!(l.material, KnobMaterial::Rubber) {
                l.specular = 0.35;
            }
        }
        if let Some(c) = p.opt("color").filter(|c| c.starts_with('#')) {
            l.color = c.into();
            if p.opt("pointer").is_none() {
                l.pointer = if luma(c) > 0.5 {
                    "#1a1a1a".into()
                } else {
                    "#f2f0ea".into()
                };
            }
            if p.opt("skirt_color").is_none() && l.cap != Cap::Pointer {
                l.skirt_color = shade(c, 0.25);
            }
        }
        if let Some(s) = p.opt("skirt") {
            l.skirt = match s {
                "plain" => Skirt::Plain,
                "flared" => Skirt::Flared,
                "ridged" => Skirt::Ridged,
                "chrome" => Skirt::Chrome,
                _ => Skirt::None,
            };
        }
        if let Some(c) = p.opt("skirt_color").filter(|c| c.starts_with('#')) {
            l.skirt_color = c.into();
        }
        if let Some(c) = p.opt("pointer").filter(|c| c.starts_with('#')) {
            l.pointer = c.into();
        }
        l.bevel = num("bevel").unwrap_or(l.bevel).clamp(0.0, 2.0);
        l.specular = num("spec").unwrap_or(l.specular).clamp(0.0, 2.0);
        l.shadow = num("shadow").unwrap_or(l.shadow).clamp(0.0, 2.0);
        l.ticks = match p.opt("ticks") {
            Some("lines") => Ticks::Lines,
            Some("none") => Ticks::None,
            _ => Ticks::Dots,
        };
        l.legend_top = p.opt("legend") == Some("top");
        l
    }
}

/// A circle as a path (for joining into multi-shape paths).
fn circle_d(cx: f64, cy: f64, r: f64) -> String {
    format!(
        "M {:.3} {cy:.3} A {r:.3} {r:.3} 0 1 1 {:.3} {cy:.3} A {r:.3} {r:.3} 0 1 1 {:.3} {cy:.3} Z ",
        cx - r,
        cx + r,
        cx - r
    )
}

fn seg(a: (f64, f64), b: (f64, f64), w: f64) -> String {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = dx.hypot(dy).max(1e-6);
    let (nx, ny) = (-dy / len * w / 2.0, dx / len * w / 2.0);
    format!(
        "{} ",
        draw::polygon(&[
            (a.0 + nx, a.1 + ny),
            (b.0 + nx, b.1 + ny),
            (b.0 - nx, b.1 - ny),
            (a.0 - nx, a.1 - ny)
        ])
    )
}

/// Smooth lobes: `lobes` bumps of `depth` on radius `r` (outer edge `r`).
fn lobes(cx: f64, cy: f64, r: f64, lobes: u32, depth: f64, sharp: f64) -> String {
    let n = (lobes * 16) as usize;
    let pts: Vec<(f64, f64)> = (0..n)
        .map(|i| {
            let t = i as f64 / n as f64;
            let c = (t * f64::from(lobes) * std::f64::consts::TAU).cos();
            // `sharp` > 1 flattens the tops and pinches the valleys.
            let s = c.signum() * c.abs().powf(1.0 / sharp);
            polar_pt(cx, cy, r - depth * 0.5 + depth * 0.5 * s, t * 360.0)
        })
        .collect();
    draw::polygon(&pts)
}

fn polar_pt(cx: f64, cy: f64, r: f64, deg: f64) -> (f64, f64) {
    draw::polar(cx, cy, r, deg)
}

fn caption(s: &str, cx: f64, cy: f64, w: f64, size: f64, ink: &str, case: TextCase) -> Node {
    let mut n = draw::text(
        "legend",
        s,
        cx - w / 2.0,
        cy - size * 0.68,
        w,
        size,
        ink,
        TextAlign::Center,
    );
    if let NodeKind::Text(t) = &mut n.kind {
        t.font_weight = Some(700.0);
        t.text_case = case;
        t.letter_spacing = size * 0.06;
    }
    n
}

/// A radial fill about the shaft (rotation-invariant): `edge` at the rim,
/// `mid` through the face.
fn about_shaft(mid: &str, edge: &str) -> crate::document::proto::Paint {
    draw::radial(&[(0.0, mid), (0.7, mid), (1.0, edge)])
}

/// A knob's travel in degrees: `Place::sweep`, else a stepped knob's detents
/// (its bare options), else 300°.
#[must_use]
pub fn travel(p: &Place) -> f64 {
    let steps = p
        .options
        .iter()
        .flatten()
        .filter(|o| !o.contains('='))
        .count();
    p.sweep.unwrap_or(if steps > 0 {
        crate::surface::kit::registry::detent_sweep(steps)
    } else {
        300.0
    })
}

/// The `cap` frame [`build_with`] makes, in the knob's own frame: `(side,
/// footprint radius)` — a square `side` wide with the knob `radius` round
/// in its middle.
#[must_use]
pub fn cap_box(p: &Place) -> (f64, f64) {
    let d = p.size.unwrap_or(48.0).max(16.0);
    let r = d / 2.0;
    let dot = (d * 0.032).max(0.9);
    (2.0 * (r * 1.26 + dot + d * 0.1), r)
}

/// Place a rendered knob: the builder behind `knob.render`.
pub fn build(b: &mut Builder<'_>, parent: NodeId, p: &Place, look: &Look) -> NodeId {
    build_with(b, parent, p, look, None)
}

/// [`build`], with everything that stands off the panel — occlusion,
/// skirt, wall, the turning body, the lit overlays — in a `cap` frame (the
/// knob's box less its printed ring) carrying `shader`: a GPU renderer
/// draws the knob with it (`knob.3d`), everything else this vector knob.
#[allow(clippy::too_many_lines)]
pub fn build_with(
    b: &mut Builder<'_>,
    parent: NodeId,
    p: &Place,
    look: &Look,
    shader: Option<crate::document::proto::ShaderFx>,
) -> NodeId {
    let d = p.size.unwrap_or(48.0).max(16.0);
    let r = d / 2.0;
    let k = d / 48.0;
    let ink = p.ink_color(PANEL_INK);
    let labels: Vec<String> = p
        .options
        .iter()
        .flatten()
        .filter(|o| !o.contains('='))
        .cloned()
        .collect();
    let stepped = !labels.is_empty();
    let labels = if stepped {
        labels
    } else {
        p.scale.clone().unwrap_or_default()
    };
    let sweep = travel(p);
    let print_ring = p.scale.as_ref().is_none_or(|s| !s.is_empty()) && look.ticks != Ticks::None;

    let ring_r = r * 1.26;
    let dot = (d * 0.032).max(0.9);
    let half = ring_r + dot + d * 0.1;
    let side = 2.0 * half;
    let c = half;
    let legend = p.legend.as_deref().unwrap_or("");
    let lsize = (d * 0.2).max(8.0);
    let lh = lsize * 1.9;
    let top = if look.legend_top { lh } else { 0.0 };
    let name = if p.name.is_empty() {
        "Rendered knob"
    } else {
        p.name.as_str()
    };
    let root = b.add(parent, draw::frame(name, 0.0, 0.0, side, side + lh));
    let knob = b.add(root, draw::frame("knob", 0.0, top, side, side));

    // ── printed ring (fixed, the panel's ink) ──
    if print_ring {
        let n = if labels.is_empty() { 11 } else { labels.len() };
        let angle = |i: usize| (i as f64 / (n.max(2) - 1) as f64 - 0.5) * sweep;
        let dd: String = match look.ticks {
            Ticks::Lines => (0..n)
                .map(|i| {
                    seg(
                        draw::polar(c, c, ring_r - dot * 1.5, angle(i)),
                        draw::polar(c, c, ring_r + dot * 1.5, angle(i)),
                        dot,
                    )
                })
                .collect(),
            _ => (0..n)
                .map(|i| {
                    let (x, y) = draw::polar(c, c, ring_r, angle(i));
                    circle_d(x, y, dot)
                })
                .collect(),
        };
        b.add(
            knob,
            draw::path("scale dots", 0.0, 0.0, side, side, dd, draw::solid(&ink)),
        );
        let size = (d * 0.17).max(draw::SCALE_TYPE_MIN);
        for (i, s) in labels.iter().enumerate() {
            if s.is_empty() {
                continue;
            }
            let mut t = draw::radial_label(
                "numeral",
                s,
                (c, c),
                ring_r + dot * 2.0,
                angle(i),
                size,
                &ink,
            );
            if let NodeKind::Text(td) = &mut t.kind {
                td.text_case = p.case();
                td.font_weight = Some(600.0);
            }
            b.add(knob, t);
        }
    }

    // Everything below stands off the panel: under a shader, in its own frame.
    let knob = match shader {
        Some(fx) => {
            let mut cap = draw::frame("cap", 0.0, 0.0, side, side);
            cap.shader = Some(fx);
            b.add(knob, cap)
        }
        None => knob,
    };

    // ── geometry ──
    let has_skirt = look.skirt != Skirt::None;
    let cap_r = match (look.cap, has_skirt) {
        (Cap::Pointer, _) => r * 0.82,
        (_, true) => {
            if look.skirt == Skirt::Flared {
                r * 0.7
            } else {
                r * 0.8
            }
        }
        (_, false) => r,
    };
    // The cap's flat top, inside its edge.
    let face_r = match look.cap {
        Cap::Fluted => cap_r * 0.74,
        Cap::Knurled => cap_r * 0.8,
        Cap::Domed => cap_r * 0.97,
        Cap::ChickenHead => cap_r * 0.56,
        Cap::Pointer => cap_r * 0.62,
        Cap::Flat => cap_r * (0.9 - 0.04 * look.bevel),
    };
    // How far the knob stands off the panel, for its shadows and wall.
    let height = d * 0.34 * (0.6 + 0.4 * look.bevel);
    let wall = d * 0.085 * look.bevel;
    let col = look.color.as_str();
    let (lit_edge, dark_edge) = (tint(col, 0.35), shade(col, 0.55));

    // ── fixed tiers: occlusion, skirt, wall ──
    let (ox, oy) = light::shadow_offset(d * 0.1);
    let ao = draw::circle(
        "occlusion",
        c + ox,
        c + oy,
        r * 1.14,
        draw::radial(&[
            (0.0, &alpha("#000000", 0.7 * look.shadow.min(1.4))),
            (0.72, &alpha("#000000", 0.45 * look.shadow.min(1.4))),
            (1.0, "#00000000"),
        ]),
    );
    b.add(knob, ao);
    let outer_r = match (look.cap, has_skirt) {
        (_, true) => r,
        (Cap::ChickenHead, false) => cap_r * 0.62,
        _ => cap_r,
    };
    let foot_col = if has_skirt {
        look.skirt_color.as_str()
    } else {
        col
    };
    // The side of the knob (or its skirt), seen under the top: lit on the
    // light's side, falling into shadow on the far side.
    let side_paint = lin(
        light::sheen_angle() - 55.0,
        &[
            (0.0, tint(foot_col, 0.12)),
            (0.45, shade(foot_col, 0.3)),
            (1.0, shade(foot_col, 0.7)),
        ],
    );
    let foot_drop = if has_skirt { wall * 0.6 } else { wall };
    let foot = draw::with_effects(
        draw::circle("foot", c, c + foot_drop, outer_r, side_paint),
        light::stand(height, look.shadow),
    );
    b.add(knob, foot);
    if has_skirt {
        let s = look.skirt_color.as_str();
        let paint = match look.skirt {
            Skirt::Chrome => draw::linear(
                light::sheen_angle(),
                &[
                    (0.0, "#f4f6f8"),
                    (0.35, "#aeb4ba"),
                    (0.5, "#3a3e44"),
                    (0.62, "#8d949b"),
                    (1.0, "#e3e6e9"),
                ],
            ),
            _ => lin(
                light::sheen_angle(),
                &[
                    (0.0, tint(s, 0.22)),
                    (0.5, s.to_string()),
                    (1.0, shade(s, 0.45)),
                ],
            ),
        };
        b.add(knob, draw::circle("skirt", c, c, r, paint));
        if look.skirt == Skirt::Flared {
            // The flare's slope: lit toward the light, a crisp lip at the cap.
            b.add(
                knob,
                draw::path(
                    "skirt slope",
                    0.0,
                    0.0,
                    side,
                    side,
                    ring(c, c, cap_r, r),
                    light::sheen(0.22, 0.3),
                ),
            );
        }
    }
    // The cap's side wall, seen under its top edge.
    if has_skirt || look.cap == Cap::ChickenHead {
        let wall_r = match look.cap {
            Cap::Pointer => cap_r * 0.84,
            Cap::ChickenHead => cap_r * 0.62,
            _ => cap_r,
        };
        let wp = lin(
            light::sheen_angle() - 55.0,
            &[
                (0.0, tint(col, 0.1)),
                (0.45, shade(col, 0.3)),
                (1.0, shade(col, 0.7)),
            ],
        );
        b.add(
            knob,
            draw::circle(
                "wall",
                c,
                c + wall * if has_skirt { 0.7 } else { 1.0 },
                wall_r,
                wp,
            ),
        );
    }

    // ── the body: what turns ──
    let body = b.add(knob, draw::frame("body", 0.0, 0.0, side, side));
    let mid = col.to_string();
    let edge = shade(col, 0.35);
    if look.skirt == Skirt::Ridged {
        let teeth = ((r * std::f64::consts::TAU) / (2.4 * k))
            .round()
            .clamp(24.0, 72.0) as u32;
        let s = look.skirt_color.as_str();
        b.add(
            body,
            draw::path(
                "skirt teeth",
                0.0,
                0.0,
                side,
                side,
                lobes(c, c, r, teeth, r * 0.06, 1.0),
                about_shaft(&tint(s, 0.05), &shade(s, 0.4)),
            ),
        );
    }
    match look.cap {
        Cap::Fluted => {
            b.add(
                body,
                draw::path(
                    "flutes",
                    0.0,
                    0.0,
                    side,
                    side,
                    lobes(c, c, cap_r, 8, cap_r * 0.13, 1.6),
                    about_shaft(&mid, &shade(col, 0.2)),
                ),
            );
            // The flutes' grooves, as darker bands down each valley.
            let grooves: String = (0..8)
                .map(|i| {
                    let a = (f64::from(i) + 0.5) * 45.0;
                    seg(
                        draw::polar(c, c, face_r * 1.02, a),
                        draw::polar(c, c, cap_r * 0.9, a),
                        cap_r * 0.07,
                    )
                })
                .collect();
            b.add(
                body,
                draw::path(
                    "grooves",
                    0.0,
                    0.0,
                    side,
                    side,
                    grooves,
                    draw::solid(&alpha("#000000", 0.35)),
                ),
            );
            b.add(
                body,
                draw::circle("face", c, c, face_r, about_shaft(&tint(col, 0.04), &mid)),
            );
        }
        Cap::Knurled => {
            let teeth = ((cap_r * std::f64::consts::TAU) / (2.2 * k))
                .round()
                .clamp(24.0, 90.0) as u32;
            b.add(
                body,
                draw::path(
                    "knurl",
                    0.0,
                    0.0,
                    side,
                    side,
                    lobes(c, c, cap_r, teeth, cap_r * 0.05, 1.0),
                    about_shaft(&mid, &edge),
                ),
            );
            let ribs: String = (0..teeth)
                .map(|i| {
                    let a = f64::from(i) * 360.0 / f64::from(teeth);
                    seg(
                        draw::polar(c, c, face_r, a),
                        draw::polar(c, c, cap_r * 0.97, a),
                        (cap_r * std::f64::consts::TAU / f64::from(teeth)) * 0.35,
                    )
                })
                .collect();
            b.add(
                body,
                draw::path(
                    "ribs",
                    0.0,
                    0.0,
                    side,
                    side,
                    ribs,
                    draw::solid(&alpha("#000000", 0.4)),
                ),
            );
            b.add(
                body,
                draw::circle("face", c, c, face_r, about_shaft(&tint(col, 0.03), &mid)),
            );
        }
        Cap::ChickenHead => {
            // The boss and the beak: one outline, tapering to the tip.
            let (bw, tip) = (cap_r * 0.62, r * 0.98);
            let base = draw::polygon(&[
                draw::polar(c, c, bw * 1.18, 180.0),
                draw::polar(c, c, bw * 0.95, -120.0),
                draw::polar(c, c, bw * 0.92, -50.0),
                draw::polar(c, c, tip * 0.9, -7.0),
                draw::polar(c, c, tip, 0.0),
                draw::polar(c, c, tip * 0.9, 7.0),
                draw::polar(c, c, bw * 0.92, 50.0),
                draw::polar(c, c, bw * 0.95, 120.0),
            ]);
            let d_all = format!("{base} {}", circle_d(c, c, bw));
            let wing = draw::with_effects(
                draw::path(
                    "wing",
                    0.0,
                    0.0,
                    side,
                    side,
                    d_all,
                    about_shaft(&mid, &edge),
                ),
                vec![draw::drop_shadow(
                    &alpha("#000000", 0.5 * look.shadow),
                    0.0,
                    0.0,
                    d * 0.05,
                )],
            );
            b.add(body, wing);
            b.add(
                body,
                draw::circle("face", c, c, face_r, about_shaft(&tint(col, 0.05), &mid)),
            );
        }
        Cap::Pointer => {
            let tip = r * 1.02;
            let drop = format!(
                "{} {}",
                draw::polygon(&[
                    draw::polar(c, c, cap_r * 0.8, -42.0),
                    draw::polar(c, c, tip * 0.88, -10.0),
                    draw::polar(c, c, tip, 0.0),
                    draw::polar(c, c, tip * 0.88, 10.0),
                    draw::polar(c, c, cap_r * 0.8, 42.0)
                ]),
                circle_d(c, c, cap_r * 0.84)
            );
            let cap = draw::with_effects(
                draw::path(
                    "cap",
                    0.0,
                    0.0,
                    side,
                    side,
                    drop,
                    about_shaft(&mid, &shade(col, 0.18)),
                ),
                vec![draw::drop_shadow(
                    &alpha("#000000", 0.55 * look.shadow),
                    0.0,
                    0.0,
                    d * 0.06,
                )],
            );
            b.add(body, cap);
        }
        Cap::Flat | Cap::Domed => {
            let (m, e) = if look.cap == Cap::Domed {
                (tint(col, 0.06), shade(col, 0.45))
            } else {
                (mid.clone(), edge.clone())
            };
            b.add(body, draw::circle("cap", c, c, cap_r, about_shaft(&m, &e)));
        }
    }
    // The pointer: a line from near the centre to the edge (the pointer cap's
    // own point is the index; its line runs up it).
    let (p0, p1, pw) = match look.cap {
        Cap::Pointer => (cap_r * 0.2, r * 0.86, d * 0.05),
        Cap::ChickenHead => (cap_r * 0.15, r * 0.9, d * 0.045),
        Cap::Fluted | Cap::Knurled => (face_r * 0.3, face_r * 0.96, d * 0.055),
        _ => (cap_r * 0.3, cap_r * 0.9, d * 0.055),
    };
    // A pointer cap's own point is its index; it takes a line only when
    // `pointer=` asks for one.
    if look.cap != Cap::Pointer || p.opt("pointer").is_some() {
        let pointer = draw::rounded(
            draw::rect(
                "pointer",
                c - pw / 2.0,
                c - p1,
                pw,
                p1 - p0,
                draw::solid(&look.pointer),
            ),
            pw / 2.0,
        );
        b.add(
            body,
            draw::with_effects(
                pointer,
                vec![draw::drop_shadow("#00000066", 0.0, 0.0, pw * 0.6)],
            ),
        );
    }

    // ── fixed overlays: the room's light on the cap ──
    let spec = look.specular;
    let (hx, hy) = light::hotspot();
    let face_light = match (look.material, look.cap) {
        (KnobMaterial::Rubber, _) => light::sheen(0.08, 0.28),
        (KnobMaterial::Chrome, _) => draw::linear(
            light::sheen_angle(),
            &[
                (0.0, "#ffffffa0"),
                (0.3, "#ffffff30"),
                (0.48, "#00000070"),
                (0.56, "#00000030"),
                (1.0, "#ffffff50"),
            ],
        ),
        (_, Cap::Domed) => draw::radial_at(
            hx,
            hy,
            0.9,
            &[
                (0.0, &alpha("#ffffff", 0.3)),
                (0.45, "#ffffff08"),
                (0.75, "#00000030"),
                (1.0, "#00000090"),
            ],
        ),
        _ => light::sheen(0.14, 0.32),
    };
    b.add(knob, draw::circle("face light", c, c, face_r, face_light));
    if look.material == KnobMaterial::Metal {
        // A spun face: the bow-tie sheen lined up with the light, and rings.
        let stops = [
            (0.0, "#ffffff90"),
            (0.125, "#00000040"),
            (0.25, "#ffffff10"),
            (0.375, "#00000040"),
            (0.5, "#ffffff90"),
            (0.625, "#00000040"),
            (0.75, "#ffffff10"),
            (0.875, "#00000040"),
            (1.0, "#ffffff90"),
        ];
        b.add(
            knob,
            draw::circle(
                "spun light",
                c,
                c,
                face_r,
                draw::conic(light::AZIMUTH, &stops),
            ),
        );
        let mut rings = String::new();
        let mut rr = face_r * 0.1;
        while rr < face_r {
            rings.push_str(&ring(c, c, rr, rr + 0.25 * k));
            rr += 1.1 * k;
        }
        b.add(
            knob,
            draw::path(
                "spun light rings",
                0.0,
                0.0,
                side,
                side,
                rings,
                draw::solid("#0000001a"),
            ),
        );
    }
    // The rim: lit on the upper left, dark on the lower right.
    let bevel_w = (d * 0.05 * look.bevel).max(0.6);
    let rim_outer = match look.cap {
        Cap::Fluted | Cap::Knurled | Cap::ChickenHead | Cap::Pointer => face_r,
        _ => cap_r,
    };
    let rim = draw::linear(
        light::sheen_angle(),
        &[
            (0.0, &alpha(&lit_edge, 0.9)),
            (0.35, &alpha(&lit_edge, 0.25)),
            (0.55, "#00000000"),
            (1.0, &alpha(&dark_edge, 0.8)),
        ],
    );
    b.add(
        knob,
        draw::path(
            "rim light",
            0.0,
            0.0,
            side,
            side,
            ring(c, c, rim_outer - bevel_w, rim_outer),
            rim,
        ),
    );
    if matches!(look.cap, Cap::Fluted | Cap::Knurled) {
        // Light across the flutes' tops: over the band inside the valleys.
        let valley = if look.cap == Cap::Fluted {
            cap_r * 0.87
        } else {
            cap_r * 0.95
        };
        b.add(
            knob,
            draw::path(
                "flute light",
                0.0,
                0.0,
                side,
                side,
                ring(c, c, face_r, valley),
                light::sheen(0.28, 0.45),
            ),
        );
    }
    if has_skirt && look.skirt != Skirt::Ridged {
        let sr = draw::linear(
            light::sheen_angle(),
            &[(0.0, "#ffffff55"), (0.4, "#ffffff00"), (1.0, "#00000066")],
        );
        b.add(
            knob,
            draw::path(
                "skirt rim light",
                0.0,
                0.0,
                side,
                side,
                ring(c, c, r - bevel_w * 0.7, r),
                sr,
            ),
        );
    }
    if spec > 0.0 {
        let gloss = match look.material {
            KnobMaterial::Rubber => 0.25,
            KnobMaterial::Bakelite => 1.1,
            KnobMaterial::Chrome => 1.2,
            _ => 0.85,
        };
        let a = (0.75 * spec * gloss).min(1.0);
        let (sx, sy) = (
            c + (hx - 0.5) * 2.0 * face_r * 0.9,
            c + (hy - 0.5) * 2.0 * face_r * 0.9,
        );
        let sr = face_r * if look.cap == Cap::Domed { 0.42 } else { 0.3 };
        b.add(
            knob,
            draw::circle(
                "specular light",
                sx,
                sy,
                sr,
                rad(
                    0.5,
                    0.5,
                    0.5,
                    &[
                        (0.0, alpha("#ffffff", a)),
                        (0.35, alpha("#ffffff", a * 0.35)),
                        (1.0, "#ffffff00".into()),
                    ],
                ),
            ),
        );
        if look.cap == Cap::ChickenHead || look.cap == Cap::Pointer {
            // The beak's ridge catches a line of light along its left flank.
            let hl = draw::path(
                "ridge light",
                0.0,
                0.0,
                side,
                side,
                draw::sector(c, c, face_r * 0.86, face_r, -150.0, -20.0),
                draw::solid(&alpha("#ffffff", 0.3 * spec)),
            );
            b.add(knob, hl);
        }
    }

    // ── legend ──
    let ly = if look.legend_top {
        lsize * 0.95
    } else {
        top + side - (half - ring_r - dot) * 0.2 + lsize * 0.4
    };
    let legend_node = b.add(
        root,
        caption(legend, c, ly, side * 1.4, lsize, &ink, p.case()),
    );
    let ctl = p
        .control(ControlKind::Knob {
            sweep_degrees: sweep,
        })
        .slot(SlotRole::Needle, body.0)
        .slot(SlotRole::Label, legend_node.0);
    b.controls.insert(root, ctl);
    if let Some(touch) = p.touch {
        crate::surface::kit::registry::tighten(b, root, (c, c + top), d, touch);
    }
    root
}

fn render(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    build(b, parent, p, &Look::from_place(p))
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![ComponentDef {
        id: "knob.render",
        family: Family::Knob,
        title: "Rendered knob",
        description: "A 3D-rendered plugin knob seen from above: a cap (variant: `flat`, `domed`, `fluted`, `knurled`, `chicken_head`, `pointer` teardrop) on an optional skirt, with soft occlusion under it, a rim catching the upper-left light, a fixed gloss hotspot, and the pointer turning. Options: `material=plastic|rubber|metal|chrome|bakelite`, `color=#hex`, `skirt=none|plain|flared|ridged|chrome`, `skirt_color=#hex`, `pointer=#hex`, `bevel=`/`spec=`/`shadow=` (0..2), `ticks=dots|lines|none`, `legend=top`. For pedal and amp faces in the Neural-render style. Size = knob diameter.",
        variants: &[
            "flat",
            "domed",
            "fluted",
            "knurled",
            "chicken_head",
            "pointer",
        ],
        default_size: 48.0,
        binds: "param",
        build: render,
    }]
}
