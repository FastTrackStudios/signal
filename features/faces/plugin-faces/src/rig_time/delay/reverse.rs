//! Reverse: the echo that plays backwards. The loop cuts what it hears
//! into slices one delay time long and plays each one back to front, so
//! every repeat swells up out of nothing and stops dead where the note
//! began. The picture: the note as it was played, a sharp hit and its
//! decay, ghosted at the left; then each repeat its mirror image — a
//! swell rising to a cliff — landing a delay time further on, lower by
//! the feedback; the slices bracketed under the line; and the read head
//! running backwards across the first slice, once every delay time.
//!
//! Two finishes: light, cream enamel; dark, oxide black — a backwards
//! arrow worked into the panel.

use frame::document::proto::NodeId;
use frame::document::proto::motion::{Motion, Target};
use frame::surface::kit::{draw, motion};
use frame::surface::widgets::Builder;

use super::shell::{self, Device, Look};
use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, DLY};

/// The swells' light.
const SWELL: &str = "#7fb2ff";

fn look(dark: bool) -> Look {
    if dark {
        Look {
            ink: "#ecebe8",
            dim: "#8f8c87",
            panel: [(0.0, "#262524"), (0.6, "#1d1c1b"), (1.0, "#151414")],
            edge: "#383633",
            well: [(0.0, "#0b0f18"), (1.0, "#06090f")],
            knob: [
                "color=#2d2c2a",
                "pointer=#bcd6ff",
                "index=line",
                "index_w=0.12",
                "skirt=plain",
                "skirt_color=#0d0d0c",
            ],
            wheel: [
                "color=#1a1918",
                "pointer=#bcd6ff",
                "index=dot",
                "skirt=plain",
                "skirt_color=#090909",
            ],
            accent: kit::BLUE_LIGHT,
            scrim: "#1d1c1be6",
            light: SWELL,
            faint: "#9ab8e8",
            metal: false,
        }
    } else {
        Look {
            ink: "#24221f",
            dim: "#7a756c",
            panel: [(0.0, "#f4f0e6"), (0.6, "#e9e3d5"), (1.0, "#ddd5c3")],
            edge: "#c4baa5",
            well: [(0.0, "#0b0f18"), (1.0, "#06090f")],
            knob: [
                "color=#262523",
                "pointer=#bcd6ff",
                "index=line",
                "index_w=0.12",
                "skirt=plain",
                "skirt_color=#11100f",
            ],
            wheel: [
                "color=#2b2926",
                "pointer=#eef3ff",
                "index=dot",
                "skirt=plain",
                "skirt_color=#4a4640",
            ],
            accent: kit::BLUE_DEEP,
            scrim: "#e9e3d5e6",
            light: SWELL,
            faint: "#9ab8e8",
            metal: false,
        }
    }
}

/// A broad backwards chevron pressed into the panel behind the knobs.
fn dress(b: &mut Builder<'_>, panel: NodeId, (w, h): (f64, f64), l: &Look) {
    // Between the picture and the Time knob: a tall face has no room.
    if h > kit::H {
        return;
    }
    let tint = ui::alpha(l.ink, 0.045);
    let (x, y, s) = (w - 380.0, h / 2.0, h * 0.42);
    for k in 0..2 {
        let dx = f64::from(k) * s * 0.7;
        let d = shell::closed(&[
            (x + dx, y - s),
            (x + dx + s * 0.32, y - s),
            (x + dx - s * 0.48 + s * 0.32, y),
            (x + dx + s * 0.32, y + s),
            (x + dx, y + s),
            (x + dx - s * 0.48, y),
        ]);
        b.add(
            panel,
            draw::path("chevron", 0.0, 0.0, w, h, d, draw::solid(&tint)),
        );
    }
}

/// The note's envelope `bw` wide, `hh` tall, standing on its foot: a hit
/// and its decay; reversed, a swell up to a cliff.
fn envelope(bw: f64, hh: f64, reversed: bool) -> Vec<(f64, f64)> {
    (0..=32)
        .map(|k| {
            let u = f64::from(k) / 32.0;
            let t = if reversed { 1.0 - u } else { u };
            let e = (-4.0 * t).exp() * (t * 30.0).min(1.0);
            (bw * u, hh - hh * e)
        })
        .collect()
}

fn picture(b: &mut Builder<'_>, v: NodeId, (w, h): (f64, f64), l: &Look) {
    let base = h - 16.0;
    let top = 24.0;
    let hh = base - top;
    let lead = 14.0;
    let span = w - lead - 18.0;
    let window = shell::window(span);
    let bw = if w < 300.0 { 26.0 } else { 46.0 };
    b.add(
        v,
        ui::strokes(
            "baseline",
            &[vec![(0.0, base), (w, base)]],
            1.0,
            draw::solid(&ui::alpha(l.faint, 0.3)),
        ),
    );
    // The note as played, ghosted: a hit, then its decay.
    let fwd = envelope(bw, hh, false);
    let mut d = shell::poly(&fwd);
    d.push_str(&format!("L {bw:.1} {hh:.1} L 0 {hh:.1} Z"));
    b.add(
        v,
        draw::path(
            "note",
            lead,
            top,
            bw,
            hh,
            d,
            draw::solid(&ui::alpha(l.faint, 0.16)),
        ),
    );
    b.add(
        v,
        shell::trace(
            "note edge",
            (lead, top, bw, hh),
            shell::poly(&fwd),
            &ui::alpha(l.faint, 0.6),
            1.2,
        ),
    );
    ui::silk_left(
        b,
        v,
        "Played",
        lead + 2.0,
        base + 8.0,
        5.0,
        &ui::alpha(l.faint, 0.6),
        700.0,
    );
    // The repeats: each the slice back to front, ending where it lands.
    let rev = envelope(bw, hh, true);
    let mut fill = shell::poly(&rev);
    fill.push_str(&format!("L {bw:.1} {hh:.1} L 0 {hh:.1} Z"));
    for i in 1..=8_u32 {
        let g = b.add(
            v,
            draw::frame(&format!("repeat {i}"), lead - bw, top, bw, hh),
        );
        b.add(
            g,
            ui::glowing(
                draw::path(
                    "swell",
                    0.0,
                    0.0,
                    bw,
                    hh,
                    fill.clone(),
                    draw::linear(
                        90.0,
                        &[
                            (0.0, &ui::alpha(SWELL, 0.0)),
                            (1.0, &ui::alpha(SWELL, 0.55)),
                        ],
                    ),
                ),
                SWELL,
                0.5,
                kit::BLOOM,
            ),
        );
        b.add(
            g,
            ui::glowing(
                shell::trace(
                    "edge",
                    (0.0, 0.0, bw, hh),
                    shell::poly(&rev),
                    "#dbe8ff",
                    1.5,
                ),
                SWELL,
                1.2,
                kit::GLOW,
            ),
        );
        shell::slide(b, g, i, window, span);
        shell::fall(b, g, i, 1.0);
        // The slice's mark under the line: where it ends.
        let m = b.add(
            v,
            draw::frame(&format!("slice {i}"), lead - 1.0, base, 2.0, 8.0),
        );
        b.add(
            m,
            draw::rect(
                "cut",
                0.0,
                0.0,
                2.0,
                7.0,
                draw::solid(&ui::alpha(l.faint, 0.55)),
            ),
        );
        shell::slide(b, m, i, window, span);
    }
    // The slices' bracket: the first one, a delay time wide.
    let br = b.add(v, draw::frame("slice", lead, base + 3.0, span, 6.0));
    b.add(
        br,
        draw::rect(
            "bracket",
            0.0,
            2.0,
            span,
            1.0,
            draw::solid(&ui::alpha(SWELL, 0.6)),
        ),
    );
    motion::add(
        b,
        br,
        Motion::new(Target::StretchX)
            .input(format!("clamp(param({DLY}/time) / {window}, 0, 1.2)"))
            .range(0.0, 1.0)
            .pivot(0.0, 0.5)
            .dynamics(shell::spring()),
    );
    // The read head: running backwards across the first slice, once a
    // delay time.
    let head = b.add(
        v,
        draw::frame("read head", lead - 1.0, top - 6.0, 2.0, hh + 10.0),
    );
    b.add(
        head,
        ui::glowing(
            draw::rect(
                "head",
                0.0,
                0.0,
                2.0,
                hh + 10.0,
                draw::linear(
                    180.0,
                    &[(0.0, "#ffffff00"), (0.3, "#ffffffcc"), (1.0, "#ffffff")],
                ),
            ),
            kit::BLUE_LIGHT,
            1.0,
            kit::GLOW,
        ),
    );
    let back = format!(
        "(1 - fract(time * 1000 / max(param({DLY}/time), 20))) * clamp(param({DLY}/time) / {window}, 0, 1.2) * param({DLY}/on)"
    );
    motion::add(b, head, Motion::new(Target::X).input(back).range(0.0, span));
    ui::silk_right(
        b,
        v,
        &shell::ms_label(window),
        w - 8.0,
        base + 8.0,
        5.0,
        &ui::alpha(l.faint, 0.5),
        700.0,
    );
}

const DEVICE: Device = Device {
    name: "Reverse",
    base: "65-dly-reverse",
    caption: "Backwards slices",
    title: (draw::family::OSWALD, 500.0, 19.0, 0.08),
    title_w: 100.0,
    front: [
        ("feedback", "Repeats"),
        ("high_pass", "Low Cut"),
        ("high_cut", "High Cut"),
        ("level", "Level"),
    ],
    phone: [
        ("feedback", "Repeats"),
        ("high_cut", "High Cut"),
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
