//! Rhythm: the echo as a pattern — a drum machine's step grid. Two rows
//! of pads, left and right: the note hits both at the first step; the
//! repeats fall one a step, ping-ponging from row to row, each step a
//! delay time wide (the grid's lines move with the time) and each pad
//! dimmer by the feedback; the Pan knob leans the pattern to one row.
//! Under the pads each repeat's velocity, and a cursor stepping along
//! the pattern in time.
//!
//! Two finishes: light, a drum machine's warm grey; dark, its black.

use document_proto::NodeId;
use document_proto::motion::{Motion, Target};
use surface_kit::{draw, motion};
use surface_widgets::Builder;

use super::shell::{self, Device, Look};
use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, DLY};

/// The lit pads.
const PAD: &str = "#8fc0ff";
/// The steps drawn.
const STEPS: u32 = 12;

fn look(dark: bool) -> Look {
    if dark {
        Look {
            ink: "#e9e9e7",
            dim: "#8d8d89",
            panel: [(0.0, "#252526"), (0.6, "#1c1c1d"), (1.0, "#141415")],
            edge: "#38383a",
            well: [(0.0, "#101317"), (1.0, "#0a0c0f")],
            knob: [
                "color=#2b2b2d",
                "pointer=#bcd6ff",
                "index=line",
                "index_w=0.12",
                "skirt=plain",
                "skirt_color=#0c0c0d",
            ],
            wheel: [
                "color=#d9d7d1",
                "pointer=#1e3a8a",
                "index=dot",
                "skirt=plain",
                "skirt_color=#4a4945",
            ],
            accent: kit::BLUE_LIGHT,
            scrim: "#1c1c1de6",
            light: PAD,
            faint: "#9fb3cc",
            metal: false,
        }
    } else {
        Look {
            ink: "#232322",
            dim: "#73726d",
            panel: [(0.0, "#e9e7e1"), (0.6, "#dcd9d1"), (1.0, "#cdc9bf")],
            edge: "#b5b1a6",
            well: [(0.0, "#101317"), (1.0, "#0a0c0f")],
            knob: [
                "color=#2b2b2d",
                "pointer=#dbe8ff",
                "index=line",
                "index_w=0.12",
                "skirt=plain",
                "skirt_color=#0c0c0d",
            ],
            wheel: [
                "color=#f1efe9",
                "pointer=#1e3a8a",
                "index=dot",
                "skirt=plain",
                "skirt_color=#a19d92",
            ],
            accent: kit::BLUE_DEEP,
            scrim: "#dcd9d1e6",
            light: PAD,
            faint: "#9fb3cc",
            metal: false,
        }
    }
}

/// The machine's step strip: a row of little step marks along the bottom
/// of the panel, every fourth bolder, under the picture.
fn dress(b: &mut Builder<'_>, panel: NodeId, (w, h): (f64, f64), l: &Look) {
    if h > kit::H || (w - kit::W).abs() > 1.0 {
        return;
    }
    let marks: Vec<Vec<(f64, f64)>> = (0..32)
        .map(|k| {
            let x = 24.0 + f64::from(k) * 16.6;
            let t = if k % 4 == 0 { 4.0 } else { 2.0 };
            vec![(x, h - 7.0 - t / 2.0), (x, h - 7.0 + t / 2.0)]
        })
        .collect();
    b.add(
        panel,
        ui::bars("steps", &marks, 6.0, &ui::alpha(l.ink, 0.12)),
    );
}

fn picture(b: &mut Builder<'_>, v: NodeId, (w, h): (f64, f64), l: &Look) {
    let small = w < 300.0;
    let lead = if small { 18.0 } else { 26.0 };
    let span = w - lead - 16.0;
    let window = shell::window(span) * 1.2;
    // Clear of the caption — and on a tall face of the nameplate.
    let top = if w > 600.0 { 54.0 } else { 22.0 };
    let vel_h = if w > 600.0 {
        10.0
    } else {
        (h * 0.18).max(14.0)
    };
    let rows_h = h - top - vel_h - 14.0;
    let pad_h = (rows_h / 2.0 - 4.0).min(28.0);
    let pad_w = if small { 9.0 } else { 16.0 };
    let row_y = [top + rows_h / 4.0, top + rows_h * 3.0 / 4.0];
    let vel_base = h - 8.0;
    ui::silk(
        b,
        v,
        "L",
        9.0,
        row_y[0],
        6.0,
        &ui::alpha(l.faint, 0.7),
        800.0,
    );
    ui::silk(
        b,
        v,
        "R",
        9.0,
        row_y[1],
        6.0,
        &ui::alpha(l.faint, 0.7),
        800.0,
    );
    b.add(
        v,
        ui::strokes(
            "velocity floor",
            &[vec![(lead - 4.0, vel_base), (w, vel_base)]],
            0.8,
            draw::solid(&ui::alpha(l.faint, 0.25)),
        ),
    );
    // The cursor, stepping along the pattern a delay time a step.
    let cur = b.add(
        v,
        draw::frame(
            "cursor",
            lead - pad_w / 2.0 - 3.0,
            top - 2.0,
            pad_w + 6.0,
            rows_h + 4.0,
        ),
    );
    b.add(
        cur,
        draw::rounded(
            draw::rect(
                "cursor",
                0.0,
                0.0,
                pad_w + 6.0,
                rows_h + 4.0,
                draw::solid(&ui::alpha(PAD, 0.14)),
            ),
            3.0,
        ),
    );
    let step = format!(
        "floor(fract(time * 1000 / ({STEPS} * max(param({DLY}/time), 20))) * {STEPS}) * param({DLY}/on) * param({DLY}/time) / {window}"
    );
    motion::add(b, cur, Motion::new(Target::X).input(step).range(0.0, span));
    // Step 0: the note, on both rows.
    for y in row_y {
        b.add(
            v,
            ui::glowing(
                draw::rounded(
                    draw::rect(
                        "note pad",
                        lead - pad_w / 2.0,
                        y - pad_h / 2.0,
                        pad_w,
                        pad_h,
                        draw::solid("#eef5ff"),
                    ),
                    2.5,
                ),
                PAD,
                1.2,
                kit::BLOOM,
            ),
        );
    }
    b.add(
        v,
        ui::glowing(
            draw::rect(
                "note velocity",
                lead - 1.5,
                vel_base - vel_h,
                3.0,
                vel_h,
                draw::solid("#eef5ff"),
            ),
            PAD,
            1.0,
            kit::GLOW,
        ),
    );
    for i in 1..=STEPS {
        // The step's line, a delay time on from the last.
        let line = b.add(
            v,
            draw::frame(
                &format!("step {i}"),
                lead - 0.5,
                top - 4.0,
                1.0,
                rows_h + 8.0,
            ),
        );
        b.add(
            line,
            draw::rect(
                "line",
                0.0,
                0.0,
                1.0,
                rows_h + 8.0,
                draw::solid(&ui::alpha(l.faint, if i % 4 == 0 { 0.32 } else { 0.14 })),
            ),
        );
        shell::slide(b, line, i, window, span);
        // The pad, left on odd repeats, right on even, leaning with the pan.
        let left = i % 2 == 1;
        let y = row_y[usize::from(!left)];
        let pad = b.add(
            v,
            draw::frame(
                &format!("pad {i}"),
                lead + 3.0,
                y - pad_h / 2.0,
                pad_w,
                pad_h,
            ),
        );
        b.add(
            pad,
            ui::glowing(
                draw::rounded(
                    draw::rect("pad", 0.0, 0.0, pad_w, pad_h, draw::solid(PAD)),
                    2.5,
                ),
                kit::BLUE,
                1.1,
                kit::GLOW,
            ),
        );
        let lean = if left {
            format!("clamp(1 - param({DLY}/pan), 0.15, 1)")
        } else {
            format!("clamp(1 + param({DLY}/pan), 0.15, 1)")
        };
        motion::add(
            b,
            pad,
            Motion::new(Target::Opacity)
                .input(format!("{} * {lean}", shell::loudness(i)))
                .range(0.06, 1.0)
                .dynamics(shell::spring()),
        );
        shell::slide(b, pad, i, window, span);
        // Its velocity.
        let vel = b.add(
            v,
            draw::frame(
                &format!("velocity {i}"),
                lead + 3.0 + pad_w / 2.0 - 1.5,
                vel_base - vel_h,
                3.0,
                vel_h,
            ),
        );
        b.add(
            vel,
            draw::rect(
                "bar",
                0.0,
                0.0,
                3.0,
                vel_h,
                draw::solid(&ui::alpha(PAD, 0.8)),
            ),
        );
        shell::slide(b, vel, i, window, span);
        shell::fall(b, vel, i, 1.0);
    }
    ui::silk_right(
        b,
        v,
        &shell::ms_label(window),
        w - 8.0,
        11.0,
        5.0,
        &ui::alpha(l.faint, 0.5),
        700.0,
    );
}

const DEVICE: Device = Device {
    name: "Rhythm",
    base: "67-dly-rhythm",
    caption: "Ping-pong pattern",
    title: (draw::family::ORBITRON, 700.0, 15.0, 0.06),
    title_w: 100.0,
    front: [
        ("feedback", "Repeats"),
        ("pan", "Pan"),
        ("duck_sens", "Duck"),
        ("level", "Level"),
    ],
    phone: [("feedback", "Repeats"), ("pan", "Pan"), ("level", "Level")],
    look,
    dress,
    picture,
};

#[must_use]
pub fn face(dark: bool) -> Face {
    face_at(dark, kit::Tier::Desktop)
}

#[must_use]
pub fn face_at(dark: bool, tier: kit::Tier) -> Face {
    shell::face_at(&DEVICE, dark, tier)
}

#[must_use]
pub fn back_at(dark: bool, tier: kit::Tier) -> Face {
    shell::back_at(&DEVICE, dark, tier)
}

#[must_use]
pub fn tall(dark: bool) -> Face {
    shell::tall(&DEVICE, dark)
}
