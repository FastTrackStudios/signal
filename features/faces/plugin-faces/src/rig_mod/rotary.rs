//! The rotary speaker: its cabinet opened from above — the treble horn
//! spinning in its housing at the top, the bass drum and its scoop turning
//! below — at the slow (chorale) or fast (tremolo) speed. Switching speed
//! they spin up and down as the real rotors do: the light horn quickly,
//! the heavy drum late. The horn throws its sound as a beam round the
//! cabinet, as bright as the depth.
//!
//! A walnut cabinet, lit in the Motion group's pink.

use frame::document::proto::NodeId;
use frame::document::proto::motion::{Dynamics, Motion, Target};
use frame::surface::kit::{draw, motion};
use frame::surface::proto::{ParamInfo, Range, Unit};
use frame::surface::widgets::Builder;

use super::{MOTION, PINK, Spec, Theme};
use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit;

/// Walnut.
const ROTARY: Theme = Theme {
    panel: [(0.0, "#3a2318"), (1.0, "#21130c")],
    edge: "#5a3826",
    window: [(0.0, "#1c110b"), (1.0, "#0c0604")],
    ink: "#fbe9dc",
    dim: "#b8917c",
    accent: PINK,
    knob: "#2c1b12",
    skirt: "#140b07",
    bypass: "red",
};

const SPEC: Spec = Spec {
    name: "Rotary",
    t: &ROTARY,
    ns: MOTION,
    picture,
    base: "105-motion-rotary",
};

/// The row: the speed switch, then the knobs (on a phone, after the Time
/// knob — the fast speed).
const FRONT: super::Rows = (
    &[
        ("fast", "Speed"),
        ("rate", "Fast Rate"),
        ("drive", "Drive"),
        ("balance", "Horn/Drum"),
        ("mix", "Mix"),
    ],
    &[("fast", "Speed"), ("drive", "Drive"), ("mix", "Mix")],
);

fn a(id: &str) -> String {
    format!("{MOTION}/{id}")
}

/// The rotary block's params: the fast speed (the slow one is the
/// cabinet's own), the speed switch, the preamp's drive, the horn against
/// the drum, the mix.
fn params() -> Vec<ParamInfo> {
    vec![
        ui::param(&a("rate"), "Rate", Range::log(0.05, 12.0, Unit::Hz), 6.7),
        ui::toggle(&a("fast"), "Fast", false),
        ui::param(
            &a("drive"),
            "Drive",
            Range::linear(0.0, 1.0, Unit::None),
            0.2,
        ),
        ui::param(
            &a("balance"),
            "Balance",
            Range::linear(-1.0, 1.0, Unit::None),
            0.0,
        ),
        ui::param(&a("mix"), "Mix", Range::linear(0.0, 1.0, Unit::None), 1.0),
        ui::param(
            &a("depth"),
            "Depth",
            Range::linear(0.0, 1.0, Unit::None),
            0.5,
        ),
        ui::toggle(&a("on"), "Bypass", true),
    ]
}

/// The rotary's face at `tier` (it has no back: all it has is on it).
pub(super) fn face_at(tier: kit::Tier) -> Face {
    super::front(&SPEC, FRONT, false, tier, params())
}

/// A rotor's speed, turns a second: slow, or the fast rate, reached
/// through the rotor's inertia (`spin_up`, the spring's stiffness: the
/// horn's light, the drum's heavy), `scale` of the horn's.
fn rotor_speed(name: &str, spin_up: f64, scale: f64) -> Motion {
    let target = format!(
        "{scale} * (0.8 + (param({MOTION}/rate) - 0.8) * param({MOTION}/fast)) * param({MOTION}/on)"
    );
    Motion::new(Target::None)
        .named(name)
        .input(target)
        .dynamics(Dynamics::spring(spin_up, 2.0 * spin_up.sqrt()))
}

/// The horn rotor at (`cx`, `cy`), `r` the housing's radius: the flared
/// horn and its dummy opposite on one spindle, the beam it throws.
fn horn(b: &mut Builder<'_>, v: NodeId, (cx, cy, r): (f64, f64, f64), t: &Theme) {
    b.add(
        v,
        draw::stroked(
            draw::circle(
                "horn housing",
                cx,
                cy,
                r,
                draw::radial_at(0.5, 0.5, 0.5, &[(0.0, "#00000000"), (1.0, "#00000066")]),
            ),
            &ui::alpha(t.dim, 0.5),
            1.0,
        ),
    );
    let side = 2.0 * r;
    let rotor = b.add(v, draw::frame("Horn", cx - r, cy - r, side, side));
    // The beam: a wedge of light out of the bell.
    let beam = format!(
        "M {r:.1} {r:.1} L {:.1} {:.1} L {:.1} {:.1} Z",
        r * 1.92,
        r * 0.62,
        r * 1.92,
        r * 1.38
    );
    let beam = b.add(
        rotor,
        draw::path(
            "beam",
            0.0,
            0.0,
            side,
            side,
            beam,
            draw::linear(
                0.0,
                &[(0.0, &ui::alpha(PINK, 0.5)), (1.0, &ui::alpha(PINK, 0.0))],
            ),
        ),
    );
    motion::add(
        b,
        beam,
        Motion::new(Target::Opacity)
            .input(format!(
                "(0.25 + 0.75 * norm({MOTION}/depth)) * param({MOTION}/on)"
            ))
            .range(0.0, 1.0),
    );
    // The horn: a throat flaring to the bell, and the dummy, a stub.
    let flare = format!(
        "M {:.1} {:.1} L {:.1} {:.1} L {:.1} {:.1} L {:.1} {:.1} Z",
        r,
        r - 3.0,
        r * 1.62,
        r * 0.7,
        r * 1.62,
        r * 1.3,
        r,
        r + 3.0
    );
    b.add(
        rotor,
        draw::with_effects(
            draw::path(
                "horn",
                0.0,
                0.0,
                side,
                side,
                flare,
                draw::linear(
                    90.0,
                    &[(0.0, "#c9c2bd"), (0.5, "#f4efe9"), (1.0, "#8f8780")],
                ),
            ),
            vec![draw::drop_shadow("#000000a0", 0.0, 2.0, 3.0)],
        ),
    );
    b.add(
        rotor,
        draw::rounded(
            draw::rect(
                "dummy",
                r * 0.42,
                r - 4.0,
                r * 0.58,
                8.0,
                draw::linear(90.0, &[(0.0, "#6f6964"), (1.0, "#3a3532")]),
            ),
            3.0,
        ),
    );
    b.add(
        rotor,
        draw::circle(
            "spindle",
            r,
            r,
            5.0,
            draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#f1d6c4"), (1.0, "#5a3826")]),
        ),
    );
    motion::add(b, rotor, rotor_speed("horn speed", 30.0, 1.0));
    motion::add(
        b,
        rotor,
        Motion::new(Target::Rotate)
            .input("motion(horn speed)")
            .dynamics(Dynamics::Spin)
            .range(0.0, 360.0)
            .pivot(0.5, 0.5),
    );
}

/// The drum rotor at (`cx`, `cy`), radius `r`: the drum and its curved
/// scoop, slower than the horn and later to change.
fn drum(b: &mut Builder<'_>, v: NodeId, (cx, cy, r): (f64, f64, f64)) {
    b.add(
        v,
        draw::circle("drum well", cx, cy, r + 3.0, draw::solid("#00000066")),
    );
    let side = 2.0 * r;
    let rotor = b.add(v, draw::frame("Drum", cx - r, cy - r, side, side));
    b.add(
        rotor,
        draw::with_effects(
            draw::circle(
                "drum",
                r,
                r,
                r,
                draw::radial_at(0.45, 0.4, 0.75, &[(0.0, "#5b3a28"), (1.0, "#2a1a11")]),
            ),
            vec![draw::inner_shadow("#ffffff1a", 0.0, 1.0, 0.0)],
        ),
    );
    // The scoop: half the drum open, a curved baffle throwing the bass out.
    let scoop = draw::sector(r, r, r * 0.35, r * 0.96, -80.0, 80.0);
    b.add(
        rotor,
        draw::path(
            "scoop",
            0.0,
            0.0,
            side,
            side,
            scoop,
            draw::linear(0.0, &[(0.0, "#140b07"), (1.0, &ui::alpha(PINK, 0.35))]),
        ),
    );
    let baffle: Vec<(f64, f64)> = (0..=24)
        .map(|i| draw::polar(r, r, r * 0.66, -80.0 + 160.0 * f64::from(i) / 24.0))
        .collect();
    ui::line(b, rotor, "baffle", &baffle, 2.0, "#e8c3ad");
    b.add(
        rotor,
        draw::circle("hub", r, r, 4.5, draw::solid("#d9b49e")),
    );
    motion::add(b, rotor, rotor_speed("drum speed", 6.0, 0.85));
    motion::add(
        b,
        rotor,
        Motion::new(Target::Rotate)
            .input("motion(drum speed)")
            .dynamics(Dynamics::Spin)
            .range(0.0, 360.0)
            .pivot(0.5, 0.5),
    );
}

/// The rotary's picture in its window `v` (`vw × vh`): the horn at the
/// left, the drum at the right, named under them.
fn picture(b: &mut Builder<'_>, v: NodeId, vw: f64, vh: f64, t: &Theme) {
    // Louvres behind, the cabinet's slats.
    let slats: Vec<Vec<(f64, f64)>> = (0..14)
        .map(|i| {
            let y = 6.0 + f64::from(i) * (vh - 12.0) / 13.0;
            vec![(0.0, y), (vw, y)]
        })
        .collect();
    b.add(
        v,
        ui::strokes("louvres", &slats, 3.0, draw::solid("#ffffff07")),
    );
    let r = (vh * 0.36).min(vw * 0.2);
    let cy = vh * 0.5 - 4.0;
    horn(b, v, (vw * 0.29, cy, r), t);
    drum(b, v, (vw * 0.72, cy, r * 1.05));
    ui::silk(b, v, "Horn", vw * 0.29, vh - 9.0, 6.0, t.dim, 800.0);
    ui::silk(b, v, "Drum", vw * 0.72, vh - 9.0, 6.0, t.dim, 800.0);
    // The speed the cabinet is at: its two words, the one it is on lit.
    for (word, on, x) in [
        ("Chorale", format!("1 - param({MOTION}/fast)"), 10.0),
        ("Tremolo", format!("param({MOTION}/fast)"), 50.0),
    ] {
        let s = ui::silk_left(b, v, word, x, 12.0, 5.5, PINK, 800.0);
        motion::add(
            b,
            s,
            Motion::new(Target::Opacity)
                .input(format!("0.3 + 0.7 * {on}"))
                .range(0.0, 1.0),
        );
    }
}
