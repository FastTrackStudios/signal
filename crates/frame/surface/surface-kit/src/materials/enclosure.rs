//! `panel.pedal_3d`: a stompbox as a product render shows it — the lid in
//! a material from [`super::paint`], its rounded edge catching the light
//! and rolling over into the enclosure's front face, the base plate under
//! that, side jacks with their nuts standing proud, optional lid screws,
//! and the whole box standing on the board with a contact and a cast
//! shadow.
//!
//! Sized like `panel.pedal` (`docs/form-factors.md` §1.2, §4.6): every
//! class is 286 design px tall and differs only in width (variant:
//! `micro` 92, `nano` 144, `standard` 156, `compact` 172, `wide` 222,
//! `medium` 241, `large` 286, `bigbox` 404, or `treadle` 241 × 600).
//!
//! Options: `color=#hex`, `finish=powder|powder:coarse|brushed|hammertone|gloss|satin|spun|chrome|…`
//! (any material spec), `jacks=side|top|both|none`, `screws` (four lid
//! screws), `base=#hex` (the front face and base), `front=` (the front
//! face's height; `0` = none), `jack_inset=` (how far side jacks push the
//! lid in from each side), `radius=`, `w=`/`h=`.
//! No control: the returned frame spans the footprint; place components in
//! it. The lid's rect is also a child frame named `lid` for callers that
//! want to clip to it.

use document_proto::{CornerRadii, NodeId};
use surface_widgets::Builder;

use super::light;
use super::paint::{Material, fill};
use crate::controls::parts::{alpha, fixed, lin, mixs, shade, tint};
use crate::draw;
use crate::panels::pedal::class_size;
use crate::registry::{ComponentDef, Family, Place};

fn num(p: &Place, key: &str) -> Option<f64> {
    p.opt(key).and_then(|v| v.parse().ok())
}

/// A jack seen from above: the barrel's hex nut standing out of the side
/// (or top), the threaded sleeve beyond it.
fn jack(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64), sideways: bool) {
    // Nut: faceted, so bands of light across it.
    let across = if sideways { 180.0 } else { 90.0 };
    let nut = draw::rounded(
        draw::rect(
            "jack nut",
            x,
            y,
            w,
            h,
            draw::linear(
                across,
                &[
                    (0.0, "#2c2d30"),
                    (0.18, "#8a8c90"),
                    (0.3, "#3a3b3e"),
                    (0.62, "#1a1b1d"),
                    (0.8, "#55575b"),
                    (1.0, "#141416"),
                ],
            ),
        ),
        (w.min(h) * 0.22).max(1.0),
    );
    b.add(
        parent,
        fixed(draw::with_effects(nut, light::stand(3.0, 0.9))),
    );
    // Sleeve: a darker, narrower barrel beyond the nut.
    let (sx, sy, sw, sh) = if sideways {
        (x + w * 0.2, y + h * 0.22, w * 0.6, h * 0.56)
    } else {
        (x + w * 0.22, y + h * 0.2, w * 0.56, h * 0.6)
    };
    b.add(
        parent,
        fixed(draw::rect(
            "jack sleeve",
            sx,
            sy,
            sw,
            sh,
            draw::linear(
                across,
                &[(0.0, "#101012"), (0.4, "#4a4b4f"), (1.0, "#0c0c0d")],
            ),
        )),
    );
}

/// A pan-head screw: a small domed head with a cross slot.
pub fn screw(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64, r: f64) {
    let (hx, hy) = light::hotspot();
    let head = draw::with_effects(
        draw::circle(
            "screw",
            cx,
            cy,
            r,
            draw::radial_at(
                hx,
                hy,
                0.75,
                &[(0.0, "#f2f3f4"), (0.5, "#9a9da1"), (1.0, "#43464a")],
            ),
        ),
        light::stand(r * 0.5, 0.8),
    );
    b.add(parent, fixed(head));
    let t = (r * 0.28).max(0.6);
    let (lx, ly) = (r, r);
    let d = format!(
        "{} {}",
        draw::polygon(&[
            (lx - r * 0.7, ly - t / 2.0),
            (lx + r * 0.7, ly - t / 2.0),
            (lx + r * 0.7, ly + t / 2.0),
            (lx - r * 0.7, ly + t / 2.0)
        ]),
        draw::polygon(&[
            (lx - t / 2.0, ly - r * 0.7),
            (lx + t / 2.0, ly - r * 0.7),
            (lx + t / 2.0, ly + r * 0.7),
            (lx - t / 2.0, ly + r * 0.7)
        ])
    );
    b.add(
        parent,
        fixed(draw::path(
            "screw slot",
            cx - r,
            cy - r,
            2.0 * r,
            2.0 * r,
            d,
            draw::solid("#2a2c2fcc"),
        )),
    );
}

/// The finish a spec names, painted in `color`.
fn finish_material(spec: &str, color: &str) -> Material {
    match spec {
        "gloss" => Material::Plastic {
            color: color.into(),
            gloss: 1.0,
        },
        "satin" => Material::Plastic {
            color: color.into(),
            gloss: 0.35,
        },
        "anodized" | "anodised" => Material::Brushed {
            color: color.into(),
            dir: super::paint::Dir::Vertical,
        },
        s => Material::parse(s, Some(color)).unwrap_or(Material::Powder {
            color: color.into(),
            coarse: false,
        }),
    }
}

#[allow(clippy::too_many_lines)]
fn pedal_3d(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let class = p.variant.as_deref().unwrap_or("standard");
    let (cw, ch) = class_size(class);
    let w = num(p, "w").or(p.size).unwrap_or(cw);
    let h = num(p, "h").unwrap_or(ch);
    let color = p
        .opt("color")
        .filter(|c| c.starts_with('#'))
        .unwrap_or("#3a3c40")
        .to_string();
    let finish = p.opt("finish").unwrap_or("powder");
    let jacks = p.opt("jacks").unwrap_or("side");
    let radius = num(p, "radius").unwrap_or((w.min(h) * 0.075).clamp(6.0, 16.0));
    let base = p
        .opt("base")
        .filter(|c| c.starts_with('#'))
        .map_or_else(|| shade(&color, 0.42), ToString::to_string);
    let side_j = matches!(jacks, "side" | "both");
    let top_j = matches!(jacks, "top" | "both");
    // How far the lid sits in from each side (where side jacks stand).
    let nut_w = (w * 0.06).clamp(7.0, 12.0);
    let jw = if side_j {
        num(p, "jack_inset").map_or(nut_w, |v| v.clamp(0.0, w * 0.3))
    } else {
        0.0
    };
    let jt = if top_j { 10.0 } else { 0.0 };
    let (bx, by, bw, bh) = (jw, jt, w - 2.0 * jw, h - jt);
    // The front face under the lid's edge, and the base plate under that.
    // `front=0`: no front face or base plate (the lid runs to the bottom).
    let front =
        num(p, "front").map_or_else(|| (bh * 0.07).clamp(8.0, 22.0), |v| v.clamp(0.0, bh * 0.3));
    let plate = if front > 0.0 {
        (bh * 0.012).clamp(2.0, 5.0)
    } else {
        0.0
    };
    let lid_h = bh - front;

    let root = b.add(parent, draw::frame(&p.name, 0.0, 0.0, w, h));
    if side_j {
        let jy = by + bh * 0.56;
        let jh = (bh * 0.13).clamp(22.0, 44.0);
        // The nut stands out of the side, a little under the lid's edge.
        jack(b, root, (jw - nut_w, jy, nut_w + 4.0, jh), true);
        jack(b, root, (w - jw - 4.0, jy, nut_w + 4.0, jh), true);
    }
    if top_j {
        for fx in [0.25, 0.75] {
            jack(b, root, (bx + bw * fx - 16.0, 0.0, 32.0, jt + 5.0), false);
        }
    }

    // The enclosure's silhouette: the front face's colour, standing on the board.
    let body = draw::rounded(
        draw::rect(
            "enclosure",
            bx,
            by,
            bw,
            bh,
            lin(
                180.0,
                &[
                    (0.0, shade(&base, 0.2)),
                    (0.9, base.clone()),
                    (0.93, shade(&base, 0.35)),
                    (1.0, shade(&base, 0.55)),
                ],
            ),
        ),
        radius,
    );
    b.add(
        root,
        fixed(draw::with_effects(body, light::stand(14.0, 1.0))),
    );
    if front > 0.0 {
        // The front face: lit a little on the left, the edge where the lid rolls over.
        let face = lin(
            90.0,
            &[
                (0.0, tint(&base, 0.12)),
                (0.4, base.clone()),
                (1.0, shade(&base, 0.25)),
            ],
        );
        let mut fr = draw::rect(
            "front face",
            bx,
            by + lid_h - radius,
            bw,
            front + radius - plate,
            face,
        );
        fr.corner_radii = CornerRadii {
            top_left: 0.0,
            top_right: 0.0,
            bottom_right: radius * 0.6,
            bottom_left: radius * 0.6,
        };
        b.add(root, fixed(fr));
        let mut pl = draw::rect(
            "base plate",
            bx + 1.5,
            by + bh - plate,
            bw - 3.0,
            plate,
            draw::solid(&shade(&base, 0.7)),
        );
        pl.corner_radii = CornerRadii {
            top_left: 0.0,
            top_right: 0.0,
            bottom_right: radius * 0.5,
            bottom_left: radius * 0.5,
        };
        b.add(root, fixed(pl));
    }

    // The lid, in its finish.
    let mut lid = draw::rounded(draw::frame("lid", bx, by, bw, lid_h), radius);
    lid.clips_content = true;
    let lid = b.add(root, fixed(lid));
    fill(
        b,
        lid,
        (0.0, 0.0, bw, lid_h),
        radius,
        &finish_material(finish, &color),
    );
    // Its edge rolling over: lit along the top and left, darkening into the
    // front face along the bottom.
    let roll = (radius * 0.9).max(4.0);
    let top_roll = lin(
        180.0,
        &[(0.0, alpha("#ffffff", 0.3)), (1.0, "#ffffff00".into())],
    );
    b.add(
        lid,
        fixed(draw::rect("edge light", 0.0, 0.0, bw, roll, top_roll)),
    );
    let bottom_roll = lin(
        180.0,
        &[(0.0, "#00000000".into()), (1.0, alpha("#000000", 0.45))],
    );
    b.add(
        lid,
        fixed(draw::rect(
            "edge shade",
            0.0,
            lid_h - roll * 1.2,
            bw,
            roll * 1.2,
            bottom_roll,
        )),
    );
    for n in light::edge(
        "bevel",
        (0.0, 0.0, bw, lid_h),
        radius,
        roll * 0.45,
        1.0,
        true,
    ) {
        b.add(lid, n);
    }
    b.add(
        lid,
        fixed(draw::stroked(
            draw::rounded(
                draw::rect("outline", 0.0, 0.0, bw, lid_h, draw::solid("#00000000")),
                radius,
            ),
            &alpha("#000000", 0.5),
            1.0,
        )),
    );
    // The crease where the lid's roll meets the front face catches a line of light.
    if front > 0.0 {
        b.add(
            root,
            fixed(draw::rect(
                "crease light",
                bx + radius,
                by + lid_h - 0.5,
                bw - 2.0 * radius,
                1.0,
                draw::solid(&alpha(&mixs(&color, "#ffffff", 0.6), 0.35)),
            )),
        );
    }

    if p.flag("screws") || p.opt("screws").is_some_and(|v| v != "0") {
        let inset = radius * 0.95 + 3.0;
        let r = (w.min(h) * 0.018).clamp(2.5, 4.5);
        for (sx, sy) in [
            (inset, inset),
            (bw - inset, inset),
            (inset, lid_h - inset),
            (bw - inset, lid_h - inset),
        ] {
            screw(b, lid, sx, sy, r);
        }
    }
    root
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![ComponentDef {
        id: "panel.pedal_3d",
        family: Family::Panel,
        title: "Pedal enclosure (rendered)",
        description: "A stompbox as a product render shows it: the lid in a material finish with its rounded edge lit and rolling into the front face, the base plate, side jacks with hex nuts, optional lid screws, and contact + cast shadows. Sized by class (variant: `micro` 92 … `standard` 156 … `bigbox` 404 wide, all 286 tall; `treadle` 241 × 600). Options: `color=#hex`, `finish=powder|powder:coarse|brushed|hammertone|gloss|satin|chrome|…`, `jacks=side|top|both|none`, `screws`, `base=#hex`, `front=` (front-face height, 0 = none), `jack_inset=` (the lid's inset from each side for side jacks), `radius=`, `w=`/`h=`. No control; a parent for knobs, the footswitch and silkscreen.",
        variants: &[
            "micro", "nano", "standard", "compact", "wide", "medium", "large", "bigbox", "treadle",
        ],
        default_size: 156.0,
        binds: "none",
        build: pedal_3d,
    }]
}
