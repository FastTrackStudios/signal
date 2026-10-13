//! The flanger: a comb through the spectrum — notch after notch, evenly
//! spaced, the comb a short delay cuts — squeezing together and drawing
//! apart as the LFO sweeps the delay (as fast as the rate, as far as the
//! depth): the jet. Feedback sharpens the teeth and rings between them.
//! Under it, the delay itself, swinging along its few milliseconds.
//!
//! Brushed steel round a black window, lit in the Modulation group's blue.

use document_proto::NodeId;
use document_proto::motion::{Motion, Target};
use surface_kit::{draw, motion};
use surface_widgets::Builder;

use super::{MOD, Spec, Theme};
use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit;

/// Brushed steel, a black window.
const FLANGER: Theme = Theme {
    panel: [(0.0, "#eef0f3"), (1.0, "#cdd2d9")],
    edge: "#a8afb9",
    window: [(0.0, "#141922"), (1.0, "#090c11")],
    ink: "#1b222c",
    dim: "#5f6875",
    accent: "#0284c7",
    knob: "#f6f7f9",
    skirt: "#8a929e",
    bypass: "blue",
};

const SPEC: Spec = Spec {
    name: "Flanger",
    t: &FLANGER,
    ns: MOD,
    picture,
    base: "104-mod-flanger",
};

/// The comb's light: brighter than the steel's own blue, for the black
/// window.
const LIGHT: &str = "#7dd3fc";

const FRONT: super::Rows = (
    &[
        ("rate", "Rate"),
        ("depth", "Depth"),
        ("feedback", "Regen"),
        ("mix", "Mix"),
    ],
    &[("depth", "Depth"), ("feedback", "Regen"), ("mix", "Mix")],
);
const BACK: super::Rows = (
    &[("color", "Manual"), ("width", "Width")],
    &[("color", "Manual"), ("width", "Width")],
);

/// The flanger's face at `tier`.
pub(super) fn face_at(tier: kit::Tier) -> Face {
    super::front(&SPEC, FRONT, true, tier, super::chorus_params())
}

/// Its back at `tier`: the manual delay and the stereo width.
pub(super) fn back_at(tier: kit::Tier) -> Face {
    super::back(&SPEC, BACK, tier, super::chorus_params())
}

/// The sweep, 0..1: the LFO, as far as the depth.
fn lfo() -> String {
    format!(
        "norm({MOD}/depth) * (0.5 + 0.5 * sin(6.2832 * time * param({MOD}/rate))) * param({MOD}/on) + 0.5 * (1 - norm({MOD}/depth))"
    )
}

/// The flanger's picture in its window `v` (`vw × vh`).
fn picture(b: &mut Builder<'_>, v: NodeId, vw: f64, vh: f64, t: &Theme) {
    let (top, base) = (24.0, vh - 30.0);
    let rows: Vec<Vec<(f64, f64)>> = (0..5)
        .map(|i| {
            let y = top + (base - top) * f64::from(i) / 4.0;
            vec![(0.0, y), (vw, y)]
        })
        .collect();
    b.add(v, ui::strokes("grid", &rows, 0.6, draw::solid("#ffffff0c")));

    // The comb, drawn long and squeezed from the left: the longer the
    // delay, the closer the teeth.
    let tooth = vw / 5.0;
    let len = vw * 1.9;
    let comb = b.add(v, draw::frame("Comb", 0.0, 0.0, len, vh));
    let gentle = (0.09, 0.0);
    let sharp = (0.04, 0.34);
    let shape =
        |closed: bool, s: (f64, f64)| super::phaser::response(len, tooth, (top, base), s, closed);
    let fill = draw::path(
        "comb",
        0.0,
        0.0,
        len,
        vh,
        shape(true, gentle),
        draw::linear(
            180.0,
            &[
                (0.0, &ui::alpha(LIGHT, 0.4)),
                (1.0, &ui::alpha(LIGHT, 0.03)),
            ],
        ),
    );
    let fill = b.add(comb, fill);
    motion::add(
        b,
        fill,
        Motion::new(Target::Morph {
            shapes: vec![shape(true, gentle), shape(true, sharp)],
        })
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
            "teeth",
            0.0,
            0.0,
            len,
            vh,
            shape(false, gentle),
            draw::solid("#00000000"),
        ),
        "#e6f7ff",
        1.5,
    );
    let curve = b.add(comb, ui::glowing(curve, LIGHT, 0.9, 6.0));
    motion::add(
        b,
        curve,
        Motion::new(Target::Morph {
            shapes: vec![shape(false, gentle), shape(false, sharp)],
        })
        .input(format!("norm({MOD}/feedback)"))
        .range(0.0, 1.0),
    );
    motion::add(
        b,
        comb,
        Motion::new(Target::StretchX)
            .input(lfo())
            .range(1.0, 0.5)
            .pivot(0.0, 0.5),
    );

    // The delay, under the comb: its track over the flanger's few
    // milliseconds, the bead where it is now.
    let (tx0, tx1, ty) = (40.0, vw - 30.0, vh - 13.0);
    ui::silk_left(b, v, "Delay", 8.0, ty, 5.5, t.skirt, 800.0);
    b.add(
        v,
        draw::rounded(
            draw::rect(
                "track",
                tx0,
                ty - 1.5,
                tx1 - tx0,
                3.0,
                draw::solid("#ffffff1a"),
            ),
            1.5,
        ),
    );
    let ticks: Vec<Vec<(f64, f64)>> = (0..=10)
        .map(|i| {
            let x = tx0 + (tx1 - tx0) * f64::from(i) / 10.0;
            vec![(x, ty + 4.0), (x, ty + if i % 5 == 0 { 8.0 } else { 6.0 })]
        })
        .collect();
    b.add(v, ui::strokes("ms", &ticks, 0.8, draw::solid("#ffffff33")));
    let bead = b.add(
        v,
        ui::glowing(
            draw::circle("now", tx0, ty, 3.6, draw::solid("#e6f7ff")),
            LIGHT,
            1.0,
            7.0,
        ),
    );
    motion::add(
        b,
        bead,
        Motion::new(Target::X).input(lfo()).range(0.0, tx1 - tx0),
    );
    ui::silk_right(b, v, "ms", vw - 8.0, ty, 5.5, t.skirt, 800.0);
    ui::silk_left(b, v, "Comb", 10.0, 12.0, 5.5, "#e6f7ff", 800.0);
}
