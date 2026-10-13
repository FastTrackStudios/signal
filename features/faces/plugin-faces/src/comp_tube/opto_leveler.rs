//! The optical leveling amplifier — laid out from photos of the grey 3U
//! unit (Teletronix / UA LA-2A), with no maker's names on it.
//!
//! Light grey painted steel between plain folded ears. Top left, the red
//! slanted name with its double swoosh rule; the COMPRESS/LIMIT bat toggle
//! (and the hex plug beside it) below. GAIN left of centre and PEAK
//! REDUCTION right of it: glossy black knobs with a square tab moulded on
//! the tail, read against bold numerals 0–100 turned round the circle. The
//! VU sits dead centre in a charcoal picture frame with a chamfered
//! opening; top right, the meter selector (OUTPUT +10 / GAIN REDUCTION /
//! OUTPUT +4) — the same knob — over the POWER toggle. Positions are
//! fractions of the photographed panel.

use frame::document::proto::NodeId;
use frame::surface::kit::{Ink as KitInk, Place, draw};
use frame::surface::proto::{MeterTap, SourceSelect};
use frame::surface::widgets::Builder;

use super::ui::{self, Ink};
use crate::face::{Face, canvas};

pub mod lane;

/// The photographed panel, 1920 px across its ears, at this width.
const W: f64 = 906.0;
const H: f64 = 258.0;
/// Photo px → face px.
const S: f64 = W / 1920.0;
/// Black numerals, the grey-brown of the captions, the name's red.
const INK: &str = "#262523";
const DIM: &str = "#4d4b47";
const RED: &str = "#a0302b";
const CHROME: &str = "#c9c9c4";

/// A photo coordinate on this face.
const fn at(x: f64, y: f64) -> (f64, f64) {
    (x * S, y * S)
}

pub fn face() -> Face {
    let scene = canvas("Opto Leveler", build);
    let params = vec![
        ui::dial_0_100("opto/gain", "Gain", 40.0).short("Gain"),
        ui::dial_0_100("opto/peak_reduction", "Peak Reduction", 45.0).short("Peak Red"),
        ui::toggle("opto/limit", "Compress / Limit", false).short("Limit"),
        ui::choice(
            "opto/meter",
            "Meter",
            &["Output +10", "Gain Reduction", "Output +4"],
            1,
        ),
        ui::toggle("opto/power", "Power", true),
    ];
    Face {
        file: "04-opto-leveler",
        plugin: "comp",
        scene,
        params,
        streams: ui::streams("opto", "Opto Leveler"),
    }
}

fn build(b: &mut Builder<'_>, root: NodeId) {
    let panel = ui::put(
        b,
        root,
        "panel.painted",
        &Place::new("Leveler panel").variant("grey").options(&[
            &format!("w={W}"),
            &format!("h={H}"),
            "ends=none",
        ]),
    );
    ui::repaint(
        b,
        panel,
        &[(0.0, "#cbcdca"), (0.5, "#bfc1be"), (1.0, "#afb1ae")],
    );

    // ── Folded ears and the hardware ─────────────────────────────────
    let ear = draw::linear(90.0, &[(0.0, "#b3b5b2"), (1.0, "#a5a7a4")]);
    ui::ear_strip(b, panel, 0.0, 100.0 * S, H, ear.clone(), 100.0 * S);
    ui::ear_strip(b, panel, 1815.0 * S, W - 1815.0 * S, H, ear, 1815.0 * S);
    for y in [160.0, 385.0] {
        ui::rack_bolt(b, panel, at(35.0, y), 17.0);
        ui::rack_bolt(b, panel, at(1885.0, y), 17.0);
    }
    for (x, y) in [
        (183.0, 42.0),
        (1728.0, 38.0),
        (155.0, 495.0),
        (697.0, 495.0),
        (1228.0, 495.0),
        (1765.0, 495.0),
    ] {
        ui::phillips(b, panel, at(x, y), 10.0, CHROME);
    }
    ui::phillips(b, panel, at(1147.0, 92.0), 9.0, CHROME);
    ui::phillips(b, panel, at(960.0, 408.0), 5.5, CHROME);

    // ── The name: red, slanted, over a double swoosh ─────────────────
    let (x0, y0) = at(203.0, 120.0);
    ui::silk_slanted(
        b,
        panel,
        "Opto Leveler",
        x0,
        y0,
        Ink::new(15.5, RED).weight(800.0).track(0.0),
        0.22,
    );
    ui::silk_left(
        b,
        panel,
        "Leveling Amplifier",
        x0 + 6.0,
        at(0.0, 157.0).1,
        Ink::new(5.8, RED).weight(600.0).track(0.42),
    );
    swoosh(b, panel);
    let side = Ink::new(6.3, DIM).weight(600.0).track(0.12);
    ui::silk(
        b,
        panel,
        "Leveling Amplifier",
        at(575.0, 0.0).0,
        at(0.0, 118.0).1,
        side,
    );
    ui::silk(
        b,
        panel,
        "Photo-Optical",
        at(575.0, 0.0).0,
        at(0.0, 150.0).1,
        side,
    );

    // COMPRESS / LIMIT, and the hex plug beside it.
    let (tx, ty) = at(200.0, 365.0);
    let sw = ui::put(
        b,
        panel,
        "switch.toggle_bare",
        &Place::new("Compress / Limit")
            .size(34.0)
            .legend(" ")
            .options(&["Limit", "Compress"])
            .ink(KitInk::Color(DIM.into()))
            .bind(ui::bind("opto/limit")),
    );
    place_toggle(b, sw, tx, ty);
    let (hx, hy) = at(300.0, 365.0);
    hex_plug(b, panel, hx, hy, 11.5);

    // ── Gain | meter | Peak reduction ────────────────────────────────
    let scale = [
        "0", "10", "20", "30", "40", "50", "60", "70", "80", "90", "100",
    ];
    let numerals = Ink::new(12.0, INK).weight(700.0).track(0.0);
    let caption = Ink::new(7.2, DIM).weight(600.0).track(0.16);
    for (name, addr, x) in [
        ("Gain", "opto/gain", 560.0),
        ("Peak Reduction", "opto/peak_reduction", 1365.0),
    ] {
        let c = at(x, 320.0);
        let k = ui::rotary_at(
            b,
            panel,
            "knob.tab_pointer",
            &Place::new(name)
                .size(50.0)
                .legend(" ")
                .sweep(290.0)
                .scale(&scale)
                .touch(frame::surface::kit::TOUCH_MIN)
                .bind(ui::bind(addr)),
            c.0,
            c.1,
        );
        ui::hide_ring(b, k);
        ui::ring_numerals(b, panel, c, 43.0, 290.0, &scale, numerals);
        ui::ring_ticks(b, panel, c, (31.5, 34.0), 290.0, 21, 0.8, "#2625234d");
        ui::silk(b, panel, name, c.0, at(0.0, 466.0).1, caption);
    }

    // The meter switch really switches it: the output on its +10 range,
    // gain reduction, or the output on +4.
    let (mx, my) = at(960.0, 225.0);
    vu_frame(b, panel, mx, my);
    let vu = Place::new("Gain Reduction meter")
        .size(146.0)
        .variant("open")
        .legend(" ")
        .options(&["glass"])
        .bind(ui::stream("opto/gr"))
        .select(SourceSelect::new(
            "opto/meter",
            vec![
                Some(MeterTap::new("opto/out").offset(-10.0)),
                Some(MeterTap::new("opto/gr")),
                Some(MeterTap::new("opto/out").offset(-4.0)),
            ],
        ));
    ui::centred_at(b, panel, "meter.vu_ivory", &vu, mx, my);
    // The card is cream, aged under its lamp, not white.
    let (cw, ch) = (146.0, 146.0 * 0.58);
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
    let card = Ink::new(6.2, "#3a3129").weight(600.0).track(0.06);
    ui::silk(b, panel, "VU", mx - 60.0, my - 17.0, card.size(8.0));
    ui::silk(
        b,
        panel,
        "VU",
        mx + 60.0,
        my - 17.0,
        card.size(8.0).color("#a32a22"),
    );
    ui::silk(
        b,
        panel,
        "VU Level Indicator",
        mx,
        my - 33.0,
        card.size(4.6).track(0.12),
    );

    // ── Meter selector over power, right ─────────────────────────────
    let (sx, sy) = at(1620.0, 185.0);
    let meter = ui::rotary_at(
        b,
        panel,
        "knob.tab_pointer",
        &Place::new("Meter")
            .size(50.0)
            .legend(" ")
            .options(&["+10", "GR", "+4"])
            .sweep(100.0)
            .touch(frame::surface::kit::TOUCH_MIN)
            .bind(ui::bind("opto/meter")),
        sx,
        sy,
    );
    ui::hide_ring(b, meter);
    let small = Ink::new(6.4, DIM).weight(600.0).track(0.1);
    ui::silk(b, panel, "Gain Reduction", sx, at(0.0, 68.0).1, small);
    ui::silk(
        b,
        panel,
        "Output +10",
        at(1520.0, 0.0).0,
        at(0.0, 98.0).1,
        small,
    );
    ui::silk(
        b,
        panel,
        "Output +4",
        at(1716.0, 0.0).0,
        at(0.0, 98.0).1,
        small,
    );

    let (px, py) = at(1620.0, 365.0);
    let power = ui::put(
        b,
        panel,
        "switch.toggle_bare",
        &Place::new("Power")
            .size(34.0)
            .legend(" ")
            .options(&["On", "Power"])
            .ink(KitInk::Color(DIM.into()))
            .bind(ui::bind("opto/power")),
    );
    place_toggle(b, power, px, py);

    ui::fit_root(b, root, panel);
}

/// Put a `switch.toggle_bare` with its bushing at (`cx`, `cy`).
pub(super) fn place_toggle(b: &mut Builder<'_>, sw: NodeId, cx: f64, cy: f64) {
    let bushing = b.scene.walk(sw).into_iter().find_map(|id| {
        b.scene.node(id).filter(|n| n.name == "bushing").map(|n| {
            (
                n.transform.tx + n.size.width / 2.0,
                n.transform.ty + n.size.height / 2.0,
            )
        })
    });
    let (bx, by) = bushing.unwrap_or_default();
    ui::move_to(b, sw, cx - bx, cy - by);
}

/// The swoosh: two red rules under the name that kick up and run on.
fn swoosh(b: &mut Builder<'_>, panel: NodeId) {
    for (dy, dx) in [(0.0, 0.0), (8.0, 4.0)] {
        let pts = [
            (200.0, 176.0 + dy),
            (438.0 + dx, 176.0 + dy),
            (478.0 + dx, 88.0 + dy),
            (665.0, 88.0 + dy),
        ];
        let mut d = String::new();
        for w in pts.windows(2) {
            let (a, c) = (at(w[0].0, w[0].1), at(w[1].0, w[1].1));
            let (ux, uy) = (c.0 - a.0, c.1 - a.1);
            let len = ux.hypot(uy);
            let (nx, ny) = (-uy / len * 0.6, ux / len * 0.6);
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
            draw::path("swoosh", 0.0, 0.0, W, H, d, draw::solid(RED)),
        );
    }
}

/// The meter's charcoal picture frame, its opening chamfered and catching
/// the light on its right and lower faces.
fn vu_frame(b: &mut Builder<'_>, panel: NodeId, cx: f64, cy: f64) {
    let (ow, oh) = (390.0 * S, 230.0 * S);
    let (iw, ih) = (148.0, 87.0);
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
    let c = 9.0;
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
                W,
                H,
                draw::polygon(&pts),
                draw::solid(color),
            ),
        );
    }
}

/// The hex-headed plug beside the mode toggle.
fn hex_plug(b: &mut Builder<'_>, panel: NodeId, cx: f64, cy: f64, r: f64) {
    let nut = draw::path(
        "hex plug",
        0.0,
        0.0,
        W,
        H,
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
            20.0,
        ),
    );
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
            110.0,
        ),
    );
}
