//! The big twin-channel variable-mu valve limiter — laid out from photos of
//! the 1950s unit (Fairchild 670), with no maker's names on it.
//!
//! A black engraved plate set into a light grey cabinet front. Down the
//! left of the plate, a black bakelite meter (gain reduction on a cream
//! card); then the METERING selector
//! (BAL / ZERO / BAL, with its ZERO and BAL trim screws under it), the
//! chunky scalloped INPUT GAIN (20 → 0 dB in 2 dB steps) and THRESHOLD
//! (0–10) knobs, and the six-position TIME CONSTANT; the AGC
//! switch (LEFT/RIGHT or LAT/VERT — stereo or mid/side) under the time
//! constant. On the grey to the left: the ON toggle, the pilot lamp
//! and the fuse. White engraving, numerals turned round their knobs.
//!
//! Shown as one channel: the unit's top half. The plugin runs both sides
//! from these controls (linked), so the second row of the real unit isn't
//! repeated; the AGC switch still picks left/right or lateral/vertical
//! (mid/side). The metering switch's BAL positions put the output level on
//! the meter (the unit uses them to trim the push-pull balance); ZERO reads
//! gain reduction.

use document_proto::NodeId;
use surface_kit::{Place, draw};
use surface_proto::{MeterTap, Range, SourceSelect, Unit};
use surface_widgets::Builder;

use super::ui::{self, Ink};
use crate::face::{Face, canvas};

pub mod lane;

/// The cabinet front, and the black plate in it (at `PX`, `PY`).
const W: f64 = 1060.0;
const H: f64 = PY + PH + 45.0;
const PX: f64 = 110.0;
const PY: f64 = 45.0;
const PW: f64 = 900.0;
/// The plate: one channel's row, the top half of the two-channel plate.
const PH: f64 = 320.0;
/// Vertical positions are fractions of the full two-channel plate's
/// height, so the row keeps the real unit's spacing.
const ROW_SCALE: f64 = 500.0;
const INK: &str = "#eeece6";
const CHROME: &str = "#c6c6c0";
/// Input gain's printed steps, attenuation first.
const GAIN_STEPS: [&str; 11] = ["20", "18", "16", "14", "12", "10", "8", "6", "4", "2", "0"];

/// A point on the plate, as fractions of it.
fn p(u: f64, v: f64) -> (f64, f64) {
    (PX + u * PW, PY + v * ROW_SCALE)
}

pub fn face() -> Face {
    let scene = canvas("Twin Valve Limiter", build);
    let params = vec![
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
        ui::choice(
            "twinvalve/metering",
            "Metering",
            &["Bal −", "Zero", "Bal +"],
            1,
        )
        .short("Meter"),
        ui::choice("twinvalve/agc", "AGC", &["Left / Right", "Lat / Vert"], 0),
        ui::toggle("twinvalve/power", "Power", true),
    ];
    let streams = ui::streams("twinvalve", "Twin Valve Limiter");
    Face {
        file: "06-twin-valve",
        plugin: "comp",
        scene,
        params,
        streams,
    }
}

fn build(b: &mut Builder<'_>, root: NodeId) {
    let front = ui::put(
        b,
        root,
        "panel.painted",
        &Place::new("Cabinet front").variant("grey").options(&[
            &format!("w={W}"),
            &format!("h={H}"),
            "ends=none",
        ]),
    );
    ui::repaint(
        b,
        front,
        &[(0.0, "#d6d6d2"), (0.5, "#cacac6"), (1.0, "#bdbdb9")],
    );
    for (x, y) in [
        (W - 25.0, 60.0),
        (W - 25.0, H - 60.0),
        (30.0, H - 22.0),
        (W - 70.0, H - 22.0),
    ] {
        ui::phillips(b, front, (x, y), 11.0, CHROME);
    }

    // ── The plate ────────────────────────────────────────────────────
    let plate = draw::rounded(
        draw::rect(
            "plate",
            PX,
            PY,
            PW,
            PH,
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

    // ── On the grey: ON, the pilot lamp, the fuse ────────────────────
    let lx = PX / 2.0;
    let on = ui::put(
        b,
        front,
        "switch.toggle_bare",
        &Place::new("Power")
            .size(38.0)
            .legend(" ")
            .options(&["On", ""])
            .ink(surface_kit::Ink::Color("#2a2a28".into()))
            .bind(ui::bind("twinvalve/power")),
    );
    super::opto_leveler::place_toggle(b, on, lx, 110.0);
    ui::centred_at(
        b,
        front,
        "button.jewel",
        &Place::new("Pilot")
            .size(30.0)
            .variant("white")
            .bind(ui::bind("twinvalve/power")),
        lx,
        220.0,
    );
    fuse(b, front, lx, 320.0);

    let plate_ink = Ink::new(8.8, INK).weight(800.0).track(0.06);
    channel(b, front, plate_ink);

    // ── AGC, under the time constant ─────────────────────────────────
    let c = p(0.885, 0.53);
    let agc = ui::rotary_at(
        b,
        front,
        "knob.paddle",
        &Place::new("AGC")
            .size(46.0)
            .legend(" ")
            .options(&["1", "2"])
            .sweep(60.0)
            .touch(surface_kit::TOUCH_MIN)
            .bind(ui::bind("twinvalve/agc")),
        c.0,
        c.1,
    );
    ui::hide_ring(b, agc);
    ui::silk(b, front, "AGC", c.0, p(0.0, 0.405).1, plate_ink);
    let small = plate_ink.size(7.4);
    ui::silk_right(b, front, "Left", p(0.852, 0.0).0, p(0.0, 0.44).1, small);
    ui::silk_right(b, front, "Right", p(0.852, 0.0).0, p(0.0, 0.47).1, small);
    ui::silk_left(b, front, "Lat", p(0.918, 0.0).0, p(0.0, 0.44).1, small);
    ui::silk_left(b, front, "Vert", p(0.918, 0.0).0, p(0.0, 0.47).1, small);
    agc_mark(b, front, p(0.885, 0.455));

    // ── The name, under the knobs ────────────────────────────────────
    let (nx, ny) = p(0.47, 0.56);
    ui::silk_slanted(
        b,
        front,
        "Twin Valve",
        nx,
        ny,
        Ink::new(23.0, INK).weight(500.0).track(0.02).plain(),
        0.18,
    );
    let fine = Ink::new(4.8, INK).weight(600.0).track(0.1);
    ui::silk_left(b, front, "Variable-Mu Valve", nx + 142.0, ny - 4.0, fine);
    ui::silk_left(b, front, "Two-Channel", nx + 142.0, ny + 3.5, fine);
    ui::silk_left(
        b,
        front,
        "Limiter",
        nx + 214.0,
        ny,
        plate_ink.size(8.5).weight(600.0),
    );

    ui::fit_root(b, root, front);
}

/// The channel row: meter, metering switch, input gain, threshold, time
/// constant.
fn channel(b: &mut Builder<'_>, front: NodeId, ink: Ink<'_>) {
    let at = p;
    let addr = |n: &str| format!("twinvalve/{n}");
    let gr = "twinvalve/gr";

    // The meter: gain reduction; the BAL positions show the output.
    let (mx, my) = at(0.165, 0.245);
    let out = MeterTap::new("twinvalve/out");
    let select = SourceSelect::new(
        addr("metering"),
        vec![Some(out.clone()), Some(MeterTap::new(gr)), Some(out)],
    );
    meter(
        b,
        front,
        (mx, my),
        &Place::new("Gain Reduction meter")
            .bind(ui::stream(gr))
            .select(select),
    );

    // METERING, with the trims under it.
    let c = at(0.335, 0.27);
    let m = ui::rotary_at(
        b,
        front,
        "knob.paddle",
        &Place::new("Metering")
            .size(80.0)
            .legend(" ")
            .options(&["1", "2", "3"])
            .sweep(80.0)
            .touch(surface_kit::TOUCH_MIN)
            .bind(ui::bind(&addr("metering"))),
        c.0,
        c.1,
    );
    ui::hide_ring(b, m);
    ui::silk(b, front, "Metering", c.0, at(0.0, 0.08).1, ink);
    ui::ring_numerals(
        b,
        front,
        c,
        54.0,
        80.0,
        &["Bal", "Zero", "Bal"],
        ink.size(10.0).weight(800.0),
    );
    for (u, name) in [(0.30, "Zero"), (0.40, "Bal")] {
        let s = at(u, 0.43);
        ui::silk(b, front, name, s.0, s.1 - 17.0, ink.size(7.6));
        trim(b, front, s);
    }

    // INPUT GAIN: stepped, numerals and odd-dB rings turned round it.
    let c = at(0.49, 0.29);
    let k = ui::rotary_at(
        b,
        front,
        "knob.fluted_stack",
        &Place::new("Input Gain")
            .size(100.0)
            .legend(" ")
            .scale(&GAIN_STEPS)
            .sweep(290.0)
            .touch(surface_kit::TOUCH_MIN)
            .bind(ui::bind(&addr("input_gain"))),
        c.0,
        c.1,
    );
    ui::hide_ring(b, k);
    ui::silk(b, front, "Input Gain", c.0, at(0.0, 0.08).1, ink);
    let numerals = ink.size(10.0).weight(600.0).track(0.0);
    ui::ring_numerals(
        b,
        front,
        c,
        62.0,
        290.0,
        &interleave(&GAIN_STEPS, ""),
        numerals,
    );
    ui::ring_numerals(
        b,
        front,
        c,
        62.0,
        290.0,
        &interleave(&["", "", "", "", "", "", "", "", "", "", ""], "o"),
        numerals.size(6.5),
    );

    // THRESHOLD: 0–10.
    let c = at(0.685, 0.29);
    let t = ui::rotary_at(
        b,
        front,
        "knob.fluted_stack",
        &Place::new("Threshold")
            .size(100.0)
            .legend(" ")
            .sweep(280.0)
            .touch(surface_kit::TOUCH_MIN)
            .bind(ui::bind(&addr("threshold"))),
        c.0,
        c.1,
    );
    ui::hide_ring(b, t);
    ui::silk(b, front, "Threshold", c.0, at(0.0, 0.08).1, ink);
    ui::ring_numerals(
        b,
        front,
        c,
        62.0,
        280.0,
        &["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10"],
        numerals,
    );

    // TIME CONSTANT: six positions over the top.
    let c = at(0.885, 0.235);
    let tc = ui::rotary_at(
        b,
        front,
        "knob.paddle",
        &Place::new("Time Constant")
            .size(48.0)
            .legend(" ")
            .options(&["1", "2", "3", "4", "5", "6"])
            .sweep(150.0)
            .touch(surface_kit::TOUCH_MIN)
            .bind(ui::bind(&addr("time_constant"))),
        c.0,
        c.1,
    );
    ui::hide_ring(b, tc);
    ui::silk(b, front, "Time Constant", c.0, c.1 - 56.0, ink);
    ui::ring_numerals(
        b,
        front,
        c,
        38.0,
        150.0,
        &["1", "2", "3", "4", "5", "6"],
        numerals.size(9.0),
    );
}

/// `labels` with `between` set between each pair.
fn interleave(labels: &[&'static str], between: &'static str) -> Vec<&'static str> {
    let mut v = Vec::new();
    for (i, s) in labels.iter().enumerate() {
        if i > 0 {
            v.push(between);
        }
        v.push(*s);
    }
    v
}

/// A black bakelite meter case: the cream card in its top half behind a
/// window, the movement's zero-set rib and screw below.
fn meter(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), place: &Place) {
    let side = 172.0;
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
        9.0,
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
    let cw = 146.0;
    let ch = cw * 0.58;
    let top = y + 13.0;
    // The window's recess round the card.
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
    // The zero-set rib and its screw.
    let rib_top = top + ch + 8.0;
    let rib = draw::rounded(
        draw::rect(
            "zero rib",
            cx - 13.0,
            rib_top,
            26.0,
            y + side - 14.0 - rib_top,
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
            26.0,
            draw::solid("#00000099"),
        ),
    );
    let s = (cx, y + side - 22.0);
    b.add(
        parent,
        draw::circle(
            "zero screw",
            s.0,
            s.1,
            6.5,
            draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#3a3a3c"), (1.0, "#0a0a0b")]),
        ),
    );
    b.add(
        parent,
        draw::rotated(
            draw::rect(
                "zero screw slot",
                s.0 - 5.0,
                s.1 - 0.7,
                10.0,
                1.4,
                draw::solid("#000000"),
            ),
            35.0,
        ),
    );
}

/// A screwdriver trim through the plate (ZERO, BAL).
fn trim(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64)) {
    b.add(
        parent,
        draw::circle(
            "trim bushing",
            cx,
            cy,
            7.5,
            draw::radial_at(
                0.36,
                0.3,
                0.9,
                &[(0.0, "#e6e6e0"), (0.6, "#8f8f89"), (1.0, "#3c3c38")],
            ),
        ),
    );
    b.add(
        parent,
        draw::circle(
            "trim shaft",
            cx,
            cy,
            4.2,
            draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#d2d2cc"), (1.0, "#5c5c58")]),
        ),
    );
    b.add(
        parent,
        draw::rotated(
            draw::rect(
                "trim slot",
                cx - 3.8,
                cy - 0.6,
                7.6,
                1.2,
                draw::solid("#1a1a1a"),
            ),
            -30.0,
        ),
    );
}

/// The AGC switch's mark: a step between its two positions.
fn agc_mark(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64)) {
    let pts = [
        (-14.0, -6.0),
        (-6.0, -6.0),
        (-6.0, 6.0),
        (6.0, -6.0),
        (6.0, 6.0),
        (14.0, 6.0),
    ];
    let mut d = String::new();
    for w in pts.windows(2) {
        let (a, c) = ((cx + w[0].0, cy + w[0].1), (cx + w[1].0, cy + w[1].1));
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
        parent,
        draw::path("agc mark", 0.0, 0.0, W, H, d, draw::solid(INK)),
    );
}

/// The fuse holder: a knurled black cap in a chrome ring, FUSE on its face.
fn fuse(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64) {
    b.add(
        parent,
        draw::with_effects(
            draw::circle(
                "fuse ring",
                cx,
                cy,
                17.0,
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
        parent,
        draw::path(
            "fuse cap",
            0.0,
            0.0,
            W,
            H,
            draw::lobed(cx, cy, 14.0, 24, 0.8),
            draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#3c3c3e"), (1.0, "#0c0c0d")]),
        ),
    );
    ui::silk(
        b,
        parent,
        "Fuse",
        cx,
        cy,
        Ink::new(5.2, INK).weight(700.0).track(0.1),
    );
    ui::silk(
        b,
        parent,
        "5A",
        cx,
        cy + 27.0,
        Ink::new(8.0, "#2a2a28").weight(700.0).track(0.04),
    );
}
