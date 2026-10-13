//! The phaser: its frequency response, as the effect is — a curve across
//! the spectrum with two deep notches cut in it by four all-pass stages,
//! the notches sweeping up and down the band on the LFO (as fast as the
//! rate, as wide as the depth). Feedback sharpens them and lifts resonant
//! peaks between; the stages' lamps chase each other round.
//!
//! A night-blue panel, the curve lit in the Modulation group's blue.

use frame::document::proto::NodeId;
use frame::document::proto::motion::{Motion, Target};
use frame::surface::kit::{draw, motion};
use frame::surface::widgets::Builder;

use super::{MOD, Spec, Theme};
use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit;

/// Night blue.
const PHASER: Theme = Theme {
    panel: [(0.0, "#13203a"), (1.0, "#0a1222")],
    edge: "#22365c",
    window: [(0.0, "#0a1326"), (1.0, "#050a16")],
    ink: "#e6f3ff",
    dim: "#7f9bbd",
    accent: "#38bdf8",
    knob: "#16243f",
    skirt: "#081020",
    bypass: "blue",
};

const SPEC: Spec = Spec {
    name: "Phaser",
    t: &PHASER,
    ns: MOD,
    picture,
    base: "103-mod-phaser",
};

/// The front's knobs: on a wide panel, and on a phone after the Time knob
/// (the rate).
const FRONT: super::Rows = (
    &[
        ("rate", "Rate"),
        ("depth", "Depth"),
        ("feedback", "Feedback"),
        ("mix", "Mix"),
    ],
    &[("depth", "Depth"), ("feedback", "Feedback"), ("mix", "Mix")],
);
/// The back's.
const BACK: super::Rows = (
    &[("color", "Center"), ("width", "Width")],
    &[("color", "Center"), ("width", "Width")],
);

/// The phaser's face at `tier`.
pub(super) fn face_at(tier: kit::Tier) -> Face {
    super::front(&SPEC, FRONT, true, tier, super::chorus_params())
}

/// Its back at `tier`: the sweep's centre and the stereo width.
pub(super) fn back_at(tier: kit::Tier) -> Face {
    super::back(&SPEC, BACK, tier, super::chorus_params())
}

/// The response, `len` long from x = 0 in a window `top`..`base` tall, a
/// notch every `period`: notches `sigma` wide (of a period) and, between
/// them, peaks `peak` high. As a closed fill (`closed`) or the curve alone.
pub(super) fn response(
    len: f64,
    period: f64,
    (top, base): (f64, f64),
    (sigma, peak): (f64, f64),
    closed: bool,
) -> String {
    let n = 240;
    let mut d = if closed {
        format!("M 0 {base:.1} ")
    } else {
        String::new()
    };
    for k in 0..=n {
        let x = len * f64::from(k) / f64::from(n);
        let u = x / period;
        // Where in its period: the notch at the middle, a peak at the ends.
        let (notch, crest) = (u.fract() - 0.5, (u + 0.5).fract() - 0.5);
        let g = (0.62 - 0.56 * (-(notch / sigma).powi(2)).exp()
            + peak * (-(crest / 0.1).powi(2)).exp())
        .clamp(0.03, 1.0);
        let y = base - (base - top) * g;
        let cmd = if k == 0 && !closed { "M" } else { "L" };
        d += &format!("{cmd} {x:.2} {y:.2} ");
    }
    if closed {
        d += &format!("L {len:.2} {base:.1} Z");
    }
    d
}

/// The phaser's picture in its window `v` (`vw × vh`).
fn picture(b: &mut Builder<'_>, v: NodeId, vw: f64, vh: f64, t: &Theme) {
    let (top, base) = (26.0, vh - 16.0);
    // The spectrum's grid: decades and their steps, and the unity line.
    let mut grid = Vec::new();
    let decade = vw / 3.0;
    for dcd in 0..3 {
        for step in 1..10 {
            let x = (f64::from(dcd) + f64::from(step).log10()) * decade;
            grid.push(vec![(x, top - 8.0), (x, base)]);
        }
    }
    b.add(v, ui::strokes("grid", &grid, 0.6, draw::solid("#ffffff0d")));
    let unity = base - (base - top) * 0.62;
    ui::line(
        b,
        v,
        "unity",
        &[(0.0, unity), (vw, unity)],
        0.8,
        &ui::alpha(t.dim, 0.35),
    );

    // The curve, two notches to the window, drawn a period either side so
    // the sweep never shows an end. Feedback morphs it from gentle notches
    // to sharp ones with resonant peaks between.
    let period = vw / 2.0;
    let len = vw + 2.0 * period;
    let sweep = b.add(v, draw::frame("Sweep", -period, 0.0, len, vh));
    let gentle = (0.17, 0.0);
    let sharp = (0.06, 0.32);
    let morph = |closed: bool| Target::Morph {
        shapes: vec![
            response(len, period, (top, base), gentle, closed),
            response(len, period, (top, base), sharp, closed),
        ],
    };
    let fill = draw::path(
        "response",
        0.0,
        0.0,
        len,
        vh,
        response(len, period, (top, base), gentle, true),
        draw::linear(
            180.0,
            &[
                (0.0, &ui::alpha(t.accent, 0.45)),
                (1.0, &ui::alpha(t.accent, 0.04)),
            ],
        ),
    );
    let fill = b.add(sweep, fill);
    motion::add(
        b,
        fill,
        Motion::new(morph(true))
            .input(format!("norm({MOD}/feedback)"))
            .range(0.0, 1.0),
    );
    motion::add(
        b,
        fill,
        Motion::new(Target::Opacity)
            .input(format!("0.3 + 0.7 * param({MOD}/mix)"))
            .range(0.0, 1.0),
    );
    let curve = draw::stroked(
        draw::path(
            "curve",
            0.0,
            0.0,
            len,
            vh,
            response(len, period, (top, base), gentle, false),
            draw::solid("#00000000"),
        ),
        "#e0f4ff",
        1.8,
    );
    let curve = b.add(sweep, ui::glowing(curve, t.accent, 0.9, 6.0));
    motion::add(
        b,
        curve,
        Motion::new(morph(false))
            .input(format!("norm({MOD}/feedback)"))
            .range(0.0, 1.0),
    );
    // The sweep: up and down the band on the LFO, as wide as the depth.
    let lfo = format!(
        "norm({MOD}/depth) * (0.5 + 0.5 * sin(6.2832 * time * param({MOD}/rate))) * param({MOD}/on) + 0.5 * (1 - norm({MOD}/depth))"
    );
    motion::add(
        b,
        sweep,
        Motion::new(Target::X)
            .input(lfo)
            .range(-period * 0.45, period * 0.45),
    );

    // The four stages' lamps, chasing round at the rate.
    let (lx, ly) = (vw - 58.0, 13.0);
    ui::silk_left(b, v, "Stages", lx - 34.0, ly, 5.5, t.dim, 800.0);
    for k in 0..4 {
        let x = lx + 12.0 * f64::from(k);
        b.add(
            v,
            draw::circle("stage well", x, ly, 3.4, draw::solid("#00000080")),
        );
        let lamp = b.add(
            v,
            ui::glowing(
                draw::circle("stage", x, ly, 2.4, draw::solid(t.accent)),
                t.accent,
                0.9,
                5.0,
            ),
        );
        let chase = format!(
            "(0.2 + 0.8 * pow(0.5 + 0.5 * sin(6.2832 * time * param({MOD}/rate) - {:.3}), 4)) * param({MOD}/on)",
            f64::from(k) * std::f64::consts::FRAC_PI_2
        );
        motion::add(
            b,
            lamp,
            Motion::new(Target::Opacity).input(chase).range(0.0, 1.0),
        );
    }
    ui::silk_left(b, v, "Response", 10.0, 12.0, 5.5, t.ink, 800.0);
    ui::silk_left(b, v, "Lo", 8.0, vh - 7.0, 5.5, t.dim, 800.0);
    ui::silk_right(b, v, "Hi", vw - 8.0, vh - 7.0, 5.5, t.dim, 800.0);
}
