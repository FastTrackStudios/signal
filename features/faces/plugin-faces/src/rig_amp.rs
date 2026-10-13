//! The Amp row's utilities as faces: the noise gate and the patch's level
//! and pan.
//!
//! - Gate (`51-gate`): the input's level, live, rising in a meter; the
//!   threshold a handle across it you drag; below the threshold the
//!   meter is shaded shut, above it the gate is open — and the OPEN / SHUT
//!   lamps say which, as you play. Attack and release under it. The live
//!   level is `gate/in` (dBFS), a value the host feeds.
//! - Level (`53-trim`): the patch's pan knob over a long-throw console
//!   fader for its level, on a dB scale.

use document_proto::motion::{Dynamics, Motion, Target};
use document_proto::{NodeId, TextAlign};
use surface_kit::{draw, motion};
use surface_proto::{Control, ControlKind, Orientation, Range, SlotRole, Unit};
use surface_widgets::Builder;

use crate::face::{Face, canvas};
use crate::reverb::ui;
use crate::rig_faces::PLUGIN;
use crate::rig_time::kit;

pub const GATE: (f64, f64) = (250.0, 392.0);
pub const TRIM: (f64, f64) = (92.0, 340.0);

const INK: &str = "#e6e9ee";
const DIM: &str = "#7c828c";
const OPEN: &str = "#4ade80";
const SHUT: &str = "#f87171";

/// The panel both sit on: dark, square, flush.
fn panel(b: &mut Builder<'_>, root: NodeId, (w, h): (f64, f64)) -> NodeId {
    let mut p = draw::frame("Panel", 0.0, 0.0, w, h);
    p.fills = vec![draw::linear(180.0, &[(0.0, "#131518"), (1.0, "#0a0b0d")])];
    b.add(
        root,
        draw::with_effects(p, vec![draw::inner_shadow("#ffffff12", 0.0, 1.0, 0.0)]),
    )
}

/// A vertical fader on `addr`: a slot `len` long at (`cx`, `y`), its cap
/// `cap` tall and `cap_w` wide. Returns the control's root.
#[allow(clippy::too_many_arguments)]
pub(crate) fn fader(
    b: &mut Builder<'_>,
    parent: NodeId,
    addr: &str,
    (cx, y): (f64, f64),
    len: f64,
    (cap_w, cap): (f64, f64),
    draw_cap: impl FnOnce(&mut Builder<'_>, NodeId, f64, f64),
) -> NodeId {
    let bw = cap_w + 8.0;
    let root = b.add(
        parent,
        draw::frame(&format!("{addr} fader"), cx - bw / 2.0, y, bw, len),
    );
    let lane = b.add(
        root,
        draw::frame("lane", (bw - cap_w) / 2.0, 0.0, cap_w, len),
    );
    let c = b.add(lane, draw::frame("cap", 0.0, 0.0, cap_w, cap));
    draw_cap(b, c, cap_w, cap);
    let ctl = Control::new(ControlKind::Fader {
        orientation: Orientation::Vertical,
    })
    .slot(SlotRole::Thumb, c.0)
    .bound(ui::bind(addr));
    b.controls.insert(root, ctl);
    root
}

/// A lit word, its brightness following `input` (0…1).
fn lamp(
    b: &mut Builder<'_>,
    parent: NodeId,
    word: &str,
    (cx, cy): (f64, f64),
    color: &str,
    input: String,
) {
    let w = 46.0;
    let g = b.add(parent, draw::frame(word, cx - w / 2.0, cy - 9.0, w, 18.0));
    b.add(
        g,
        draw::stroked(
            draw::rounded(
                draw::rect("bezel", 0.0, 0.0, w, 18.0, draw::solid("#0c0d0f")),
                4.0,
            ),
            "#2a2d33",
            1.0,
        ),
    );
    let lit = b.add(g, draw::frame("lit", 0.0, 0.0, w, 18.0));
    b.add(
        lit,
        draw::emissive(
            draw::rounded(
                draw::rect(
                    "glow",
                    1.0,
                    1.0,
                    w - 2.0,
                    16.0,
                    draw::solid(&ui::alpha(color, 0.25)),
                ),
                3.0,
            ),
            draw::hex(color),
            0.8,
            8.0,
        ),
    );
    b.add(
        lit,
        ui::text(
            "word",
            word,
            0.0,
            9.0,
            w,
            8.0,
            color,
            TextAlign::Center,
            800.0,
            true,
        ),
    );
    motion::add(
        b,
        lit,
        Motion::new(Target::Opacity)
            .input(input)
            .range(0.08, 1.0)
            .dynamics(Dynamics::spring(400.0, 36.0)),
    );
}

fn gate() -> Face {
    let (w, h) = GATE;
    let scene = canvas("Gate", |b, root| {
        ui::fit_root(b, root, w, h);
        let p = panel(b, root, GATE);
        let sans = draw::Lettering::family(draw::family::INTER).weight(600.0);
        crate::suite_studio_blues::parts::set(b, p, "GATE", 42.0, 24.0, 16.0, INK, &sans, 0.16);
        kit::bypass(b, p, "gate", w - 30.0, 24.0, 24.0, "green", "On");

        // Open: the input above the threshold (a dB of hysteresis to it).
        let open = "clamp((param(gate/in) - param(gate/threshold)) * 0.7 + 0.3, 0, 1) * param(gate/on) + (1 - param(gate/on))".to_string();
        lamp(b, p, "OPEN", (w * 0.3, 54.0), OPEN, open.clone());
        lamp(b, p, "SHUT", (w * 0.7, 54.0), SHUT, format!("1 - ({open})"));

        // ── The meter ──────────────────────────────────────────────────
        let (mx, my, mw, mh) = (54.0, 78.0, 58.0, h - 108.0);
        // The dB scale beside it.
        for (db, label) in [
            (0.0, "0"),
            (-18.0, "-18"),
            (-36.0, "-36"),
            (-54.0, "-54"),
            (-72.0, "-72"),
            (-90.0, "-90"),
        ] {
            let y = my + mh * (db / -90.0);
            b.add(
                p,
                ui::strokes(
                    "tick",
                    &[vec![(mx - 6.0, y), (mx - 1.0, y)]],
                    1.0,
                    draw::solid("#ffffff40"),
                ),
            );
            b.add(
                p,
                ui::text(
                    "db",
                    label,
                    mx - 44.0,
                    y,
                    36.0,
                    7.0,
                    DIM,
                    TextAlign::Right,
                    700.0,
                    false,
                ),
            );
        }
        let mut track = draw::frame("Meter", mx, my, mw, mh);
        track.fills = vec![draw::linear(
            90.0,
            &[(0.0, "#07080a"), (0.5, "#111317"), (1.0, "#07080a")],
        )];
        track.clips_content = true;
        track.corner_radii = document_proto::CornerRadii::uniform(3.0);
        track.effects = vec![draw::inner_shadow("#000000", 0.0, 2.0, 4.0)];
        let track = b.add(p, track);
        // The level, live: green where it clears the threshold.
        let bar = b.add(track, draw::frame("level", 8.0, 0.0, mw - 16.0, mh));
        b.add(
            bar,
            draw::emissive(
                draw::rect(
                    "bar",
                    0.0,
                    0.0,
                    mw - 16.0,
                    mh,
                    draw::linear(
                        180.0,
                        &[(0.0, "#fde047"), (0.18, "#86efac"), (1.0, "#16a34a")],
                    ),
                ),
                draw::hex(OPEN),
                0.5,
                6.0,
            ),
        );
        motion::add(
            b,
            bar,
            Motion::new(Target::StretchY)
                .input("clamp((param(gate/in) + 90) / 90, 0, 1)".to_string())
                .range(0.0, 1.0)
                .pivot(0.5, 1.0)
                .dynamics(Dynamics::spring(500.0, 40.0)),
        );
        // Below the threshold: shut, shaded over whatever plays there.
        let shade = b.add(track, draw::frame("shut", 0.0, 0.0, mw, mh));
        b.add(
            shade,
            draw::rect(
                "shade",
                0.0,
                0.0,
                mw,
                mh,
                draw::linear(180.0, &[(0.0, "#1a0b0dcc"), (1.0, "#0b0506e6")]),
            ),
        );
        let hatch: Vec<Vec<(f64, f64)>> = (0..40)
            .map(|i| {
                let y = f64::from(i) * 9.0;
                vec![(0.0, y), (mw, y - mw * 0.6)]
            })
            .collect();
        b.add(
            shade,
            ui::strokes("hatch", &hatch, 1.0, draw::solid("#f8717114")),
        );
        motion::add(
            b,
            shade,
            Motion::new(Target::StretchY)
                .input("clamp((param(gate/threshold) + 90) / 90, 0, 1)".to_string())
                .range(0.0, 1.0)
                .pivot(0.5, 1.0)
                .dynamics(Dynamics::spring(400.0, 36.0)),
        );
        // The threshold: a handle across the meter, dragged up and down.
        let cap_h = 20.0;
        fader(
            b,
            p,
            "gate/threshold",
            (mx + mw / 2.0, my - cap_h / 2.0),
            mh + cap_h,
            (mw + 30.0, cap_h),
            |b, c, cw, ch| {
                b.add(
                    c,
                    draw::rect(
                        "line",
                        0.0,
                        ch / 2.0 - 0.75,
                        cw,
                        1.5,
                        draw::solid("#fde68a"),
                    ),
                );
                let tab = draw::rounded(
                    draw::rect(
                        "tab",
                        cw - 30.0,
                        2.0,
                        30.0,
                        ch - 4.0,
                        draw::linear(180.0, &[(0.0, "#fef3c7"), (1.0, "#fbbf24")]),
                    ),
                    4.0,
                );
                b.add(
                    c,
                    draw::with_effects(tab, vec![draw::drop_shadow("#000000a0", 0.0, 1.5, 3.0)]),
                );
                b.add(
                    c,
                    ui::text(
                        "thr",
                        "THR",
                        cw - 30.0,
                        ch / 2.0,
                        30.0,
                        7.0,
                        "#3a2a06",
                        TextAlign::Center,
                        800.0,
                        true,
                    ),
                );
            },
        );

        // ── Threshold, attack, release ─────────────────────────────────
        let rx = 196.0;
        ui::silk(b, p, "Threshold", rx, 92.0, 6.5, DIM, 800.0);
        crate::delay::parts::readout(
            b,
            p,
            "Threshold value",
            ui::bind("gate/threshold"),
            (rx - 40.0, 108.0, 80.0),
            11.0,
            "#fde68a",
            TextAlign::Center,
        );
        let knob = [
            "color=#1b1e23",
            "pointer=#86efac",
            "index=line",
            "index_w=0.1",
            "skirt=plain",
            "skirt_color=#0a0b0d",
        ];
        kit::knob3d(
            b,
            p,
            "gate/attack",
            "Attack",
            36.0,
            (rx, 170.0),
            &knob,
            INK,
            DIM,
        );
        kit::knob3d(
            b,
            p,
            "gate/release",
            "Release",
            36.0,
            (rx, 262.0),
            &knob,
            INK,
            DIM,
        );
        ui::silk(
            b,
            p,
            "Shut below · open above",
            w / 2.0,
            h - 14.0,
            6.0,
            DIM,
            700.0,
        );
        ui::flush(b, root, w, h);
    });
    let params = vec![
        ui::param(
            "gate/threshold",
            "Threshold",
            Range::linear(-90.0, 0.0, Unit::Db),
            -50.0,
        )
        .featured(),
        ui::param(
            "gate/attack",
            "Attack",
            Range::log(0.1, 50.0, Unit::Milliseconds),
            1.0,
        ),
        ui::param(
            "gate/release",
            "Release",
            Range::log(5.0, 500.0, Unit::Milliseconds),
            120.0,
        ),
        ui::param(
            "gate/in",
            "Input",
            Range::linear(-90.0, 0.0, Unit::Db),
            -90.0,
        ),
        ui::toggle("gate/on", "Gate", true).featured(),
    ];
    Face {
        file: "51-gate",
        plugin: PLUGIN,
        scene,
        params,
        streams: surface_kit::viz::catalog::comp_streams("gate", 240),
    }
}

fn trim() -> Face {
    let (w, h) = TRIM;
    let scene = canvas("Level", |b, root| {
        ui::fit_root(b, root, w, h);
        let p = panel(b, root, TRIM);
        let sans = draw::Lettering::family(draw::family::INTER).weight(600.0);
        crate::suite_studio_blues::parts::set(b, p, "LEVEL", w / 2.0, 20.0, 12.0, INK, &sans, 0.16);
        // Pan.
        let knob = [
            "color=#1b1e23",
            "pointer=#e6e9ee",
            "index=line",
            "index_w=0.1",
            "skirt=plain",
            "skirt_color=#0a0b0d",
        ];
        let k = kit::knob3d(
            b,
            p,
            "trim/pan",
            "Pan",
            34.0,
            (w / 2.0, 58.0),
            &knob,
            INK,
            DIM,
        );
        let _ = k;
        ui::silk(b, p, "L", w / 2.0 - 26.0, 72.0, 6.0, DIM, 800.0);
        ui::silk(b, p, "R", w / 2.0 + 26.0, 72.0, 6.0, DIM, 800.0);
        // The level: a long-throw fader on a dB scale, +24 at the top.
        let (fy, flen) = (122.0, h - 150.0);
        let cap_h = 30.0;
        let travel = flen - cap_h;
        for (db, label) in [
            (24.0, "+24"),
            (12.0, "+12"),
            (6.0, "+6"),
            (0.0, "0"),
            (-6.0, "-6"),
            (-12.0, "-12"),
            (-24.0, "-24"),
        ] {
            let y = fy + cap_h / 2.0 + travel * (24.0 - db) / 48.0;
            let col = if db == 0.0 { INK } else { "#ffffff38" };
            b.add(
                p,
                ui::strokes(
                    "tick",
                    &[vec![(w / 2.0 + 12.0, y), (w / 2.0 + 18.0, y)]],
                    1.0,
                    draw::solid(col),
                ),
            );
            b.add(
                p,
                ui::text(
                    "db",
                    label,
                    w / 2.0 + 20.0,
                    y,
                    24.0,
                    6.0,
                    if db == 0.0 { INK } else { DIM },
                    TextAlign::Left,
                    700.0,
                    false,
                ),
            );
        }
        // The slot.
        let slot = draw::rounded(
            draw::rect(
                "slot",
                w / 2.0 - 2.5,
                fy + cap_h / 2.0,
                5.0,
                travel,
                draw::solid("#050506"),
            ),
            2.5,
        );
        b.add(
            p,
            draw::with_effects(
                slot,
                vec![
                    draw::inner_shadow("#000000", 0.0, 1.0, 2.0),
                    draw::drop_shadow("#ffffff14", 0.0, 1.0, 0.0),
                ],
            ),
        );
        fader(
            b,
            p,
            "trim/gain_db",
            (w / 2.0, fy),
            flen,
            (22.0, cap_h),
            |b, c, cw, ch| {
                let body = draw::rounded(
                    draw::rect(
                        "cap",
                        0.0,
                        0.0,
                        cw,
                        ch,
                        draw::linear(
                            180.0,
                            &[
                                (0.0, "#5b5f66"),
                                (0.45, "#2c2f34"),
                                (0.55, "#1c1e22"),
                                (1.0, "#3b3e44"),
                            ],
                        ),
                    ),
                    3.0,
                );
                b.add(
                    c,
                    draw::with_effects(body, vec![draw::drop_shadow("#000000c0", 0.0, 3.0, 5.0)]),
                );
                b.add(
                    c,
                    draw::rect(
                        "line",
                        1.0,
                        ch / 2.0 - 0.75,
                        cw - 2.0,
                        1.5,
                        draw::solid("#f4f4f5"),
                    ),
                );
            },
        );
        crate::delay::parts::readout(
            b,
            p,
            "Level value",
            ui::bind("trim/gain_db"),
            (4.0, h - 14.0, w - 8.0),
            8.0,
            INK,
            TextAlign::Center,
        );
        ui::flush(b, root, w, h);
    });
    let params = vec![
        ui::param(
            "trim/gain_db",
            "Level",
            Range::linear(-24.0, 24.0, Unit::Db),
            0.0,
        )
        .featured(),
        ui::param("trim/pan", "Pan", Range::linear(-1.0, 1.0, Unit::None), 0.0).featured(),
    ];
    Face {
        file: "53-trim",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

/// The gate's type keys — GATE | EXPANDER, each `sw` × `sh` from (`sx`,
/// `sy`), lettered at `size` — and OPEN / SHUT lit beside them as you play.
fn gate_header(b: &mut Builder<'_>, p: NodeId, (sx, sy, sw, sh): (f64, f64, f64, f64), size: f64) {
    use surface_proto::ButtonMode;
    // GATE | EXPANDER: the type, over the meter at the left.
    for (i, word) in ["Gate", "Expander"].iter().enumerate() {
        let cell = b.add(
            p,
            draw::frame(
                &format!("gate/mode {word}"),
                sx + (sw + 4.0) * i as f64,
                sy,
                sw,
                sh,
            ),
        );
        b.add(
            cell,
            draw::stroked(
                draw::rounded(
                    draw::rect("well", 0.0, 0.0, sw, sh, draw::solid("#0c0d0f")),
                    8.0,
                ),
                "#2a2d33",
                1.0,
            ),
        );
        let lit = b.add(
            cell,
            draw::emissive(
                draw::rounded(
                    draw::rect(
                        "lit",
                        1.0,
                        1.0,
                        sw - 2.0,
                        sh - 2.0,
                        draw::solid(&ui::alpha(OPEN, 0.28)),
                    ),
                    7.0,
                ),
                draw::hex(OPEN),
                0.5,
                6.0,
            ),
        );
        let t = b.add(
            cell,
            ui::text(
                "legend",
                word,
                0.0,
                sh / 2.0,
                sw,
                size,
                INK,
                TextAlign::Center,
                800.0,
                true,
            ),
        );
        let mut c = Control::new(ControlKind::Button {
            mode: ButtonMode::Select {
                index: i as u32,
                count: 2,
            },
        })
        .slot(SlotRole::Led, lit.0)
        .slot(SlotRole::Label, t.0);
        c.binding = Some(ui::bind("gate/mode"));
        b.controls.insert(cell, c);
    }
    // OPEN and SHUT, lit as you play.
    let open = "clamp((param(gate/in) - param(gate/threshold)) * 0.7 + 0.3, 0, 1) * param(gate/on) + (1 - param(gate/on))".to_string();
    lamp(
        b,
        p,
        "OPEN",
        (sx + 2.0 * sw + 8.0 + 34.0, sy + sh / 2.0),
        OPEN,
        open.clone(),
    );
    lamp(
        b,
        p,
        "SHUT",
        (sx + 2.0 * sw + 8.0 + 86.0, sy + sh / 2.0),
        SHUT,
        format!("1 - ({open})"),
    );
}

/// The gate's meter in a window (`vx`, `vy`, `vw`, `vh`): the input's
/// level across it, shaded shut under the threshold, the threshold a handle
/// along it. Returns the window (it takes the width).
fn gate_meter(b: &mut Builder<'_>, p: NodeId, (vx, vy, vw, vh): (f64, f64, f64, f64)) -> NodeId {
    use document_proto::ResizeX;
    // The meter window.
    let mut v = draw::frame("Meter window", vx, vy, vw, vh);
    v.fills = vec![draw::linear(180.0, &[(0.0, "#0b0c0e"), (1.0, "#060708")])];
    v.clips_content = true;
    v.corner_radii = document_proto::CornerRadii::uniform(8.0);
    v.effects = vec![draw::inner_shadow("#000000c0", 0.0, 2.0, 6.0)];
    let v = b.add(p, v);
    let (mx, my, mw, mh) = (14.0, 30.0, vw - 28.0, vh - 44.0);
    let stretched = |mut n: document_proto::Node| {
        n.layout_child.resize_x = Some(ResizeX::Stretch);
        n
    };
    let mut track = draw::frame("Meter", mx, my, mw, mh);
    track.fills = vec![draw::linear(
        180.0,
        &[(0.0, "#07080a"), (0.5, "#111317"), (1.0, "#07080a")],
    )];
    track.clips_content = true;
    track.corner_radii = document_proto::CornerRadii::uniform(5.0);
    track.effects = vec![draw::inner_shadow("#000000", 0.0, 2.0, 4.0)];
    let track = b.add(v, stretched(track));
    // The level, live, rising to the right.
    let bar = b.add(
        track,
        stretched(draw::frame("level", 0.0, 6.0, mw, mh - 12.0)),
    );
    b.add(
        bar,
        stretched(draw::emissive(
            draw::rect(
                "bar",
                0.0,
                0.0,
                mw,
                mh - 12.0,
                draw::linear(
                    90.0,
                    &[(0.0, "#16a34a"), (0.82, "#86efac"), (1.0, "#fde047")],
                ),
            ),
            draw::hex(OPEN),
            0.5,
            6.0,
        )),
    );
    motion::add(
        b,
        bar,
        Motion::new(Target::StretchX)
            .input("clamp((param(gate/in) + 90) / 90, 0, 1)".to_string())
            .range(0.0, 1.0)
            .pivot(0.0, 0.5)
            .dynamics(Dynamics::spring(500.0, 40.0)),
    );
    // Under the threshold: shut — hatched as a gate, a fade as an
    // expander (its level falls away, not off).
    let shade = b.add(track, stretched(draw::frame("shut", 0.0, 0.0, mw, mh)));
    let hard = b.add(shade, stretched(draw::frame("hard", 0.0, 0.0, mw, mh)));
    b.add(
        hard,
        stretched(draw::rect(
            "shade",
            0.0,
            0.0,
            mw,
            mh,
            draw::linear(90.0, &[(0.0, "#0b0506ee"), (1.0, "#1a0b0dd0")]),
        )),
    );
    let hatch: Vec<Vec<(f64, f64)>> = (0..((mw * 1.6 / 10.0) as i32))
        .map(|i| {
            let x = f64::from(i) * 10.0;
            vec![(x, mh), (x + mh * 0.6, 0.0)]
        })
        .collect();
    b.add(
        hard,
        ui::strokes("hatch", &hatch, 1.0, draw::solid("#f8717120")),
    );
    motion::add(
        b,
        hard,
        Motion::new(Target::Opacity)
            .input("1 - param(gate/mode)".to_string())
            .range(0.0, 1.0),
    );
    let soft = b.add(
        shade,
        stretched(draw::rect(
            "soft",
            0.0,
            0.0,
            mw,
            mh,
            draw::linear(
                90.0,
                &[(0.0, "#0b0506f0"), (0.7, "#1a0b0d90"), (1.0, "#1a0b0d00")],
            ),
        )),
    );
    motion::add(
        b,
        soft,
        Motion::new(Target::Opacity)
            .input("param(gate/mode)".to_string())
            .range(0.0, 1.0),
    );
    motion::add(
        b,
        shade,
        Motion::new(Target::StretchX)
            .input("clamp((param(gate/threshold) + 90) / 90, 0, 1)".to_string())
            .range(0.0, 1.0)
            .pivot(0.0, 0.5)
            .dynamics(Dynamics::spring(400.0, 36.0)),
    );
    // The threshold: a handle along the meter, the whole window
    // tall to grab, its dB on its tab.
    let (cap_w, cap_h) = (64.0, vh);
    let froot = b.add(
        v,
        stretched(draw::frame(
            "gate/threshold fader",
            mx - cap_w / 2.0,
            0.0,
            mw + cap_w,
            cap_h,
        )),
    );
    let lane = b.add(
        froot,
        stretched(draw::frame("lane", 0.0, 0.0, mw + cap_w, cap_h)),
    );
    let c = b.add(lane, draw::frame("cap", 0.0, 0.0, cap_w, cap_h));
    b.add(
        c,
        draw::with_effects(
            draw::rect(
                "line",
                cap_w / 2.0 - 1.25,
                22.0,
                2.5,
                cap_h - 26.0,
                draw::solid("#fde68a"),
            ),
            vec![draw::drop_shadow("#fbbf2480", 0.0, 0.0, 4.0)],
        ),
    );
    let tab = draw::rounded(
        draw::rect(
            "tab",
            2.0,
            2.0,
            cap_w - 4.0,
            22.0,
            draw::linear(180.0, &[(0.0, "#fef3c7"), (1.0, "#fbbf24")]),
        ),
        6.0,
    );
    b.add(
        c,
        draw::with_effects(tab, vec![draw::drop_shadow("#000000a0", 0.0, 1.5, 3.0)]),
    );
    crate::delay::parts::readout(
        b,
        c,
        "Threshold value",
        ui::bind("gate/threshold"),
        (2.0, 13.0, cap_w - 4.0),
        11.0,
        "#3a2a06",
        TextAlign::Center,
    );
    let ctl = Control::new(ControlKind::Fader {
        orientation: Orientation::Horizontal,
    })
    .slot(SlotRole::Thumb, c.0)
    .bound(ui::bind("gate/threshold"));
    b.controls.insert(froot, ctl);
    ui::silk_left(
        b,
        v,
        "Shut below · open above",
        14.0,
        vh - 8.0,
        7.0,
        DIM,
        700.0,
    );
    v
}

/// The gate's pane size on the desktop: a cell of its grid (a MacBook's,
/// the module sidebar open, the patch's level strip beside it).
pub const GATE_PANE: (f64, f64) = (330.0, 200.0);

/// The gate in a desktop pane: the phone lane's parts stacked for a squarer
/// room — the nameplate along the top, GATE | EXPANDER and OPEN / SHUT
/// under it, the meter the rest, the bypass and More down the right.
/// `back`: Attack, Release, Hold, Hysteresis, Range across it.
#[must_use]
pub fn gate_pane(back: bool) -> Face {
    let (w, h) = GATE_PANE;
    let knob = [
        "color=#1b1e23",
        "pointer=#86efac",
        "index=line",
        "index_w=0.1",
        "skirt=plain",
        "skirt_color=#0a0b0d",
    ];
    let scene = canvas("Gate", |b, root| {
        ui::fit_root(b, root, w, h);
        let p = panel(b, root, (w, h));
        // The right column: the bypass, More under it.
        let col_x = w - 62.0;
        if back {
            ui::silk_left(b, p, "Advanced", 14.0, 52.0, 9.0, INK, 800.0);
            let ks = [
                ("attack", "Attack"),
                ("release", "Release"),
                ("hold", "Hold"),
                ("hysteresis", "Hyst"),
                ("range", "Range"),
            ];
            let (x0, x1) = (34.0, col_x - 30.0);
            let step = (x1 - x0) / (ks.len() - 1) as f64;
            for (i, (id, legend)) in ks.iter().enumerate() {
                kit::knob3d(
                    b,
                    p,
                    &format!("gate/{id}"),
                    legend,
                    40.0,
                    (x0 + step * i as f64, 118.0),
                    &knob,
                    INK,
                    DIM,
                );
            }
        } else {
            gate_header(b, p, (12.0, 42.0, 70.0, 26.0), 10.0);
            let (vx, vy) = (12.0, 76.0);
            let v = gate_meter(b, p, (vx, vy, col_x - 10.0 - vx, h - vy - 10.0));
            kit::responsive(b, p, &[v]);
        }
        let head = draw::Lettering::family(draw::family::INTER).weight(700.0);
        kit::nameplate_right(
            b,
            p,
            "gate",
            w,
            (8.0, 26.0),
            80.0,
            150.0,
            11.0,
            (OPEN, DIM),
            |b, t| {
                crate::suite_studio_blues::parts::set(
                    b, t, "GATE", 34.0, 13.0, 14.0, INK, &head, 0.2,
                );
            },
        );
        kit::bypass(b, p, "gate", col_x + 28.0, 104.0, 40.0, "green", "On");
        kit::flip_arrow_at(
            b,
            p,
            "gate",
            (col_x + 4.0, 136.0, 48.0, 40.0),
            back,
            8.0,
            (INK, "#ffffff14"),
        );
        ui::flush(b, root, w, h);
    });
    let mut f = gate_phone(back);
    f.scene = scene;
    f.file = if back {
        "51-gate-pane-back"
    } else {
        "51-gate-pane"
    };
    f
}

/// Both faces.
#[must_use]
/// The FX row's box on an iPad: the gate made for it.
pub const ROW: (f64, f64) = (860.0, 254.0);

/// The gate for the iPad's FX row: its picture across most of the row —
/// the level scrolling past the threshold, the attenuation hanging from the
/// top while it is shut, the threshold a line to drag — and Attack and
/// Release at a finger's size beside it.
#[must_use]
pub fn gate_row() -> Face {
    let (w, h) = ROW;
    let side = 200.0;
    let scene = canvas("Gate", move |b, root| {
        ui::fit_root(b, root, w, h);
        let mut p = draw::frame("Panel", 0.0, 0.0, w, h);
        p.fills = vec![draw::linear(180.0, &[(0.0, "#14181e"), (1.0, "#0c0f13")])];
        let p = b.add(root, p);
        let place = surface_kit::Place::new("Gate").size(w - side).options(&[
            "ns=gate",
            &format!("h={h}"),
            "len=240",
        ]);
        ui::put_at(b, p, "viz.gate", &place, 0.0, 0.0);
        let knob = [
            "color=#1b1e23",
            "pointer=#86efac",
            "index=line",
            "index_w=0.1",
            "skirt=plain",
            "skirt_color=#0a0b0d",
        ];
        let cx = w - side / 2.0;
        kit::knob3d(
            b,
            p,
            "gate/attack",
            "Attack",
            52.0,
            (cx - 46.0, h / 2.0 + 6.0),
            &knob,
            INK,
            DIM,
        );
        kit::knob3d(
            b,
            p,
            "gate/release",
            "Release",
            52.0,
            (cx + 46.0, h / 2.0 + 6.0),
            &knob,
            INK,
            DIM,
        );
    });
    let params = vec![
        ui::param(
            "gate/threshold",
            "Threshold",
            Range::linear(-90.0, 0.0, Unit::Db),
            -50.0,
        )
        .featured(),
        ui::param(
            "gate/attack",
            "Attack",
            Range::log(0.1, 50.0, Unit::Milliseconds),
            1.0,
        ),
        ui::param(
            "gate/release",
            "Release",
            Range::log(5.0, 500.0, Unit::Milliseconds),
            120.0,
        ),
        ui::toggle("gate/on", "Gate", true).featured(),
    ];
    Face {
        file: "51-gate-row",
        plugin: PLUGIN,
        scene,
        params,
        streams: surface_kit::viz::catalog::comp_streams("gate", 240),
    }
}

/// The patch's level for the iPad's FX row: its dB large at the left, a
/// long-throw fader across the row on a dB scale, and the pan dial.
#[must_use]
pub fn trim_row() -> Face {
    let (w, h) = ROW;
    let scene = canvas("Level", move |b, root| {
        ui::fit_root(b, root, w, h);
        let mut p = draw::frame("Panel", 0.0, 0.0, w, h);
        p.fills = vec![draw::linear(180.0, &[(0.0, "#16181c"), (1.0, "#0c0d10")])];
        let p = b.add(root, p);
        let sans = draw::Lettering::family(draw::family::INTER).weight(700.0);
        crate::suite_studio_blues::parts::set(b, p, "LEVEL", 80.0, 40.0, 13.0, DIM, &sans, 0.2);
        crate::delay::parts::readout(
            b,
            p,
            "Level value",
            ui::bind("trim/gain_db"),
            (10.0, h / 2.0 - 4.0, 140.0),
            30.0,
            INK,
            TextAlign::Center,
        );
        // The fader: across, −24 at the left, +24 at the right.
        let (fx, flen, fy) = (170.0, w - 170.0 - 170.0, h / 2.0 - 30.0);
        let (cap_w, cap_h) = (36.0, 60.0);
        let travel = flen - cap_w;
        for (db, label) in [
            (-24.0, "-24"),
            (-12.0, "-12"),
            (-6.0, "-6"),
            (0.0, "0"),
            (6.0, "+6"),
            (12.0, "+12"),
            (24.0, "+24"),
        ] {
            let x = fx + cap_w / 2.0 + travel * (db + 24.0) / 48.0;
            let col = if db == 0.0 { INK } else { "#ffffff38" };
            b.add(
                p,
                ui::strokes(
                    "tick",
                    &[vec![(x, fy + cap_h + 8.0), (x, fy + cap_h + 16.0)]],
                    1.2,
                    draw::solid(col),
                ),
            );
            b.add(
                p,
                ui::text(
                    "db",
                    label,
                    x - 20.0,
                    fy + cap_h + 28.0,
                    40.0,
                    10.0,
                    if db == 0.0 { INK } else { DIM },
                    TextAlign::Center,
                    700.0,
                    false,
                ),
            );
        }
        let slot = draw::rounded(
            draw::rect(
                "slot",
                fx + cap_w / 2.0,
                fy + cap_h / 2.0 - 3.0,
                travel,
                6.0,
                draw::solid("#050506"),
            ),
            3.0,
        );
        b.add(
            p,
            draw::with_effects(
                slot,
                vec![
                    draw::inner_shadow("#000000", 0.0, 1.0, 2.0),
                    draw::drop_shadow("#ffffff14", 0.0, 1.0, 0.0),
                ],
            ),
        );
        let root_f = b.add(p, draw::frame("trim/gain_db fader", fx, fy, flen, cap_h));
        let lane = b.add(root_f, draw::frame("lane", 0.0, 0.0, flen, cap_h));
        let c = b.add(lane, draw::frame("cap", 0.0, 0.0, cap_w, cap_h));
        let body = draw::rounded(
            draw::rect(
                "cap",
                0.0,
                0.0,
                cap_w,
                cap_h,
                draw::linear(
                    90.0,
                    &[
                        (0.0, "#5b5f66"),
                        (0.45, "#2c2f34"),
                        (0.55, "#1c1e22"),
                        (1.0, "#3b3e44"),
                    ],
                ),
            ),
            5.0,
        );
        b.add(
            c,
            draw::with_effects(body, vec![draw::drop_shadow("#000000c0", 0.0, 3.0, 6.0)]),
        );
        b.add(
            c,
            draw::rect(
                "line",
                cap_w / 2.0 - 1.0,
                4.0,
                2.0,
                cap_h - 8.0,
                draw::solid("#f4f4f5"),
            ),
        );
        let ctl = Control::new(ControlKind::Fader {
            orientation: Orientation::Horizontal,
        })
        .slot(SlotRole::Thumb, c.0)
        .bound(ui::bind("trim/gain_db"));
        b.controls.insert(root_f, ctl);
        // Pan.
        let knob = [
            "color=#1b1e23",
            "pointer=#e6e9ee",
            "index=line",
            "index_w=0.1",
            "skirt=plain",
            "skirt_color=#0a0b0d",
        ];
        kit::knob3d(
            b,
            p,
            "trim/pan",
            "Pan",
            56.0,
            (w - 85.0, h / 2.0 - 4.0),
            &knob,
            INK,
            DIM,
        );
    });
    let params = vec![
        ui::param(
            "trim/gain_db",
            "Level",
            Range::linear(-24.0, 24.0, Unit::Db),
            0.0,
        )
        .featured(),
        ui::param("trim/pan", "Pan", Range::linear(-1.0, 1.0, Unit::None), 0.0).featured(),
    ];
    Face {
        file: "53-trim-row",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

pub fn faces() -> Vec<Face> {
    vec![
        gate(),
        gate_phone(false),
        gate_phone(true),
        gate_pane(false),
        gate_pane(true),
        gate_row(),
        trim(),
        trim_row(),
    ]
}

/// The gate as a phone lane: the lane is its meter — the input's level
/// across it, shaded shut under the threshold (hatched as a gate, fading
/// as an expander), the threshold a handle you drag along it, its dB on
/// the handle — with GATE | EXPANDER and OPEN / SHUT over it, the
/// nameplate (the preset ‹ ›, GATE), the bypass and More. `back`: the
/// deeper controls — Attack, Release, Hold, Hysteresis, Range.
#[must_use]
pub fn gate_phone(back: bool) -> Face {
    let tier = kit::Tier::Phone;
    let w = tier.lane_w();
    let h = kit::H;
    let knob = [
        "color=#1b1e23",
        "pointer=#86efac",
        "index=line",
        "index_w=0.1",
        "skirt=plain",
        "skirt_color=#0a0b0d",
    ];
    let scene = canvas("Gate", |b, root| {
        ui::fit_root(b, root, w, h);
        let p = panel(b, root, (w, h));
        let (bx, _, _) = kit::bypass_spot(tier);
        if back {
            kit::back_knobs(
                b,
                p,
                tier,
                &[
                    ("attack", "Attack"),
                    ("release", "Release"),
                    ("hold", "Hold"),
                    ("hysteresis", "Hysteresis"),
                    ("range", "Range"),
                ],
                INK,
                |b, pp, id, legend, d, at| {
                    kit::knob3d(b, pp, &format!("gate/{id}"), legend, d, at, &knob, INK, DIM);
                },
            );
        } else {
            gate_header(b, p, (12.0, 8.0, 92.0, 30.0), 11.0);
            // The meter: the lane's width to the bypass.
            let (vx, vy) = (12.0, 46.0);
            let v = gate_meter(b, p, (vx, vy, bx - 50.0 - vx, h - vy - 10.0));
            kit::responsive(b, p, &[v]);
        }
        let head = draw::Lettering::family(draw::family::INTER).weight(700.0);
        kit::head(b, p, "gate", back, |b, layer| {
            kit::nameplate_top_right(
                b,
                layer,
                "gate",
                tier,
                (8.0, 30.0),
                96.0,
                210.0,
                12.0,
                (OPEN, DIM),
                |b, t| {
                    crate::suite_studio_blues::parts::set(
                        b, t, "GATE", 42.0, 15.0, 16.0, INK, &head, 0.2,
                    );
                },
            );
        });
        kit::face_bypass(b, p, "gate", tier, "green", "On");
        kit::flip_arrow(b, p, "gate", tier, back, (INK, "#ffffff14"));
        ui::flush(b, root, w, h);
    });
    let mut params = vec![
        ui::param(
            "gate/threshold",
            "Threshold",
            Range::linear(-90.0, 0.0, Unit::Db),
            -50.0,
        )
        .featured(),
        ui::param(
            "gate/attack",
            "Attack",
            Range::log(0.1, 50.0, Unit::Milliseconds),
            1.0,
        ),
        ui::param(
            "gate/release",
            "Release",
            Range::log(5.0, 500.0, Unit::Milliseconds),
            120.0,
        ),
        ui::param(
            "gate/hold",
            "Hold",
            Range::linear(0.0, 500.0, Unit::Milliseconds),
            0.0,
        ),
        ui::param(
            "gate/hysteresis",
            "Hysteresis",
            Range::linear(0.0, 12.0, Unit::Db),
            0.0,
        ),
        ui::param(
            "gate/range",
            "Range",
            Range::linear(0.0, 90.0, Unit::Db),
            90.0,
        ),
        ui::choice("gate/mode", "Type", &["Gate", "Expander"], 0),
        ui::param(
            "gate/in",
            "Input",
            Range::linear(-90.0, 0.0, Unit::Db),
            -90.0,
        ),
        ui::toggle("gate/on", "Gate", true).featured(),
    ];
    params.extend(kit::nameplate_params("gate"));
    params.extend(kit::flip_params("gate"));
    Face {
        file: if back {
            "51-gate-phone-back"
        } else {
            "51-gate-phone"
        },
        plugin: PLUGIN,
        scene,
        params,
        streams: surface_kit::viz::catalog::comp_streams("gate", 240),
    }
}
