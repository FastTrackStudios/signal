//! `panel.pedal`: a stompbox enclosure seen from above, sized by the pedal
//! form factor's class (`docs/form-factors.md` §1.2, §4.6): every class
//! shares one height (286 design px, the Standard 125B's depth) and differs
//! only in width, so a row of pedals lines up top and bottom and their
//! footswitches sit on one line. Treadles (wah, expression) are the one
//! exception at 241 × 600.
//!
//! | variant | width | real box |
//! |---|---|---|
//! | `micro` | 92 | Hammond 1590A |
//! | `nano` | 144 | 1590B |
//! | `standard` | 156 | 125B |
//! | `compact` | 172 | Boss compact |
//! | `wide` | 222 | 1590BB |
//! | `medium` | 241 | Strymon small |
//! | `large` | 286 | 1590XX |
//! | `bigbox` | 404 | double / big box |
//! | `treadle` | 241 × 600 | wah / expression |
//!
//! Drawn: the enclosure's lid in its paint (`color=#hex`) and finish
//! (`finish=powder|brushed|satin|gloss` — powder-coat stipple, brushed
//! anodising, plain satin, lacquer with a sheen), rounded corners
//! (`radius=`), the base showing under the lid's front edge (`base=#hex`),
//! and jacks (`jacks=side|top|both|none`). No control: place components in
//! the returned frame, which spans the whole footprint.

use document_proto::NodeId;
use surface_widgets::Builder;

use crate::controls::parts::{alpha, fixed, mixs};
use crate::draw;
use crate::registry::{ComponentDef, Family, Place};

/// The height every pedal shares, design px.
pub const PEDAL_H: f64 = 286.0;

/// A class's footprint, design px.
#[must_use]
pub fn class_size(class: &str) -> (f64, f64) {
    match class {
        "micro" => (92.0, PEDAL_H),
        "nano" => (144.0, PEDAL_H),
        "compact" => (172.0, PEDAL_H),
        "wide" => (222.0, PEDAL_H),
        "medium" => (241.0, PEDAL_H),
        "large" => (286.0, PEDAL_H),
        "bigbox" => (404.0, PEDAL_H),
        "treadle" => (241.0, 600.0),
        _ => (156.0, PEDAL_H),
    }
}

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) as f64 / f64::from(u32::MAX >> 1)
    }
}

fn num(p: &Place, key: &str) -> Option<f64> {
    p.opt(key).and_then(|v| v.parse().ok())
}

/// A jack on the enclosure's side or top: the nut and the barrel, seen
/// from above as a short dark stub.
fn jack(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64)) {
    let body = draw::rounded(
        draw::rect(
            "jack",
            x,
            y,
            w,
            h,
            draw::linear(
                if w > h { 180.0 } else { 90.0 },
                &[(0.0, "#3c3d40"), (0.45, "#121214"), (1.0, "#2a2b2e")],
            ),
        ),
        1.5,
    );
    b.add(
        parent,
        fixed(draw::with_effects(
            body,
            vec![draw::drop_shadow("#00000099", 0.0, 1.0, 2.0)],
        )),
    );
    let (nx, ny, nw, nh) = if w > h {
        (x + w * 0.1, y + h * 0.18, w * 0.28, h * 0.64)
    } else {
        (x + w * 0.18, y + h * 0.1, w * 0.64, h * 0.28)
    };
    b.add(
        parent,
        fixed(draw::rect(
            "jack nut",
            nx,
            ny,
            nw,
            nh,
            draw::linear(
                180.0,
                &[(0.0, "#c9cacc"), (0.5, "#6d6e71"), (1.0, "#a8a9ab")],
            ),
        )),
    );
}

#[allow(clippy::too_many_lines)]
fn pedal(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let class = p.variant.as_deref().unwrap_or("standard");
    let (cw, ch) = class_size(class);
    let w = num(p, "w").or(p.size).unwrap_or(cw);
    let h = num(p, "h").unwrap_or(ch);
    let color = p.opt("color").unwrap_or("#3a3c40").to_string();
    let finish = p.opt("finish").unwrap_or("powder");
    let jacks = p.opt("jacks").unwrap_or("side");
    let radius = num(p, "radius").unwrap_or((w.min(h) * 0.07).clamp(6.0, 16.0));
    let base = p
        .opt("base")
        .map_or_else(|| mixs(&color, "#000000", 0.45), ToString::to_string);
    let side_j = matches!(jacks, "side" | "both");
    let top_j = matches!(jacks, "top" | "both");
    // The lid is inset where jacks stick out, so the footprint holds them.
    let jw = if side_j { 5.0 } else { 0.0 };
    let jt = if top_j { 6.0 } else { 0.0 };
    let (lx, ly, lw, lh) = (jw, jt, w - 2.0 * jw, h - jt);
    let lip = (h * 0.035).clamp(6.0, 14.0);

    let root = b.add(parent, draw::frame(&p.name, 0.0, 0.0, w, h));
    if side_j {
        let jy = ly + lh * 0.36;
        jack(b, root, (0.0, jy, jw + 2.0, 26.0));
        jack(b, root, (w - jw - 2.0, jy, jw + 2.0, 26.0));
    }
    if top_j {
        for fx in [0.28, 0.72] {
            jack(b, root, (lx + lw * fx - 11.0, 0.0, 22.0, jt + 2.0));
        }
    }

    // The base under the lid: seen as a darker band along the front edge.
    let base_n = draw::rounded(
        draw::rect(
            "base",
            lx,
            ly,
            lw,
            lh,
            draw::linear(180.0, &[(0.0, &base), (1.0, &mixs(&base, "#000000", 0.35))]),
        ),
        radius,
    );
    b.add(
        root,
        fixed(draw::with_effects(
            base_n,
            vec![
                draw::drop_shadow("#000000b3", 0.0, 6.0, 14.0),
                draw::drop_shadow("#00000066", 0.0, 2.0, 3.0),
            ],
        )),
    );

    // The lid.
    let lid_h = lh - lip;
    let (top, mid, bot) = match finish {
        "gloss" => (
            mixs(&color, "#ffffff", 0.16),
            color.clone(),
            mixs(&color, "#000000", 0.22),
        ),
        "brushed" => (
            mixs(&color, "#ffffff", 0.1),
            color.clone(),
            mixs(&color, "#000000", 0.14),
        ),
        _ => (
            mixs(&color, "#ffffff", 0.06),
            color.clone(),
            mixs(&color, "#000000", 0.12),
        ),
    };
    let mut lid = draw::frame("lid", lx, ly, lw, lid_h);
    lid.fills = vec![draw::linear(
        172.0,
        &[(0.0, &top), (0.5, &mid), (1.0, &bot)],
    )];
    lid.clips_content = true;
    let lid = draw::with_effects(
        draw::stroked(draw::rounded(lid, radius), &alpha("#000000", 0.55), 1.0),
        vec![
            draw::inner_shadow("#ffffff2e", 0.0, 1.0, 1.0),
            draw::inner_shadow("#00000059", 0.0, -2.0, 3.0),
        ],
    );
    let lid = b.add(root, lid);

    let mut rng = Lcg(0x9ed4_1ce5 ^ (w as u64) << 8 ^ (color.len() as u64));
    match finish {
        "powder" => {
            // Stipple: fine light and dark specks, the powder coat's tooth.
            for (c, a, n) in [
                ("#ffffff", 0.09, 1.0),
                ("#000000", 0.12, 1.0),
                ("#ffffff", 0.18, 0.1),
            ] {
                let mut d = String::new();
                let count = ((lw * lid_h) / 9.0 * n) as usize;
                for _ in 0..count {
                    let (x, y) = (rng.next() * lw, rng.next() * lid_h);
                    let s = 0.3 + rng.next() * 0.35;
                    d.push_str(&format!("M {x:.1} {y:.1} h {s:.1} v {s:.1} h {:.1} Z ", -s));
                }
                b.add(
                    lid,
                    fixed(draw::path(
                        "stipple",
                        0.0,
                        0.0,
                        lw,
                        lid_h,
                        d,
                        draw::solid(&alpha(c, a)),
                    )),
                );
            }
        }
        "brushed" => {
            // Vertical brushing, as a machined lid.
            for (c, a, step) in [
                ("#ffffff", 0.05, 2.0),
                ("#000000", 0.05, 3.0),
                ("#ffffff", 0.04, 7.0),
                ("#000000", 0.06, 13.0),
            ] {
                let mut d = String::new();
                let mut x = rng.next() * step;
                while x < lw {
                    let t = 0.5 + rng.next() * 0.8;
                    d.push_str(&format!("M {x:.1} 0 h {t:.1} V {lid_h:.1} h {:.1} Z ", -t));
                    x += step * (0.6 + rng.next() * 0.8);
                }
                b.add(
                    lid,
                    fixed(draw::path(
                        "brushing",
                        0.0,
                        0.0,
                        lw,
                        lid_h,
                        d,
                        draw::solid(&alpha(c, a)),
                    )),
                );
            }
        }
        _ => {}
    }
    // The room's light: a broad sheen from the top left.
    let sheen_a = if finish == "gloss" { 0.16 } else { 0.08 };
    let sheen = draw::linear(
        118.0,
        &[
            (0.0, &alpha("#ffffff", sheen_a)),
            (0.35, "#ffffff00"),
            (0.7, "#00000000"),
            (1.0, &alpha("#000000", 0.12)),
        ],
    );
    b.add(lid, fixed(draw::rect("sheen", 0.0, 0.0, lw, lid_h, sheen)));
    // The lid's front edge catching light over the base.
    b.add(
        root,
        fixed(draw::rect(
            "edge",
            lx + radius,
            ly + lid_h - 1.0,
            lw - 2.0 * radius,
            1.0,
            draw::solid(&alpha("#ffffff", 0.18)),
        )),
    );
    root
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![ComponentDef {
        id: "panel.pedal",
        family: Family::Panel,
        title: "Pedal enclosure",
        description: "A stompbox enclosure from above, sized by pedal class (variant: `micro` 92, `nano` 144, `standard` 156, `compact` 172, `wide` 222, `medium` 241, `large` 286, `bigbox` 404 wide — all 286 tall — or `treadle` 241 × 600). Options: `color=#hex` paint, `finish=powder|brushed|satin|gloss`, `base=#hex` (the base under the lid's front edge), `jacks=side|top|both|none`, `radius=`, `w=`/`h=`. No control; a parent for knobs, the stomp switch and silkscreen.",
        variants: &[
            "micro", "nano", "standard", "compact", "wide", "medium", "large", "bigbox", "treadle",
        ],
        default_size: 156.0,
        binds: "none",
        build: pedal,
    }]
}
