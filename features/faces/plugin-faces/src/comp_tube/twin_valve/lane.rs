//! The twin-channel valve limiter in a host's FX column (see
//! [`crate::lane`]).
//!
//! - **Lane**: the engraved black plate in its grey front — the bakelite
//!   meter (gain reduction), INPUT GAIN, THRESHOLD, the TIME CONSTANT and
//!   the slanted name.
//! - **Tall**: ON, the pilot lamp and the fuse down the grey at the left;
//!   on the plate the meter, the METERING switch with its trims, the name
//!   and the AGC switch across the top, and INPUT GAIN, THRESHOLD and TIME
//!   CONSTANT across the bottom.

use document_proto::NodeId;
use surface_kit::{Place, TOUCH_MIN, draw};
use surface_proto::{MeterTap, Range, SourceSelect, Unit};
use surface_widgets::Builder;

use super::super::ui::{self, Ink};
use super::{CHROME, GAIN_STEPS, INK, interleave, trim};
use crate::face::{Face, canvas};
use crate::lane::Shape;

pub fn faces() -> Vec<Face> {
    Shape::ALL
        .into_iter()
        .map(|shape| {
            let scene = canvas(
                &format!("Twin Valve Limiter ({})", shape.suffix()),
                |b, root| build(b, root, shape),
            );
            let mut params = vec![
                ui::param(
                    "twinvalve/input_gain",
                    "Input Gain",
                    Range {
                        steps: Some(11),
                        ..Range::linear(-20.0, 0.0, Unit::Db)
                    },
                    0.0,
                )
                .short("Input"),
                ui::param(
                    "twinvalve/threshold",
                    "Threshold",
                    Range::linear(0.0, 10.0, Unit::None),
                    4.0,
                )
                .short("Thresh"),
                ui::choice(
                    "twinvalve/time_constant",
                    "Time Constant",
                    &["1", "2", "3", "4", "5 (auto)", "6 (auto)"],
                    1,
                )
                .short("Time"),
            ];
            if shape == Shape::Tall {
                params.extend([
                    ui::choice(
                        "twinvalve/metering",
                        "Metering",
                        &["Bal −", "Zero", "Bal +"],
                        1,
                    )
                    .short("Meter"),
                    ui::choice("twinvalve/agc", "AGC", &["Left / Right", "Lat / Vert"], 0),
                    ui::toggle("twinvalve/power", "Power", true),
                ]);
            }
            Face {
                file: shape.file("06-twin-valve"),
                plugin: "comp",
                scene,
                params,
                streams: ui::streams("twinvalve", "Twin Valve Limiter"),
            }
        })
        .collect()
}

/// Where a shape puts things, panel px.
struct Spots {
    /// The plate: x, y, w, h.
    plate: (f64, f64, f64, f64),
    /// The meter case's centre and side.
    meter: (f64, f64, f64),
    /// INPUT GAIN and THRESHOLD shafts and size; the knob titles' line.
    gain: (f64, f64),
    threshold: (f64, f64),
    big: f64,
    titles: f64,
    /// TIME CONSTANT's shaft and size.
    time: (f64, f64, f64),
    /// The name's left end and line; its size.
    name: (f64, f64, f64),
}

const fn spots(shape: Shape) -> Spots {
    match shape {
        Shape::Lane => Spots {
            plate: (8.0, 8.0, 744.0, 148.0),
            meter: (92.0, 82.0, 136.0),
            gain: (256.0, 94.0),
            threshold: (408.0, 94.0),
            big: 68.0,
            titles: 24.0,
            time: (546.0, 102.0, 54.0),
            name: (608.0, 66.0, 21.0),
        },
        Shape::Tall => Spots {
            plate: (84.0, 12.0, 664.0, 304.0),
            meter: (178.0, 96.0, 156.0),
            gain: (268.0, 258.0),
            threshold: (468.0, 258.0),
            big: 80.0,
            titles: 186.0,
            time: (654.0, 264.0, 58.0),
            name: (452.0, 62.0, 30.0),
        },
    }
}

fn build(b: &mut Builder<'_>, root: NodeId, shape: Shape) {
    let (w, h) = shape.size();
    let s = spots(shape);
    let tall = shape == Shape::Tall;
    let front = ui::put(
        b,
        root,
        "panel.painted",
        &Place::new("Cabinet front").variant("grey").options(&[
            &format!("w={w}"),
            &format!("h={h}"),
            "ends=none",
        ]),
    );
    ui::repaint(
        b,
        front,
        &[(0.0, "#d6d6d2"), (0.5, "#cacac6"), (1.0, "#bdbdb9")],
    );
    if let Some(n) = b.scene.node_mut(front) {
        n.corner_radii = document_proto::CornerRadii::uniform(8.0);
    }

    // ── The plate ────────────────────────────────────────────────────
    let (px, py, pw, ph) = s.plate;
    let plate = draw::rounded(
        draw::rect(
            "plate",
            px,
            py,
            pw,
            ph,
            draw::linear(
                180.0,
                &[(0.0, "#1a1a1b"), (0.5, "#111112"), (1.0, "#0a0a0b")],
            ),
        ),
        5.0,
    );
    b.add(
        front,
        draw::with_effects(
            draw::stroked(plate, "#000000", 1.0),
            vec![
                draw::drop_shadow("#00000073", 0.0, 2.0, 5.0),
                draw::inner_shadow("#ffffff1a", 0.0, 1.0, 0.0),
            ],
        ),
    );

    let ink = Ink::new(if tall { 10.5 } else { 9.5 }, INK)
        .weight(800.0)
        .track(0.06);
    let numerals = ink
        .size(if tall { 11.0 } else { 10.0 })
        .weight(600.0)
        .track(0.0);

    // ── The meter ────────────────────────────────────────────────────
    let mut gr = Place::new("Gain Reduction meter").bind(ui::stream("twinvalve/gr"));
    if tall {
        let out = MeterTap::new("twinvalve/out");
        gr = gr.select(SourceSelect::new(
            "twinvalve/metering",
            vec![
                Some(out.clone()),
                Some(MeterTap::new("twinvalve/gr")),
                Some(out),
            ],
        ));
    }
    meter(b, front, s.meter, &gr);

    // ── INPUT GAIN and THRESHOLD ─────────────────────────────────────
    let r = s.big * 0.62 + 8.0;
    let (gx, gy) = s.gain;
    let p = Place::new("Input Gain")
        .size(s.big)
        .legend(" ")
        .scale(&GAIN_STEPS)
        .sweep(290.0)
        .touch(TOUCH_MIN)
        .bind(ui::bind("twinvalve/input_gain"));
    let k = ui::rotary_at(b, front, "knob.fluted_stack", &p, gx, gy);
    ui::hide_ring(b, k);
    ui::silk(b, front, "Input Gain", gx, s.titles, ink);
    ui::ring_numerals(
        b,
        front,
        (gx, gy),
        r,
        290.0,
        &interleave(&GAIN_STEPS, ""),
        numerals,
    );
    ui::ring_numerals(
        b,
        front,
        (gx, gy),
        r,
        290.0,
        &interleave(&[""; 11], "o"),
        numerals.size(6.5),
    );

    let (tx, ty) = s.threshold;
    let p = Place::new("Threshold")
        .size(s.big)
        .legend(" ")
        .sweep(280.0)
        .touch(TOUCH_MIN)
        .bind(ui::bind("twinvalve/threshold"));
    let t = ui::rotary_at(b, front, "knob.fluted_stack", &p, tx, ty);
    ui::hide_ring(b, t);
    ui::silk(b, front, "Threshold", tx, s.titles, ink);
    ui::ring_numerals(
        b,
        front,
        (tx, ty),
        r,
        280.0,
        &["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10"],
        numerals,
    );

    // ── TIME CONSTANT: six positions over the top ────────────────────
    let (cx, cy, d) = s.time;
    let p = Place::new("Time Constant")
        .size(d)
        .legend(" ")
        .options(&["1", "2", "3", "4", "5", "6"])
        .sweep(150.0)
        .touch(TOUCH_MIN)
        .bind(ui::bind("twinvalve/time_constant"));
    let tc = ui::rotary_at(b, front, "knob.paddle", &p, cx, cy);
    ui::hide_ring(b, tc);
    ui::silk(b, front, "Time Constant", cx, s.titles, ink);
    ui::ring_numerals(
        b,
        front,
        (cx, cy),
        d * 0.8,
        150.0,
        &["1", "2", "3", "4", "5", "6"],
        numerals,
    );

    // ── The name ─────────────────────────────────────────────────────
    let (nx, ny, size) = s.name;
    ui::silk_slanted(
        b,
        front,
        "Twin Valve",
        nx,
        ny,
        Ink::new(size, INK).weight(500.0).track(0.02).plain(),
        0.18,
    );
    let fine = Ink::new(size * 0.24, INK).weight(600.0).track(0.1);
    let under = ny + size * 0.95;
    ui::silk_left(b, front, "Variable-Mu Valve", nx + 4.0, under, fine);
    ui::silk_left(b, front, "Two-Channel", nx + 4.0, under + size * 0.36, fine);
    ui::silk_left(
        b,
        front,
        "Limiter",
        nx + size * 4.1,
        under + size * 0.18,
        ink.size(size * 0.38).weight(600.0),
    );

    if tall {
        grey_strip(b, front, (w, h));
        metering(b, front, ink, numerals);
        agc(b, front, ink, (w, h));
    } else {
        for (x, y) in [(w - 18.0, 18.0), (w - 18.0, h - 18.0)] {
            ui::phillips(b, front, (x, y), 8.0, CHROME);
        }
    }
    ui::fit_root(b, root, front);
}

/// The black bakelite case, `side` square on (`cx`, `cy`): the cream card
/// in its top half behind a window, the zero-set rib and screw below.
fn meter(b: &mut Builder<'_>, parent: NodeId, (cx, cy, side): (f64, f64, f64), place: &Place) {
    let k = side / 172.0;
    let (x, y) = (cx - side / 2.0, cy - side / 2.0);
    let case = draw::rounded(
        draw::rect(
            "meter case",
            x,
            y,
            side,
            side,
            draw::linear(
                165.0,
                &[(0.0, "#2a2a2c"), (0.4, "#161618"), (1.0, "#0b0b0c")],
            ),
        ),
        9.0 * k,
    );
    b.add(
        parent,
        draw::with_effects(
            draw::stroked(case, "#000000", 1.0),
            vec![
                draw::drop_shadow("#000000a6", 0.0, 3.0, 7.0),
                draw::inner_shadow("#ffffff26", 0.0, 1.2, 1.0),
            ],
        ),
    );
    let cw = 146.0 * k;
    let ch = cw * 0.58;
    let top = y + 13.0 * k;
    let recess = draw::rounded(
        draw::rect(
            "meter window",
            cx - cw / 2.0 - 3.0,
            top - 3.0,
            cw + 6.0,
            ch + 6.0,
            draw::solid("#050506"),
        ),
        6.0,
    );
    b.add(
        parent,
        draw::with_effects(recess, vec![draw::inner_shadow("#000000", 0.0, 2.0, 3.0)]),
    );
    let p = place
        .clone()
        .size(cw)
        .variant("open")
        .legend("VU")
        .options(&["glass"]);
    let m = ui::put(b, parent, "meter.vu_ivory", &p);
    ui::move_to(b, m, cx - cw / 2.0, top);
    b.add(
        parent,
        draw::rounded(
            draw::rect(
                "card tint",
                cx - cw / 2.0,
                top,
                cw,
                ch,
                draw::solid("#e8d6ae40"),
            ),
            3.0,
        ),
    );
    let rib_top = top + ch + 8.0 * k;
    let rib = draw::rounded(
        draw::rect(
            "zero rib",
            cx - 13.0 * k,
            rib_top,
            26.0 * k,
            y + side - 14.0 * k - rib_top,
            draw::linear(
                90.0,
                &[(0.0, "#101011"), (0.5, "#2a2a2c"), (1.0, "#0c0c0d")],
            ),
        ),
        3.0,
    );
    b.add(
        parent,
        draw::with_effects(rib, vec![draw::drop_shadow("#00000099", 0.0, 1.0, 2.0)]),
    );
    b.add(
        parent,
        draw::rect(
            "zero rib groove",
            cx - 0.6,
            rib_top + 3.0,
            1.2,
            26.0 * k,
            draw::solid("#00000099"),
        ),
    );
    let s = (cx, y + side - 22.0 * k);
    b.add(
        parent,
        draw::circle(
            "zero screw",
            s.0,
            s.1,
            6.5 * k,
            draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#3a3a3c"), (1.0, "#0a0a0b")]),
        ),
    );
    b.add(
        parent,
        draw::rotated(
            draw::rect(
                "zero screw slot",
                s.0 - 5.0 * k,
                s.1 - 0.7,
                10.0 * k,
                1.4,
                draw::solid("#000000"),
            ),
            35.0,
        ),
    );
}

/// The tall face's METERING switch (BAL / ZERO / BAL) with its trims.
fn metering(b: &mut Builder<'_>, front: NodeId, ink: Ink<'_>, numerals: Ink<'_>) {
    let c = (338.0, 94.0);
    let p = Place::new("Metering")
        .size(78.0)
        .legend(" ")
        .options(&["1", "2", "3"])
        .sweep(80.0)
        .touch(TOUCH_MIN)
        .bind(ui::bind("twinvalve/metering"));
    let m = ui::rotary_at(b, front, "knob.paddle", &p, c.0, c.1);
    ui::hide_ring(b, m);
    ui::silk(b, front, "Metering", c.0, 30.0, ink);
    ui::ring_numerals(
        b,
        front,
        c,
        50.0,
        80.0,
        &["Bal", "Zero", "Bal"],
        numerals.weight(800.0),
    );
    for (dx, name) in [(-30.0, "Zero"), (30.0, "Bal")] {
        let s = (c.0 + dx, 160.0);
        ui::silk(b, front, name, s.0, s.1 - 16.0, ink.size(8.0));
        trim(b, front, s);
    }
}

/// The tall face's AGC switch (LEFT / RIGHT or LAT / VERT), top right.
fn agc(b: &mut Builder<'_>, front: NodeId, ink: Ink<'_>, (w, h): (f64, f64)) {
    let c = (680.0, 130.0);
    let p = Place::new("AGC")
        .size(48.0)
        .legend(" ")
        .options(&["1", "2"])
        .sweep(60.0)
        .touch(TOUCH_MIN)
        .bind(ui::bind("twinvalve/agc"));
    let agc = ui::rotary_at(b, front, "knob.paddle", &p, c.0, c.1);
    ui::hide_ring(b, agc);
    ui::silk(b, front, "AGC", c.0, 30.0, ink);
    let small = ink.size(8.0);
    ui::silk_right(b, front, "Left", c.0 - 24.0, 64.0, small);
    ui::silk_right(b, front, "Right", c.0 - 24.0, 78.0, small);
    ui::silk_left(b, front, "Lat", c.0 + 24.0, 64.0, small);
    ui::silk_left(b, front, "Vert", c.0 + 24.0, 78.0, small);
    // The mark: a step between the two positions.
    let (cx, cy) = (c.0, 71.0);
    let pts = [
        (-14.0, -6.0),
        (-6.0, -6.0),
        (-6.0, 6.0),
        (6.0, -6.0),
        (6.0, 6.0),
        (14.0, 6.0),
    ];
    let mut d = String::new();
    for seg in pts.windows(2) {
        let (a, e) = (
            (cx + seg[0].0, cy + seg[0].1),
            (cx + seg[1].0, cy + seg[1].1),
        );
        let (ux, uy) = (e.0 - a.0, e.1 - a.1);
        let len = ux.hypot(uy);
        let (nx, ny) = (-uy / len * 0.6, ux / len * 0.6);
        d.push_str(&draw::polygon(&[
            (a.0 + nx, a.1 + ny),
            (e.0 + nx, e.1 + ny),
            (e.0 - nx, e.1 - ny),
            (a.0 - nx, a.1 - ny),
        ]));
        d.push(' ');
    }
    b.add(
        front,
        draw::path("agc mark", 0.0, 0.0, w, h, d, draw::solid(INK)),
    );
}

/// Down the grey at the left of the tall face: ON, the pilot lamp, the
/// fuse.
fn grey_strip(b: &mut Builder<'_>, front: NodeId, (w, h): (f64, f64)) {
    let lx = 42.0;
    let on = ui::put(
        b,
        front,
        "switch.toggle_bare",
        &Place::new("Power")
            .size(44.0)
            .legend(" ")
            .options(&["On", ""])
            .ink(surface_kit::Ink::Color("#2a2a28".into()))
            .bind(ui::bind("twinvalve/power")),
    );
    super::super::opto_leveler::place_toggle(b, on, lx, 84.0);
    ui::centred_at(
        b,
        front,
        "button.jewel",
        &Place::new("Pilot")
            .size(34.0)
            .variant("white")
            .bind(ui::bind("twinvalve/power")),
        lx,
        174.0,
    );
    let cy = 254.0;
    b.add(
        front,
        draw::with_effects(
            draw::circle(
                "fuse ring",
                lx,
                cy,
                18.0,
                draw::radial_at(
                    0.36,
                    0.3,
                    0.9,
                    &[(0.0, "#f0f0ea"), (0.6, "#9a9a94"), (1.0, "#44443f")],
                ),
            ),
            vec![draw::drop_shadow("#00000066", 0.0, 1.5, 2.5)],
        ),
    );
    b.add(
        front,
        draw::path(
            "fuse cap",
            0.0,
            0.0,
            w,
            h,
            draw::lobed(lx, cy, 15.0, 24, 0.8),
            draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#3c3c3e"), (1.0, "#0c0c0d")]),
        ),
    );
    ui::silk(
        b,
        front,
        "Fuse",
        lx,
        cy,
        Ink::new(5.6, INK).weight(700.0).track(0.1),
    );
    ui::silk(
        b,
        front,
        "5A",
        lx,
        cy + 29.0,
        Ink::new(8.5, "#2a2a28").weight(700.0).track(0.04),
    );
}
