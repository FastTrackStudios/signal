//! `panel.amp_head` and `panel.amp_combo`: an amp cabinet as a product
//! render shows it — a box covered in tolex (or any material) whose rounded
//! edges catch the light, metal corner protectors, a handle on top, rubber
//! feet under it, and a front baffle holding a faceplate slot and a grille
//! area framed by piping, each with its own depth (the faceplate stands
//! proud, the grille sits back).
//!
//! ```text
//! <root>                       w × h (handle and feet included)
//!   feet, handle strap          (behind the box)
//!   box: material, bevel, shadow
//!   faceplate  ← a frame: its material, bevel, screws; put controls in it
//!   grille     ← a frame: its cloth or metal, recessed; piping round it
//!   corners, handle brackets    (in front)
//! ```
//!
//! Options (both): `w=`, `h=` (or `Place::size` = width), `tolex=` any
//! material spec (`tolex:bronco`, `leather`, `croc`, `wood`, …; default
//! `tolex:levant`) painted `color=#hex`; `grille=` a material spec
//! (`cloth:wheat`, `cloth:oxblood`, `cloth:basket`, `cloth:black`,
//! `diamond`, `chevron`, `perforated`, `expanded`); `face=` the
//! faceplate's material (`brushed`, `powder`, `plastic`, `chrome`, …) in
//! `face_color=#hex`; `face_at=top|bottom`; `face_frac=` its share of the
//! baffle; `piping=#hex|none`; `corners=chrome|black|brass|none`;
//! `handle=leather|strap|none` (`handle_color=#hex`); `feet=none`.
//!
//! [`cabinet`] builds one from Rust and returns the faceplate and grille
//! frames too.

use document_proto::{CornerRadii, NodeId};
use surface_widgets::Builder;

use super::enclosure::screw;
use super::light;
use super::paint::{Material, fill};
use crate::controls::parts::{alpha, fixed, lin, shade, tint};
use crate::draw;
use crate::registry::{ComponentDef, Family, Place};

/// Head or combo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Head,
    Combo,
}

/// The handle on top.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handle {
    /// A stitched leather handle arching between two chrome brackets.
    Leather,
    /// A flat moulded strap lying across the top.
    Strap,
    None,
}

/// Everything a cabinet varies by.
#[derive(Debug, Clone, PartialEq)]
pub struct CabinetSpec {
    pub kind: Kind,
    pub w: f64,
    pub h: f64,
    pub covering: Material,
    pub grille: Material,
    pub face: Material,
    pub face_at_top: bool,
    /// The faceplate's share of the baffle's height.
    pub face_frac: f64,
    pub piping: Option<String>,
    /// Corner protectors: `chrome`, `black`, `brass`, or none.
    pub corners: Option<String>,
    pub handle: Handle,
    pub handle_color: String,
    pub feet: bool,
}

impl CabinetSpec {
    /// Defaults for a kind at `w × h`.
    #[must_use]
    pub fn new(kind: Kind, w: f64, h: f64) -> Self {
        Self {
            kind,
            w,
            h,
            covering: Material::Tolex {
                grain: super::paint::Grain::Levant,
                color: "#1c1b1a".into(),
                scale: 1.0,
            },
            grille: Material::GrilleCloth {
                cloth: super::paint::Cloth::Wheat,
            },
            face: Material::Brushed {
                color: "#b8babc".into(),
                dir: super::paint::Dir::Horizontal,
            },
            face_at_top: true,
            face_frac: if kind == Kind::Head { 0.36 } else { 0.24 },
            piping: Some("#d8cfb8".into()),
            corners: Some("chrome".into()),
            handle: if kind == Kind::Head {
                Handle::Leather
            } else {
                Handle::Strap
            },
            handle_color: "#6e4128".into(),
            feet: true,
        }
    }

    /// The spec a `Place` asks for.
    #[must_use]
    pub fn from_place(kind: Kind, p: &Place) -> Self {
        let num = |k: &str| p.opt(k).and_then(|v| v.parse::<f64>().ok());
        let (dw, aspect) = if kind == Kind::Head {
            (900.0, 0.44)
        } else {
            (900.0, 0.7)
        };
        let w = num("w").or(p.size).unwrap_or(dw);
        let h = num("h").unwrap_or(w * aspect);
        let mut s = Self::new(kind, w, h);
        let color = p.opt("color").filter(|c| c.starts_with('#'));
        if let Some(m) = Material::parse(p.opt("tolex").unwrap_or("tolex:levant"), color) {
            s.covering = m;
        }
        if let Some(m) = p
            .opt("grille")
            .and_then(|g| Material::parse(g, p.opt("grille_color")))
        {
            s.grille = m;
        }
        if let Some(m) = Material::parse(
            p.opt("face").unwrap_or("brushed"),
            p.opt("face_color").or(Some("#b8babc")),
        ) {
            s.face = m;
        }
        s.face_at_top = p.opt("face_at") != Some("bottom");
        if let Some(f) = num("face_frac") {
            s.face_frac = f.clamp(0.1, 0.9);
        }
        match p.opt("piping") {
            Some("none") => s.piping = None,
            Some(c) if c.starts_with('#') => s.piping = Some(c.into()),
            _ => {}
        }
        match p.opt("corners") {
            Some("none") => s.corners = None,
            Some(c) => s.corners = Some(c.into()),
            None => {}
        }
        s.handle = match p.opt("handle") {
            Some("leather") => Handle::Leather,
            Some("strap") => Handle::Strap,
            Some("none") => Handle::None,
            _ => s.handle,
        };
        if let Some(c) = p.opt("handle_color").filter(|c| c.starts_with('#')) {
            s.handle_color = c.into();
        }
        s.feet = p.opt("feet") != Some("none");
        s
    }
}

/// A built cabinet: its root and the two frames to fill.
#[derive(Debug, Clone, Copy)]
pub struct Cabinet {
    pub root: NodeId,
    /// The faceplate: put controls and silkscreen in it.
    pub faceplate: NodeId,
    /// The grille area (cloth or metal): put a logo in it.
    pub grille: NodeId,
}

fn metal(kind: &str) -> [&'static str; 4] {
    match kind {
        "black" => ["#5a5c60", "#2a2b2e", "#111214", "#3a3c40"],
        "brass" => ["#fbe7ae", "#c9a45a", "#6d5423", "#a88540"],
        _ => ["#ffffff", "#c8cdd2", "#555b62", "#9aa1a8"],
    }
}

/// A corner protector over the box's corner at (`x`, `y`), `s` px, facing
/// `(fx, fy)` (−1 = left/top, +1 = right/bottom), rounded `r` outside.
fn corner(
    b: &mut Builder<'_>,
    parent: NodeId,
    (x, y): (f64, f64),
    s: f64,
    r: f64,
    (fx, fy): (f64, f64),
    kind: &str,
) {
    let m = metal(kind);
    let mut n = draw::rect(
        "corner",
        x,
        y,
        s,
        s,
        draw::linear(
            light::sheen_angle(),
            &[(0.0, m[0]), (0.35, m[1]), (0.7, m[2]), (1.0, m[3])],
        ),
    );
    let (o, i) = (r.max(s * 0.3), s * 0.18);
    n.corner_radii = CornerRadii {
        top_left: if fx < 0.0 && fy < 0.0 { o } else { i },
        top_right: if fx > 0.0 && fy < 0.0 { o } else { i },
        bottom_right: if fx > 0.0 && fy > 0.0 { o } else { i },
        bottom_left: if fx < 0.0 && fy > 0.0 { o } else { i },
    };
    b.add(
        parent,
        fixed(draw::with_effects(n.clone(), light::stand(s * 0.18, 1.0))),
    );
    for mut e in light::edge("corner edge", (x, y, s, s), 0.0, s * 0.08, 1.2, true) {
        e.corner_radii = n.corner_radii;
        b.add(parent, e);
    }
    // The rivet: toward the corner.
    let (cx, cy) = (x + s * (0.5 - fx * 0.12), y + s * (0.5 - fy * 0.12));
    let (hx, hy) = light::hotspot();
    let rivet = draw::circle(
        "rivet",
        cx,
        cy,
        s * 0.12,
        draw::radial_at(hx, hy, 0.8, &[(0.0, m[0]), (0.6, m[1]), (1.0, m[2])]),
    );
    b.add(
        parent,
        fixed(draw::with_effects(rivet, light::stand(s * 0.05, 0.9))),
    );
}

/// Piping round `(x, y, w, h)`: a cord `t` px thick, with its shadow on the
/// baffle and a lit line along its top.
fn piping(
    b: &mut Builder<'_>,
    parent: NodeId,
    (x, y, w, h): (f64, f64, f64, f64),
    r: f64,
    t: f64,
    color: &str,
) {
    let (sx, sy) = light::shadow_offset(t * 1.2);
    let (lx, ly) = light::toward_light();
    let ring = |name: &str, dx: f64, dy: f64, c: &str, wt: f64| {
        let n = draw::rounded(
            draw::rect(
                name,
                x + dx - t / 2.0,
                y + dy - t / 2.0,
                w + t,
                h + t,
                draw::solid("#00000000"),
            ),
            r,
        );
        draw::stroked(n, c, wt)
    };
    b.add(
        parent,
        fixed(ring("piping shadow", sx, sy, &alpha("#000000", 0.55), t)),
    );
    b.add(
        parent,
        fixed(ring("piping", 0.0, 0.0, &shade(color, 0.15), t)),
    );
    b.add(
        parent,
        fixed(ring(
            "piping lit",
            lx * t * 0.2,
            ly * t * 0.2,
            &alpha(&tint(color, 0.5), 0.7),
            (t * 0.4).max(0.6),
        )),
    );
}

/// Build a cabinet under `parent`.
#[allow(clippy::too_many_lines)]
pub fn cabinet(b: &mut Builder<'_>, parent: NodeId, name: &str, s: &CabinetSpec) -> Cabinet {
    let (w, h) = (s.w, s.h);
    let handle_h = match s.handle {
        Handle::Leather => h * 0.09,
        Handle::Strap => h * 0.035,
        Handle::None => 0.0,
    };
    let feet_h = if s.feet { (h * 0.035).max(4.0) } else { 0.0 };
    let (bx, by, bw, bh) = (0.0, handle_h, w, h - handle_h - feet_h);
    let r = (bh.min(bw) * 0.035).clamp(4.0, 18.0);
    let root = b.add(parent, draw::frame(name, 0.0, 0.0, w, h));

    // ── behind the box: feet, the handle's strap ──
    if s.feet {
        let fw = (w * 0.1).max(24.0);
        for fx in [w * 0.08, w * 0.92 - fw] {
            let foot = draw::rounded(
                draw::rect(
                    "foot",
                    fx,
                    by + bh - feet_h,
                    fw,
                    feet_h * 2.0,
                    lin(180.0, &[(0.0, "#2a2a2c".into()), (1.0, "#060607".into())]),
                ),
                feet_h * 0.5,
            );
            b.add(
                root,
                fixed(draw::with_effects(foot, light::stand(feet_h, 1.0))),
            );
        }
    }
    let hc = s.handle_color.as_str();
    match s.handle {
        Handle::Leather => {
            // An arch: a thick band between two brackets, a stitched edge.
            let (x0, x1) = (w * 0.38, w * 0.62);
            let t = handle_h * 0.5;
            let top = handle_h * 0.08;
            let base = by + r * 0.3;
            let d = format!(
                "M {x0:.2} {base:.2} C {:.2} {top:.2} {:.2} {top:.2} {x1:.2} {base:.2} L {:.2} {base:.2} C {:.2} {:.2} {:.2} {:.2} {:.2} {base:.2} Z",
                x0 + (x1 - x0) * 0.08,
                x1 - (x1 - x0) * 0.08,
                x1 - t,
                x1 - t - (x1 - x0) * 0.06,
                top + t,
                x0 + t + (x1 - x0) * 0.06,
                top + t,
                x0 + t
            );
            let strap = draw::path(
                "handle",
                0.0,
                0.0,
                w,
                h,
                d.clone(),
                lin(
                    180.0,
                    &[
                        (0.0, tint(hc, 0.25)),
                        (0.5, hc.to_string()),
                        (1.0, shade(hc, 0.45)),
                    ],
                ),
            );
            b.add(
                root,
                fixed(draw::with_effects(strap, light::stand(handle_h * 0.3, 0.9))),
            );
            b.add(
                root,
                fixed(draw::path(
                    "handle grain",
                    0.0,
                    0.0,
                    w,
                    h,
                    d,
                    light::sheen(0.18, 0.25),
                )),
            );
        }
        Handle::Strap => {
            let (x0, sw) = (w * 0.4, w * 0.2);
            let strap = draw::rounded(
                draw::rect(
                    "handle",
                    x0,
                    by - handle_h * 0.9,
                    sw,
                    handle_h * 1.6,
                    lin(180.0, &[(0.0, tint(hc, 0.2)), (1.0, shade(hc, 0.4))]),
                ),
                handle_h * 0.6,
            );
            b.add(
                root,
                fixed(draw::with_effects(strap, light::stand(handle_h * 0.6, 0.9))),
            );
        }
        Handle::None => {}
    }

    // ── the box ──
    let shadow = draw::rounded(
        draw::rect("box shadow", bx, by, bw, bh, draw::solid("#0c0c0c")),
        r,
    );
    b.add(
        root,
        fixed(draw::with_effects(shadow, light::stand(h * 0.03, 1.1))),
    );
    fill(b, root, (bx, by, bw, bh), r, &s.covering);
    // The covering wrapping round the box's edges: lit top-left, dark bottom-right.
    for n in light::edge("box edge", (bx, by, bw, bh), r, r * 0.7, 1.2, true) {
        b.add(root, n);
    }
    b.add(
        root,
        fixed(draw::stroked(
            draw::rounded(
                draw::rect("box outline", bx, by, bw, bh, draw::solid("#00000000")),
                r,
            ),
            "#00000080",
            1.0,
        )),
    );

    // ── the baffle: faceplate and grille ──
    let border = (bh * 0.075).clamp(10.0, 44.0);
    let (ix, iy, iw, ih) = (
        bx + border,
        by + border,
        bw - 2.0 * border,
        bh - 2.0 * border,
    );
    let gap = (ih * 0.03).clamp(4.0, 14.0);
    let fh = ih * s.face_frac;
    let (face_y, grille_y) = if s.face_at_top {
        (iy, iy + fh + gap)
    } else {
        (iy + ih - fh, iy)
    };
    let gh = ih - fh - gap;
    let fr = (r * 0.4).max(2.0);

    // The grille sits back in the baffle.
    let mut g = draw::rounded(draw::frame("grille", ix, grille_y, iw, gh), fr);
    g.clips_content = true;
    let grille = b.add(root, g);
    fill(b, grille, (0.0, 0.0, iw, gh), fr, &s.grille);
    for n in light::edge(
        "grille recess",
        (0.0, 0.0, iw, gh),
        fr,
        border * 0.25,
        1.2,
        false,
    ) {
        b.add(grille, n);
    }
    if let Some(c) = &s.piping {
        piping(
            b,
            root,
            (ix, grille_y, iw, gh),
            fr,
            (bh * 0.012).clamp(1.5, 5.0),
            c,
        );
    }

    // The faceplate stands proud of the baffle.
    let face = draw::with_effects(
        draw::rounded(draw::frame("faceplate", ix, face_y, iw, fh), fr),
        light::stand(border * 0.12, 0.9),
    );
    let faceplate = b.add(root, face);
    fill(b, faceplate, (0.0, 0.0, iw, fh), fr, &s.face);
    for n in light::edge("faceplate bevel", (0.0, 0.0, iw, fh), fr, 2.0, 1.0, true) {
        b.add(faceplate, n);
    }
    let sr = (fh * 0.025).clamp(2.5, 5.0);
    for (sx, sy) in [
        (sr * 2.2, sr * 2.2),
        (iw - sr * 2.2, sr * 2.2),
        (sr * 2.2, fh - sr * 2.2),
        (iw - sr * 2.2, fh - sr * 2.2),
    ] {
        screw(b, faceplate, sx, sy, sr);
    }

    // ── in front: corners, handle brackets ──
    if let Some(kind) = &s.corners {
        let cs = (bh * 0.075).clamp(14.0, 44.0);
        for (fx, fy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            let x = if fx < 0.0 {
                bx - 1.0
            } else {
                bx + bw - cs + 1.0
            };
            let y = if fy < 0.0 {
                by - 1.0
            } else {
                by + bh - cs + 1.0
            };
            corner(b, root, (x, y), cs, r + 1.0, (fx, fy), kind);
        }
    }
    if s.handle == Handle::Leather {
        let bs = handle_h * 0.55;
        for cx in [w * 0.38 + handle_h * 0.15, w * 0.62 - handle_h * 0.15] {
            let m = metal("chrome");
            let br = draw::rounded(
                draw::rect(
                    "handle bracket",
                    cx - bs * 0.6,
                    by - bs * 0.55,
                    bs * 1.2,
                    bs * 0.9,
                    draw::linear(
                        light::sheen_angle(),
                        &[(0.0, m[0]), (0.5, m[1]), (1.0, m[2])],
                    ),
                ),
                bs * 0.25,
            );
            b.add(
                root,
                fixed(draw::with_effects(br, light::stand(bs * 0.3, 1.0))),
            );
        }
    }
    Cabinet {
        root,
        faceplate,
        grille,
    }
}

fn head(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    cabinet(b, parent, &p.name, &CabinetSpec::from_place(Kind::Head, p)).root
}

fn combo(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    cabinet(b, parent, &p.name, &CabinetSpec::from_place(Kind::Combo, p)).root
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![
        ComponentDef {
            id: "panel.amp_head",
            family: Family::Panel,
            title: "Amp head (rendered)",
            description: "An amp head as a product render shows it: a tolex-covered box with lit rounded edges, metal corners, a leather handle on chrome brackets, rubber feet, a faceplate standing proud (a `faceplate` child frame to put controls in) and a recessed grille framed by piping. Options: `w=`/`h=`, `tolex=tolex:levant|bronco|elephant|fine|leather|croc|wood…` in `color=#hex`, `grille=cloth:wheat|oxblood|basket|black|silver|diamond|chevron|perforated|expanded`, `face=brushed|powder|plastic|chrome…` in `face_color=#hex`, `face_at=top|bottom`, `face_frac=`, `piping=#hex|none`, `corners=chrome|black|brass|none`, `handle=leather|strap|none`, `handle_color=#hex`, `feet=none`. Size = width (default 900 × 396).",
            variants: &[],
            default_size: 900.0,
            binds: "none",
            build: head,
        },
        ComponentDef {
            id: "panel.amp_combo",
            family: Family::Panel,
            title: "Amp combo (rendered)",
            description: "A combo amp as a product render shows it: a tolex-covered cabinet with a top control panel (a `faceplate` child frame) over a large recessed grille framed by piping, metal corners, a strap handle and feet, all with depth. Same options as `panel.amp_head`. Size = width (default 900 × 630).",
            variants: &[],
            default_size: 900.0,
            binds: "none",
            build: combo,
        },
    ]
}
