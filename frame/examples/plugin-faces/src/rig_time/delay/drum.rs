//! Drum: the magnetic drum echo (an Echorec's), its lid off. A steel drum
//! with a band of oxide round its rim turns under a ring of heads — the
//! record head, four playback heads, the erase head — faster the shorter
//! the time, so the note written at the record head reaches the n-th head
//! n times the time later. Each playback head's lamp flashes as it does,
//! dimmer down the line by the feedback. Beside the drum, what the heads
//! play: the four head taps, numbered, then the swell the feedback sends
//! round again, each a delay time along and lower by the feedback.
//!
//! Hammertone: silver-grey, or bottle green; the drum is steel.

use document_proto::NodeId;
use document_proto::motion::{Motion, Target};
use surface_kit::{draw, motion};
use surface_widgets::Builder;

use super::machine::{self, Look, Machine, Room, ON, PERIOD};
use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, DLY};

const OXIDE: &str = "#3a2c22";
const LAMP: &str = kit::BLUE_LAMP;
const DIM: &str = "#8b958f";

/// The heads' places round the drum (0 = up, clockwise): the record head,
/// the four playback heads a delay time apart, the erase head.
const REC: f64 = -20.0;
const PLAY: [f64; 4] = [28.0, 76.0, 124.0, 172.0];
const ERASE: f64 = 236.0;
/// The drum turns `PLAY`'s spacing (48°) in a delay time: once round in
/// this many.
const TURN: f64 = 7.5;

const MACHINE: Machine = Machine {
    name: "Drum",
    file: "68-dly-drum",
    light: Look {
        ink: "#1f2321",
        dim: "#5f6662",
        panel: [(0.0, "#d3d6d2"), (0.6, "#bfc3be"), (1.0, "#a9aea8")],
        edge: "#8d928c",
        texture: "#ffffff38",
        well: [(0.0, "#1d201f"), (1.0, "#101211")],
        knob: ["color=#1c1e1d", "pointer=#e9eef6", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#a3a9ae"],
        wheel: ["color=#2b2f2d", "pointer=#7fb2ff", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#c7ccd0"],
        accent: kit::BLUE_DEEP,
        scrim: "#c9ccc8e6",
    },
    dark: Look {
        ink: "#e4ebe6",
        dim: "#8d9c94",
        panel: [(0.0, "#25322d"), (0.6, "#1b2622"), (1.0, "#121a17")],
        edge: "#34453e",
        texture: "#ffffff0d",
        well: [(0.0, "#121514"), (1.0, "#090b0a")],
        knob: ["color=#151917", "pointer=#bcd6ff", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#6d767c"],
        wheel: ["color=#202624", "pointer=#bcd6ff", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#8d969c"],
        accent: kit::BLUE_LIGHT,
        scrim: "#1b2622e6",
    },
    title: (draw::family::MICHROMA, 400.0, 15.0, 0.12, 0.92),
    front: [("feedback", "Swell"), ("high_cut", "Tone"), ("mod_depth", "Wow"), ("level", "Level")],
    phone: [("feedback", "Swell"), ("high_cut", "Tone"), ("level", "Level")],
    main: [("feedback", "Swell"), ("high_cut", "Tone"), ("mod_depth", "Wow"), ("level", "Level")],
    picture,
    glass: false,
};

/// A head at `deg` round the drum at (`cx`, `cy`), its face on radius `r`:
/// a block standing out from the rim, its gap toward the oxide.
fn head(b: &mut Builder<'_>, parent: NodeId, (cx, cy, r): (f64, f64, f64), deg: f64, name: &str, gap: &str) {
    let (x, y) = draw::polar(cx, cy, r + 6.0, deg);
    let block = draw::rounded(draw::rect(name, x - 6.0, y - 5.0, 12.0, 10.0, draw::linear(180.0, &[(0.0, "#4a4f55"), (1.0, "#1b1d20")])), 2.0);
    b.add(parent, draw::with_effects(draw::rotated(draw::stroked(block, "#9aa1a8", 0.8), deg), vec![draw::drop_shadow("#000000a0", 0.0, 1.5, 2.0)]));
    let (gx, gy) = draw::polar(cx, cy, r + 1.8, deg);
    b.add(parent, draw::rotated(draw::rect("gap", gx - 4.0, gy - 0.7, 8.0, 1.4, draw::solid(gap)), deg));
}

/// The drum at (`cx`, `cy`), radius `r`, and its heads.
fn drum(b: &mut Builder<'_>, well: NodeId, (cx, cy, r): (f64, f64, f64)) {
    // The deck under the drum: its shadow.
    b.add(well, draw::circle("deck", cx, cy + 2.0, r + 6.0, draw::radial_at(0.5, 0.5, 0.5, &[(0.0, "#000000a0"), (0.8, "#00000060"), (1.0, "#00000000")])));
    // The drum: turned steel, catching the light.
    let body = b.add(well, draw::circle("drum", cx, cy, r, draw::linear(115.0, &[(0.0, "#eef1f3"), (0.35, "#a6adb3"), (0.6, "#d6dadd"), (1.0, "#646a70")])));
    kit::steel(b, body);
    // The oxide band round its rim.
    b.add(well, machine::outline(draw::circle("oxide", cx, cy, r - 4.0, draw::solid("#00000000")), OXIDE, 8.0));
    // What turns: the flux written on the oxide, the spokes under the hub.
    let rot = b.add(well, draw::frame("turning", cx - r, cy - r, 2.0 * r, 2.0 * r));
    let flux: Vec<Vec<(f64, f64)>> = (0..36)
        .filter(|i| crate::reverb::jitter(f64::from(*i) * 0.9 + 1.0) > 0.35)
        .map(|i| {
            let a0 = f64::from(i) * 10.0;
            let len = 3.0 + 5.0 * crate::reverb::jitter(f64::from(i) * 1.7);
            (0..=4).map(|k| draw::polar(r, r, r - 4.0, a0 + len * f64::from(k) / 4.0)).collect()
        })
        .collect();
    b.add(rot, ui::glowing(ui::strokes("flux", &flux, 3.0, draw::solid(&ui::alpha(kit::BLUE, 0.8))), kit::BLUE, 0.7, kit::GLOW));
    let spokes: Vec<Vec<(f64, f64)>> = (0..3)
        .map(|i| {
            let a = f64::from(i) * 120.0;
            vec![draw::polar(r, r, r * 0.24, a), draw::polar(r, r, r - 10.0, a)]
        })
        .collect();
    b.add(rot, ui::strokes("spokes", &spokes, 2.4, draw::solid("#2a2e3299")));
    for i in 0..3 {
        let (x, y) = draw::polar(r, r, r * 0.62, f64::from(i) * 120.0 + 60.0);
        b.add(rot, draw::circle("rivet", x, y, 1.8, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#f4f6f8"), (1.0, "#545a60")])));
    }
    let turn = format!("fract(time * 1000 / ({TURN} * {PERIOD}))");
    motion::add(b, rot, Motion::new(Target::Rotate).input(turn).range(0.0, 360.0).pivot(0.5, 0.5));
    // The hub and its nut.
    b.add(well, draw::with_effects(draw::circle("hub", cx, cy, r * 0.24, draw::radial_at(0.4, 0.35, 0.9, &[(0.0, "#f2f4f6"), (1.0, "#4c5157")])), vec![draw::drop_shadow("#00000080", 0.0, 1.0, 2.0)]));
    b.add(well, draw::path("nut", 0.0, 0.0, 0.0, 0.0, draw::regular(cx, cy, r * 0.1, 6, 0.0), draw::solid("#3a3f44")));
    // The heads: record, playback, erase.
    let at = (cx, cy, r);
    head(b, well, at, REC, "record head", "#d04848");
    for (i, deg) in PLAY.iter().enumerate() {
        head(b, well, at, *deg, &format!("playback head {}", i + 1), "#1a1c1e");
    }
    head(b, well, at, ERASE, "erase head", "#1a1c1e");
    // Each playback head's lamp, on its head, flashing on its repeat.
    for (i, deg) in PLAY.iter().enumerate() {
        let (x, y) = draw::polar(cx, cy, r + 7.0, *deg);
        let lamp = b.add(well, draw::emissive(draw::circle("head lamp", x, y, 2.4, draw::solid(LAMP)), draw::hex(LAMP), 1.2, kit::BLOOM));
        motion::add(b, lamp, Motion::new(Target::Opacity).input(machine::flash(i as u32 + 1)).range(0.2, 1.0));
    }
}

/// What the heads play in (`x, y, w, h`): the four head taps, numbered, then
/// the feedback's swell — a delay time apart, the swell lower by the
/// feedback each time round.
fn taps(b: &mut Builder<'_>, well: NodeId, (x, y, w, h): (f64, f64, f64, f64)) {
    let mut f = draw::frame("Taps", x, y, w, h);
    f.clips_content = true;
    let f = b.add(well, f);
    let base = h - 12.0;
    b.add(f, ui::strokes("baseline", &[vec![(0.0, base), (w, base)]], 1.0, draw::solid(&ui::alpha(DIM, 0.5))));
    ui::silk_left(b, f, "0", 0.0, base + 7.0, 5.0, DIM, 700.0);
    ui::silk_right(b, f, "3 s", w, base + 7.0, 5.0, DIM, 700.0);
    b.add(f, ui::glowing(draw::rect("note", 2.0, 8.0, 2.0, base - 8.0, draw::solid(kit::BLUE_LIGHT)), kit::BLUE_LIGHT, 1.2, kit::GLOW));
    let window = 3000.0;
    let span = w - 10.0;
    let top = 14.0;
    for i in 1..=10_u32 {
        let fi = f64::from(i);
        let col = b.add(f, draw::frame(&format!("repeat {i}"), 0.0, 0.0, 12.0, h));
        let head = i <= 4;
        let color = if head { kit::BLUE_LIGHT } else { kit::BLUE };
        let bar = b.add(col, draw::frame("level", 4.5, top, 3.0, base - top));
        let fill = draw::rect("bar", 0.0, 0.0, 3.0, base - top, draw::solid(color));
        b.add(bar, if head { ui::glowing(fill, color, 1.2, kit::GLOW) } else { fill });
        let cap = b.add(col, draw::emissive(draw::circle("cap", 6.0, top, if head { 3.2 } else { 2.4 }, draw::solid(color)), draw::hex(color), 1.1, kit::GLOW));
        if head {
            ui::silk(b, col, &i.to_string(), 6.0, 5.0, 5.5, DIM, 800.0);
        }
        let at = format!("clamp({fi} * param({DLY}/time) / {window}, 0, 1.1)");
        motion::add(b, col, Motion::new(Target::X).input(at).range(-4.0, span - 4.0).dynamics(machine::spring()));
        // The heads play at full level, a little less down the line; past
        // them, the swell, falling by the feedback each time round.
        let level = if head {
            format!("{} * (0.25 + 0.75 * {ON})", 1.0 - 0.08 * (fi - 1.0))
        } else {
            format!("0.8 * pow(param({DLY}/feedback), {}) * (0.25 + 0.75 * {ON})", i - 4)
        };
        motion::add(b, bar, Motion::new(Target::StretchY).input(level.clone()).range(0.0, 1.0).pivot(0.5, 1.0).dynamics(machine::spring()));
        motion::add(b, cap, Motion::new(Target::Y).input(format!("1 - {level}")).range(0.0, base - top).dynamics(machine::spring()));
    }
}

fn picture(b: &mut Builder<'_>, well: NodeId, (w, h): (f64, f64), _l: &Look, room: Room, _dark: bool) {
    let (cx, cy, r) = match room {
        Room::Phone => (w / 2.0, h / 2.0 + 4.0, 48.0),
        Room::Lane => (78.0, h / 2.0, 52.0),
        Room::Tall => (70.0, h / 2.0 + 2.0, 42.0),
    };
    drum(b, well, (cx, cy, r));
    let x = cx + r + 34.0;
    match room {
        Room::Phone => {}
        Room::Lane => taps(b, well, (x, 16.0, w - x - 14.0, h - 22.0)),
        Room::Tall => taps(b, well, (x, 40.0, w - x - 16.0, h - 46.0)),
    }
    if room != Room::Phone {
        ui::silk_left(b, well, "Magnetic drum · 4 heads", x, 8.0, 5.5, DIM, 800.0);
    }
}

#[must_use]
pub fn face(dark: bool) -> Face {
    face_at(dark, kit::Tier::Desktop)
}

#[must_use]
pub fn face_at(dark: bool, tier: kit::Tier) -> Face {
    MACHINE.face_at(dark, tier)
}

#[must_use]
pub fn back_at(dark: bool, tier: kit::Tier) -> Face {
    MACHINE.back_at(dark, tier)
}

#[must_use]
pub fn tall(dark: bool) -> Face {
    MACHINE.tall(dark)
}
