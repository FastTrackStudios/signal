//! The rig's own units composed for a block of Signal's routing grid,
//! minimal: each only what makes it that unit — the Katana's one big knob,
//! the Klon's gold and its Centaur, the Bluesbreaker's plate, the 808's
//! green and its switch, the spring tank ringing, the Distressor's
//! gain-reduction lamps.
//!
//! The block's top and foot carry its preset and its name on bands of
//! their own: each unit sits in the middle, between them.

use document_proto::motion::{Motion, Target};
use document_proto::{NodeId, TextAlign};
use serde_json::json;
use surface_kit::draw::{self, Lettering, family};
use surface_kit::motion;
use surface_widgets::Builder;

use crate::face::Face;
use crate::reverb::ui;
use crate::rig_cells::{BLEED, S, block, face};
use crate::suite_studio_blues::parts::set;

/// Each unit's params: on (its bypass).
const NS: &str = "unit";

fn on() -> Vec<surface_proto::ParamInfo> {
    vec![ui::toggle(&format!("{NS}/on"), "Bypass", true)]
}

/// A knob seen from above: its cap, a pointer at `deg` (0 up).
fn knob(
    b: &mut Builder<'_>,
    p: NodeId,
    (cx, cy): (f64, f64),
    r: f64,
    cap: (&str, &str),
    pointer: &str,
    deg: f64,
) {
    b.add(
        p,
        draw::with_effects(
            draw::circle(
                "knob",
                cx,
                cy,
                r,
                draw::radial_at(0.38, 0.32, 0.9, &[(0.0, cap.0), (1.0, cap.1)]),
            ),
            vec![draw::drop_shadow("#00000080", 0.0, 2.0, 3.0)],
        ),
    );
    let tip = draw::polar(cx, cy, r * 0.82, deg);
    let root = draw::polar(cx, cy, r * 0.25, deg);
    b.add(
        p,
        ui::strokes(
            "pointer",
            &[vec![root, tip]],
            (r * 0.14).max(1.6),
            draw::solid(pointer),
        ),
    );
}

/// A lamp, lit while the unit is on.
fn lamp(b: &mut Builder<'_>, p: NodeId, (cx, cy): (f64, f64), colour: &str) {
    b.add(
        p,
        draw::circle("lamp ring", cx, cy, 5.5, draw::solid("#0b0b0c")),
    );
    let l = b.add(
        p,
        draw::emissive(
            draw::circle("lamp", cx, cy, 3.6, draw::solid(colour)),
            draw::hex(colour),
            1.0,
            9.0,
        ),
    );
    motion::add(
        b,
        l,
        Motion::new(Target::Opacity)
            .input(format!("param({NS}/on)"))
            .range(0.12, 1.0),
    );
}

fn words(b: &mut Builder<'_>, p: NodeId, s: &str, cy: f64, size: f64, colour: &str, l: &Lettering) {
    b.add(
        p,
        draw::styled_text(
            "legend",
            s,
            0.0,
            cy - size * 0.7,
            S,
            size,
            colour,
            TextAlign::Center,
            l,
        ),
    );
}

/// The Keeley Katana: its name, wide and white, in the inset line on the
/// black lid, the blade's K under it.
fn katana() -> Face {
    let scene = block(
        "Katana",
        &[(0.0, "#1a1a1d"), (1.0, "#0a0a0b")],
        |b, v, _| {
            let ink = "#ececef";
            b.add(
                v,
                draw::stroked(
                    draw::rounded(
                        draw::rect(
                            "border",
                            12.0,
                            50.0,
                            S - 24.0,
                            86.0,
                            draw::solid("#00000000"),
                        ),
                        10.0,
                    ),
                    ink,
                    1.6,
                ),
            );
            set(
                b,
                v,
                "KATANA",
                S / 2.0,
                80.0,
                30.0,
                ink,
                &Lettering::family(family::INTER).weight(500.0).width(92.0),
                0.04,
            );
            b.add(
                v,
                draw::stroked(
                    draw::rounded(
                        draw::rect(
                            "mark",
                            S / 2.0 - 11.0,
                            104.0,
                            22.0,
                            22.0,
                            draw::solid("#00000000"),
                        ),
                        3.0,
                    ),
                    ink,
                    1.5,
                ),
            );
            set(
                b,
                v,
                "K",
                S / 2.0,
                115.5,
                15.0,
                ink,
                &Lettering::family(family::INTER).weight(800.0).italic(),
                0.0,
            );
        },
    );
    face("40-unit-katana-block", scene, on())
}

/// The Klon Centaur: its gold, and the centaur at a gallop across it.
fn klon() -> Face {
    let scene = block(
        "Klon",
        &[(0.0, "#e2b25a"), (0.55, "#c99038"), (1.0, "#9c6a22")],
        |b, v, _| {
            b.add(
                v,
                draw::rect(
                    "sheen",
                    0.0,
                    0.0,
                    S,
                    S,
                    draw::linear(
                        115.0,
                        &[
                            (0.0, "#ffe2a03a"),
                            (0.4, "#ffffff00"),
                            (0.75, "#4a2c0628"),
                            (1.0, "#ffe2a01a"),
                        ],
                    ),
                ),
            );
            crate::rig_blues::drives::centaur(b, v, (S / 2.0, 94.0), 128.0, "#2b1c08");
        },
    );
    face("41-unit-klon-block", scene, on())
}

/// The Marshall Bluesbreaker: its charcoal, and the blue italic name
/// between two double rules, as on the original.
fn bluesbreaker() -> Face {
    let scene = block(
        "Bluesbreaker",
        &[
            (0.0, "#34363b"),
            (0.3, "#2c2e33"),
            (0.32, "#1d1e21"),
            (1.0, "#2a2c30"),
        ],
        |b, v, _| {
            let blue = "#4fb8ea";
            let ly = 92.0;
            for dy in [-21.0, -17.0, 17.0, 21.0] {
                b.add(
                    v,
                    draw::rect("rule", 10.0, ly + dy, S - 20.0, 1.8, draw::solid(blue)),
                );
            }
            set(
                b,
                v,
                "BLUES BREAKER",
                S / 2.0,
                ly,
                21.0,
                blue,
                &Lettering::family(family::OSWALD).weight(700.0).italic(),
                0.02,
            );
            lamp(b, v, (S / 2.0, 126.0), "#e0281c");
        },
    );
    face("42-unit-bluesbreaker-block", scene, on())
}

/// The Ibanez TS808: its green, the grey name band, and the black treadle
/// ribbed in chevrons with the maker's script.
fn ts808() -> Face {
    let scene = block("TS808", &[(0.0, "#6fd257"), (1.0, "#46a630")], |b, v, _| {
        let (by, bh) = (50.0, 24.0);
        b.add(
            v,
            draw::rect(
                "band",
                0.0,
                by,
                S,
                bh,
                draw::linear(180.0, &[(0.0, "#46494c"), (1.0, "#2a2c2f")]),
            ),
        );
        set(
            b,
            v,
            "TUBE SCREAMER",
            S / 2.0,
            by + bh / 2.0,
            15.0,
            "#f1f1ef",
            &Lettering::family(family::INTER).weight(700.0).width(88.0),
            0.02,
        );
        let (tx, ty, tw, th) = (16.0, by + bh + 8.0, S - 32.0, 62.0);
        let mut tread = draw::frame("treadle", tx, ty, tw, th);
        tread.fills = vec![draw::linear(
            180.0,
            &[(0.0, "#2c2c2e"), (0.08, "#18181a"), (1.0, "#0c0c0d")],
        )];
        tread.corner_radii = document_proto::CornerRadii::uniform(6.0);
        tread.clips_content = true;
        let tread = b.add(v, tread);
        let (mut lit, mut dark) = (Vec::new(), Vec::new());
        for r in 0..6 {
            let y = 6.0 + f64::from(r) * 9.0;
            for k in 0..3 {
                let (x0, x1) = (
                    tw * (0.06 + 0.3 * f64::from(k)),
                    tw * (0.3 + 0.3 * f64::from(k)),
                );
                let xm = (x0 + x1) / 2.0;
                dark.push(vec![(x0, y + 4.0), (xm, y), (x1, y + 4.0)]);
                lit.push(vec![(x0, y + 5.2), (xm, y + 1.2), (x1, y + 5.2)]);
            }
        }
        b.add(
            tread,
            ui::strokes("rib shadow", &dark, 2.0, draw::solid("#050506")),
        );
        b.add(
            tread,
            ui::strokes("rib light", &lit, 0.9, draw::solid("#3a3a3e")),
        );
        set(
            b,
            tread,
            "Ibanez",
            tw / 2.0,
            th * 0.55,
            20.0,
            "#4a4a50",
            &Lettering::family(family::DANCING_SCRIPT)
                .weight(700.0)
                .italic(),
            0.0,
        );
    });
    face("43-unit-ts808-block", scene, on())
}

/// A spring tank: the open pan and its springs, ringing as the reverb's
/// decay, damping and modulation set — the full spring picture's tank,
/// across the block.
fn spring() -> Face {
    let a = crate::reverb::spring::Springs {
        dwell: "spring/decay",
        damping: "spring/damping",
        tension: "spring/tension",
        drip: "spring/modulation",
        on: "spring/on",
        input: "spring/in",
        output: "spring/out",
    };
    let scene = block(
        "Spring",
        &[(0.0, "#2a2c30"), (1.0, "#16171a")],
        |b, v, _| {
            let (w, h) = (S - 16.0, 92.0);
            let mut tank = draw::frame("Tank", 8.0, 46.0, w, h);
            tank.clips_content = true;
            tank.corner_radii = document_proto::CornerRadii::uniform(6.0);
            let tank = b.add(v, tank);
            crate::reverb::spring::tank_art_with(b, tank, &a, w, h, 2, "");
        },
    );
    let range = |id: &str, name: &str, d: f64| {
        ui::param(
            &format!("spring/{id}"),
            name,
            surface_proto::Range::linear(0.0, 1.0, surface_proto::Unit::None),
            d,
        )
    };
    face(
        "45-unit-spring-block",
        scene,
        vec![
            range("decay", "Decay", 0.35),
            range("damping", "Damping", 0.3),
            range("tension", "Tension", 0.5),
            range("modulation", "Modulation", 0.2),
            ui::toggle("spring/on", "Reverb", true),
        ],
    )
}

/// The Distressor: its black face, its name, and the gain-reduction lamps
/// working across it.
pub fn distressor() -> Face {
    let scene = block(
        "Distressor",
        &[(0.0, "#1b1b1d"), (1.0, "#08080a")],
        |b, v, _| {
            words(
                b,
                v,
                "DISTRESSOR",
                62.0,
                12.0,
                "#e9e9ec",
                &Lettering::family(family::MICHROMA).weight(400.0),
            );
            // Ten lamps: green, amber, red — as much as it is taking off.
            let gr = "(0.4 + 0.3 * sin(time * 2.1) + 0.15 * sin(time * 5.3))";
            for i in 0..10 {
                let x = 30.0 + f64::from(i) * 14.0;
                let colour = if i < 5 {
                    "#4ade80"
                } else if i < 8 {
                    "#facc15"
                } else {
                    "#ef4444"
                };
                b.add(
                    v,
                    draw::rounded(
                        draw::rect("well", x - 5.0, 84.0, 10.0, 18.0, draw::solid("#050506")),
                        2.0,
                    ),
                );
                let l = b.add(
                    v,
                    draw::emissive(
                        draw::rounded(
                            draw::rect("lamp", x - 4.0, 85.0, 8.0, 16.0, draw::solid(colour)),
                            2.0,
                        ),
                        draw::hex(colour),
                        0.9,
                        7.0,
                    ),
                );
                motion::add(
                    b,
                    l,
                    Motion::new(Target::Opacity)
                        .input(format!("clamp(({gr} * 10 - {i}) * param(dist/on), 0, 1)"))
                        .range(0.08, 1.0),
                );
            }
            knob(
                b,
                v,
                (44.0, 124.0),
                11.0,
                ("#f1f2f4", "#8b8f95"),
                "#111",
                240.0,
            );
            knob(
                b,
                v,
                (140.0, 124.0),
                11.0,
                ("#f1f2f4", "#8b8f95"),
                "#111",
                200.0,
            );
        },
    );
    face(
        "55-post-comp-distressor-block",
        scene,
        vec![ui::toggle("dist/on", "In", true)],
    )
}

#[must_use]
pub fn faces() -> Vec<Face> {
    vec![
        katana(),
        klon(),
        bluesbreaker(),
        ts808(),
        spring(),
        distressor(),
    ]
}

/// `faces.json`'s `blocks` for them: a drive by its pedal, the pre
/// effects' spring and any spring reverb, the post compressor.
#[must_use]
pub fn manifest() -> Vec<serde_json::Value> {
    let bleed = BLEED / S;
    let e = |m: &[&str], face: &str| json!({ "match": m, "face": face, "ns": NS, "bleed": bleed });
    vec![
        e(
            &["drive:keeley katana", "drive:katana"],
            "40-unit-katana-block",
        ),
        e(&["drive:klon centaur", "drive:klon"], "41-unit-klon-block"),
        e(
            &["drive:marshall bluesbreaker", "drive:bluesbreaker"],
            "42-unit-bluesbreaker-block",
        ),
        e(
            &["drive:ibanez ts808", "drive:ts808", "drive:tube screamer"],
            "43-unit-ts808-block",
        ),
        json!({ "match": ["pre:reverb", "reverb:spring"], "face": "45-unit-spring-block", "ns": "spring", "bleed": bleed }),
        json!({ "match": ["post comp"], "face": "55-post-comp-distressor-block", "ns": "dist", "bleed": bleed }),
    ]
}
