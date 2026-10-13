//! Convolution: a real space, captured — drawn as the sampling reverb's
//! display, the impulse response it is convolving with. The IR as a
//! recording shows it: the direct spike, the early reflections, then the
//! dense tail dying away, and under it its spectrogram, the highs dying
//! first. The size time-stretches the IR; the trim flag cuts its tail (the
//! decay — drag it) and what lies past it goes dark; a playhead runs the
//! kept part while the unit plays. A brushed aluminium faceplate round it,
//! as the rack units had.
//!
//! Two finishes: light, silver aluminium; dark, black anodised.

use document_proto::motion::{Motion, Target};
use document_proto::NodeId;
use surface_kit::{draw, motion};
use surface_widgets::Builder;

use super::shell::{self, Look, Room, Unit};
use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, VERB};

const UNIT: Unit = Unit {
    name: "Convolution",
    base: "94-verb-convolution",
    title_w: 132.0,
    legend: "On",
    lane: [("size", "Stretch"), ("predelay", "Pre-Delay"), ("level", "Level")],
    tall: [("size", "Stretch"), ("predelay", "Pre-Delay"), ("tone", "Tone"), ("level", "Level")],
    look,
    ground,
    picture,
    title,
};

/// The display's light: the section's purple.
const TRACE: &str = kit::PURPLE_LAMP;
const HOT: &str = "#efe6ff";

struct Finish {
    look: Look,
    plate: [(f64, &'static str); 5],
    screw: &'static str,
    print: &'static str,
}

const LIGHT: Finish = Finish {
    look: Look {
        ink: "#1e2024",
        dim: "#5b6068",
        accent: "#6a3fc4",
        scrim: "#d9dce0e6",
        knob: ["color=#d9dce0", "pointer=#2a2d33", "index=line", "index_w=0.12", "skirt=plain", "skirt_color=#7d828a"],
        wheel: ["color=#e6e8eb", "pointer=#6a3fc4", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#6d727a"],
    },
    plate: [(0.0, "#e9ebee"), (0.3, "#c9ccd1"), (0.55, "#e4e6e9"), (0.8, "#bfc3c8"), (1.0, "#d6d9dd")],
    screw: "#8c9097",
    print: "#8a83a0",
};

const DARK: Finish = Finish {
    look: Look {
        ink: "#e6e7ea",
        dim: "#8b9098",
        accent: kit::PURPLE_LAMP,
        scrim: "#222428e6",
        knob: ["color=#232529", "pointer=#e6e7ea", "index=line", "index_w=0.12", "skirt=plain", "skirt_color=#4d5157"],
        wheel: ["color=#2a2c30", "pointer=#c4b5fd", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#5d6168"],
    },
    plate: [(0.0, "#36383d"), (0.3, "#232529"), (0.55, "#33353a"), (0.8, "#1d1f22"), (1.0, "#2a2c30")],
    screw: "#15161a",
    print: "#7c7592",
};

fn finish(dark: bool) -> &'static Finish {
    if dark { &DARK } else { &LIGHT }
}

fn look(dark: bool) -> &'static Look {
    &finish(dark).look
}

/// The IR's time-stretch: the size.
fn stretch() -> String {
    format!("lerp(0.55, 1, norm({VERB}/size))")
}

/// Where the trim cuts the tail (0..1 of the window): the decay.
fn trim() -> String {
    format!("lerp(0.2, 1, norm({VERB}/decay))")
}

/// The IR's amplitude at `t` (0..1 of the window) for sample `i`: the
/// direct spike, early reflections, then the dense tail.
fn ir(t: f64, i: f64) -> f64 {
    let j = crate::reverb::jitter(i * 1.73);
    let early = [(0.035, 0.62), (0.05, 0.48), (0.066, 0.55), (0.081, 0.36), (0.1, 0.42), (0.118, 0.3)];
    if t < 0.012 {
        return 0.04 * j;
    }
    if t < 0.02 {
        return 1.0 - (t - 0.012) * 60.0;
    }
    let refl = early.iter().map(|(at, a)| if (t - at).abs() < 0.004 { a * (1.0 - (t - at).abs() / 0.004) } else { 0.0 }).fold(0.0, f64::max);
    let build = ((t - 0.02) / 0.1).clamp(0.0, 1.0);
    let tail = 0.62 * build * (-2.7 * (t - 0.02)).exp() * (0.35 + 0.65 * j);
    refl.max(tail).max(0.015 * j)
}

/// The faceplate: brushed aluminium (a leaf under the controls, so only it
/// takes the steel), screws at its ends.
fn ground(b: &mut Builder<'_>, root: NodeId, (w, h): (f64, f64), dark: bool) -> NodeId {
    let f = finish(dark);
    let panel = shell::panel(b, root, (w, h), &[(0.0, f.plate[0].1), (0.5, f.plate[2].1), (1.0, f.plate[4].1)], 5.0);
    let brushed = b.add(panel, draw::rounded(draw::rect("brushed", 0.0, 0.0, w, h, draw::linear(100.0, &f.plate)), 5.0));
    kit::steel(b, brushed);
    for (x, y) in [(7.0, 8.0), (7.0, h - 8.0), (w - 7.0, 8.0), (w - 7.0, h - 8.0)] {
        b.add(panel, draw::with_effects(draw::circle("screw", x, y, 2.6, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#ffffff80"), (1.0, f.screw)])), vec![draw::drop_shadow("#00000060", 0.0, 0.8, 1.0)]));
        b.add(panel, ui::line_node("cross", &[(x - 1.6, y), (x + 1.6, y)], 0.6, "#1b1c20"));
        b.add(panel, ui::line_node("cross", &[(x, y - 1.6), (x, y + 1.6)], 0.6, "#1b1c20"));
    }
    panel
}

/// The display's contents in `well` (`w × h`): the file, the ruler, the IR
/// and its spectrogram, the trim and the playhead.
fn display(b: &mut Builder<'_>, well: NodeId, (w, h): (f64, f64), room: Room, f: &Finish) {
    let compact = room == Room::Phone;
    let ix = 10.0;
    let iw = w - 20.0;
    let wave_top = 26.0;
    let spec_h = if compact { 18.0 } else { (h * 0.18).round() };
    let wave_h = h - wave_top - spec_h - 22.0;
    let mid = wave_top + wave_h / 2.0;
    let spec_y = wave_top + wave_h + 4.0;
    // The file.
    let name = if compact { "IR · St. Bavo · 4 s" } else { "IR · St. Bavo, Haarlem · nave · 48 kHz · stereo · 4 s" };
    ui::silk_left(b, well, name, ix, 8.0, 5.5, f.print, 800.0);
    if room == Room::Lane {
        ui::silk_right(b, well, "Pre-delay ms", w - 50.0, 9.0, 5.0, f.print, 800.0);
        kit::seg_readout(b, well, &format!("{VERB}/predelay"), (w - 48.0, 9.0, 38.0), 10.0, 3, 0, TRACE);
    }
    // The ruler.
    let ticks: Vec<Vec<(f64, f64)>> = (0..=16).map(|k| {
        let x = ix + iw * f64::from(k) / 16.0;
        vec![(x, 17.0), (x, if k % 4 == 0 { 22.0 } else { 19.5 })]
    }).collect();
    b.add(well, ui::strokes("ruler", &ticks, 0.6, draw::solid(&ui::alpha(f.print, 0.8))));
    if !compact {
        for k in 1..4 {
            ui::silk_left(b, well, &format!("{k} s"), ix + iw * f64::from(k) / 4.0 + 2.0, 19.5, 4.5, &ui::alpha(f.print, 0.8), 700.0);
        }
    }
    b.add(well, ui::line_node("zero", &[(ix, mid), (ix + iw, mid)], 0.5, &ui::alpha(TRACE, 0.3)));
    // The IR: drawn at full length, time-stretched by the size.
    let g = b.add(well, draw::frame("IR", ix, wave_top, iw, spec_y + spec_h - wave_top));
    motion::add(b, g, Motion::new(Target::StretchX).input(stretch()).range(0.0, 1.0).pivot(0.0, 0.5));
    let n = if compact { 160 } else { 360 };
    let half = wave_h / 2.0;
    let (mut top, mut bottom) = (Vec::new(), Vec::new());
    for i in 0..=n {
        let t = f64::from(i) / f64::from(n);
        let x = t * iw;
        top.push((x, half - half * ir(t, f64::from(i))));
        bottom.push((x, half + half * ir(t, f64::from(i) + 0.5)));
    }
    let mut d = format!("M 0 {half:.1} ");
    for (x, y) in top.iter().chain(bottom.iter().rev()) {
        d += &format!("L {x:.1} {y:.1} ");
    }
    d.push('Z');
    b.add(g, ui::glowing(draw::path("waveform", 0.0, 0.0, iw, wave_h, d, draw::linear(180.0, &[(0.0, HOT), (0.5, TRACE), (1.0, HOT)])), TRACE, 0.5, kit::GLOW));
    // Its spectrogram: the highs (top) die first.
    let cols = if compact { 32 } else { 56 };
    let cw = iw / f64::from(cols);
    for c in 0..cols {
        let t = (f64::from(c) + 0.5) / f64::from(cols);
        let e = |band: f64| {
            let k = 1.6 + 6.0 * band;
            let onset = if t < 0.015 { 0.2 } else { 1.0 };
            (onset * (-k * t).exp() * (0.75 + 0.25 * crate::reverb::jitter(f64::from(c) * 3.1 + band))).clamp(0.0, 1.0)
        };
        let stops = [(0.0, ui::alpha(HOT, e(1.0))), (0.35, ui::alpha(TRACE, e(0.66))), (0.7, ui::alpha(TRACE, e(0.33) * 0.95)), (1.0, ui::alpha("#7d5fd8", e(0.0) * 0.95))];
        let stops: Vec<(f64, &str)> = stops.iter().map(|(p, s)| (*p, s.as_str())).collect();
        b.add(g, draw::rect("bin", f64::from(c) * cw, spec_y - wave_top, cw + 0.4, spec_h, draw::linear(180.0, &stops)));
    }
    // The playhead: it runs the kept part while the unit plays.
    let ph = b.add(g, draw::frame("playhead", -1.0, 0.0, 2.0, wave_h));
    b.add(ph, ui::glowing(draw::rect("line", 0.0, 0.0, 2.0, wave_h, draw::solid(HOT)), TRACE, 1.2, kit::BLOOM));
    motion::add(b, ph, Motion::new(Target::X).input(format!("fract(time * 0.3) * min(1, {} / {})", trim(), stretch())).range(0.0, iw));
    motion::add(b, ph, Motion::new(Target::Opacity).input(format!("param({VERB}/on)")).range(0.0, 0.9));
    // Past the trim, dark: pinned at the right, stretched back to the flag.
    let past = b.add(well, draw::frame("trimmed", ix, wave_top - 2.0, iw, spec_y + spec_h - wave_top + 4.0));
    b.add(past, draw::rect("shade", 0.0, 0.0, iw, spec_y + spec_h - wave_top + 4.0, draw::linear(90.0, &[(0.0, "#0a090dd0"), (0.06, "#0a090de8"), (1.0, "#0a090df0")])));
    motion::add(b, past, Motion::new(Target::StretchX).input(format!("1 - {}", trim())).range(0.0, 1.0).pivot(1.0, 0.5));
    // The trim flag: drag it for the decay.
    let fh = h - wave_top - 2.0;
    shell::decay_handle(b, well, "Trim", (ix + 0.2 * iw, ix + iw, wave_top), (28.0, fh), |b, t| {
        let mut l = ui::line_node("cut", &[(14.0, 0.0), (14.0, fh - 12.0)], 0.9, &ui::alpha(HOT, 0.8));
        l.stroke.dash_pattern = vec![3.0, 2.0];
        b.add(t, l);
        b.add(t, ui::glowing(draw::rounded(draw::rect("flag", 0.0, fh - 12.0, 28.0, 10.0, draw::solid(TRACE)), 2.5), TRACE, 0.6, kit::GLOW));
        ui::plain(b, t, "Trim", 14.0, fh - 7.0, 5.5, "#1a0f2c", 800.0);
    });
}

/// The picture: the display in its black bezel, under glass.
fn picture(b: &mut Builder<'_>, panel: NodeId, (x, y, w, h): (f64, f64, f64, f64), room: Room, dark: bool) -> Vec<NodeId> {
    let f = finish(dark);
    b.add(panel, draw::with_effects(draw::rounded(draw::rect("bezel", x - 3.0, y - 3.0, w + 6.0, h + 6.0, draw::linear(180.0, &[(0.0, "#2a2b30"), (1.0, "#0b0b0d")])), 8.0), vec![draw::drop_shadow("#ffffff40", 0.0, 1.0, 0.0), draw::inner_shadow("#00000090", 0.0, 1.0, 2.0)]));
    let bezel = b.scene.node(panel).and_then(|n| n.children.last().copied());
    let well = shell::well(b, panel, "Display", (x, y, w, h), &[(0.0, "#141218"), (1.0, "#0a090d")], 5.0);
    display(b, well, (w, h), room, f);
    kit::glass(b, well);
    let mut v = vec![well];
    v.extend(bezel);
    v
}

/// The nameplate's title: "Convolution" in a technical sans.
fn title(b: &mut Builder<'_>, t: NodeId, h: f64, size: f64, dark: bool) {
    let l = draw::Lettering::family(draw::family::SAIRA).weight(600.0);
    crate::suite_studio_blues::parts::set(b, t, "Convolution", 58.0, h / 2.0, size * 0.8, finish(dark).look.ink, &l, -0.08);
}

#[must_use]
pub fn face(dark: bool) -> Face {
    face_at(dark, kit::Tier::Desktop)
}

/// The face at `tier`: the display beside the controls.
#[must_use]
pub fn face_at(dark: bool, tier: kit::Tier) -> Face {
    shell::face_at(&UNIT, dark, tier)
}

/// Its back at `tier`.
#[must_use]
pub fn back_at(dark: bool, tier: kit::Tier) -> Face {
    shell::back_at(&UNIT, dark, tier)
}

/// Its tall face.
#[must_use]
pub fn tall(dark: bool) -> Face {
    shell::tall(&UNIT, dark)
}
