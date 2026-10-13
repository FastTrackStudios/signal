//! The modern stereo vari-mu in a host's FX column (see [`crate::lane`]).
//!
//! - **Lane**: one channel of it, as a track's insert uses the unit — the
//!   white reduction meter in its black case on the gunmetal steel, then
//!   the black sub-panel: THRESHOLD, ATTACK, RECOVERY, OUTPUT, the shared
//!   DUAL INPUT and LIMIT / COMPRESS. It binds the left channel's
//!   controls; with SEP / LINK on link (the default) the right follows.
//! - **Tall**: the whole unit — both meters across the top with the name
//!   and each side's LIMIT and BYPASS between them, and the sub-panel's two
//!   rows of fin knobs and its toggle cluster across the bottom.

use document_proto::NodeId;
use surface_kit::{Place, TOUCH_MIN, draw};
use surface_proto::{Range, StreamInfo, StreamKind, Unit};
use surface_widgets::Builder;

use super::super::ui::{self, Ink};
use super::{CHROME, INK, SWEEP};
use crate::face::{Face, canvas};
use crate::lane::Shape;

pub fn faces() -> Vec<Face> {
    Shape::ALL
        .into_iter()
        .map(|shape| {
            let scene = canvas(&format!("Vari-Mu ({})", shape.suffix()), |b, root| {
                build(b, root, shape)
            });
            let tall = shape == Shape::Tall;
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
                if ch == "r" && !tall {
                    continue;
                }
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
                ]);
                if tall {
                    params.push(
                        ui::toggle(
                            &format!("varimu/hp_sc_{ch}"),
                            &format!("{name} HP Sidechain"),
                            true,
                        )
                        .short("HP SC"),
                    );
                    params.push(
                        ui::toggle(
                            &format!("varimu/bypass_{ch}"),
                            &format!("{name} In / Bypass"),
                            true,
                        )
                        .short("In"),
                    );
                }
            }
            let mut streams = ui::streams("varimu", "Vari-Mu");
            if tall {
                params.push(ui::toggle("varimu/link", "Sep / Link", true).short("Link"));
                params.push(ui::toggle("varimu/power", "Power", true));
                streams.push(StreamInfo {
                    id: surface_proto::EntryId::new("varimu/gr_r"),
                    name: "Vari-Mu Gain Reduction (Right)".into(),
                    kind: StreamKind::GainReduction,
                    rate_hz: 30.0,
                });
            }
            Face {
                file: shape.file("07-vari-mu"),
                plugin: "comp",
                scene,
                params,
                streams,
            }
        })
        .collect()
}

/// How big a shape draws its knobs and print.
#[derive(Clone, Copy)]
struct Scale {
    /// The fin knob's size, and its tick ring (inner, outer radius).
    knob: f64,
    ring: (f64, f64),
    /// Caption and end-word sizes.
    cap: f64,
    word: f64,
}

const LANE: Scale = Scale {
    knob: 50.0,
    ring: (30.0, 36.0),
    cap: 10.0,
    word: 9.0,
};
const TALL: Scale = Scale {
    knob: 44.0,
    ring: (27.0, 32.0),
    cap: 9.5,
    word: 8.5,
};

fn build(b: &mut Builder<'_>, root: NodeId, shape: Shape) {
    let (w, h) = shape.size();
    let panel = ui::put(
        b,
        root,
        "panel.brushed",
        &Place::new("Vari-Mu panel").variant("natural").options(&[
            &format!("w={w}"),
            &format!("h={h}"),
            "ends=none",
        ]),
    );
    ui::repaint(
        b,
        panel,
        &[(0.0, "#8b8591"), (0.5, "#7a7480"), (1.0, "#68626d")],
    );
    if let Some(n) = b.scene.node_mut(panel) {
        n.corner_radii = document_proto::CornerRadii::uniform(8.0);
    }
    match shape {
        Shape::Lane => lane(b, panel, (w, h)),
        Shape::Tall => tall(b, panel, (w, h)),
    }
    ui::fit_root(b, root, panel);
}

fn lane(b: &mut Builder<'_>, panel: NodeId, (w, h): (f64, f64)) {
    let s = LANE;
    for (x, y) in [
        (12.0, 12.0),
        (12.0, h - 12.0),
        (w - 12.0, 12.0),
        (w - 12.0, h - 12.0),
    ] {
        ui::phillips(b, panel, (x, y), 8.0, CHROME);
    }
    meter(b, panel, (98.0, 64.0, 150.0), "Left", "varimu/gr");
    ui::silk(
        b,
        panel,
        "Vari-Mu",
        98.0,
        140.0,
        Ink::new(17.0, "#e6e4ea").weight(300.0).track(0.10),
    );

    sub_panel(b, panel, (186.0, 10.0, 554.0, h - 20.0));
    let y = 86.0;
    knob(
        b,
        panel,
        ("Left Threshold", "Threshold", "varimu/threshold_l"),
        (236.0, y),
        (Some(("Min", "Max")), s),
    );
    knob(
        b,
        panel,
        ("Left Attack", "Attack", "varimu/attack_l"),
        (336.0, y),
        (Some(("Slow", "Fast")), s),
    );
    recovery(
        b,
        panel,
        ("Left Recovery", "varimu/recovery_l"),
        (440.0, y),
        s,
    );
    knob(
        b,
        panel,
        ("Left Output", "Output", "varimu/output_l"),
        (544.0, y),
        (None, s),
    );
    knob(
        b,
        panel,
        ("Dual Input", "Dual Input", "varimu/input"),
        (640.0, y),
        (None, s),
    );
    // LIMIT / COMPRESS at the sub-panel's end.
    let c = (708.0, y);
    toggle(
        b,
        panel,
        ("Left Limit / Compress", "varimu/limit_l"),
        c,
        30.0,
    );
    let word = Ink::new(s.word, INK).weight(600.0).track(0.02);
    ui::silk(b, panel, "Limit", c.0, c.1 - 34.0, word);
    ui::silk(b, panel, "Compress", c.0, c.1 + 34.0, word);
}

fn tall(b: &mut Builder<'_>, panel: NodeId, (w, h): (f64, f64)) {
    let s = TALL;
    for (x, y) in [
        (12.0, 12.0),
        (12.0, h - 12.0),
        (w - 12.0, 12.0),
        (w - 12.0, h - 12.0),
    ] {
        ui::phillips(b, panel, (x, y), 8.0, CHROME);
    }
    // ── Both meters, the name and the steel's toggles between them ───
    meter(b, panel, (114.0, 78.0, 182.0), "Left", "varimu/gr");
    meter(b, panel, (w - 114.0, 78.0, 182.0), "Right", "varimu/gr_r");
    ui::silk(
        b,
        panel,
        "Vari-Mu",
        w / 2.0,
        30.0,
        Ink::new(24.0, "#e6e4ea").weight(300.0).track(0.10),
    );
    let line = Ink::new(8.5, "#eeedf0").weight(700.0).track(0.0);
    ui::silk(b, panel, "Stereo Variable Mu", w / 2.0, 52.0, line);
    ui::silk(b, panel, "Limiter Compressor", w / 2.0, 64.0, line);
    let steel = Ink::new(8.0, "#f4f4f6").weight(700.0).track(0.02);
    for (ch, side, limit_x, bypass_x) in [
        ("l", "Left", 290.0, 238.0),
        ("r", "Right", w - 290.0, w - 238.0),
    ] {
        let c = (limit_x, 108.0);
        toggle(
            b,
            panel,
            (
                &format!("{side} Limit / Compress"),
                &format!("varimu/limit_{ch}"),
            ),
            c,
            24.0,
        );
        ui::silk(b, panel, "Limit", c.0, 84.0, steel);
        ui::silk(b, panel, "Compress", c.0, 132.0, steel);
        let c = (bypass_x, 108.0);
        toggle(
            b,
            panel,
            (&format!("{side} Bypass"), &format!("varimu/bypass_{ch}")),
            c,
            24.0,
        );
        ui::silk(b, panel, "In", c.0, 84.0, steel);
        ui::silk(b, panel, "Bypass", c.0, 132.0, steel);
    }

    // ── The sub-panel: both channels' controls ───────────────────────
    sub_panel(b, panel, (20.0, 150.0, w - 40.0, h - 162.0));
    let xs = [92.0, 236.0, 380.0, 524.0, 668.0];
    let (r1, r2) = (200.0, 276.0);
    recovery(
        b,
        panel,
        ("Left Recovery", "varimu/recovery_l"),
        (xs[0], r1),
        s,
    );
    knob(
        b,
        panel,
        ("Left Output", "Output", "varimu/output_l"),
        (xs[1], r1),
        (None, s),
    );
    knob(
        b,
        panel,
        ("Dual Input", "Dual Input", "varimu/input"),
        (xs[2], r1),
        (None, s),
    );
    knob(
        b,
        panel,
        ("Right Output", "Output", "varimu/output_r"),
        (xs[3], r1),
        (None, s),
    );
    recovery(
        b,
        panel,
        ("Right Recovery", "varimu/recovery_r"),
        (xs[4], r1),
        s,
    );
    knob(
        b,
        panel,
        ("Left Threshold", "Threshold", "varimu/threshold_l"),
        (xs[0], r2),
        (Some(("Min", "Max")), s),
    );
    knob(
        b,
        panel,
        ("Left Attack", "Attack", "varimu/attack_l"),
        (xs[1], r2),
        (Some(("Slow", "Fast")), s),
    );
    knob(
        b,
        panel,
        ("Right Attack", "Attack", "varimu/attack_r"),
        (xs[3], r2),
        (Some(("Slow", "Fast")), s),
    );
    knob(
        b,
        panel,
        ("Right Threshold", "Threshold", "varimu/threshold_r"),
        (xs[4], r2),
        (Some(("Min", "Max")), s),
    );

    // The toggle cluster under DUAL INPUT.
    let small = Ink::new(7.6, INK).weight(600.0).track(0.02);
    let word = Ink::new(s.word, INK).weight(600.0).track(0.02);
    for (x, addr, name) in [
        (xs[2] - 52.0, "varimu/hp_sc_l", "Left HP SC"),
        (xs[2] + 52.0, "varimu/hp_sc_r", "Right HP SC"),
    ] {
        let c = (x, 262.0);
        toggle(b, panel, (name, addr), c, 22.0);
        ui::silk(b, panel, "HP SC", c.0, 232.0, small);
        ui::silk(b, panel, "In", c.0, 242.0, small);
    }
    let c = (xs[2], 256.0);
    toggle(b, panel, ("Sep / Link", "varimu/link"), c, 22.0);
    ui::silk_right(b, panel, "Sep", c.0 - 10.0, c.1, word);
    ui::silk_left(b, panel, "Link", c.0 + 10.0, c.1, word);
    let c = (xs[2], 290.0);
    toggle(b, panel, ("Power", "varimu/power"), c, 22.0);
    ui::silk(b, panel, "Power", c.0, 310.0, word);
}

/// The black sub-panel, its corner screws.
fn sub_panel(b: &mut Builder<'_>, panel: NodeId, (x, y, w, h): (f64, f64, f64, f64)) {
    let sub = draw::rounded(
        draw::rect(
            "control panel",
            x,
            y,
            w,
            h,
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
    for (sx, sy) in [
        (x + 9.0, y + 9.0),
        (x + w - 9.0, y + 9.0),
        (x + 9.0, y + h - 9.0),
        (x + w - 9.0, y + h - 9.0),
    ] {
        ui::phillips(b, panel, (sx, sy), 6.0, CHROME);
    }
}

/// A fin knob in a ring of white ticks, its caption above and, where the
/// unit prints them, its end words below.
fn knob(
    b: &mut Builder<'_>,
    panel: NodeId,
    (name, legend, addr): (&str, &str, &str),
    c: (f64, f64),
    (ends, s): (Option<(&str, &str)>, Scale),
) {
    let p = Place::new(name)
        .size(s.knob)
        .legend(" ")
        .sweep(SWEEP)
        .scale(&[])
        .touch(TOUCH_MIN)
        .bind(ui::bind(addr));
    let k = ui::rotary_at(b, panel, "knob.fin", &p, c.0, c.1);
    ui::hide_ring(b, k);
    ui::ring_ticks(b, panel, c, s.ring, SWEEP, 11, 1.0, INK);
    ui::ring_arc(b, panel, c, s.ring.1, SWEEP, 0.9, INK);
    ui::silk(
        b,
        panel,
        legend,
        c.0,
        c.1 - s.ring.1 - 10.0,
        Ink::new(s.cap, INK).weight(600.0).track(0.02),
    );
    if let Some((lo, hi)) = ends {
        let word = Ink::new(s.word, INK).weight(600.0).track(0.02);
        let y = c.1 + s.ring.1 + 4.0;
        ui::silk(b, panel, lo, c.0 - s.ring.1 * 0.7, y, word);
        ui::silk(b, panel, hi, c.0 + s.ring.1 * 0.7, y, word);
    }
}

/// RECOVERY: a five-way fin switch with ticks only at its stops.
fn recovery(
    b: &mut Builder<'_>,
    panel: NodeId,
    (name, addr): (&str, &str),
    c: (f64, f64),
    s: Scale,
) {
    let p = Place::new(name)
        .size(s.knob)
        .legend(" ")
        .options(&["1", "2", "3", "4", "5"])
        .sweep(126.0)
        .touch(TOUCH_MIN)
        .bind(ui::bind(addr));
    let k = ui::rotary_at(b, panel, "knob.fin", &p, c.0, c.1);
    ui::hide_ring(b, k);
    ui::ring_ticks(b, panel, c, s.ring, 126.0, 5, 1.0, INK);
    ui::silk(
        b,
        panel,
        "Recovery",
        c.0,
        c.1 - s.ring.1 - 10.0,
        Ink::new(s.cap, INK).weight(600.0).track(0.02),
    );
    let word = Ink::new(s.word, INK).weight(600.0).track(0.02);
    ui::silk_right(
        b,
        panel,
        "Slow",
        c.0 - s.ring.1,
        c.1 - s.ring.1 * 0.45,
        word,
    );
    ui::silk_left(
        b,
        panel,
        "Fast",
        c.0 + s.ring.1,
        c.1 - s.ring.1 * 0.45,
        word,
    );
}

/// A bare bat toggle with its bushing at `c`, no printed legends.
fn toggle(
    b: &mut Builder<'_>,
    panel: NodeId,
    (name, addr): (&str, &str),
    (cx, cy): (f64, f64),
    size: f64,
) {
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
    super::super::opto_leveler::place_toggle(b, t, cx, cy);
}

/// The white reduction card in the top of a black case `w` wide.
fn meter(b: &mut Builder<'_>, panel: NodeId, (cx, cy, w): (f64, f64, f64), side: &str, gr: &str) {
    let h = w * 220.0 / 292.0;
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
    let cw = w * 268.0 / 292.0;
    let top = y + w * 0.03;
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
    b.add(
        panel,
        draw::with_effects(
            draw::circle(
                "zero screw",
                cx,
                y + h * 0.82,
                w * 0.03,
                draw::solid("#141415"),
            ),
            vec![draw::inner_shadow("#ffffff1a", 0.0, -0.6, 1.0)],
        ),
    );
}
