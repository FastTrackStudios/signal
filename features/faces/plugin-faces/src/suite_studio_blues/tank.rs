//! The tabletop harmonic-tremolo and spring-reverb unit that sits after the
//! amp: a navy case on copper feet, a copper mesh window across the top,
//! its name in wide pale capitals, and two sections bracketed under their
//! names — tremolo (on/off, a rate lamp pulsing at the speed, INTENSITY,
//! SPEED, the subdivision on a small display, SYNC/FREE) and reverb (MIX,
//! TONE, DECAY, its lamp, on/off). Tabletop, 1200 × 490.

use document_proto::motion::{Dynamics, Motion, Num, Target, Wave};
use document_proto::{CornerRadii, NodeId};
use surface_kit::materials::{light, paint};
use surface_kit::{Place, draw, motion};
use surface_proto::{Range, SlotRole, Unit, ValueFormat};
use surface_widgets::Builder;

use super::parts::{self, Cap, PLUGIN, knob, print, print_left, print_right, toggle};
use crate::face::{Face, canvas};
use crate::reverb::ui;

const W: f64 = 1200.0;
const H: f64 = 490.0;
const BODY_H: f64 = 448.0;
const INK: &str = "#e2d2c6";
const DIM: &str = "#c9b8ac";

fn screw(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64) {
    b.add(
        parent,
        draw::with_effects(
            draw::circle(
                "screw",
                cx,
                cy,
                9.0,
                draw::radial_at(0.4, 0.35, 0.9, &[(0.0, "#5a5f70"), (1.0, "#15171f")]),
            ),
            vec![draw::inner_shadow("#00000090", 0.0, 1.0, 2.0)],
        ),
    );
    b.add(
        parent,
        draw::circle(
            "screw head",
            cx,
            cy,
            5.5,
            draw::radial_at(0.4, 0.35, 0.9, &[(0.0, "#8a8f9c"), (1.0, "#2a2d36")]),
        ),
    );
}

/// A bracket over a section name, its ticks reaching up to the controls.
fn bracket_up(b: &mut Builder<'_>, parent: NodeId, x0: f64, x1: f64, y: f64, ticks: &[f64]) {
    let mut lines = vec![vec![(x0, y), (x1, y)]];
    lines.extend(ticks.iter().map(|x| vec![(*x, y), (*x, y - 9.0)]));
    b.add(
        parent,
        ui::strokes("bracket", &lines, 1.6, draw::solid(DIM)),
    );
}

/// A bracket under a section name: a rule with ticks down at `ticks`.
fn bracket(b: &mut Builder<'_>, parent: NodeId, x0: f64, x1: f64, y: f64, ticks: &[f64]) {
    let mut lines = vec![vec![(x0, y), (x1, y)]];
    lines.extend(ticks.iter().map(|x| vec![(*x, y), (*x, y + 9.0)]));
    b.add(
        parent,
        ui::strokes("bracket", &lines, 1.6, draw::solid(DIM)),
    );
}

pub fn spring_tank() -> Face {
    spring_tank_as(&Tank::PLUGIN)
}

/// Where the tank's controls bind, and what it is called.
pub struct Tank {
    pub title: &'static str,
    pub file: &'static str,
    pub plugin: &'static str,
    pub trem_on: &'static str,
    pub intensity: &'static str,
    pub speed: &'static str,
    pub division: &'static str,
    pub sync: &'static str,
    pub mix: &'static str,
    pub tone: &'static str,
    pub decay: &'static str,
    pub verb_on: &'static str,
    pub lfo: &'static str,
    /// The rig's ranges (a tremolo block's depth 0..1 and rate in Hz, a
    /// reverb block's level in dB, tone -1..1, decay 0..1) rather than the
    /// plugin's percentages.
    pub rig: bool,
}

impl Tank {
    /// The suite's own unit.
    pub const PLUGIN: Self = Self {
        title: "SPRING TANK",
        file: "10-tabletop-spring-tank",
        plugin: PLUGIN,
        trem_on: "tank/trem_on",
        intensity: "tank/intensity",
        speed: "tank/speed",
        division: "tank/division",
        sync: "tank/sync",
        mix: "tank/mix",
        tone: "tank/tone",
        decay: "tank/decay",
        verb_on: "tank/verb_on",
        lfo: "tank/lfo",
        rig: false,
    };
}

/// The tank drawn with `t`'s bindings.
#[must_use]
pub fn spring_tank_as(t: &Tank) -> Face {
    let scene = canvas(
        if t.rig { "Gravity Tank" } else { "Spring Tank" },
        |b, root| {
            ui::fit_root(b, root, W, H);
            // Copper feet under the case.
            for x in [92.0, 1016.0] {
                let foot = draw::rect(
                    "foot",
                    x,
                    BODY_H - 20.0,
                    92.0,
                    H - BODY_H + 20.0,
                    draw::linear(
                        90.0,
                        &[
                            (0.0, "#8a5234"),
                            (0.3, "#e8a57a"),
                            (0.55, "#f6c9a4"),
                            (1.0, "#7a4228"),
                        ],
                    ),
                );
                b.add(root, draw::rounded(foot, 6.0));
            }
            let mut case = draw::frame("Case", 0.0, 0.0, W, BODY_H);
            case.corner_radii = CornerRadii::uniform(32.0);
            case.stroke = document_proto::Stroke {
                paints: vec![draw::linear(
                    90.0,
                    &[(0.0, "#a8664a"), (0.5, "#5a3a34"), (1.0, "#a8664a")],
                )],
                weight: 3.0,
                ..document_proto::Stroke::default()
            };
            case.effects = vec![draw::drop_shadow("#000000c0", 0.0, 8.0, 20.0)];
            let case = b.add(root, case);
            // Navy powder-coat, its rolled edge lit from the upper left (edges,
            // not inner shadows: a big effect layer can blank the page).
            paint::fill(
                b,
                case,
                (0.0, 0.0, W, BODY_H),
                32.0,
                &paint::Material::Powder {
                    color: "#30334a".into(),
                    coarse: false,
                },
            );
            for e in light::edge("case edge", (0.0, 0.0, W, BODY_H), 32.0, 6.0, 1.1, true) {
                b.add(case, e);
            }
            for (x, y) in [
                (28.0, 31.0),
                (W - 28.0, 31.0),
                (28.0, BODY_H - 31.0),
                (W - 28.0, BODY_H - 31.0),
            ] {
                screw(b, case, x, y);
            }
            // The mesh window in its copper surround.
            let (mx, my, mw, mh) = (51.0, 50.0, W - 102.0, 110.0);
            b.add(
                case,
                draw::with_effects(
                    draw::rounded(
                        draw::rect(
                            "surround",
                            mx - 5.0,
                            my - 5.0,
                            mw + 10.0,
                            mh + 10.0,
                            draw::linear(
                                180.0,
                                &[(0.0, "#f2c7a6"), (0.5, "#c98a66"), (1.0, "#f0c09c")],
                            ),
                        ),
                        24.0,
                    ),
                    vec![draw::drop_shadow("#00000080", 0.0, 2.0, 4.0)],
                ),
            );
            let mesh = Place {
                options: Some(vec![format!("w={mw}"), format!("h={mh}"), "r=20".into()]),
                ..Place::new("Mesh").variant("mesh")
            };
            let m = ui::put_at(b, case, "panel.amp_grille", &mesh, mx, my);
            if let Some(n) = b.scene.node_mut(m) {
                n.effects.clear();
            }
            for e in light::edge("mesh recess", (mx, my, mw, mh), 20.0, 6.0, 1.2, false) {
                b.add(case, e);
            }

            // The name and the two sections.
            print_left(b, case, t.title, 40.0, 212.0, 40.0, INK, 600.0, 0.08);
            print_left(
                b,
                case,
                "Harmonic Tremolo",
                44.0,
                262.0,
                16.0,
                INK,
                500.0,
                0.03,
            );
            print_right(
                b,
                case,
                "Spring Reverb",
                1160.0,
                262.0,
                16.0,
                INK,
                500.0,
                0.03,
            );
            bracket(b, case, 45.0, 576.0, 290.0, &[45.0, 266.0, 381.0, 576.0]);
            bracket(
                b,
                case,
                715.0,
                1160.0,
                290.0,
                &[715.0, 830.0, 942.0, 1160.0],
            );

            let row = 356.0;
            let label_y = 420.0;
            // Tremolo.
            print(b, case, "ON", 102.0, 322.0, 12.0, INK, 500.0, 0.08);
            toggle(b, case, t.trem_on, "Tremolo", 24.0, 102.0, row);
            print(b, case, "OFF", 102.0, 394.0, 12.0, INK, 500.0, 0.08);
            let lamp = ui::stream_lamp(
                b,
                case,
                "Tremolo lamp",
                t.lfo,
                "",
                171.0,
                row,
                30.0,
                "#3cf0a0",
                INK,
            );
            let glow = b
                .controls
                .get(&lamp)
                .and_then(|c| c.slots_of(SlotRole::Glow).next())
                .map(NodeId);
            if let Some(glow) = glow {
                motion::add(b, glow, idle_lfo(t));
                motion::add(
                    b,
                    glow,
                    Motion::new(Target::Opacity)
                        .input(format!("or(scalar({}), motion(lfo))", t.lfo))
                        .dynamics(Dynamics::spring(900.0, 50.0)),
                );
            }
            for (addr, name, cx) in [(t.intensity, "INTENSITY", 266.0), (t.speed, "SPEED", 381.0)] {
                knob(b, case, Cap::Cream, addr, name, 84.0, cx, row);
                print(b, case, name, cx, label_y, 12.0, INK, 500.0, 0.12);
            }
            parts::segments(
                b,
                case,
                t.division,
                (455.0, row - 19.0, 76.0, 38.0),
                5,
                15.0,
                "#c4d2ff",
                ValueFormat::default(),
                true,
            );
            b.add(
                case,
                draw::rect("dash", 535.0, row - 0.8, 14.0, 1.6, draw::solid(DIM)),
            );
            print(b, case, "SYNC", 577.0, 318.0, 12.0, INK, 500.0, 0.12);
            toggle(b, case, t.sync, "Sync", 24.0, 577.0, row);
            print(b, case, "FREE", 577.0, 394.0, 12.0, INK, 500.0, 0.12);
            // Reverb.
            for (addr, name, cx) in [
                (t.mix, "MIX", 714.0),
                (t.tone, "TONE", 830.0),
                (t.decay, "DECAY", 942.0),
            ] {
                knob(b, case, Cap::Cream, addr, name, 84.0, cx, row);
                print(b, case, name, cx, label_y, 12.0, INK, 500.0, 0.12);
            }
            let jewel = Place::new("Reverb lamp")
                .size(30.0)
                .variant("green")
                .bind(ui::bind(t.verb_on))
                .legend("")
                .touch(surface_kit::TOUCH_MIN);
            let j = ui::put(b, case, "button.jewel", &jewel, 1033.0, row);
            ui::strip_legend(b, j);
            print(b, case, "ON", 1101.0, 322.0, 12.0, INK, 500.0, 0.08);
            toggle(b, case, t.verb_on, "Reverb", 24.0, 1101.0, row);
            print(b, case, "OFF", 1101.0, 394.0, 12.0, INK, 500.0, 0.08);
        },
    );
    let divisions = [
        "1/1", "1/2", "1/4", "1/4D", "1/4T", "1/8", "1/8D", "1/8T", "1/16", "1/16D", "1/16T",
    ];
    let params = if t.rig {
        vec![
            ui::toggle(t.trem_on, "Tremolo", false).featured(),
            ui::param(
                t.intensity,
                "Intensity",
                Range::linear(0.0, 1.0, Unit::None),
                0.4,
            )
            .featured(),
            ui::param(t.speed, "Speed", Range::log(0.05, 12.0, Unit::Hz), 5.0).featured(),
            ui::choice(t.division, "Division", &divisions, 6),
            ui::choice(t.sync, "Sync", &["Free", "Sync"], 0),
            ui::param(t.mix, "Mix", Range::linear(-60.0, 12.0, Unit::Db), -16.5).featured(),
            ui::param(t.tone, "Tone", Range::linear(-1.0, 1.0, Unit::None), 0.0),
            ui::param(t.decay, "Decay", Range::linear(0.0, 1.0, Unit::None), 0.35),
            ui::toggle(t.verb_on, "Reverb", true).featured(),
        ]
    } else {
        vec![
            ui::toggle(t.trem_on, "Tremolo", true).featured(),
            ui::percent(t.intensity, "Intensity", 50.0).featured(),
            ui::param(t.speed, "Speed", Range::log(0.5, 12.0, Unit::Hz), 4.0).featured(),
            ui::choice(t.division, "Division", &divisions, 6),
            ui::choice(t.sync, "Sync", &["Free", "Sync"], 1),
            ui::percent(t.mix, "Mix", 50.0).featured(),
            ui::percent(t.tone, "Tone", 50.0),
            ui::percent(t.decay, "Decay", 50.0),
            ui::toggle(t.verb_on, "Reverb", true).featured(),
        ]
    };
    Face {
        file: t.file,
        plugin: t.plugin,
        scene,
        params,
        streams: vec![ui::lfo_stream(t.lfo, "Tremolo LFO")],
    }
}

/// The tank for a rig row: the same navy case, copper trim and cream caps,
/// without the mesh window — the name in the corner, the two sections side
/// by side, knobs as big as the height allows. `w × h`.
#[must_use]
pub fn spring_tank_compact(t: &Tank, file: &'static str, w: f64, h: f64) -> Face {
    let scene = canvas("Gravity Tank (compact)", |b, root| {
        ui::fit_root(b, root, w, h);
        let mut case = draw::frame("Case", 0.0, 0.0, w, h);
        case.corner_radii = CornerRadii::uniform(18.0);
        case.stroke = document_proto::Stroke {
            paints: vec![draw::linear(
                90.0,
                &[(0.0, "#a8664a"), (0.5, "#5a3a34"), (1.0, "#a8664a")],
            )],
            weight: 3.0,
            ..document_proto::Stroke::default()
        };
        let case = b.add(root, case);
        paint::fill(
            b,
            case,
            (0.0, 0.0, w, h),
            18.0,
            &paint::Material::Powder {
                color: "#30334a".into(),
                coarse: false,
            },
        );
        for e in light::edge("case edge", (0.0, 0.0, w, h), 18.0, 5.0, 1.1, true) {
            b.add(case, e);
        }
        // The mesh window in its copper surround, across the top: the
        // unit's face, kept whole.
        let (mx, my, mw, mh) = (22.0, 18.0, w - 44.0, h * 0.3);
        b.add(
            case,
            draw::with_effects(
                draw::rounded(
                    draw::rect(
                        "surround",
                        mx - 4.0,
                        my - 4.0,
                        mw + 8.0,
                        mh + 8.0,
                        draw::linear(
                            180.0,
                            &[(0.0, "#f2c7a6"), (0.5, "#c98a66"), (1.0, "#f0c09c")],
                        ),
                    ),
                    14.0,
                ),
                vec![draw::drop_shadow("#00000080", 0.0, 2.0, 4.0)],
            ),
        );
        let mesh = Place {
            options: Some(vec![format!("w={mw}"), format!("h={mh}"), "r=11".into()]),
            ..Place::new("Mesh").variant("mesh")
        };
        let m = ui::put_at(b, case, "panel.amp_grille", &mesh, mx, my);
        if let Some(n) = b.scene.node_mut(m) {
            n.effects.clear();
        }
        for e in light::edge("mesh recess", (mx, my, mw, mh), 11.0, 4.0, 1.2, false) {
            b.add(case, e);
        }
        // From the bottom up: the section names, their brackets reaching up
        // to the knobs, the knobs' names, the knobs.
        let d = (h * 0.34).min(w * 0.13);
        let section_y = h - 16.0;
        let bracket_y = section_y - 16.0;
        let label_y = bracket_y - 16.0;
        let row = label_y - 14.0 - d * 0.5;
        // Tremolo: on, intensity, speed.
        print(b, case, "ON", w * 0.05, row - 30.0, 11.0, INK, 500.0, 0.08);
        toggle(b, case, t.trem_on, "Tremolo", 22.0, w * 0.05, row);
        print(b, case, "OFF", w * 0.05, row + 30.0, 11.0, INK, 500.0, 0.08);
        // The tremolo's lamp: it pulses at the speed, swinging as deep as
        // the intensity (steady at none), dark while the tremolo is off —
        // the rate and depth at a glance.
        let lamp = ui::stream_lamp(
            b,
            case,
            "Tremolo lamp",
            t.lfo,
            "",
            w * 0.105,
            row,
            22.0,
            "#3cf0a0",
            INK,
        );
        let glow = b
            .controls
            .get(&lamp)
            .and_then(|c| c.slots_of(SlotRole::Glow).next())
            .map(NodeId);
        if let Some(glow) = glow {
            motion::add(b, glow, idle_lfo(t));
            let pulse = format!(
                "or(scalar({lfo}), lerp(1, motion(lfo), param({depth}))) * param({on})",
                lfo = t.lfo,
                depth = t.intensity,
                on = t.trem_on
            );
            motion::add(
                b,
                glow,
                Motion::new(Target::Opacity)
                    .input(pulse)
                    .dynamics(Dynamics::spring(900.0, 50.0)),
            );
        }
        for (addr, name, fx) in [(t.intensity, "INTENSITY", 0.2), (t.speed, "SPEED", 0.345)] {
            knob(b, case, Cap::Cream, addr, name, d, w * fx, row);
            print(b, case, name, w * fx, label_y, 11.0, INK, 500.0, 0.12);
        }
        // Reverb: mix, tone, decay, on.
        for (addr, name, fx) in [
            (t.mix, "MIX", 0.52),
            (t.tone, "TONE", 0.67),
            (t.decay, "DECAY", 0.82),
        ] {
            knob(b, case, Cap::Cream, addr, name, d, w * fx, row);
            print(b, case, name, w * fx, label_y, 11.0, INK, 500.0, 0.12);
        }
        print(b, case, "ON", w * 0.95, row - 30.0, 11.0, INK, 500.0, 0.08);
        toggle(b, case, t.verb_on, "Reverb", 22.0, w * 0.95, row);
        print(b, case, "OFF", w * 0.95, row + 30.0, 11.0, INK, 500.0, 0.08);
        // The sections named along the bottom, bracketed as on the unit.
        bracket_up(b, case, w * 0.03, w * 0.4, bracket_y, &[w * 0.03, w * 0.4]);
        bracket_up(
            b,
            case,
            w * 0.46,
            w * 0.97,
            bracket_y,
            &[w * 0.46, w * 0.97],
        );
        print(
            b,
            case,
            "Harmonic Tremolo",
            w * 0.215,
            section_y,
            13.0,
            INK,
            500.0,
            0.03,
        );
        print(
            b,
            case,
            "Spring Reverb",
            w * 0.715,
            section_y,
            13.0,
            INK,
            500.0,
            0.03,
        );
    });
    let full = spring_tank_as(t);
    Face {
        file,
        plugin: t.plugin,
        scene,
        params: full.params,
        streams: full.streams,
    }
}

/// Which half of the tank a unit is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TankHalf {
    /// The harmonic tremolo: its switch, its lamp, INTENSITY, SPEED.
    Tremolo,
    /// The spring reverb: MIX, TONE, DECAY, its switch.
    Spring,
}

/// One half of the tank as its own unit (so a preset can load the tremolo
/// without the spring, or another reverb beside it): the navy case, the
/// copper-framed mesh across the top, the half's controls as big as the
/// height allows, its name bracketed along the bottom. `w × h`.
#[must_use]
pub fn tank_unit(t: &Tank, half: TankHalf, file: &'static str, w: f64, h: f64) -> Face {
    let scene = canvas(
        if half == TankHalf::Tremolo {
            "Harmonic Tremolo"
        } else {
            "Spring Reverb"
        },
        |b, root| {
            ui::fit_root(b, root, w, h);
            let mut case = draw::frame("Case", 0.0, 0.0, w, h);
            case.corner_radii = CornerRadii::uniform(18.0);
            case.stroke = document_proto::Stroke {
                paints: vec![draw::linear(
                    90.0,
                    &[(0.0, "#a8664a"), (0.5, "#5a3a34"), (1.0, "#a8664a")],
                )],
                weight: 3.0,
                ..document_proto::Stroke::default()
            };
            let case = b.add(root, case);
            paint::fill(
                b,
                case,
                (0.0, 0.0, w, h),
                18.0,
                &paint::Material::Powder {
                    color: "#30334a".into(),
                    coarse: false,
                },
            );
            for e in light::edge("case edge", (0.0, 0.0, w, h), 18.0, 5.0, 1.1, true) {
                b.add(case, e);
            }
            let (mx, my, mw, mh) = (20.0, 18.0, w - 40.0, h * 0.3);
            b.add(
                case,
                draw::with_effects(
                    draw::rounded(
                        draw::rect(
                            "surround",
                            mx - 4.0,
                            my - 4.0,
                            mw + 8.0,
                            mh + 8.0,
                            draw::linear(
                                180.0,
                                &[(0.0, "#f2c7a6"), (0.5, "#c98a66"), (1.0, "#f0c09c")],
                            ),
                        ),
                        14.0,
                    ),
                    vec![draw::drop_shadow("#00000080", 0.0, 2.0, 4.0)],
                ),
            );
            let mesh = Place {
                options: Some(vec![format!("w={mw}"), format!("h={mh}"), "r=11".into()]),
                ..Place::new("Mesh").variant("mesh")
            };
            let m = ui::put_at(b, case, "panel.amp_grille", &mesh, mx, my);
            if let Some(n) = b.scene.node_mut(m) {
                n.effects.clear();
            }
            for e in light::edge("mesh recess", (mx, my, mw, mh), 11.0, 4.0, 1.2, false) {
                b.add(case, e);
            }
            let d = (h * 0.34).min(w * 0.26);
            let section_y = h - 16.0;
            let bracket_y = section_y - 16.0;
            let label_y = bracket_y - 16.0;
            let row = label_y - 14.0 - d * 0.5;
            let switch = |b: &mut Builder<'_>, addr: &str, name: &str, x: f64| {
                print(b, case, "ON", x, row - 30.0, 11.0, INK, 500.0, 0.08);
                toggle(b, case, addr, name, 22.0, x, row);
                print(b, case, "OFF", x, row + 30.0, 11.0, INK, 500.0, 0.08);
            };
            match half {
                TankHalf::Tremolo => {
                    switch(b, t.trem_on, "Tremolo", w * 0.08);
                    // The lamp pulses at the speed, as deep as the intensity,
                    // dark while the tremolo is off.
                    let lamp = ui::stream_lamp(
                        b,
                        case,
                        "Tremolo lamp",
                        t.lfo,
                        "",
                        w * 0.2,
                        row,
                        22.0,
                        "#3cf0a0",
                        INK,
                    );
                    let glow = b
                        .controls
                        .get(&lamp)
                        .and_then(|c| c.slots_of(SlotRole::Glow).next())
                        .map(NodeId);
                    if let Some(glow) = glow {
                        motion::add(b, glow, idle_lfo(t));
                        let pulse = format!(
                            "or(scalar({lfo}), lerp(1, motion(lfo), param({depth}))) * param({on})",
                            lfo = t.lfo,
                            depth = t.intensity,
                            on = t.trem_on
                        );
                        motion::add(
                            b,
                            glow,
                            Motion::new(Target::Opacity)
                                .input(pulse)
                                .dynamics(Dynamics::spring(900.0, 50.0)),
                        );
                    }
                    for (addr, name, fx) in
                        [(t.intensity, "INTENSITY", 0.47), (t.speed, "SPEED", 0.77)]
                    {
                        knob(b, case, Cap::Cream, addr, name, d, w * fx, row);
                        print(b, case, name, w * fx, label_y, 11.0, INK, 500.0, 0.12);
                    }
                }
                TankHalf::Spring => {
                    for (addr, name, fx) in [
                        (t.mix, "MIX", 0.17),
                        (t.tone, "TONE", 0.43),
                        (t.decay, "DECAY", 0.69),
                    ] {
                        knob(b, case, Cap::Cream, addr, name, d, w * fx, row);
                        print(b, case, name, w * fx, label_y, 11.0, INK, 500.0, 0.12);
                    }
                    switch(b, t.verb_on, "Reverb", w * 0.9);
                }
            }
            bracket_up(
                b,
                case,
                w * 0.05,
                w * 0.95,
                bracket_y,
                &[w * 0.05, w * 0.95],
            );
            let name = if half == TankHalf::Tremolo {
                "Harmonic Tremolo"
            } else {
                "Spring Reverb"
            };
            print(b, case, name, w * 0.5, section_y, 13.0, INK, 500.0, 0.03);
        },
    );
    let full = spring_tank_as(t);
    let mine = |a: &str| match half {
        TankHalf::Tremolo => [t.trem_on, t.intensity, t.speed, t.division, t.sync].contains(&a),
        TankHalf::Spring => [t.mix, t.tone, t.decay, t.verb_on].contains(&a),
    };
    let params = full
        .params
        .into_iter()
        .filter(|p| mine(&p.addr.to_string()))
        .collect();
    let streams = if half == TankHalf::Tremolo {
        full.streams
    } else {
        Vec::new()
    };
    Face {
        file,
        plugin: t.plugin,
        scene,
        params,
        streams,
    }
}

/// The tremolo lamp's own LFO, for when no engine sends one: at the rate
/// knob's speed, and only while the tremolo is on. Its input is the on
/// switch, so an LFO that is off is still — it doesn't keep the face (and
/// the window around it) redrawing for a lamp that is dark.
fn idle_lfo(t: &Tank) -> Motion {
    Motion::new(Target::None)
        .named("lfo")
        .input(format!("param({})", t.trem_on))
        .dynamics(Dynamics::Lfo {
            rate: Num::Expr(format!("param({})", t.speed)),
            shape: Wave::Sine,
            phase: 0.0,
        })
}
