//! The modern stereo variable-mu limiter/compressor — laid out from the
//! photo of the 2U unit (Manley Stereo Variable Mu), with no maker's names
//! on it.
//!
//! A brushed gunmetal faceplate with a white gain-reduction meter in a
//! black case at each end, reading right to left. Between them a black
//! sub-panel carries both channels: RECOVERY, OUTPUT, the shared DUAL
//! INPUT, OUTPUT, RECOVERY along the top; THRESHOLD, ATTACK, the toggle
//! cluster (HP SC IN, SEP/LINK, HP SC IN over POWER), ATTACK, THRESHOLD
//! along the bottom — matte black fin knobs inside rings of white ticks.
//! On the steel beside each meter: LIMIT/COMPRESS and BYPASS toggles, the
//! balance test jacks and the ADJUST and METER trims.

use document_proto::NodeId;
use surface_kit::{Place, draw};
use surface_proto::{Range, StreamInfo, StreamKind, Unit};
use surface_widgets::Builder;

use super::ui::{self, Ink};
use crate::face::{Face, canvas};

pub mod lane;

/// The photographed panel: 1740 × 317 px from (90, 138) in the photo.
const W: f64 = 1100.0;
const H: f64 = 200.0;
const S: f64 = W / 1740.0;
const INK: &str = "#f2f2f0";
const CHROME: &str = "#c8c8c6";
/// The knobs' travel inside their tick rings.
const SWEEP: f64 = 300.0;

/// A photo coordinate on this face.
fn at(x: f64, y: f64) -> (f64, f64) {
    ((x - 90.0) * S, (y - 138.0) * S)
}

pub fn face() -> Face {
    let scene = canvas("Vari-Mu", build);
    let mut params = vec![
        ui::param(
            "varimu/input",
            "Dual Input",
            Range {
                steps: Some(11),
                ..Range::linear(-8.0, 8.0, Unit::Db)
            },
            0.0,
        )
        .short("Input"),
    ];
    for (ch, name) in [("l", "Left"), ("r", "Right")] {
        params.extend([
            ui::param(
                &format!("varimu/threshold_{ch}"),
                &format!("{name} Threshold"),
                Range::linear(-40.0, 0.0, Unit::Db),
                -20.0,
            )
            .short("Thresh"),
            ui::param(
                &format!("varimu/attack_{ch}"),
                &format!("{name} Attack"),
                Range::linear(70.0, 25.0, Unit::Milliseconds),
                45.0,
            ),
            ui::choice(
                &format!("varimu/recovery_{ch}"),
                &format!("{name} Recovery"),
                &["8 s", "2.5 s", "1.2 s", "0.6 s", "0.2 s"],
                2,
            ),
            ui::param(
                &format!("varimu/output_{ch}"),
                &format!("{name} Output"),
                Range::linear(-6.0, 18.0, Unit::Db),
                0.0,
            ),
            ui::toggle(
                &format!("varimu/limit_{ch}"),
                &format!("{name} Limit / Compress"),
                false,
            )
            .short("Limit"),
            ui::toggle(
                &format!("varimu/hp_sc_{ch}"),
                &format!("{name} HP Sidechain"),
                true,
            )
            .short("HP SC"),
            ui::toggle(
                &format!("varimu/bypass_{ch}"),
                &format!("{name} In / Bypass"),
                true,
            )
            .short("In"),
        ]);
    }
    params.push(ui::toggle("varimu/link", "Sep / Link", true).short("Link"));
    params.push(ui::toggle("varimu/power", "Power", true));
    let mut streams = ui::streams("varimu", "Vari-Mu");
    streams.push(StreamInfo {
        id: surface_proto::EntryId::new("varimu/gr_r"),
        name: "Vari-Mu Gain Reduction (Right)".into(),
        kind: StreamKind::GainReduction,
        rate_hz: 30.0,
    });
    Face {
        file: "07-vari-mu",
        plugin: "comp",
        scene,
        params,
        streams,
    }
}

fn build(b: &mut Builder<'_>, root: NodeId) {
    let panel = ui::put(
        b,
        root,
        "panel.brushed",
        &Place::new("Vari-Mu panel").variant("natural").options(&[
            &format!("w={W}"),
            &format!("h={H}"),
            "ends=none",
        ]),
    );
    ui::repaint(
        b,
        panel,
        &[(0.0, "#8b8591"), (0.5, "#7a7480"), (1.0, "#68626d")],
    );

    // ── Rack slots and the screw columns ─────────────────────────────
    for (x, y) in [
        (113.0, 157.0),
        (113.0, 436.0),
        (1807.0, 157.0),
        (1807.0, 436.0),
    ] {
        ui::rack_slot(b, panel, at(x, y), 17.0, 11.0);
    }
    for x in [213.0, 1708.0] {
        for y in [185.0, 240.0, 342.0, 402.0] {
            ui::phillips(b, panel, at(x, y), 9.0, CHROME);
        }
    }

    // ── The name, across the top ─────────────────────────────────────
    ui::silk(
        b,
        panel,
        "Vari-Mu",
        at(706.0, 0.0).0,
        at(0.0, 157.0).1,
        Ink::new(19.0, "#e6e4ea").weight(300.0).track(0.10),
    );
    ui::silk_left(
        b,
        panel,
        "Stereo Variable Mu Limiter Compressor",
        at(818.0, 0.0).0,
        at(0.0, 158.0).1,
        Ink::new(10.6, "#eeedf0").weight(700.0).track(0.0),
    );

    // ── The meters ───────────────────────────────────────────────────
    meter(b, panel, at(384.0, 255.0), "Left", "varimu/gr");
    meter(b, panel, at(1535.0, 255.0), "Right", "varimu/gr_r");

    // ── The black sub-panel ──────────────────────────────────────────
    let (x0, y0) = at(605.0, 180.0);
    let (x1, y1) = at(1320.0, 440.0);
    let sub = draw::rounded(
        draw::rect(
            "control panel",
            x0,
            y0,
            x1 - x0,
            y1 - y0,
            draw::linear(180.0, &[(0.0, "#141416"), (1.0, "#0c0c0e")]),
        ),
        9.0,
    );
    b.add(
        panel,
        draw::with_effects(
            draw::stroked(sub, "#000000", 1.0),
            vec![
                draw::drop_shadow("#00000066", 0.0, 1.0, 2.0),
                draw::inner_shadow("#ffffff14", 0.0, 1.0, 0.0),
            ],
        ),
    );
    for (x, y) in [
        (620.0, 193.0),
        (1305.0, 193.0),
        (620.0, 425.0),
        (1305.0, 425.0),
    ] {
        ui::phillips(b, panel, at(x, y), 7.0, CHROME);
    }

    let cap = Ink::new(8.6, INK).weight(600.0).track(0.02);
    let word = Ink::new(8.0, INK).weight(600.0).track(0.02);
    // A fin knob in a ring of white ticks, its caption above.
    let knob = |b: &mut Builder<'_>,
                name: &str,
                legend: &str,
                addr: &str,
                (x, y): (f64, f64),
                ends: Option<(&str, &str)>| {
        let c = at(x, y);
        let k = ui::rotary_at(
            b,
            panel,
            "knob.fin",
            &Place::new(name)
                .size(42.0)
                .legend(" ")
                .sweep(SWEEP)
                .scale(&[])
                .touch(surface_kit::TOUCH_MIN)
                .bind(ui::bind(addr)),
            c.0,
            c.1,
        );
        ui::hide_ring(b, k);
        ui::ring_ticks(b, panel, c, (25.5, 30.5), SWEEP, 11, 1.0, INK);
        ui::ring_arc(b, panel, c, 30.5, SWEEP, 0.9, INK);
        let top = at(0.0, if y < 300.0 { 190.0 } else { 315.0 }).1;
        ui::silk(b, panel, legend, c.0, top, cap);
        if let Some((lo, hi)) = ends {
            let yb = at(0.0, 421.0).1;
            ui::silk(b, panel, lo, c.0 - 19.5, yb, word);
            ui::silk(b, panel, hi, c.0 + 19.5, yb, word);
        }
    };
    // Recovery: a five-way switch with ticks only at its stops.
    let recovery = |b: &mut Builder<'_>, name: &str, addr: &str, x: f64| {
        let c = at(x, 245.0);
        let k = ui::rotary_at(
            b,
            panel,
            "knob.fin",
            &Place::new(name)
                .size(42.0)
                .legend(" ")
                .options(&["1", "2", "3", "4", "5"])
                .sweep(126.0)
                .touch(surface_kit::TOUCH_MIN)
                .bind(ui::bind(addr)),
            c.0,
            c.1,
        );
        ui::hide_ring(b, k);
        ui::ring_ticks(b, panel, c, (25.5, 30.5), 126.0, 5, 1.0, INK);
        ui::silk(b, panel, "Recovery", c.0, at(0.0, 190.0).1, cap);
        ui::silk_right(b, panel, "Slow", c.0 - 29.0, c.1 - 14.0, word);
        ui::silk_left(b, panel, "Fast", c.0 + 29.0, c.1 - 14.0, word);
    };
    recovery(b, "Left Recovery", "varimu/recovery_l", 685.0);
    knob(
        b,
        "Left Output",
        "Output",
        "varimu/output_l",
        (823.0, 245.0),
        None,
    );
    knob(
        b,
        "Dual Input",
        "Dual Input",
        "varimu/input",
        (962.0, 245.0),
        None,
    );
    knob(
        b,
        "Right Output",
        "Output",
        "varimu/output_r",
        (1100.0, 245.0),
        None,
    );
    recovery(b, "Right Recovery", "varimu/recovery_r", 1238.0);
    knob(
        b,
        "Left Threshold",
        "Threshold",
        "varimu/threshold_l",
        (687.0, 370.0),
        Some(("Min", "Max")),
    );
    knob(
        b,
        "Left Attack",
        "Attack",
        "varimu/attack_l",
        (823.0, 370.0),
        Some(("Slow", "Fast")),
    );
    knob(
        b,
        "Right Attack",
        "Attack",
        "varimu/attack_r",
        (1100.0, 370.0),
        Some(("Slow", "Fast")),
    );
    knob(
        b,
        "Right Threshold",
        "Threshold",
        "varimu/threshold_r",
        (1238.0, 370.0),
        Some(("Min", "Max")),
    );

    // ── The toggle cluster ───────────────────────────────────────────
    let small = Ink::new(7.2, INK).weight(600.0).track(0.02);
    for (x, addr, name) in [
        (887.0, "varimu/hp_sc_l", "Left HP SC"),
        (1027.0, "varimu/hp_sc_r", "Right HP SC"),
    ] {
        let c = at(x, 327.0);
        toggle(b, panel, name, addr, c);
        ui::silk(b, panel, "HP SC", c.0, at(0.0, 283.0).1, small);
        ui::silk(b, panel, "In", c.0, at(0.0, 296.0).1, small);
    }
    let c = at(963.0, 327.0);
    toggle(b, panel, "Sep / Link", "varimu/link", c);
    ui::silk_right(b, panel, "Sep", c.0 - 9.0, c.1, word);
    ui::silk_left(b, panel, "Link", c.0 + 9.0, c.1, word);
    let c = at(963.0, 388.0);
    toggle(b, panel, "Power", "varimu/power", c);
    ui::silk(b, panel, "Power", c.0, at(0.0, 421.0).1, word);

    // ── On the steel beside each meter ───────────────────────────────
    let steel = Ink::new(7.2, "#f4f4f6").weight(700.0).track(0.02);
    for (side, ch, limit_x, meter_x, bypass_x, jacks, adjust_x) in [
        ("Left", "l", 567.0, 570.0, 300.0, [385.0, 425.0], 477.0),
        (
            "Right",
            "r",
            1357.0,
            1355.0,
            1620.0,
            [1487.0, 1525.0],
            1447.0,
        ),
    ] {
        let c = at(limit_x, 345.0);
        toggle(
            b,
            panel,
            &format!("{side} Limit / Compress"),
            &format!("varimu/limit_{ch}"),
            c,
        );
        ui::silk(b, panel, "Limit", c.0, at(0.0, 318.0).1, steel);
        ui::silk(b, panel, "Compress", c.0, at(0.0, 376.0).1, steel);
        let c = at(bypass_x, 402.0);
        toggle(
            b,
            panel,
            &format!("{side} Bypass"),
            &format!("varimu/bypass_{ch}"),
            c,
        );
        ui::silk(b, panel, "In", c.0, at(0.0, 372.0).1, steel);
        ui::silk(b, panel, "Bypass", c.0, at(0.0, 432.0).1, steel);
        trim_hole(b, panel, at(meter_x, 402.0));
        ui::silk(
            b,
            panel,
            "Meter",
            at(meter_x, 0.0).0,
            at(0.0, 432.0).1,
            steel,
        );
        trim_hole(b, panel, at(adjust_x, 402.0));
        ui::silk(
            b,
            panel,
            "Adjust",
            at(adjust_x, 0.0).0,
            at(0.0, 432.0).1,
            steel,
        );
        for x in jacks {
            jack(b, panel, at(x, 404.0));
        }
        ui::silk(
            b,
            panel,
            "Balance",
            at((jacks[0] + jacks[1]) / 2.0, 0.0).0,
            at(0.0, 432.0).1,
            steel,
        );
    }

    ui::fit_root(b, root, panel);
}

/// A small bare bat toggle with its bushing at `c`, no printed legends
/// (the face prints its own round it).
fn toggle(b: &mut Builder<'_>, panel: NodeId, name: &str, addr: &str, (cx, cy): (f64, f64)) {
    let size = 20.0;
    let t = ui::put(
        b,
        panel,
        "switch.toggle_bare",
        &Place::new(name)
            .size(size)
            .legend(" ")
            .options(&["", ""])
            .bind(ui::bind(addr)),
    );
    super::opto_leveler::place_toggle(b, t, cx, cy);
}

/// The meter: the white reduction card in the top of a black case.
fn meter(b: &mut Builder<'_>, panel: NodeId, (cx, cy): (f64, f64), side: &str, gr: &str) {
    let (w, h) = (292.0 * S, 220.0 * S);
    let (x, y) = (cx - w / 2.0, cy - h / 2.0);
    let case = draw::rounded(
        draw::rect(
            "meter case",
            x,
            y,
            w,
            h,
            draw::linear(180.0, &[(0.0, "#1b1b1d"), (1.0, "#0f0f10")]),
        ),
        2.0,
    );
    b.add(
        panel,
        draw::with_effects(
            draw::stroked(case, "#000000", 1.0),
            vec![
                draw::drop_shadow("#00000080", 0.0, 2.0, 4.0),
                draw::inner_shadow("#ffffff14", 0.0, 1.0, 0.0),
            ],
        ),
    );
    let cw = 268.0 * S;
    let top = y + 5.0;
    let bezel = draw::rounded(
        draw::rect(
            "meter bezel",
            cx - cw / 2.0 - 2.0,
            top - 2.0,
            cw + 4.0,
            cw * 0.5 + 4.0,
            draw::linear(180.0, &[(0.0, "#d8d8d6"), (1.0, "#8a8a88")]),
        ),
        1.5,
    );
    b.add(panel, bezel);
    let m = ui::put(
        b,
        panel,
        "meter.reduction",
        &Place::new(format!("{side} Gain Reduction meter"))
            .size(cw)
            .legend("Reduction dB")
            .keep_case()
            .options(&["glass"])
            .bind(ui::stream(gr)),
    );
    ui::move_to(b, m, cx - cw / 2.0, top);
    // The movement's zero-set screw in the case below the window.
    b.add(
        panel,
        draw::with_effects(
            draw::circle("zero screw", cx, y + h * 0.80, 6.0, draw::solid("#141415")),
            vec![draw::inner_shadow("#ffffff1a", 0.0, -0.6, 1.0)],
        ),
    );
}

/// A screwdriver trim behind a hole in the steel.
fn trim_hole(b: &mut Builder<'_>, panel: NodeId, (cx, cy): (f64, f64)) {
    b.add(
        panel,
        draw::with_effects(
            draw::circle("trim hole", cx, cy, 4.8, draw::solid("#0a0a0b")),
            vec![
                draw::inner_shadow("#000000", 0.0, 1.0, 1.5),
                draw::drop_shadow("#ffffff26", 0.0, 0.6, 0.0),
            ],
        ),
    );
}

/// A banana test jack: a black insulator round a dark socket.
fn jack(b: &mut Builder<'_>, panel: NodeId, (cx, cy): (f64, f64)) {
    b.add(
        panel,
        draw::with_effects(
            draw::circle(
                "jack",
                cx,
                cy,
                7.0,
                draw::radial_at(
                    0.38,
                    0.32,
                    0.85,
                    &[(0.0, "#4a4a4c"), (0.5, "#1a1a1c"), (1.0, "#060607")],
                ),
            ),
            vec![draw::drop_shadow("#00000073", 0.0, 1.0, 1.5)],
        ),
    );
    b.add(
        panel,
        draw::circle("jack socket", cx, cy, 2.6, draw::solid("#000000")),
    );
}
