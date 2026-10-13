//! The optical leveler in a host's FX column (see [`crate::lane`]).
//!
//! - **Lane**: the red slanted name over its swoosh, the COMPRESS / LIMIT
//!   bat below it; GAIN, the VU in its charcoal picture frame, PEAK
//!   REDUCTION — the unit's own left-to-right. The needle reads gain
//!   reduction (the meter selector and power stay on the rack face).
//! - **Tall**: the name and the meter selector either side of a big VU
//!   across the top; COMPRESS / LIMIT, GAIN, PEAK REDUCTION and POWER
//!   across the bottom.

use document_proto::NodeId;
use surface_kit::{Ink as KitInk, Place, TOUCH_MIN, draw};
use surface_proto::{MeterTap, SourceSelect};
use surface_widgets::Builder;

use super::super::ui::{self, Ink};
use super::{CHROME, DIM, INK, RED, place_toggle};
use crate::face::{Face, canvas};
use crate::lane::Shape;

pub fn faces() -> Vec<Face> {
    Shape::ALL
        .into_iter()
        .map(|shape| {
            let scene = canvas(&format!("Opto Leveler ({})", shape.suffix()), |b, root| {
                build(b, root, shape)
            });
            let mut params = vec![
                ui::dial_0_100("opto/gain", "Gain", 40.0).short("Gain"),
                ui::dial_0_100("opto/peak_reduction", "Peak Reduction", 45.0).short("Peak Red"),
                ui::toggle("opto/limit", "Compress / Limit", false).short("Limit"),
            ];
            if shape == Shape::Tall {
                params.push(ui::choice(
                    "opto/meter",
                    "Meter",
                    &["Output +10", "Gain Reduction", "Output +4"],
                    1,
                ));
                params.push(ui::toggle("opto/power", "Power", true));
            }
            Face {
                file: shape.file("04-opto-leveler"),
                plugin: "comp",
                scene,
                params,
                streams: ui::streams("opto", "Opto Leveler"),
            }
        })
        .collect()
}

/// Where a shape puts things, panel px.
struct Spots {
    /// The name block's top-left; its scale (1 = the rack face's).
    name: (f64, f64, f64),
    /// The mode toggle's bushing.
    mode: (f64, f64),
    /// GAIN and PEAK REDUCTION shafts; their knob size; the caption line.
    knobs: [f64; 2],
    knob_y: f64,
    knob_d: f64,
    caption_y: f64,
    /// The VU card's centre and width.
    vu: (f64, f64, f64),
}

const fn spots(shape: Shape) -> Spots {
    match shape {
        Shape::Lane => Spots {
            name: (24.0, 14.0, 1.15),
            mode: (84.0, 114.0),
            knobs: [274.0, 666.0],
            knob_y: 74.0,
            knob_d: 66.0,
            caption_y: 150.0,
            vu: (470.0, 81.0, 188.0),
        },
        Shape::Tall => Spots {
            name: (30.0, 42.0, 1.35),
            mode: (78.0, 258.0),
            knobs: [252.0, 516.0],
            knob_y: 248.0,
            knob_d: 72.0,
            caption_y: 318.0,
            vu: (384.0, 94.0, 226.0),
        },
    }
}

fn build(b: &mut Builder<'_>, root: NodeId, shape: Shape) {
    let (w, h) = shape.size();
    let s = spots(shape);
    let panel = ui::put(
        b,
        root,
        "panel.painted",
        &Place::new("Leveler panel").variant("grey").options(&[
            &format!("w={w}"),
            &format!("h={h}"),
            "ends=none",
        ]),
    );
    ui::repaint(
        b,
        panel,
        &[(0.0, "#cbcdca"), (0.5, "#bfc1be"), (1.0, "#afb1ae")],
    );
    if let Some(n) = b.scene.node_mut(panel) {
        n.corner_radii = document_proto::CornerRadii::uniform(8.0);
    }
    for (x, y) in [
        (12.0, 12.0),
        (w - 12.0, 12.0),
        (12.0, h - 12.0),
        (w - 12.0, h - 12.0),
    ] {
        ui::phillips(b, panel, (x, y), 9.0, CHROME);
    }

    name(b, panel, s.name, shape);

    // COMPRESS / LIMIT, and the hex plug beside it.
    let toggle_d = if shape == Shape::Tall { 46.0 } else { 40.0 };
    let sw = ui::put(
        b,
        panel,
        "switch.toggle_bare",
        &Place::new("Compress / Limit")
            .size(toggle_d)
            .legend(" ")
            .options(&["Limit", "Compress"])
            .ink(KitInk::Color(DIM.into()))
            .bind(ui::bind("opto/limit")),
    );
    place_toggle(b, sw, s.mode.0, s.mode.1);
    restyle_toggle(b, sw, if shape == Shape::Tall { 10.0 } else { 9.0 });
    hex_plug(
        b,
        panel,
        (s.mode.0 + 46.0, s.mode.1),
        if shape == Shape::Tall { 13.0 } else { 11.5 },
        (w, h),
    );

    // ── Gain and peak reduction ──────────────────────────────────────
    let scale = [
        "0", "10", "20", "30", "40", "50", "60", "70", "80", "90", "100",
    ];
    let k = s.knob_d / 50.0;
    let numerals = Ink::new(11.0 * k.min(1.3), INK).weight(700.0).track(0.0);
    let caption = Ink::new(if shape == Shape::Tall { 11.0 } else { 10.0 }, DIM)
        .weight(700.0)
        .track(0.14);
    for ((name, addr), x) in [
        ("Gain", "opto/gain"),
        ("Peak Reduction", "opto/peak_reduction"),
    ]
    .into_iter()
    .zip(s.knobs)
    {
        let c = (x, s.knob_y);
        let p = Place::new(name)
            .size(s.knob_d)
            .legend(" ")
            .sweep(290.0)
            .scale(&scale)
            .touch(TOUCH_MIN)
            .bind(ui::bind(addr));
        let knob = ui::rotary_at(b, panel, "knob.tab_pointer", &p, c.0, c.1);
        ui::hide_ring(b, knob);
        ui::ring_numerals(b, panel, c, 43.0 * k, 290.0, &scale, numerals);
        ui::ring_ticks(
            b,
            panel,
            c,
            (31.5 * k, 34.0 * k),
            290.0,
            21,
            0.9,
            "#2625234d",
        );
        ui::silk(b, panel, name, c.0, s.caption_y, caption);
    }

    // ── The VU ───────────────────────────────────────────────────────
    let (mx, my, cw) = s.vu;
    vu_frame(b, panel, (mx, my), cw, (w, h));
    let mut vu = Place::new("Gain Reduction meter")
        .size(cw)
        .variant("open")
        .legend(" ")
        .options(&["glass"])
        .bind(ui::stream("opto/gr"));
    if shape == Shape::Tall {
        vu = vu.select(SourceSelect::new(
            "opto/meter",
            vec![
                Some(MeterTap::new("opto/out").offset(-10.0)),
                Some(MeterTap::new("opto/gr")),
                Some(MeterTap::new("opto/out").offset(-4.0)),
            ],
        ));
    }
    ui::centred_at(b, panel, "meter.vu_ivory", &vu, mx, my);
    let ch = cw * 0.58;
    b.add(
        panel,
        draw::rounded(
            draw::rect(
                "card tint",
                mx - cw / 2.0,
                my - ch / 2.0,
                cw,
                ch,
                draw::solid("#e6cf8f52"),
            ),
            3.0,
        ),
    );
    let kc = cw / 146.0;
    let card = Ink::new(8.0 * kc, "#3a3129").weight(600.0).track(0.06);
    ui::silk(b, panel, "VU", mx - 60.0 * kc, my - 17.0 * kc, card);
    ui::silk(
        b,
        panel,
        "VU",
        mx + 60.0 * kc,
        my - 17.0 * kc,
        card.color("#a32a22"),
    );
    ui::silk(
        b,
        panel,
        "VU Level Indicator",
        mx,
        my - 33.0 * kc,
        card.size(4.6 * kc).track(0.12),
    );

    if shape == Shape::Tall {
        selector_and_power(b, panel);
    }
    ui::fit_root(b, root, panel);
}

/// The toggle's printed positions at a size the lane reads.
fn restyle_toggle(b: &mut Builder<'_>, sw: NodeId, size: f64) {
    for id in b.scene.walk(sw) {
        if let Some(n) = b.scene.node_mut(id)
            && let document_proto::NodeKind::Text(t) = &mut n.kind
        {
            let grow = size / t.font_size.max(1.0);
            t.font_size = size;
            t.font_weight = Some(700.0);
            let cx = n.transform.tx + n.size.width / 2.0;
            let cy = n.transform.ty + n.size.height / 2.0;
            n.size.width *= grow * 1.2;
            n.size.height = (n.size.height * grow).ceil();
            n.transform.tx = cx - n.size.width / 2.0;
            // Push the legends out from the bat as they grow.
            let push = (grow - 1.0) * 6.0;
            n.transform.ty = cy - n.size.height / 2.0 + if cy < 30.0 { -push } else { push };
        }
    }
}

/// The name: red, slanted, over a double swoosh; `(x, y)` its top-left.
fn name(b: &mut Builder<'_>, panel: NodeId, (x, y, k): (f64, f64, f64), shape: Shape) {
    let (w, h) = shape.size();
    ui::silk_slanted(
        b,
        panel,
        "Opto Leveler",
        x,
        y + 8.0 * k,
        Ink::new(15.5 * k, RED).weight(800.0).track(0.0),
        0.22,
    );
    ui::silk_left(
        b,
        panel,
        "Leveling Amplifier",
        x + 6.0 * k,
        y + 26.0 * k,
        Ink::new(5.8 * k, RED).weight(600.0).track(0.42),
    );
    // The swoosh: two rules that run under the name, kick up, and run on.
    let (len, rise, run) = if shape == Shape::Tall {
        (116.0, 26.0, 30.0)
    } else {
        (124.0, 18.0, 24.0)
    };
    for (dy, dx) in [(0.0, 0.0), (3.8 * k, 2.0 * k)] {
        let base = y + 34.0 * k + dy;
        let pts = [
            (x - 2.0, base),
            (x + len * k + dx, base),
            (x + (len + rise * 0.46) * k + dx, base - rise * k),
            (x + (len + run) * k, base - rise * k),
        ];
        let mut d = String::new();
        for seg in pts.windows(2) {
            let (a, c) = (seg[0], seg[1]);
            let (ux, uy) = (c.0 - a.0, c.1 - a.1);
            let l = ux.hypot(uy);
            let (nx, ny) = (-uy / l * 0.65, ux / l * 0.65);
            d.push_str(&draw::polygon(&[
                (a.0 + nx, a.1 + ny),
                (c.0 + nx, c.1 + ny),
                (c.0 - nx, c.1 - ny),
                (a.0 - nx, a.1 - ny),
            ]));
            d.push(' ');
        }
        b.add(
            panel,
            draw::path("swoosh", 0.0, 0.0, w, h, d, draw::solid(RED)),
        );
    }
    if shape == Shape::Tall {
        let side = Ink::new(8.0, DIM).weight(600.0).track(0.12);
        ui::silk_left(b, panel, "Leveling Amplifier", x + 4.0, y + 86.0, side);
        ui::silk_left(b, panel, "Photo-Optical", x + 4.0, y + 102.0, side);
    }
}

/// Top right on the tall face: the meter selector (OUTPUT +10 / GAIN
/// REDUCTION / OUTPUT +4) over the POWER toggle beside it.
fn selector_and_power(b: &mut Builder<'_>, panel: NodeId) {
    let (sx, sy) = (654.0, 104.0);
    let p = Place::new("Meter")
        .size(62.0)
        .legend(" ")
        .options(&["+10", "GR", "+4"])
        .sweep(100.0)
        .touch(TOUCH_MIN)
        .bind(ui::bind("opto/meter"));
    let meter = ui::rotary_at(b, panel, "knob.tab_pointer", &p, sx, sy);
    ui::hide_ring(b, meter);
    let small = Ink::new(8.5, DIM).weight(700.0).track(0.1);
    ui::silk(b, panel, "Gain Reduction", sx, 40.0, small);
    ui::silk(b, panel, "Output +10", sx - 70.0, 62.0, small);
    ui::silk(b, panel, "Output +4", sx + 70.0, 62.0, small);
    ui::silk(
        b,
        panel,
        "Meter",
        sx,
        152.0,
        Ink::new(10.0, DIM).weight(700.0).track(0.14),
    );

    let power = ui::put(
        b,
        panel,
        "switch.toggle_bare",
        &Place::new("Power")
            .size(46.0)
            .legend(" ")
            .options(&["On", "Power"])
            .ink(KitInk::Color(DIM.into()))
            .bind(ui::bind("opto/power")),
    );
    place_toggle(b, power, 690.0, 256.0);
    restyle_toggle(b, power, 10.0);
}

/// The meter's charcoal picture frame round a card `cw` wide, its opening
/// chamfered and catching the light on its right and lower faces.
fn vu_frame(b: &mut Builder<'_>, panel: NodeId, (cx, cy): (f64, f64), cw: f64, (w, h): (f64, f64)) {
    let k = cw / 146.0;
    let (iw, ih) = (148.0 * k, 87.0 * k);
    let (ow, oh) = (184.0 * k, 108.6 * k);
    let (x, y) = (cx - ow / 2.0, cy - oh / 2.0);
    let frame = draw::rounded(
        draw::rect(
            "meter frame",
            x,
            y,
            ow,
            oh,
            draw::linear(180.0, &[(0.0, "#343537"), (1.0, "#262729")]),
        ),
        2.0,
    );
    b.add(
        panel,
        draw::with_effects(
            draw::stroked(frame, "#00000080", 1.0),
            vec![draw::drop_shadow("#00000059", 0.0, 1.5, 3.0)],
        ),
    );
    let c = 9.0 * k;
    let (ix, iy) = (cx - iw / 2.0, cy - ih / 2.0);
    let (ox, oy, ow2, oh2) = (ix - c, iy - c, iw + 2.0 * c, ih + 2.0 * c);
    let faces = [
        (
            "chamfer top",
            "#18181a",
            [(ox, oy), (ox + ow2, oy), (ix + iw, iy), (ix, iy)],
        ),
        (
            "chamfer left",
            "#202022",
            [(ox, oy), (ix, iy), (ix, iy + ih), (ox, oy + oh2)],
        ),
        (
            "chamfer right",
            "#8e9093",
            [
                (ox + ow2, oy),
                (ox + ow2, oy + oh2),
                (ix + iw, iy + ih),
                (ix + iw, iy),
            ],
        ),
        (
            "chamfer bottom",
            "#505154",
            [
                (ox, oy + oh2),
                (ix, iy + ih),
                (ix + iw, iy + ih),
                (ox + ow2, oy + oh2),
            ],
        ),
    ];
    for (name, color, pts) in faces {
        b.add(
            panel,
            draw::path(
                name,
                0.0,
                0.0,
                w,
                h,
                draw::polygon(&pts),
                draw::solid(color),
            ),
        );
    }
}

/// The hex-headed plug beside the mode toggle.
fn hex_plug(b: &mut Builder<'_>, panel: NodeId, (cx, cy): (f64, f64), r: f64, (w, h): (f64, f64)) {
    let nut = draw::path(
        "hex plug",
        0.0,
        0.0,
        w,
        h,
        draw::regular(cx, cy, r, 6, 0.0),
        draw::radial_at(
            0.36,
            0.3,
            0.9,
            &[(0.0, "#f2f2ee"), (0.55, "#aaaaa4"), (1.0, "#4e4e4a")],
        ),
    );
    b.add(
        panel,
        draw::with_effects(nut, vec![draw::drop_shadow("#00000066", 0.6, 1.4, 2.0)]),
    );
    b.add(
        panel,
        draw::circle(
            "hex plug face",
            cx,
            cy,
            r * 0.62,
            draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#e2e2dc"), (1.0, "#7a7a74")]),
        ),
    );
    for deg in [20.0, 110.0] {
        b.add(
            panel,
            draw::rotated(
                draw::rect(
                    "hex plug slot",
                    cx - r * 0.5,
                    cy - 0.7,
                    r,
                    1.4,
                    draw::solid("#00000099"),
                ),
                deg,
            ),
        );
    }
}
