//! The console bus compressor in a host's FX column (see [`crate::lane`]).
//!
//! - **Lane**: the pale silver-grey front — the tall black meter housing at
//!   the left, then THRESHOLD, ATTACK, RATIO, RELEASE and MAKE-UP on their
//!   blue-capped knobs, and the backlit COMPRESSOR IN key.
//! - **Tall**: the name stack and series badge, the meter, and the three
//!   keys (COMPRESSOR IN, EXTERNAL SIDE-CHAIN, AUTO FADE) across the top;
//!   the five controls and the fade RATE across the bottom, RATE tied to
//!   its key.

use frame::document::proto::{NodeId, TextAlign};
use frame::surface::kit::{Ink, Place, TOUCH_MIN, draw};
use frame::surface::proto::{Binding, Unit};
use frame::surface::widgets::Builder;

use super::super::logo;
use super::super::ui::{self, bind};
use super::{ATTACKS, INK, RATE_VALUES, RATES, RATIOS, RELEASES};
use crate::face::{Face, canvas};
use crate::lane::Shape;

pub fn faces() -> Vec<Face> {
    Shape::ALL
        .into_iter()
        .map(|shape| {
            let scene = canvas(
                &format!("Bus Compressor ({})", shape.suffix()),
                |b, root| build(b, root, shape),
            );
            let mut params = vec![
                ui::db("buscomp/threshold", "Threshold", -30.0, 0.0, -12.0).featured(),
                ui::choice("buscomp/ratio", "Ratio", &["2:1", "4:1", "10:1"], 1).featured(),
                ui::choice(
                    "buscomp/attack",
                    "Attack",
                    &["0.1 ms", "0.3 ms", "1 ms", "3 ms", "10 ms", "30 ms"],
                    3,
                ),
                ui::choice(
                    "buscomp/release",
                    "Release",
                    &["0.1 s", "0.3 s", "0.6 s", "1.2 s", "Auto"],
                    4,
                ),
                ui::db("buscomp/makeup", "Make-up", 0.0, 18.0, 4.0).featured(),
                ui::toggle("buscomp/in", "Compressor In", true).featured(),
            ];
            if shape == Shape::Tall {
                params.extend([
                    ui::toggle("buscomp/ext_sc", "External Side-Chain", false),
                    ui::toggle("buscomp/autofade", "Auto Fade", false),
                    ui::param(
                        "buscomp/fade_rate",
                        "Fade Rate",
                        ui::printed(RATE_VALUES, Unit::Seconds),
                        10.0,
                    ),
                ]);
            }
            let streams = vec![ui::gr_stream("buscomp/gr", "Gain Reduction")];
            Face {
                file: shape.file("08-bus-comp"),
                plugin: "comp",
                scene,
                params,
                streams,
            }
        })
        .collect()
}

/// A shape's knob row: shaft line, knob size, header line, header size.
#[derive(Clone, Copy)]
struct Row {
    y: f64,
    d: f64,
    header: f64,
    size: f64,
}

fn build(b: &mut Builder<'_>, root: NodeId, shape: Shape) {
    let (w, h) = shape.size();
    ui::fit_root(b, root, w, h);
    let panel = ui::panel(b, root, "panel.painted", "api", w, h, "none");
    if let Some(n) = b.scene.node_mut(panel) {
        n.fills = vec![draw::linear(
            178.0,
            &[(0.0, "#dde1e5"), (0.5, "#cdd1d6"), (1.0, "#b4b9bf")],
        )];
        n.corner_radii = frame::document::proto::CornerRadii::uniform(8.0);
    }
    let tall = shape == Shape::Tall;
    let row = if tall {
        Row {
            y: 262.0,
            d: 62.0,
            header: 196.0,
            size: 11.0,
        }
    } else {
        Row {
            y: 94.0,
            d: 54.0,
            header: 24.0,
            size: 10.0,
        }
    };
    let xs: [f64; 5] = if tall {
        [86.0, 204.0, 322.0, 440.0, 558.0]
    } else {
        [232.0, 328.0, 424.0, 520.0, 616.0]
    };

    // ── Gain reduction: the tall black housing ───────────────────────
    let (mx, my, mw) = if tall {
        (250.0, 92.0, 176.0)
    } else {
        (92.0, h / 2.0, 150.0)
    };
    let meter = Place::new("Compression")
        .size(mw)
        .legend("Compression")
        .options(&["glass"])
        .bind(Binding::stream("buscomp/gr"));
    ui::put(b, panel, "meter.gr_compression", &meter, mx, my);

    // ── The five controls ────────────────────────────────────────────
    knob(
        b,
        panel,
        Place::new("Threshold")
            .scale(&["-30", "", "", "-15", "", "", "0"])
            .bind(bind("buscomp/threshold")),
        "Threshold",
        (xs[0], row),
    );
    knob(
        b,
        panel,
        Place::new("Attack - mS")
            .options(ATTACKS)
            .bind(bind("buscomp/attack")),
        "Attack - mS",
        (xs[1], row),
    );
    knob(
        b,
        panel,
        Place::new("Ratio")
            .options(RATIOS)
            .bind(bind("buscomp/ratio")),
        "Ratio",
        (xs[2], row),
    );
    knob(
        b,
        panel,
        Place::new("Release - S")
            .options(RELEASES)
            .bind(bind("buscomp/release")),
        "Release - S",
        (xs[3], row),
    );
    knob(
        b,
        panel,
        Place::new("Make-up")
            .scale(&["0", "", "", "9", "", "", "18"])
            .bind(bind("buscomp/makeup")),
        "Make-up",
        (xs[4], row),
    );

    if tall {
        name_stack(b, panel, (34.0, 52.0));
        badge(b, panel, (74.0, 132.0));
        let ky = 100.0;
        key(b, panel, ("buscomp/in", &["Compressor"]), (420.0, ky), None);
        divider(b, panel, 475.0, (24.0, 160.0));
        key(
            b,
            panel,
            ("buscomp/ext_sc", &["External", "Side - Chain"]),
            (530.0, ky),
            None,
        );
        divider(b, panel, 585.0, (24.0, 160.0));
        key(
            b,
            panel,
            ("buscomp/autofade", &["Auto Fade"]),
            (676.0, ky),
            None,
        );
        knob(
            b,
            panel,
            Place::new("Rate")
                .scale(RATES)
                .bind(bind("buscomp/fade_rate")),
            "Rate - S",
            (676.0, row),
        );
        // The line tying AUTO FADE to its RATE.
        ui::rule(
            b,
            panel,
            676.0 - 0.5,
            ky + 36.0,
            1.0,
            row.header - ky - 46.0,
            INK,
        );
    } else {
        key(
            b,
            panel,
            ("buscomp/in", &["Compressor"]),
            (704.0, row.y),
            Some(row.header),
        );
        ui::silk(b, panel, "Stereo Bus Comp", 704.0, 146.0, 7.5, INK, 700.0);
    }
}

/// A blue-capped skirted knob (a rotary switch where `p` has options),
/// its header above it.
fn knob(
    b: &mut Builder<'_>,
    panel: NodeId,
    p: Place,
    header: &str,
    (cx, row): (f64, Row),
) -> NodeId {
    ui::silk(b, panel, header, cx, row.header, row.size, INK, 800.0);
    let p = p
        .size(row.d)
        .variant("blue")
        .ink(Ink::Color(INK.into()))
        .touch(TOUCH_MIN)
        .keep_case();
    let k = ui::put(b, panel, "knob.capped_skirt", &p, cx, row.y);
    ui::strip_legend(b, k);
    k
}

/// A square backlit key in its raised grey bezel, its header on the line
/// `top` (the knobs' headers' line) or, with none, just over the bezel.
fn key(
    b: &mut Builder<'_>,
    panel: NodeId,
    (addr, header): (&str, &[&str]),
    (cx, cy): (f64, f64),
    top: Option<f64>,
) {
    let s = 44.0;
    let n = header.len() as f64;
    let first = top.map_or(cy - s * 0.66 - 14.0 - 12.0 * (n - 1.0), |t| {
        t - 6.0 * (n - 1.0)
    });
    for (i, l) in header.iter().enumerate() {
        ui::silk(b, panel, l, cx, first + i as f64 * 12.0, 10.0, INK, 800.0);
    }
    let bz = draw::rect(
        "key bezel",
        cx - s * 0.66,
        cy - s * 0.66,
        s * 1.32,
        s * 1.32,
        draw::linear(
            180.0,
            &[(0.0, "#e6e8ea"), (0.5, "#c3c6ca"), (1.0, "#9ea2a7")],
        ),
    );
    b.add(
        panel,
        draw::with_effects(
            draw::stroked(draw::rounded(bz, 2.5), "#00000055", 1.0),
            vec![
                draw::drop_shadow("#00000059", 0.0, 1.5, 3.0),
                draw::inner_shadow("#ffffffa0", 0.0, 1.0, 0.0),
            ],
        ),
    );
    let p = Place::new("In")
        .size(s)
        .variant("white")
        .legend("In")
        .bind(bind(addr));
    ui::put(b, panel, "button.square", &p, cx, cy);
}

/// A thin dark divider down the front from `y0` to `y1`.
fn divider(b: &mut Builder<'_>, panel: NodeId, x: f64, (y0, y1): (f64, f64)) {
    ui::rule(b, panel, x, y0, 1.0, y1 - y0, "#00000080");
    ui::rule(b, panel, x + 1.0, y0, 1.0, y1 - y0, "#ffffff66");
}

/// The maker's corner: the name stacked in three lines.
fn name_stack(b: &mut Builder<'_>, panel: NodeId, (x, y): (f64, f64)) {
    let stack = logo::Style {
        size: 19.0,
        weight: 600.0,
        italic: false,
        tracking: -0.01,
    };
    for (i, l) in ["Stereo", "Bus", "Comp"].iter().enumerate() {
        logo::mark(
            b,
            panel,
            l,
            x,
            y + i as f64 * 21.0,
            100.0,
            stack,
            INK,
            TextAlign::Left,
        );
    }
}

/// The series badge: a big slanted word over a rule, spaced small
/// capitals, and a black tag reversed out in white.
fn badge(b: &mut Builder<'_>, panel: NodeId, (bx, y): (f64, f64)) {
    let big = logo::Style {
        size: 26.0,
        weight: 700.0,
        italic: true,
        tracking: -0.02,
    };
    logo::mark(
        b,
        panel,
        "Bus",
        bx - 40.0,
        y,
        80.0,
        big,
        INK,
        TextAlign::Center,
    );
    ui::rule(b, panel, bx - 38.0, y + 15.0, 76.0, 1.2, INK);
    ui::silk(b, panel, "Stereo", bx, y + 22.0, 6.5, INK, 600.0);
    b.add(
        panel,
        draw::rect(
            "badge tag",
            bx - 38.0,
            y + 27.0,
            76.0,
            10.0,
            draw::solid(INK),
        ),
    );
    ui::silk(b, panel, "Compressor", bx, y + 32.0, 6.3, "#dde1e5", 700.0);
}
