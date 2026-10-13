//! The blue optical compressor in a host's FX column (see [`crate::lane`]).
//!
//! - **Lane**: the name block, the five big seven-sided knobs in the
//!   order a mix reaches for them — THRESHOLD, RATIO, ATTACK, RELEASE,
//!   GAIN — and the tan VU (reading gain reduction) at the right.
//! - **Tall**: the name, IN, the VU and the meter selector, the pilot
//!   jewel and the power knob across the top; the five knobs and the
//!   attack/release SELECT across the bottom.

use frame::document::proto::NodeId;
use frame::surface::kit::{Place, TOUCH_MIN, draw};
use frame::surface::proto::{MeterTap, Range, SourceSelect, StreamInfo, StreamKind, Unit};
use frame::surface::widgets::Builder;

use super::super::ui::{self, Ink};
use super::{CHROME, GAIN, INK, SELECT, SWEEP, THRESHOLD};
use crate::face::{Face, canvas};
use crate::lane::Shape;

pub fn faces() -> Vec<Face> {
    Shape::ALL
        .into_iter()
        .map(|shape| {
            let scene = canvas(
                &format!("Opto Compressor ({})", shape.suffix()),
                |b, root| build(b, root, shape),
            );
            let mut params = vec![
                ui::param(
                    "optocomp/threshold",
                    "Threshold",
                    ui::printed_range(THRESHOLD, SWEEP, Unit::Db),
                    -20.0,
                )
                .short("Thresh"),
                ui::param(
                    "optocomp/ratio",
                    "Ratio",
                    Range::linear(2.0, 10.0, Unit::Ratio),
                    4.0,
                ),
                ui::param(
                    "optocomp/attack",
                    "Attack",
                    Range::log(0.5, 300.0, Unit::Milliseconds),
                    10.0,
                ),
                ui::param(
                    "optocomp/release",
                    "Release",
                    Range::log(0.05, 10.0, Unit::Seconds),
                    0.5,
                ),
                ui::param(
                    "optocomp/gain",
                    "Gain",
                    ui::printed_range(GAIN, SWEEP, Unit::Db),
                    0.0,
                ),
            ];
            let mut streams = ui::streams("optocomp", "Opto Compressor");
            if shape == Shape::Tall {
                params.extend([
                    ui::choice(
                        "optocomp/timing",
                        "Attack / Release Select",
                        &["Fixed", "Fix-Man", "Manual"],
                        2,
                    )
                    .short("Timing"),
                    ui::choice(
                        "optocomp/meter",
                        "Meter",
                        &["Input", "Compression", "Output"],
                        1,
                    ),
                    ui::toggle("optocomp/active", "In", true),
                    ui::toggle("optocomp/power", "Power", true),
                ]);
                streams.push(StreamInfo {
                    id: frame::surface::proto::EntryId::new("optocomp/in"),
                    name: "Opto Compressor Input".into(),
                    kind: StreamKind::Level { channels: 2 },
                    rate_hz: 30.0,
                });
            }
            Face {
                file: shape.file("05-opto-comp"),
                plugin: "comp",
                scene,
                params,
                streams,
            }
        })
        .collect()
}

/// The marks each knob prints: GAIN's and THRESHOLD's as the pots' taper
/// falls, the others at their ends.
fn printed(m: &[(f64, f64)], words: [&'static str; 6]) -> Vec<(f64, &'static str)> {
    m.iter().zip(words).map(|((a, _), w)| (*a, w)).collect()
}

/// A knob: its name, its address, the marks it prints.
type KnobSpec = (&'static str, &'static str, Vec<(f64, &'static str)>);

/// A shape's sizes: knob size, the knob line, the caption line.
struct Row {
    d: f64,
    y: f64,
    caption: f64,
}

fn build(b: &mut Builder<'_>, root: NodeId, shape: Shape) {
    let (w, h) = shape.size();
    let tall = shape == Shape::Tall;
    let panel = ui::put(
        b,
        root,
        "panel.painted",
        &Place::new("Opto compressor panel")
            .variant("blue")
            .options(&[&format!("w={w}"), &format!("h={h}"), "ends=none"]),
    );
    ui::repaint(
        b,
        panel,
        &[(0.0, "#0f6aa6"), (0.5, "#0a5d95"), (1.0, "#084f80")],
    );
    if let Some(n) = b.scene.node_mut(panel) {
        n.corner_radii = frame::document::proto::CornerRadii::uniform(8.0);
    }
    for (x, y) in [
        (12.0, 12.0),
        (w - 12.0, 12.0),
        (12.0, h - 12.0),
        (w - 12.0, h - 12.0),
    ] {
        ui::phillips(b, panel, (x, y), 9.0, CHROME);
    }

    // ── The name block: at the left alone; over the meter on the lane ──
    if tall {
        name_block(b, panel, (30.0, 44.0), 1.1);
    } else {
        ui::silk(
            b,
            panel,
            "Opto-Comp",
            668.0,
            24.0,
            Ink::new(18.0, INK).weight(600.0).track(-0.02),
        );
    }

    // ── The knobs ────────────────────────────────────────────────────
    let row = if tall {
        Row {
            d: 76.0,
            y: 246.0,
            caption: 312.0,
        }
    } else {
        Row {
            d: 56.0,
            y: 74.0,
            caption: 146.0,
        }
    };
    let xs: [f64; 5] = if tall {
        [96.0, 238.0, 380.0, 522.0, 664.0]
    } else {
        [72.0, 184.0, 296.0, 408.0, 520.0]
    };
    let knobs: [KnobSpec; 5] = [
        (
            "Threshold",
            "optocomp/threshold",
            printed(THRESHOLD, ["off", "0", "−10", "−20", "−30", "−40"]),
        ),
        (
            "Ratio",
            "optocomp/ratio",
            vec![(-120.0, "2:1"), (120.0, "10:1")],
        ),
        (
            "Attack",
            "optocomp/attack",
            vec![(-120.0, "fast"), (120.0, "slow")],
        ),
        (
            "Release",
            "optocomp/release",
            vec![(-120.0, "fast"), (120.0, "slow")],
        ),
        (
            "Gain",
            "optocomp/gain",
            printed(GAIN, ["off", "−10", "0", "+10", "+20", "+30"]),
        ),
    ];
    for ((name, addr, marks), x) in knobs.iter().zip(xs) {
        knob(b, panel, (name, addr), marks, (x, &row), tall);
    }

    // ── The meter ────────────────────────────────────────────────────
    let (mx, my, cw) = if tall {
        (418.0, 92.0, 196.0)
    } else {
        (668.0, 88.0, 124.0)
    };
    meter(b, panel, (mx, my, cw), tall);

    if tall {
        top_row(b, panel);
    } else {
        ui::silk(
            b,
            panel,
            "Meter",
            mx,
            146.0,
            Ink::new(10.0, INK).weight(700.0).track(0.06),
        );
    }
    ui::fit_root(b, root, panel);
}

/// A big glossy seven-sided knob read against its white dots, its
/// caption under it.
fn knob(
    b: &mut Builder<'_>,
    panel: NodeId,
    (name, addr): (&str, &str),
    marks: &[(f64, &str)],
    (x, row): (f64, &Row),
    tall: bool,
) {
    let c = (x, row.y);
    let k = row.d / 70.0;
    let p = Place::new(name)
        .size(row.d)
        .legend(" ")
        .sweep(SWEEP)
        .scale(&[])
        .touch(TOUCH_MIN)
        .bind(ui::bind(addr));
    let knob = ui::rotary_at(b, panel, "knob.heptagon", &p, c.0, c.1);
    ui::hide_ring(b, knob);
    let labels = Ink::new(if tall { 10.5 } else { 8.5 }, INK)
        .weight(500.0)
        .track(0.02)
        .plain();
    ui::marks_at(
        b,
        panel,
        c,
        42.0 * k,
        46.0 * k,
        marks,
        4.4 * k.max(1.0),
        labels,
    );
    ui::silk(
        b,
        panel,
        name,
        c.0,
        row.caption,
        Ink::new(if tall { 11.0 } else { 10.0 }, INK)
            .weight(700.0)
            .track(0.06),
    );
}

/// A black bar selector stepped through `options`, printed round it.
fn selector(
    b: &mut Builder<'_>,
    panel: NodeId,
    (name, addr): (&str, &str),
    options: &[&str],
    c: (f64, f64),
) {
    let p = Place::new(name)
        .size(58.0)
        .legend(" ")
        .options(&["1", "2", "3"])
        .sweep(SELECT)
        .touch(TOUCH_MIN)
        .bind(ui::bind(addr));
    let k = ui::rotary_at(b, panel, "knob.bar_pointer", &p, c.0, c.1);
    ui::hide_ring(b, k);
    ui::dial_numerals(
        b,
        panel,
        c,
        44.0,
        SELECT,
        options,
        Ink::new(10.0, INK).weight(500.0).track(0.02).plain(),
    );
}

/// The tan VU in its thin black frame: gain reduction on the lane; on the
/// tall face, what the METER selector picks.
fn meter(b: &mut Builder<'_>, panel: NodeId, (mx, my, cw): (f64, f64, f64), switched: bool) {
    let pad = cw * 0.08;
    let (fw, fh) = (cw + pad, cw * 0.58 + pad);
    let frame = draw::rounded(
        draw::rect(
            "meter frame",
            mx - fw / 2.0,
            my - fh / 2.0,
            fw,
            fh,
            draw::linear(180.0, &[(0.0, "#1f2222"), (1.0, "#101212")]),
        ),
        2.0,
    );
    b.add(
        panel,
        draw::with_effects(
            draw::stroked(frame, "#000000aa", 1.0),
            vec![
                draw::drop_shadow("#00000066", 0.0, 1.5, 3.0),
                draw::inner_shadow("#ffffff1f", 0.0, 1.0, 0.0),
            ],
        ),
    );
    let mut vu = Place::new("Gain Reduction meter")
        .size(cw)
        .variant("open")
        .legend("VU")
        .options(&["glass"])
        .bind(ui::stream("optocomp/gr"));
    if switched {
        vu = vu.select(SourceSelect::new(
            "optocomp/meter",
            vec![
                Some(MeterTap::new("optocomp/in")),
                Some(MeterTap::new("optocomp/gr")),
                Some(MeterTap::new("optocomp/out")),
            ],
        ));
    }
    ui::centred_at(b, panel, "meter.vu_amber", &vu, mx, my);
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
                draw::solid("#f3e2b833"),
            ),
            3.0,
        ),
    );
}

/// The name block, `(x, y)` its first line's left end, at scale `k`.
fn name_block(b: &mut Builder<'_>, panel: NodeId, (nx, ny): (f64, f64), k: f64) {
    ui::silk_left(
        b,
        panel,
        "Opto-Comp",
        nx,
        ny,
        Ink::new(22.0 * k, INK).weight(600.0).track(-0.02),
    );
    let line = Ink::new(9.0 * k, INK).weight(500.0).track(0.0);
    let right = nx + 112.0 * k;
    ui::silk_left(b, panel, "Compressor", nx, ny + 26.0 * k, line);
    ui::silk_right(b, panel, "Valve", right, ny + 26.0 * k, line);
    ui::silk_left(b, panel, "Optical", nx, ny + 44.0 * k, line);
    ui::silk_right(b, panel, "Stereo", right, ny + 44.0 * k, line);
}

/// The tall face's top row beside the meter: IN under the name, the
/// attack/release SELECT, the METER selector, the pilot jewel over the
/// OFF / ON power knob.
fn top_row(b: &mut Builder<'_>, panel: NodeId) {
    let words = Ink::new(10.0, INK).weight(500.0).track(0.02).plain();
    let caption = Ink::new(10.5, INK).weight(700.0).track(0.06);

    let active = ui::put(
        b,
        panel,
        "switch.toggle_bare",
        &Place::new("In")
            .size(46.0)
            .legend(" ")
            .variant("black")
            .options(&["in", ""])
            .keep_case()
            .bind(ui::bind("optocomp/active")),
    );
    super::super::opto_leveler::place_toggle(b, active, 70.0, 146.0);

    let c = (238.0, 104.0);
    selector(
        b,
        panel,
        ("Attack / Release Select", "optocomp/timing"),
        &["fixed", "fix./man.", "manual"],
        c,
    );
    ui::silk(b, panel, "attack/release", c.0, 140.0, words);
    ui::silk(b, panel, "Select", c.0, 156.0, caption);

    let c = (592.0, 104.0);
    selector(
        b,
        panel,
        ("Meter", "optocomp/meter"),
        &["input", "", "output"],
        c,
    );
    ui::silk(b, panel, "compres-", c.0, 38.0, words);
    ui::silk(b, panel, "sion", c.0, 52.0, words);
    ui::silk(b, panel, "Meter", c.0, 156.0, caption);

    ui::centred_at(
        b,
        panel,
        "button.jewel",
        &Place::new("Pilot")
            .size(36.0)
            .variant("red")
            .bind(ui::bind("optocomp/power")),
        700.0,
        52.0,
    );
    let c = (700.0, 124.0);
    let p = Place::new("Power")
        .size(44.0)
        .legend(" ")
        .options(&["Off", "On"])
        .sweep(40.0)
        .touch(TOUCH_MIN)
        .bind(ui::bind("optocomp/power"));
    let power = ui::rotary_at(b, panel, "knob.bar_pointer", &p, c.0, c.1);
    ui::hide_ring(b, power);
    ui::silk(b, panel, "Off", c.0 - 4.0, 88.0, caption);
    ui::silk(b, panel, "On", c.0 + 30.0, 98.0, caption);
}
