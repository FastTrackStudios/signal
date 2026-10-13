//! The rig's faces composed for one block of Signal's routing grid: each
//! effect's picture — the tape echo's VU and head lamps, the hall's plan,
//! the cloud's sky, the chorus's voices — filling the block and animated
//! as on the full face, so the grid shows what each block is doing.
//!
//! A block face is [`S`] square (twice the grid's 92 pt cell) inside a
//! [`BLEED`] margin each side: the picture is clipped to the block's
//! rounded window, and the margin is the face's to draw into — an outline,
//! a glow — up to the padding between the block and its neighbours. The
//! host draws the whole face over the block's box grown by the margin, with
//! nothing behind it.
//!
//! `faces.json`'s `blocks` lists them by what they match: `<type>:<name>`
//! (a delay's machine, a reverb's algorithm) before `<type>`.

use frame::document::proto::motion::{Motion, Target};
use frame::document::proto::{CornerRadii, NodeId};
use frame::surface::kit::{Place, draw, motion};
use frame::surface::proto::{ParamInfo, Range, Unit};
use frame::surface::widgets::Builder;
use serde_json::json;

use crate::face::{Face, canvas};
use crate::reverb::ui;
use crate::rig_faces::PLUGIN;

/// The block, in face units: twice the grid's 92 pt cell.
pub const S: f64 = 184.0;
/// How far a block's face reaches past the block, each side: into the
/// padding between blocks — half the grid's 16 pt gap.
pub const BLEED: f64 = 16.0;

/// A block face's scene: the margin round the block, transparent, and the
/// block's window, clipped and rounded, filled with `window` (a top-down
/// gradient). `build` draws into the window (`S` square) and, for what
/// reaches past it, the root (the window sits at `BLEED`, `BLEED`).
pub fn block(
    name: &str,
    window: &[(f64, &str)],
    build: impl FnOnce(&mut Builder<'_>, NodeId, NodeId),
) -> frame::document::proto::Scene {
    let window = window.to_vec();
    canvas(name, move |b, root| {
        ui::fit_root(b, root, S + 2.0 * BLEED, S + 2.0 * BLEED);
        let mut v = draw::frame("Window", BLEED, BLEED, S, S);
        v.fills = vec![draw::linear(180.0, &window)];
        v.clips_content = true;
        v.corner_radii = CornerRadii::uniform(18.0);
        let v = b.add(root, v);
        build(b, v, root);
    })
}

/// A block face.
#[must_use]
pub fn face(
    file: &'static str,
    scene: frame::document::proto::Scene,
    params: Vec<ParamInfo>,
) -> Face {
    Face {
        file,
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

const MOD: &str = crate::rig_mod::MOD;
const MOTION: &str = crate::rig_mod::MOTION;

fn mod_params(ns: &str) -> Vec<ParamInfo> {
    vec![
        ui::param(
            &format!("{ns}/rate"),
            "Rate",
            Range::log(0.05, 12.0, Unit::Hz),
            1.0,
        ),
        ui::param(
            &format!("{ns}/depth"),
            "Depth",
            Range::linear(0.0, 1.0, Unit::None),
            0.5,
        ),
        ui::param(
            &format!("{ns}/mix"),
            "Mix",
            Range::linear(0.0, 1.0, Unit::None),
            0.5,
        ),
        ui::param(
            &format!("{ns}/feedback"),
            "Feedback",
            Range::linear(0.0, 1.0, Unit::None),
            0.0,
        ),
        ui::toggle(&format!("{ns}/on"), "Bypass", true),
    ]
}

/// The flanger: a comb of notches sweeping up and down the spectrum, the
/// jet's whoosh through it.
fn flanger() -> Face {
    let scene = block(
        "Flanger",
        &[(0.0, "#06222c"), (1.0, "#03121a")],
        |b, v, _| {
            let sweep = format!(
                "0.5 + 0.5 * sin(6.2832 * time * param({MOD}/rate)) * norm({MOD}/depth) * param({MOD}/on)"
            );
            // The comb: teeth closing and opening with the sweep.
            let comb = b.add(v, draw::frame("comb", 0.0, 0.0, S, S));
            let teeth: Vec<Vec<(f64, f64)>> = (0..16)
                .map(|i| {
                    let x = 8.0 + f64::from(i) * 12.0;
                    vec![(x, S * 0.22), (x, S * 0.78)]
                })
                .collect();
            b.add(
                comb,
                draw::emissive(
                    ui::strokes("teeth", &teeth, 2.0, draw::solid("#22d3ee88")),
                    draw::hex("#22d3ee"),
                    0.6,
                    6.0,
                ),
            );
            motion::add(
                b,
                comb,
                Motion::new(Target::StretchX)
                    .input(sweep.clone())
                    .range(0.55, 1.35)
                    .pivot(0.0, 0.5),
            );
            // The jet: one bright line riding the sweep.
            let jet: Vec<(f64, f64)> = (0..=80)
                .map(|k| {
                    let x = S * f64::from(k) / 80.0;
                    (
                        x,
                        S / 2.0 - 30.0 * (x / S * std::f64::consts::TAU * 1.5).sin(),
                    )
                })
                .collect();
            let j = b.add(v, draw::frame("jet", 0.0, 0.0, S, S));
            b.add(
                j,
                draw::emissive(
                    ui::strokes("jet", &[jet], 2.4, draw::solid("#e0faff")),
                    draw::hex("#67e8f9"),
                    0.9,
                    8.0,
                ),
            );
            motion::add(b, j, Motion::new(Target::Y).input(sweep).range(-24.0, 24.0));
        },
    );
    face("103-mod-flanger-block", scene, mod_params(MOD))
}

/// The phaser: its stages as rings, a notch going round each, the inner
/// ones faster.
fn phaser() -> Face {
    let scene = block(
        "Phaser",
        &[(0.0, "#08202e"), (1.0, "#04101a")],
        |b, v, _| {
            let c = S / 2.0;
            for (i, r) in [70.0, 54.0, 38.0, 22.0].into_iter().enumerate() {
                let ring = draw::stroked(
                    draw::circle("stage", c, c, r, draw::solid("#00000000")),
                    "#22d3ee40",
                    1.4,
                );
                let mut ring = ring;
                ring.fills.clear();
                b.add(v, ring);
                let spin = b.add(v, draw::frame("notch", c - r, c - r, 2.0 * r, 2.0 * r));
                b.add(
                    spin,
                    draw::emissive(
                        draw::circle("dot", r, 0.0, 5.0, draw::solid("#a5f3fc")),
                        draw::hex("#22d3ee"),
                        1.0,
                        8.0,
                    ),
                );
                let turn = format!(
                    "fract(time * param({MOD}/rate) * {} * param({MOD}/on))",
                    0.5 + 0.35 * i as f64
                );
                motion::add(
                    b,
                    spin,
                    Motion::new(Target::Rotate)
                        .input(turn)
                        .range(0.0, 360.0)
                        .pivot(0.5, 0.5),
                );
                motion::add(
                    b,
                    spin,
                    Motion::new(Target::Opacity)
                        .input(format!("0.35 + 0.65 * norm({MOD}/depth)"))
                        .range(0.0, 1.0),
                );
            }
        },
    );
    face("104-mod-phaser-block", scene, mod_params(MOD))
}

/// The rotary: the Leslie's horn going round over its drum.
fn rotary() -> Face {
    let scene = block(
        "Rotary",
        &[(0.0, "#0b2a20"), (1.0, "#06150f")],
        |b, v, _| {
            let c = S / 2.0;
            // The drum: a disc of louvres turning slower.
            let drum = b.add(v, draw::frame("drum", c - 62.0, c - 62.0, 124.0, 124.0));
            let louvres: Vec<Vec<(f64, f64)>> = (0..12)
                .map(|i| {
                    let a = 30.0 * f64::from(i);
                    vec![
                        draw::polar(62.0, 62.0, 20.0, a),
                        draw::polar(62.0, 62.0, 60.0, a),
                    ]
                })
                .collect();
            b.add(
                drum,
                ui::strokes("louvres", &louvres, 1.6, draw::solid("#34d39940")),
            );
            motion::add(
                b,
                drum,
                Motion::new(Target::Rotate)
                    .input(format!(
                        "fract(time * param({MOTION}/rate) * 0.18 * param({MOTION}/on))"
                    ))
                    .range(0.0, 360.0)
                    .pivot(0.5, 0.5),
            );
            // The horn: two bells on a bar.
            let horn = b.add(v, draw::frame("horn", c - 70.0, c - 70.0, 140.0, 140.0));
            b.add(
                horn,
                draw::rounded(
                    draw::rect("bar", 22.0, 66.0, 96.0, 8.0, draw::solid("#a7f3d0")),
                    4.0,
                ),
            );
            for x in [14.0, 126.0] {
                b.add(
                    horn,
                    draw::emissive(
                        draw::circle(
                            "bell",
                            x,
                            70.0,
                            14.0,
                            draw::radial_at(0.5, 0.5, 0.5, &[(0.0, "#ecfdf5"), (1.0, "#34d399")]),
                        ),
                        draw::hex("#34d399"),
                        0.8,
                        10.0,
                    ),
                );
            }
            motion::add(
                b,
                horn,
                Motion::new(Target::Rotate)
                    .input(format!(
                        "fract(time * param({MOTION}/rate) * 0.35 * param({MOTION}/on))"
                    ))
                    .range(0.0, 360.0)
                    .pivot(0.5, 0.5),
            );
            b.add(v, draw::circle("hub", c, c, 7.0, draw::solid("#064e3b")));
        },
    );
    face("105-motion-rotary-block", scene, mod_params(MOTION))
}

/// The pitch block — an octaver, a harmonizer — as the voices it plays:
/// the note you play a white line across the middle, each voice a glowing
/// wave above or below it by its interval (a shelf a semitone), denser the
/// higher it sits, as bright as its level.
fn pitch() -> Face {
    let scene = block("Pitch", &[(0.0, "#10121c"), (1.0, "#07080e")], |b, v, _| {
        // Octave shelves: the note's, an octave and two either side.
        let mid = S / 2.0;
        let per = S * 0.4 / 24.0;
        let shelves: Vec<Vec<(f64, f64)>> = [-24.0, -12.0, 12.0, 24.0]
            .iter()
            .map(|o| vec![(0.0, mid - o * per), (S, mid - o * per)])
            .collect();
        b.add(
            v,
            ui::strokes("octaves", &shelves, 0.8, draw::solid("#ffffff14")),
        );
        let wave = |cycles: f64, amp: f64| -> Vec<(f64, f64)> {
            (0..=120)
                .map(|k| {
                    let x = -S + 3.0 * S * f64::from(k) / 120.0;
                    (x, amp * (x / S * cycles * std::f64::consts::TAU).sin())
                })
                .collect()
        };
        // The dry note.
        let dry = b.add(v, draw::frame("dry", 0.0, mid, S, 0.0));
        b.add(
            dry,
            ui::strokes("dry", &[wave(3.0, 5.0)], 1.6, draw::solid("#e4e4e7")),
        );
        motion::add(
            b,
            dry,
            Motion::new(Target::Opacity)
                .input("0.25 + 0.75 * norm(pitch/dry)")
                .range(0.0, 1.0),
        );
        // The voices.
        for (id, semis, level, colour) in [
            ("a", "semitones", "a_level", "#a78bfa"),
            ("b", "b_semitones", "b_level", "#f0abfc"),
        ] {
            let g = b.add(v, draw::frame(&format!("voice {id}"), 0.0, mid, S, 0.0));
            b.add(
                g,
                draw::emissive(
                    ui::strokes("voice", &[wave(3.0, 7.0)], 2.2, draw::solid(colour)),
                    draw::hex(colour),
                    0.8,
                    8.0,
                ),
            );
            motion::add(
                b,
                g,
                Motion::new(Target::Y)
                    .input(format!("param(pitch/{semis})"))
                    .range(-24.0 * -per, 24.0 * -per),
            );
            motion::add(
                b,
                g,
                Motion::new(Target::StretchX)
                    .input(format!("pow(2, -param(pitch/{semis}) / 12)"))
                    .range(0.0, 1.0)
                    .pivot(0.5, 0.5),
            );
            motion::add(
                b,
                g,
                Motion::new(Target::X)
                    .input(format!(
                        "fract(time * 0.5 * pow(2, param(pitch/{semis}) / 12))"
                    ))
                    .range(0.0, -S / 3.0),
            );
            motion::add(
                b,
                g,
                Motion::new(Target::Opacity)
                    .input(format!("norm(pitch/{level}) * param(pitch/on)"))
                    .range(0.0, 1.0),
            );
        }
    });
    let semis = |id: &str, name: &str, d: f64| {
        ui::param(
            &format!("pitch/{id}"),
            name,
            Range::linear(-24.0, 24.0, Unit::None),
            d,
        )
    };
    let level = |id: &str, name: &str, d: f64| {
        ui::param(
            &format!("pitch/{id}"),
            name,
            Range::linear(0.0, 1.0, Unit::None),
            d,
        )
    };
    face(
        "33-pitch-block",
        scene,
        vec![
            semis("semitones", "Voice A", 12.0),
            semis("b_semitones", "Voice B", 0.0),
            level("a_level", "A Level", 1.0),
            level("b_level", "B Level", 0.0),
            level("dry", "Dry", 1.0),
            ui::toggle("pitch/on", "Bypass", true),
        ],
    )
}

/// One of the kit's visualizers filling the block: a delay's taps, a
/// reverb's space, an EQ's curve, a compressor's scope.
fn viz(
    file: &'static str,
    name: &str,
    id: &str,
    ns: &str,
    window: &[(f64, &str)],
    params: Vec<ParamInfo>,
) -> Face {
    let (id, ns) = (id.to_string(), ns.to_string());
    let scene = block(name, window, move |b, v, _| {
        let p =
            Place::new(name)
                .size(S)
                .options(&[&format!("ns={ns}"), &format!("h={S}"), "no_range"]);
        ui::put_at(b, v, &id, &p, 0.0, 0.0);
    });
    face(file, scene, params)
}

#[must_use]
pub fn faces() -> Vec<Face> {
    use frame::surface::kit::viz::{catalog, lit_catalog};
    let dark = [(0.0, "#0c0d10"), (1.0, "#08090b")];
    let mut eq = catalog::eq_params("eq", 24);
    eq.extend(catalog::eq_view_params("eq"));
    let mut comp = lit_catalog::comp_scope("comp", 128).0;
    comp.push(ui::toggle("comp/on", "Bypass", true));
    let mut v = vec![
        crate::rig_time::delay::tape::block(),
        crate::rig_time::delay::digital::block(),
        crate::rig_time::delay::analog::block(),
        crate::rig_time::reverb::plate::block(),
        crate::rig_time::reverb::hall::block(),
        crate::rig_time::reverb::cloud::block(),
        flanger(),
        phaser(),
        rotary(),
        viz(
            "69-dly-block",
            "Delay",
            "viz.delay_taps",
            "dly",
            &dark,
            lit_catalog::delay_params("dly"),
        ),
        viz(
            "89-verb-block",
            "Reverb",
            "viz.reverb_space",
            "verb",
            &dark,
            lit_catalog::reverb_params("verb"),
        ),
        viz("52-amp-eq-block", "EQ", "viz.eq_graph", "eq", &dark, eq),
        viz(
            "55-comp-block",
            "Compressor",
            "viz.comp_scope",
            "comp",
            &dark,
            comp,
        ),
    ];
    v.extend(crate::rig_mod::blocks());
    v.extend(crate::rig_faces::amp_blocks());
    v.extend(crate::rig_units::faces());
    v.push(crate::rig_blues::drives::filter_block());
    v.push(pitch());
    v.push(viz(
        "51-gate-block",
        "Gate",
        "viz.gate",
        "gate",
        &dark,
        vec![
            ui::param(
                "gate/threshold",
                "Threshold",
                Range::linear(-90.0, 0.0, Unit::Db),
                -50.0,
            ),
            ui::toggle("gate/on", "Bypass", true),
        ],
    ));
    v
}

/// `faces.json`'s `blocks`: each block face by what it matches, its
/// params' namespace, and its margin past the block (in the block's own
/// units: a fraction of the block's side).
#[must_use]
pub fn manifest() -> serde_json::Value {
    let bleed = BLEED / S;
    let e = |m: &[&str], face: &str, ns: &str| json!({ "match": m, "face": face, "ns": ns, "bleed": bleed });
    let mut all = crate::rig_units::manifest();
    all.extend([
        e(&["delay:tape"], "60-dly-tape-block", "dly"),
        e(&["delay:digital"], "61-dly-digital-block", "dly"),
        e(&["delay:analog"], "62-dly-analog-block", "dly"),
        e(&["delay"], "69-dly-block", "dly"),
        e(&["reverb:hall"], "81-verb-hall-block", "verb"),
        e(&["reverb:plate"], "82-verb-plate-block", "verb"),
        e(&["reverb:cloud"], "84-verb-cloud-block", "verb"),
        e(&["reverb"], "89-verb-block", "verb"),
        e(&["chorus"], "100-mod-chorus-block", "mod"),
        e(&["flanger"], "103-mod-flanger-block", "mod"),
        e(&["phaser"], "104-mod-phaser-block", "mod"),
        e(&["trem", "tremolo"], "101-motion-tremolo-block", "motion"),
        e(&["vibrato"], "102-motion-vibrato-block", "motion"),
        e(&["rotary"], "105-motion-rotary-block", "motion"),
        e(&["eq"], "52-amp-eq-block", "eq"),
        e(&["compressor"], "55-comp-block", "comp"),
        e(&["gate"], "51-gate-block", "gate"),
        e(&["filter"], "32-filter-viz-block", "qtron"),
        e(&["pitch"], "33-pitch-block", "pitch"),
    ]);
    serde_json::Value::Array(all)
}
