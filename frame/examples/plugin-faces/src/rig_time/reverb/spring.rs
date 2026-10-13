//! Spring: the tank itself, out of the amp and open on the bench — a
//! galvanised pan with its springs strung between two transducers, the
//! driver coil at the input end and the pickup at the output. Hit the
//! strings and the "drip" runs down the springs and back: here a bead of
//! light leaves the driver, runs to the pickup, bounces, and fades the
//! faster the shorter the decay. The springs quiver while it plays, harder
//! the longer it rings; a bigger tank is longer springs (the pickup end
//! moves out, the drip takes longer to cross).
//!
//! The decay is the felt damper clamped across the springs: slide it
//! toward the pickup and they ring longer.
//!
//! Two finishes, as the amp's reverb cabinet came: light, blonde tolex
//! with ivory knobs; dark, black tolex with silver-skirted black knobs.

use document_proto::NodeId;
use document_proto::motion::{Dynamics, Motion, Target};
use surface_kit::{draw, motion};
use surface_proto::{Axis, Control, ControlKind, HandleAxis};
use surface_widgets::Builder;

use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, VERB};

const NAME: &str = "Spring";
const FILE: &str = "83-verb-spring";
const BACK: &str = "83-verb-spring-back";
/// The tall face's rows: the tank a little under half, big knobs below.
const ROWS: kit::TallRows = kit::TallRows { split: 134.0, main: (30.0, 50.0), more: (126.0, 40.0) };
/// The drip's light: the section's purple.
const DRIP: &str = kit::PURPLE_LAMP;
/// Spring steel.
const WIRE: &str = "#e2e6ea";

/// One finish's colours.
struct Look {
    ink: &'static str,
    dim: &'static str,
    tolex: [(f64, &'static str); 3],
    weave: &'static str,
    welt: &'static str,
    knob: [&'static str; 6],
    wheel: [&'static str; 6],
    accent: &'static str,
    scrim: &'static str,
}

const LIGHT: Look = Look {
    ink: "#2e2216",
    dim: "#7a6648",
    tolex: [(0.0, "#eadcbd"), (0.55, "#dccaa2"), (1.0, "#cbb58a")],
    weave: "#6b4f2a12",
    welt: "#7a1f22",
    knob: ["color=#f2ead6", "pointer=#3b1d6e", "index=line", "index_w=0.12", "skirt=plain", "skirt_color=#5a3d22"],
    wheel: ["color=#f4ecd8", "pointer=#3b1d6e", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#4a3220"],
    accent: "#6a3fc4",
    scrim: "#e4d4b2e6",
};

const DARK: Look = Look {
    ink: "#ece8e0",
    dim: "#9b968c",
    tolex: [(0.0, "#2a292d"), (0.55, "#1f1e22"), (1.0, "#151417")],
    weave: "#ffffff07",
    welt: "#bdb8ac",
    knob: ["color=#1b1b1e", "pointer=#d9c8ff", "index=line", "index_w=0.12", "skirt=plain", "skirt_color=#b9bcc2"],
    wheel: ["color=#18181b", "pointer=#d9c8ff", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#c4c7cc"],
    accent: kit::PURPLE_LAMP,
    scrim: "#1f1e22e6",
};

fn a(id: &str) -> String {
    format!("{VERB}/{id}")
}

/// A coil seen from the side, `len` long round its axis at `cy`, radius
/// `r`, one turn every `pitch`: loops whose near side is the stroke.
fn coil(len: f64, cy: f64, r: f64, pitch: f64) -> Vec<(f64, f64)> {
    let lead = 4.0;
    let turns = ((len - 2.0 * lead) / pitch).floor().max(1.0);
    let steps = (turns * 6.0) as i32;
    let mut pts = vec![(0.0, cy)];
    for j in 0..=steps {
        let u = f64::from(j) / f64::from(steps);
        let th = u * turns * std::f64::consts::TAU;
        pts.push((lead + (len - 2.0 * lead) * u + r * 0.42 * th.cos(), cy + r * th.sin()));
    }
    pts.push((len, cy));
    pts
}

/// A transducer at (`x`, `y`), `w` × `h`: a black can with its coil and
/// magnet for each spring at `ys` (picture coordinates), and its leads.
fn transducer(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64), ys: &[f64], out: bool) -> NodeId {
    let g = b.add(parent, draw::frame(if out { "pickup" } else { "driver" }, x, y, w, h));
    let can = draw::rounded(draw::rect("can", 0.0, 0.0, w, h, draw::linear(90.0, &[(0.0, "#2c2e31"), (0.5, "#3c3f43"), (1.0, "#1a1b1d")])), 3.0);
    b.add(g, draw::with_effects(can, vec![draw::drop_shadow("#000000a0", 0.0, 2.0, 3.0), draw::inner_shadow("#ffffff22", 0.0, 1.0, 0.0)]));
    let inner = if out { 3.0 } else { w - 3.0 };
    for cy in ys {
        let cy = cy - y;
        // The coil, copper, and the magnet the spring hangs from.
        let cw = (w * 0.5).min(12.0);
        let cx0 = if out { w - cw - 3.0 } else { 3.0 };
        b.add(g, draw::rounded(draw::rect("coil", cx0, cy - 4.0, cw, 8.0, draw::linear(180.0, &[(0.0, "#e3a466"), (0.45, "#9a5a26"), (1.0, "#6e3c16")])), 2.0));
        b.add(g, draw::circle("magnet", inner, cy, 2.4, draw::radial_at(0.35, 0.3, 0.8, &[(0.0, "#d5d8dc"), (1.0, "#55595e")])));
    }
    // The leads, out through the end of the pan.
    let lx = if out { w } else { 0.0 };
    let ly = h * 0.5;
    let ex = if out { w + 9.0 } else { -9.0 };
    ui::line(b, g, "lead", &[(lx, ly - 3.0), (ex, ly - 3.0)], 1.1, "#c23a2f");
    ui::line(b, g, "lead", &[(lx, ly + 3.0), (ex, ly + 3.0)], 1.1, "#1d1d1f");
    g
}

/// The damper: a felt pad on a steel clamp across the springs, a handle
/// over the decay. Its centre travels `x0`..`x1` (picture coordinates);
/// the pad spans `top`..`bottom`.
fn damper(b: &mut Builder<'_>, parent: NodeId, (x0, x1): (f64, f64), h: f64, (top, bottom): (f64, f64)) {
    let area = b.add(parent, draw::frame("damper travel", x0, 0.0, x1 - x0, h));
    let hw = 24.0;
    let g = b.add(area, draw::frame("Damper", 0.4 * (x1 - x0) - hw / 2.0, 0.0, hw, h));
    let c = hw / 2.0;
    // The pad and its clamp bar.
    let pad = draw::rounded(draw::rect("felt", c - 6.0, top, 12.0, bottom - top, draw::linear(90.0, &[(0.0, "#463c36"), (0.5, "#5a4e46"), (1.0, "#352d28")])), 3.0);
    b.add(g, draw::with_effects(pad, vec![draw::drop_shadow("#000000b0", 2.0, 2.0, 4.0)]));
    let ribs: Vec<Vec<(f64, f64)>> = (0..((bottom - top - 4.0) / 4.0) as i32)
        .map(|i| {
            let y = top + 4.0 + f64::from(i) * 4.0;
            vec![(c - 4.0, y), (c + 4.0, y)]
        })
        .collect();
    b.add(g, ui::strokes("felt ribs", &ribs, 0.6, draw::solid("#00000040")));
    b.add(g, draw::with_effects(draw::rounded(draw::rect("clamp", c - 8.0, top - 4.0, 16.0, 5.0, draw::linear(180.0, &[(0.0, "#e7eaed"), (1.0, "#8d939a")])), 1.5), vec![draw::drop_shadow("#00000080", 0.0, 1.0, 1.5)]));
    b.add(g, draw::circle("screw", c, top - 1.5, 1.4, draw::solid("#4a4e53")));
    // Its tab: a flag that says what it is.
    let tab = draw::rounded(draw::rect("tab", c - 12.0, 2.0, 24.0, 11.0, draw::solid(DRIP)), 2.5);
    b.add(g, ui::glowing(tab, DRIP, 0.6, kit::GLOW));
    ui::plain(b, g, "Decay", c, 7.5, 5.0, "#140a24", 800.0);
    let kind = ControlKind::Handle {
        x: Some(ui::bind(&a("decay"))),
        y: None,
        wheel: None,
        x_map: HandleAxis::new(Axis::Linear { min: 0.0, max: 1.0 }),
        y_map: HandleAxis::new(Axis::Linear { min: 0.0, max: 1.0 }),
    };
    b.controls.insert(g, Control::new(kind).bound(ui::bind(&a("decay"))));
}

/// The tank in `well` (`w` × `h`), its springs centred in the band from
/// `top` down (below a nameplate over the picture).
#[allow(clippy::too_many_lines)]
fn tank(b: &mut Builder<'_>, well: NodeId, (w, h): (f64, f64), top: f64) {
    let compact = w < 300.0;
    // The pan: its lip catches the light; the floor sits in its shadow.
    let pan = draw::rounded(draw::rect("pan", 3.0, 3.0, w - 6.0, h - 6.0, draw::linear(100.0, &[(0.0, "#8f959c"), (0.3, "#d2d6da"), (0.55, "#9aa0a6"), (0.8, "#c3c8cc"), (1.0, "#a2a8ae")])), 5.0);
    let pan = b.add(well, pan);
    kit::steel(b, pan);
    let mut floor = draw::rect("floor", 8.0, 8.0, w - 16.0, h - 16.0, draw::linear(170.0, &[(0.0, "#3b4046"), (0.5, "#2d3136"), (1.0, "#24272b")]));
    floor.corner_radii = document_proto::CornerRadii::uniform(3.0);
    floor.effects = vec![draw::inner_shadow("#000000c0", 0.0, 2.0, 6.0)];
    b.add(well, floor);
    for (x, y) in [(13.0, 13.0), (w - 13.0, 13.0), (13.0, h - 13.0), (w - 13.0, h - 13.0)] {
        b.add(well, draw::circle("rivet", x, y, 2.0, draw::radial_at(0.35, 0.3, 0.8, &[(0.0, "#c9cdd1"), (1.0, "#5b6066")])));
    }

    // The springs, between the transducers.
    let n: i32 = if compact { 2 } else { 3 };
    let (band_t, band_b) = (top.max(16.0), h - 16.0);
    let cy = (band_t + band_b) / 2.0;
    let gap = ((band_b - band_t) / f64::from(n)).min(30.0);
    let r = (gap * 0.26).min(6.5);
    let ys: Vec<f64> = (0..n).map(|i| cy + (f64::from(i) - f64::from(n - 1) / 2.0) * gap).collect();
    let tw = if compact { 20.0 } else { 30.0 };
    // Under a nameplate the cans are short and their names go below.
    let under = top > 0.0;
    let can_pad = if under { 5.0 } else { 9.0 };
    let (th0, th1) = (ys[0] - r - can_pad, ys[ys.len() - 1] + r + can_pad);
    let xa = 16.0 + tw;
    let xb = w - 16.0 - tw;
    let len = xb - xa;
    // A bigger tank, longer springs: the pickup end moves out.
    let shrink = if compact { 0.0 } else { len * 0.2 };
    let size = |b: &mut Builder<'_>, id: NodeId, m: Motion| {
        motion::add(b, id, m.input(format!("norm({VERB}/size)")).dynamics(Dynamics::spring(220.0, 26.0)));
    };
    let rate = format!("lerp(0.62, 0.34, norm({VERB}/size))");
    for (i, y) in ys.iter().enumerate() {
        let f = i as f64;
        let g = b.add(well, draw::frame(&format!("spring {i}"), xa, y - r - 3.0, len, 2.0 * r + 6.0));
        let pts = coil(len, r + 3.0, r, 3.6 + 0.6 * f);
        // Its shadow on the floor: a soft band under it (cheaper than a
        // second coil, and the floor is too dark to show the loops).
        b.add(g, draw::rounded(draw::rect("shadow", 4.0, r + 3.0, len - 8.0, r + 3.0, draw::linear(180.0, &[(0.0, "#00000000"), (0.5, "#00000060"), (1.0, "#00000000")])), r));
        b.add(g, ui::line_node("wire", &pts, 0.85, WIRE));
        if shrink > 0.0 {
            size(b, g, Motion::new(Target::StretchX).range(1.0 - shrink / len, 1.0).pivot(0.0, 0.5));
        }
        // It quivers while it plays, harder the longer it rings.
        let quiver = format!("1 + (0.05 + 0.14 * norm({VERB}/decay)) * sin(time * {:.1} + {f}) * param({VERB}/on)", 23.0 + 5.0 * f);
        motion::add(b, g, Motion::new(Target::StretchY).input(quiver).range(0.0, 1.0).pivot(0.5, 0.5));
        // The drip: a bead of light down the spring and back, fading as
        // it goes — the slower, the longer the decay.
        let bead = draw::rounded(draw::rect("drip", 0.0, 0.0, 22.0, 2.0 * r + 6.0, draw::radial_at(0.5, 0.5, 0.55, &[(0.0, "#ffffff"), (0.35, DRIP), (1.0, &ui::alpha(DRIP, 0.0))])), r + 3.0);
        let bead = b.add(g, ui::glowing(bead, DRIP, 1.8, kit::BLOOM));
        let phase = format!("fract(time * {rate} + {:.2})", 0.37 * f);
        let run = format!("{:.1} * (1 - abs(1 - 2 * {phase}))", len - 22.0);
        motion::add(b, bead, Motion::new(Target::X).input(run).range(0.0, 1.0));
        let fade = format!("param({VERB}/on) * pow(1 - {phase}, lerp(3.5, 0.5, norm({VERB}/decay)))");
        motion::add(b, bead, Motion::new(Target::Opacity).input(fade).range(0.0, 1.0));
    }
    // The transducers over the springs' ends.
    transducer(b, well, (16.0, th0, tw, th1 - th0), &ys, false);
    let out = transducer(b, well, (xb, th0, tw, th1 - th0), &ys, true);
    if shrink > 0.0 {
        size(b, out, Motion::new(Target::X).range(-shrink, 0.0));
    }
    let lbl = if compact { 4.5 } else { 5.5 };
    let lbl_y = if under { th1 + 6.0 } else { (th0 - 6.0).max(12.0) };
    ui::silk(b, well, "In", 16.0 + tw / 2.0, lbl_y, lbl, "#c9ced3", 800.0);
    let ol = ui::silk(b, well, "Out", xb + tw / 2.0, lbl_y, lbl, "#c9ced3", 800.0);
    if !compact {
        size(b, ol, Motion::new(Target::X).range(-shrink, 0.0));
        damper(b, well, (xa + len * 0.3, xa + len * 0.66), h, (th0 + 2.0, th1 - 2.0));
        ui::silk_left(b, well, "Reverberation tank · 3 springs", xa + 8.0, h - 13.0, 5.5, "#aab0b6", 700.0);
    }
}

/// The picture in (`x`, `y`, `w`, `h`) on `panel`, its springs from `top`
/// down: the open tank. Returns its well.
fn picture(b: &mut Builder<'_>, panel: NodeId, (x, y, w, h): (f64, f64, f64, f64), top: f64) -> NodeId {
    let mut well = draw::frame("Tank", x, y, w, h);
    well.fills = vec![draw::linear(180.0, &[(0.0, "#121316"), (1.0, "#1c1e22")])];
    well.clips_content = true;
    well.corner_radii = document_proto::CornerRadii::uniform(6.0);
    well.effects = vec![draw::inner_shadow("#000000c0", 0.0, 2.0, 7.0)];
    let well = b.add(panel, well);
    tank(b, well, (w, h), top);
    kit::scale_children(b, well);
    well
}

/// The cabinet, `w` × `h`: tolex with its weave, and the welt piping
/// round it.
fn ground(b: &mut Builder<'_>, root: NodeId, (w, h): (f64, f64), l: &Look, welt: bool) -> NodeId {
    let mut panel = draw::frame("Panel", 0.0, 0.0, w, h);
    panel.fills = vec![draw::linear(180.0, &l.tolex)];
    panel.corner_radii = document_proto::CornerRadii::uniform(8.0);
    let panel = b.add(root, draw::with_effects(panel, vec![draw::inner_shadow("#ffffff26", 0.0, 1.0, 0.0), draw::inner_shadow("#00000050", 0.0, -2.0, 3.0)]));
    // The weave: a fine cross-hatch.
    let mut weave = Vec::new();
    let mut i = -h;
    while i < w {
        weave.push(vec![(i, 0.0), (i + h, h)]);
        weave.push(vec![(i + h, 0.0), (i, h)]);
        i += 14.0;
    }
    b.add(panel, ui::strokes("weave", &weave, 0.6, draw::solid(l.weave)));
    // (Not on a phone: its knobs' values sit along the foot.)
    if welt {
        let mut piping = draw::rounded(draw::rect("welt", 3.5, 3.5, w - 7.0, h - 7.0, draw::solid("#00000000")), 6.0);
        piping.fills.clear();
        b.add(panel, draw::stroked(piping, &ui::alpha(l.welt, 0.7), 1.4));
    }
    panel
}

/// The nameplate's title: an amp's script.
fn title(b: &mut Builder<'_>, t: NodeId, h: f64, size: f64, ink: &str) {
    let script = draw::Lettering::family(draw::family::PACIFICO).weight(400.0);
    crate::suite_studio_blues::parts::set(b, t, NAME, 34.0, h / 2.0, size, ink, &script, 0.0);
}

#[must_use]
pub fn face(dark: bool) -> Face {
    face_at(dark, kit::Tier::Desktop)
}

/// The face at `tier`: the open tank beside the cabinet's controls. On a
/// phone the tank has two springs and no damper, and the knobs are Size
/// and Level.
#[must_use]
pub fn face_at(dark: bool, tier: kit::Tier) -> Face {
    let l = if dark { &DARK } else { &LIGHT };
    let small_room = matches!(tier, kit::Tier::Phone | kit::Tier::Strip);
    let w = tier.lane_w();
    let scene = kit::tier_lane(NAME, tier, |b, root| {
        let panel = ground(b, root, (w, kit::H), l, !small_room);
        let sw = if small_room { kit::PHONE_PICTURE_R - 10.0 } else { 560.0 };
        let well = picture(b, panel, (10.0, 10.0, sw, kit::H - 20.0), 0.0);
        let lit = ui::alpha(kit::PURPLE_LAMP, 0.55);
        if small_room {
            kit::nameplate_top_right(b, panel, VERB, tier, (6.0, 30.0), 84.0, 220.0, 12.0, (l.accent, l.dim), |b, t| title(b, t, 30.0, 19.0, l.ink));
            let (tx, ty, td) = kit::PHONE_TIME;
            kit::time_knob(b, panel, VERB, (tx, ty, td), 11.0, (l.ink, l.dim, &lit), |b, p, addr| {
                kit::knob3d_bare(b, p, addr, td, (tx, ty), &l.wheel);
            });
            let (xs, ky, kd) = kit::PHONE_KNOBS;
            for (i, (id, legend)) in [("size", "Size"), ("level", "Level")].iter().enumerate() {
                kit::knob3d(b, panel, &a(id), legend, kd, (xs[i], ky), &l.knob, l.ink, l.dim);
            }
        } else {
            kit::nameplate_top_right(b, panel, VERB, tier, (10.0, 28.0), 84.0, 200.0, 10.0, (l.accent, l.dim), |b, t| title(b, t, 28.0, 17.0, l.ink));
            let x0 = 10.0 + sw + 20.0;
            let (tx, ty, td) = (x0 + 40.0, 78.0, 52.0);
            kit::time_knob(b, panel, VERB, (tx, ty, td), 8.5, (l.ink, l.dim, &lit), |b, p, addr| {
                kit::knob3d_bare(b, p, addr, td, (tx, ty), &l.wheel);
            });
            for (i, (id, legend)) in [("size", "Size"), ("tone", "Tone"), ("level", "Level")].iter().enumerate() {
                kit::knob3d(b, panel, &a(id), legend, 34.0, (x0 + 116.0 + 60.0 * i as f64, 90.0), &l.knob, l.ink, l.dim);
            }
        }
        kit::face_bypass(b, panel, VERB, tier, "purple", "On");
        kit::flip_arrow(b, panel, VERB, tier, false, (l.ink, &ui::alpha(l.dim, 0.3)));
        kit::responsive(b, panel, &[well]);
    });
    let mut f = kit::reverb_face(kit::tier_file(FILE, tier, dark), scene);
    f.params.extend(kit::nameplate_params(VERB));
    f.params.extend(kit::time_params(VERB));
    f.params.extend(kit::flip_params(VERB));
    f
}

/// Its back at `tier`: the tolex, no tank — the advanced controls across
/// it, and (standing alone) the nameplate, the bypass and the arrow.
#[must_use]
pub fn back_at(dark: bool, tier: kit::Tier) -> Face {
    let l = if dark { &DARK } else { &LIGHT };
    let small_room = matches!(tier, kit::Tier::Phone | kit::Tier::Strip);
    let w = tier.lane_w();
    let scene = kit::tier_lane(NAME, tier, |b, root| {
        let panel = ground(b, root, (w, kit::H), l, !small_room);
        kit::back_knobs(b, panel, tier, &kit::VERB_MORE, l.ink, |b, p, id, legend, d, at| {
            kit::knob3d(b, p, &a(id), legend, d, at, &l.knob, l.ink, l.dim);
        });
        let (np, preset_w, size, ts) = if small_room { ((6.0, 30.0), 220.0, 12.0, 19.0) } else { ((10.0, 28.0), 200.0, 10.0, 17.0) };
        kit::head(b, panel, VERB, true, |b, head| {
            kit::nameplate_top_right(b, head, VERB, tier, np, 84.0, preset_w, size, (l.accent, l.dim), |b, t| title(b, t, np.1, ts, l.ink));
            kit::face_bypass(b, head, VERB, tier, "purple", "On");
        });
        kit::flip_arrow(b, panel, VERB, tier, true, (l.ink, &ui::alpha(l.dim, 0.3)));
    });
    let mut f = kit::reverb_face(kit::tier_file(BACK, tier, dark), scene);
    f.params.extend(kit::nameplate_params(VERB));
    f.params.extend(kit::flip_params(VERB));
    f
}

/// Its tall face: the tank across the top, its springs below the
/// nameplate, every control under it.
#[must_use]
pub fn tall(dark: bool) -> Face {
    let l = if dark { &DARK } else { &LIGHT };
    let scene = kit::tall_lane(NAME, |b, root| {
        let panel = ground(b, root, (kit::TALL_W, kit::TALL_H), l, true);
        picture(b, panel, (10.0, 10.0, kit::TALL_W - 20.0, ROWS.split - 16.0), 44.0);
        let t = kit::Tall {
            ns: VERB,
            main: &[("size", "Size"), ("tone", "Tone"), ("damping", "HF Damp"), ("level", "Level")],
            inks: (l.ink, l.dim, &ui::alpha(kit::PURPLE_LAMP, 0.55), l.accent),
            scrim: l.scrim,
            bypass: ("purple", "On"),
            title_w: 84.0,
        };
        kit::tall_chrome_rows(b, panel, &t, ROWS, |b, p, addr, legend, d, at| {
            kit::knob3d(b, p, addr, legend, d, at, &l.knob, l.ink, l.dim);
        }, |b, p, addr, (cx, cy, d)| {
            kit::knob3d_bare(b, p, addr, d, (cx, cy), &l.wheel);
        }, |b, tn| title(b, tn, 30.0, 19.0, l.ink));
    });
    let mut f = kit::reverb_face(kit::tall_file(FILE, dark), scene);
    f.params.extend(kit::tall_params(VERB));
    f
}
