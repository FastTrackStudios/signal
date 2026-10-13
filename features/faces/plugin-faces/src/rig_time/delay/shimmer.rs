//! Shimmer: an echo that climbs — every time round the loop the repeat is
//! shifted up an octave, so the echoes stack into a cascade. The picture
//! is pitch over time on a night-blue sheet: the note on the root's line
//! at the left; each repeat a delay time further on, carrying every octave
//! it has climbed so far, the newest the brightest, a glide drawn up to
//! it from the last. The Tone knob caps the cascade (darker, the high
//! octaves fade out); the Mod knob sets the top notes twinkling.
//!
//! Two finishes: light, pearl; dark, midnight.

use frame::document::proto::NodeId;
use frame::document::proto::motion::{Motion, Target};
use frame::surface::kit::{draw, motion};
use frame::surface::widgets::Builder;

use super::shell::{self, Device, Look};
use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, DLY};

/// The notes' light.
const STAR: &str = "#d6e6ff";

fn look(dark: bool) -> Look {
    if dark {
        Look {
            ink: "#e6ebf7",
            dim: "#8590ab",
            panel: [(0.0, "#1a2036"), (0.6, "#131829"), (1.0, "#0d111e")],
            edge: "#2c3556",
            well: [(0.0, "#060a1a"), (1.0, "#0b1430")],
            knob: [
                "color=#232b45",
                "pointer=#d6e6ff",
                "index=line",
                "index_w=0.1",
                "skirt=plain",
                "skirt_color=#0a0e1b",
            ],
            wheel: [
                "color=#1a2038",
                "pointer=#d6e6ff",
                "index=dot",
                "skirt=plain",
                "skirt_color=#080b16",
            ],
            accent: kit::BLUE_LIGHT,
            scrim: "#131829e6",
            light: STAR,
            faint: "#8fb4ff",
            metal: false,
        }
    } else {
        Look {
            ink: "#1d2236",
            dim: "#6c7391",
            panel: [(0.0, "#f8f8fc"), (0.6, "#eceef6"), (1.0, "#dfe2ee")],
            edge: "#c6cbdc",
            well: [(0.0, "#060a1a"), (1.0, "#0b1430")],
            knob: [
                "color=#fbfbfe",
                "pointer=#1e3a8a",
                "index=line",
                "index_w=0.1",
                "skirt=plain",
                "skirt_color=#c3c8da",
            ],
            wheel: [
                "color=#2a3150",
                "pointer=#eef3ff",
                "index=dot",
                "skirt=plain",
                "skirt_color=#454d6e",
            ],
            accent: kit::BLUE_DEEP,
            scrim: "#eceef6e6",
            light: STAR,
            faint: "#8fb4ff",
            metal: false,
        }
    }
}

/// A pearl sheen across the panel: two soft diagonal bands.
fn dress(b: &mut Builder<'_>, panel: NodeId, (w, h): (f64, f64), _l: &Look) {
    let sheen = draw::linear(
        100.0,
        &[
            (0.0, "#ffffff00"),
            (0.45, "#ffffff10"),
            (0.5, "#bcd6ff14"),
            (0.55, "#ffffff10"),
            (1.0, "#ffffff00"),
        ],
    );
    b.add(panel, draw::rect("sheen", 0.0, 0.0, w, h, sheen));
}

/// The octaves drawn: the root and three above it.
const OCTAVES: u32 = 4;

/// A four-point star `r` across at (`cx`, `cy`).
fn star(cx: f64, cy: f64, r: f64) -> String {
    let k = r * 0.22;
    shell::closed(&[
        (cx, cy - r),
        (cx + k, cy - k),
        (cx + r, cy),
        (cx + k, cy + k),
        (cx, cy + r),
        (cx - k, cy + k),
        (cx - r, cy),
        (cx - k, cy - k),
    ])
}

fn picture(b: &mut Builder<'_>, v: NodeId, (w, h): (f64, f64), l: &Look) {
    // Clear of the caption — and on a tall face of the nameplate.
    let (top, bottom) = (if w > 600.0 { 54.0 } else { 30.0 }, h - 14.0);
    let gap = (bottom - top) / f64::from(OCTAVES - 1);
    let row = |k: u32| bottom - gap * f64::from(k);
    let lead = 30.0;
    let span = w - lead - 20.0;
    let window = shell::window(span);
    let nl = if w < 300.0 { 10.0 } else { 18.0 };
    // The octaves' lines, marked at the left.
    let lines: Vec<Vec<(f64, f64)>> = (0..OCTAVES)
        .map(|k| vec![(lead - 4.0, row(k)), (w, row(k))])
        .collect();
    b.add(
        v,
        ui::strokes(
            "octaves",
            &lines,
            0.7,
            draw::solid(&ui::alpha(l.faint, 0.16)),
        ),
    );
    for (k, s) in ["0", "+12", "+24", "+36"].iter().enumerate() {
        ui::silk_right(
            b,
            v,
            s,
            lead - 7.0,
            row(k as u32),
            5.0,
            &ui::alpha(l.faint, 0.55),
            700.0,
        );
    }
    // The note, on the root.
    b.add(
        v,
        ui::glowing(
            draw::rounded(
                draw::rect("note", lead, row(0) - 1.6, nl, 3.2, draw::solid("#ffffff")),
                1.6,
            ),
            STAR,
            1.4,
            kit::BLOOM,
        ),
    );
    // The repeats: the n-th climbs to the n-th octave (the cascade's top
    // capped where the lines end), each octave it passed still sounding.
    let tone = |k: u32| format!("clamp(norm({DLY}/high_cut) * 4.5 - {k} + 0.6, 0.12, 1)");
    for i in 1..=8_u32 {
        let g = b.add(v, draw::frame(&format!("repeat {i}"), lead, 0.0, nl, h));
        let top_k = i.min(OCTAVES - 1);
        // The wash: the octaves ringing together, a haze up the column.
        let wash = b.add(
            g,
            draw::rect(
                "wash",
                0.0,
                row(top_k),
                nl,
                row(0) - row(top_k),
                draw::linear(
                    180.0,
                    &[
                        (0.0, &ui::alpha(kit::BLUE, 0.4)),
                        (1.0, &ui::alpha(kit::BLUE, 0.04)),
                    ],
                ),
            ),
        );
        motion::add(
            b,
            wash,
            Motion::new(Target::Opacity)
                .input(tone(top_k))
                .range(0.0, 1.0),
        );
        for k in 0..=top_k {
            let newest = k == top_k;
            let col = if newest {
                STAR.to_string()
            } else {
                ui::alpha(STAR, 0.55 - 0.08 * f64::from(top_k - k))
            };
            let bar = draw::rounded(
                draw::rect(
                    &format!("octave {k}"),
                    0.0,
                    row(k) - 1.4,
                    nl,
                    2.8,
                    draw::solid(&col),
                ),
                1.4,
            );
            let bar = b.add(
                g,
                if newest {
                    ui::glowing(bar, kit::BLUE_LAMP, 1.2, kit::GLOW)
                } else {
                    bar
                },
            );
            if k > 0 {
                motion::add(
                    b,
                    bar,
                    Motion::new(Target::Opacity)
                        .input(tone(k))
                        .range(0.0, 1.0)
                        .dynamics(shell::spring()),
                );
            }
        }
        // The glide up from the last repeat's top: drawn a whole window
        // wide back to the left, squeezed to one delay time.
        let (y0, y1) = (row((i - 1).min(OCTAVES - 1)), row(top_k));
        let glide = b.add(g, draw::frame("glide", -span, 0.0, span, h));
        let curve: Vec<(f64, f64)> = (0..=16)
            .map(|s| {
                let u = f64::from(s) / 16.0;
                (
                    span * (0.18 + 0.82 * u),
                    y0 + (y1 - y0) * (u * u * (3.0 - 2.0 * u)),
                )
            })
            .collect();
        let gl = b.add(
            glide,
            ui::strokes(
                "glide",
                &[curve],
                1.0,
                draw::solid(&ui::alpha(kit::BLUE_LIGHT, 0.45)),
            ),
        );
        motion::add(
            b,
            gl,
            Motion::new(Target::Opacity)
                .input(tone(top_k))
                .range(0.0, 1.0),
        );
        motion::add(
            b,
            glide,
            Motion::new(Target::StretchX)
                .input(format!("clamp(param({DLY}/time) / {window}, 0, 1.2)"))
                .range(0.0, 1.0)
                .pivot(1.0, 0.5)
                .dynamics(shell::spring()),
        );
        // A star on the newest octave, twinkling with the Mod.
        if top_k > 0 {
            let s = b.add(
                g,
                ui::glowing(
                    draw::path(
                        "star",
                        0.0,
                        0.0,
                        nl,
                        h,
                        star(nl - 1.0, row(top_k) - 6.0, 4.5),
                        draw::solid("#ffffff"),
                    ),
                    STAR,
                    1.2,
                    kit::GLOW,
                ),
            );
            let tw = format!(
                "(0.35 + 0.65 * (0.5 + 0.5 * sin(time * {:.2} + {i}))) * norm({DLY}/mod_depth) * param({DLY}/on)",
                2.0 + 0.7 * f64::from(i)
            );
            motion::add(b, s, Motion::new(Target::Opacity).input(tw).range(0.0, 1.0));
        }
        shell::slide(b, g, i, window, span);
        shell::fade(b, g, i);
    }
    ui::silk_right(
        b,
        v,
        &shell::ms_label(window),
        w - 8.0,
        h - 7.0,
        5.0,
        &ui::alpha(l.faint, 0.5),
        700.0,
    );
}

const DEVICE: Device = Device {
    name: "Shimmer",
    base: "64-dly-shimmer",
    caption: "Octave cascade",
    title: (draw::family::PLAYFAIR_DISPLAY, 500.0, 17.0, 0.0),
    title_w: 116.0,
    front: [
        ("feedback", "Repeats"),
        ("high_cut", "Tone"),
        ("mod_depth", "Sparkle"),
        ("level", "Level"),
    ],
    phone: [
        ("feedback", "Repeats"),
        ("mod_depth", "Sparkle"),
        ("level", "Level"),
    ],
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
