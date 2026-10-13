//! The doubler, a unit of its own: seen from above, the listener's field —
//! the note in the middle, and its two doubled takes out to the sides,
//! spread by WIDTH, each a little off pitch (DETUNE, the two drifting
//! against each other) and a little late (DELAY, the rings they send out
//! trailing the note's). Bound to `doubler/*` for the engine to take up.

use frame::document::proto::motion::{Dynamics, Motion, Target};
use frame::document::proto::{CornerRadii, NodeId, ResizeX};
use frame::surface::kit::draw::{self, Lettering, family};
use frame::surface::kit::motion;
use frame::surface::proto::{Range, Unit};
use frame::surface::widgets::Builder;

use super::PLUGIN;
use super::drives::H;
use crate::face::{Face, canvas};
use crate::reverb::ui;
use crate::rig_time::kit;
use crate::suite_studio_blues::parts::set;

const NS: &str = "doubler";
const INK: &str = "#ece7f6";
const DIM: &str = "#8c86a3";
const TAKE: &str = "#b69cff";
const W: f64 = 300.0;

fn a(id: &str) -> String {
    format!("{NS}/{id}")
}

/// A source in the field at (`cx`, `cy`): a dot and the rings it sends
/// out, one ring a second (a phase offset `late` behind the note's).
fn source(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    (cx, cy): (f64, f64),
    color: &str,
    late: &str,
) -> NodeId {
    let r = 46.0;
    let g = b.add(parent, draw::frame(name, cx - r, cy - r, 2.0 * r, 2.0 * r));
    for k in 0..3 {
        let ring = b.add(g, draw::frame("ring", 0.0, 0.0, 2.0 * r, 2.0 * r));
        let mut c = draw::stroked(
            draw::circle("wave", r, r, r - 2.0, draw::solid("#00000000")),
            color,
            1.4,
        );
        c.fills.clear();
        b.add(ring, c);
        let phase = format!("fract(time * 0.8 - ({late}) + {})", f64::from(k) / 3.0);
        motion::add(
            b,
            ring,
            Motion::new(Target::Scale)
                .input(phase.clone())
                .range(0.1, 1.0)
                .pivot(0.5, 0.5),
        );
        motion::add(
            b,
            ring,
            Motion::new(Target::Opacity)
                .input(format!("(1 - {phase}) * param({NS}/on)"))
                .range(0.0, 0.8),
        );
    }
    b.add(
        g,
        ui::glowing(
            draw::circle("dot", r, r, 6.0, draw::solid(color)),
            color,
            0.9,
            6.0,
        ),
    );
    g
}

/// The field from above in `f` (`fw × fh`): the stage's arc, the
/// listener, the note in the middle and the two takes out to the sides
/// by the width, late by the delay, drifting with the detune.
fn field(b: &mut Builder<'_>, f: NodeId, fw: f64, fh: f64) {
    let (cx, cy) = (fw / 2.0, fh * 0.46);
    // The stage's arc, L and R.
    let arc: Vec<(f64, f64)> = (0..=40)
        .map(|i| {
            let t = std::f64::consts::PI * (1.0 + f64::from(i) / 40.0);
            (cx + t.cos() * fw * 0.42, cy + 4.0 + t.sin() * -fh * 0.34)
        })
        .collect();
    b.add(f, ui::strokes("arc", &[arc], 1.0, draw::solid("#ffffff1a")));
    ui::silk(b, f, "L", 14.0, cy + 6.0, 7.0, DIM, 800.0);
    ui::silk(b, f, "R", fw - 14.0, cy + 6.0, 7.0, DIM, 800.0);
    // The listener, below.
    b.add(
        f,
        draw::circle("listener", cx, fh - 14.0, 5.0, draw::solid("#ffffff30")),
    );
    // The note, in the middle; the two takes, out to the sides by the
    // width, late by the delay, drifting with the detune.
    source(b, f, "note", (cx, cy), "#f4f2fa", "0");
    for (side, drift) in [(-1.0, 0.0), (1.0, 3.14)] {
        let t = source(
            b,
            f,
            "take",
            (cx, cy),
            TAKE,
            &format!("norm({NS}/delay) * 0.25"),
        );
        motion::add(
            b,
            t,
            Motion::new(Target::X)
                .input(format!("norm({NS}/width)"))
                .range(0.0, side * fw * 0.38)
                .dynamics(Dynamics::spring(220.0, 26.0)),
        );
        motion::add(b, t, Motion::new(Target::Y).input(format!("0.5 + 0.5 * sin(time * (0.5 + 3 * norm({NS}/detune)) + {drift}) * norm({NS}/detune)")).range(-5.0, 5.0));
        motion::add(
            b,
            t,
            Motion::new(Target::Opacity)
                .input(format!("0.2 + 0.8 * param({NS}/mix)"))
                .range(0.0, 1.0),
        );
    }
}

#[must_use]
pub fn doubler_unit() -> Face {
    let scene = canvas("Doubler", |b, root| {
        ui::fit_root(b, root, W, H);
        let mut p = draw::frame("Panel", 0.0, 0.0, W, H);
        p.fills = vec![draw::linear(180.0, &[(0.0, "#1b1824"), (1.0, "#110f18")])];
        let p = b.add(
            root,
            draw::with_effects(p, vec![draw::inner_shadow("#ffffff14", 0.0, 1.0, 0.0)]),
        );
        let head = Lettering::family(family::INTER).weight(800.0);
        set(b, p, "DOUBLER", 44.0, 18.0, 12.0, TAKE, &head, 0.3);
        // Its On key by the name: the top-right corner is the host's.
        kit::bypass(b, p, NS, 106.0, 18.0, 18.0, "purple", "On");

        // ── The field, from above ───────────────────────────────────────
        let (fx, fy, fw, fh) = (12.0, 32.0, W - 24.0, 138.0);
        let mut f = draw::frame("Field", fx, fy, fw, fh);
        f.fills = vec![draw::linear(180.0, &[(0.0, "#0c0a12"), (1.0, "#08070d")])];
        f.clips_content = true;
        f.corner_radii = CornerRadii::uniform(6.0);
        f.effects = vec![draw::inner_shadow("#000000c0", 0.0, 2.0, 6.0)];
        f.layout_child.resize_x = Some(ResizeX::Stretch);
        let f = b.add(p, f);
        field(b, f, fw, fh);

        // ── Its knobs ───────────────────────────────────────────────────
        let knob = [
            "color=#1e1a2a",
            "pointer=#b69cff",
            "index=line",
            "index_w=0.1",
            "skirt=plain",
            "skirt_color=#0c0a12",
        ];
        for (i, (id, legend)) in [
            ("detune", "Detune"),
            ("delay", "Delay"),
            ("width", "Width"),
            ("mix", "Mix"),
        ]
        .iter()
        .enumerate()
        {
            kit::knob3d(
                b,
                p,
                &a(id),
                legend,
                32.0,
                (38.0 + 75.0 * i as f64, 208.0),
                &knob,
                INK,
                DIM,
            );
        }
        kit::responsive(b, p, &[f]);
    });
    let params = vec![
        ui::param(
            &a("detune"),
            "Detune",
            Range::linear(0.0, 30.0, Unit::None),
            9.0,
        )
        .featured(),
        ui::param(
            &a("delay"),
            "Delay",
            Range::linear(5.0, 40.0, Unit::Milliseconds),
            18.0,
        )
        .featured(),
        ui::param(
            &a("width"),
            "Width",
            Range::linear(0.0, 1.0, Unit::None),
            0.8,
        ),
        ui::param(&a("mix"), "Mix", Range::linear(0.0, 1.0, Unit::None), 0.6),
        ui::toggle(&a("on"), "Doubler", true),
    ];
    Face {
        file: "35-doubler",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

/// The doubler as a phone lane, in the Time faces' form: the field as its
/// picture, Detune as the big knob, Delay, Width and Mix in the knob row,
/// the nameplate (the preset ‹ ›, DOUBLER) and its On.
#[must_use]
pub fn doubler_phone() -> Face {
    let tier = kit::Tier::Phone;
    let w = tier.lane_w();
    let scene = canvas("Doubler", |b, root| {
        ui::fit_root(b, root, w, kit::H);
        let mut p = draw::frame("Panel", 0.0, 0.0, w, kit::H);
        p.fills = vec![draw::linear(180.0, &[(0.0, "#1b1824"), (1.0, "#110f18")])];
        let p = b.add(
            root,
            draw::with_effects(p, vec![draw::inner_shadow("#ffffff14", 0.0, 1.0, 0.0)]),
        );
        let (fx, fy, fw, fh) = (10.0, 10.0, kit::PHONE_PICTURE_R - 10.0, kit::H - 20.0);
        let mut f = draw::frame("Field", fx, fy, fw, fh);
        f.fills = vec![draw::linear(180.0, &[(0.0, "#0c0a12"), (1.0, "#08070d")])];
        f.clips_content = true;
        f.corner_radii = CornerRadii::uniform(6.0);
        f.effects = vec![draw::inner_shadow("#000000c0", 0.0, 2.0, 6.0)];
        let f = b.add(p, f);
        field(b, f, fw, fh);
        let knob = [
            "color=#1e1a2a",
            "pointer=#b69cff",
            "index=line",
            "index_w=0.1",
            "skirt=plain",
            "skirt_color=#0c0a12",
        ];
        let (tx, ty, td) = kit::PHONE_TIME;
        kit::knob3d(b, p, &a("detune"), "Detune", td, (tx, ty), &knob, INK, DIM);
        let (xs, ky, kd) = kit::PHONE_KNOBS;
        for (i, (id, legend)) in [("delay", "Delay"), ("width", "Width"), ("mix", "Mix")]
            .iter()
            .enumerate()
        {
            kit::knob3d(b, p, &a(id), legend, kd, (xs[i], ky), &knob, INK, DIM);
        }
        let head = Lettering::family(family::INTER).weight(800.0);
        kit::nameplate_top_right(
            b,
            p,
            NS,
            tier,
            (8.0, 30.0),
            130.0,
            200.0,
            12.0,
            (TAKE, DIM),
            |b, t| {
                set(b, t, "DOUBLER", 58.0, 15.0, 14.0, TAKE, &head, 0.25);
            },
        );
        kit::face_bypass(b, p, NS, tier, "purple", "On");
        kit::responsive(b, p, &[f]);
    });
    let mut params = doubler_unit().params;
    params.extend(kit::nameplate_params(NS));
    Face {
        file: "35-doubler-phone",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}
