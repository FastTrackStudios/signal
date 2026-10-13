//! Magneto: the reverb of the magnetic drum echo — a spinning drum coated
//! in oxide, one head writing to it, four reading it back a little later
//! each, an erase head wiping it before it comes round again. With every
//! head open and the swell up, the echoes run together into a reverb with
//! the drum's own grain. Drawn from above, the deck open: the drum turning
//! (slower the bigger the space, the heads' echoes further apart), the
//! record head's trail glowing round the rim and dying before the erase
//! head — dying later the longer the decay — and the marks it writes
//! fading as they come round.
//!
//! Beside it, where there is room, what the four heads make: each head's
//! tap, then each repeat of all four, softer and smeared, running together
//! into the wash; and the Swell lever under it, which is the decay.
//!
//! Two finishes: light, cream enamel with a red stripe and chrome; dark,
//! graphite hammertone with brass.

use document_proto::NodeId;
use document_proto::motion::{Dynamics, Motion, Target};
use surface_kit::{draw, motion};
use surface_proto::{Axis, Control, ControlKind, HandleAxis};
use surface_widgets::Builder;

use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, VERB};

const NAME: &str = "Magneto";
const FILE: &str = "88-verb-magneto";
const BACK: &str = "88-verb-magneto-back";
/// The tall face's rows: the picture a little under half, big knobs below.
const ROWS: kit::TallRows = kit::TallRows { split: 134.0, main: (30.0, 50.0), more: (126.0, 40.0) };
/// What the heads write: the section's purple.
const FLUX: &str = kit::PURPLE_LAMP;
/// The record head, the four playback heads and the erase head, by angle
/// (0 up, clockwise — the way the drum turns).
const RECORD: f64 = 225.0;
const PLAYBACK: [f64; 4] = [280.0, 320.0, 0.0, 40.0];
const ERASE: f64 = 150.0;
/// The nameplate's title width.
const TITLE_W: f64 = 104.0;

/// One finish's colours.
struct Look {
    ink: &'static str,
    dim: &'static str,
    panel: [(f64, &'static str); 3],
    stripe: &'static str,
    speckle: &'static str,
    knob: [&'static str; 6],
    wheel: [&'static str; 6],
    accent: &'static str,
    title: &'static str,
    scrim: &'static str,
}

const LIGHT: Look = Look {
    ink: "#2a2420",
    dim: "#776b5e",
    panel: [(0.0, "#f1e9d8"), (0.55, "#e4d9c2"), (1.0, "#d4c6aa")],
    stripe: "#8e2a22",
    speckle: "#5a40200c",
    knob: ["color=#2b2725", "pointer=#d9c8ff", "index=line", "index_w=0.12", "skirt=plain", "skirt_color=#c9ccd0"],
    wheel: ["color=#26221f", "pointer=#d9c8ff", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#c4c7cc"],
    accent: "#6a3fc4",
    title: "#8e2a22",
    scrim: "#e4d9c2e6",
};

const DARK: Look = Look {
    ink: "#ebe5da",
    dim: "#9a948a",
    panel: [(0.0, "#2c2d30"), (0.55, "#212225"), (1.0, "#17181a")],
    stripe: "#a8874a",
    speckle: "#ffffff08",
    knob: ["color=#c9a45c", "pointer=#2a1a0c", "index=line", "index_w=0.12", "skirt=plain", "skirt_color=#6e5428"],
    wheel: ["color=#c9a45c", "pointer=#2a1a0c", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#5e4622"],
    accent: kit::PURPLE_LAMP,
    title: "#d9b56a",
    scrim: "#212225e6",
};

fn a(id: &str) -> String {
    format!("{VERB}/{id}")
}

/// The drum's turns per second: slower the bigger the space.
fn rate() -> String {
    format!("lerp(0.5, 0.2, norm({VERB}/size))")
}

/// A head at `deg` round the drum (`c`, radius `r`): its block facing the
/// drum, its gap, and its name outside it; a lamp beside a playback head.
fn head(b: &mut Builder<'_>, parent: NodeId, c: (f64, f64), r: f64, deg: f64, name: &str, playback: bool, compact: bool) {
    let (hw, hh) = if compact { (6.0, 9.0) } else { (8.0, 12.0) };
    let (x, y) = draw::polar(c.0, c.1, r + hh / 2.0 + 1.5, deg);
    let block = draw::rounded(draw::rect("head", x - hw / 2.0, y - hh / 2.0, hw, hh, draw::linear(90.0, &[(0.0, "#55595e"), (0.5, "#c9cdd2"), (1.0, "#4a4e53")])), 1.5);
    b.add(parent, draw::with_effects(draw::rotated(block, deg), vec![draw::drop_shadow("#000000a0", 0.0, 1.5, 2.0)]));
    let (gx, gy) = draw::polar(c.0, c.1, r + 1.6, deg);
    b.add(parent, draw::circle("gap", gx, gy, 0.9, draw::solid("#141414")));
    let (lx, ly) = draw::polar(c.0, c.1, r + hh + if compact { 7.0 } else { 9.0 }, deg);
    ui::silk(b, parent, name, lx, ly, if compact { 5.0 } else { 6.0 }, "#d8d2c6", 800.0);
    if playback && !compact {
        let (px, py) = draw::polar(c.0, c.1, r + hh + 9.0, deg + 9.0);
        let lamp = b.add(parent, ui::glowing(draw::circle("lamp", px, py, 1.8, draw::solid(FLUX)), FLUX, 1.1, kit::GLOW));
        motion::add(b, lamp, Motion::new(Target::Opacity).input(format!("0.2 + 0.8 * param({VERB}/on)")).range(0.0, 1.0));
    }
}

/// The drum round `c`, radius `r`, its heads round it.
#[allow(clippy::too_many_lines)]
fn drum(b: &mut Builder<'_>, parent: NodeId, c: (f64, f64), r: f64, compact: bool) {
    // Its well in the deck.
    b.add(parent, draw::with_effects(draw::circle("deck well", c.0, c.1, r + 3.0, draw::solid("#0a0908")), vec![draw::inner_shadow("#000000", 0.0, 2.0, 5.0)]));
    // The trail the record head leaves round the rim, dying before the
    // erase head: soon at a short decay, late at a long one.
    let sweep = (ERASE - RECORD).rem_euclid(360.0);
    for (name, reach, on) in [("trail short", 0.3, format!("1 - norm({VERB}/decay)")), ("trail long", 0.95, format!("norm({VERB}/decay)"))] {
        let stops = [(0.0, ui::alpha(FLUX, 0.0)), (0.005, ui::alpha(FLUX, 0.95)), (reach * sweep / 360.0, ui::alpha(FLUX, 0.0)), (1.0, ui::alpha(FLUX, 0.0))];
        let s: Vec<(f64, &str)> = stops.iter().map(|(p, c)| (*p, c.as_str())).collect();
        let band = draw::path(name, c.0 - r - 4.0, c.1 - r - 4.0, 2.0 * r + 8.0, 2.0 * r + 8.0, draw::sector(r + 4.0, r + 4.0, r - 4.5, r + 3.0, RECORD, RECORD + sweep), draw::conic(RECORD, &s));
        let band = b.add(parent, ui::glowing(band, FLUX, 0.8, kit::GLOW));
        motion::add(b, band, Motion::new(Target::Opacity).input(format!("({on}) * param({VERB}/on)")).range(0.0, 1.0).dynamics(Dynamics::spring(200.0, 26.0)));
    }
    // The drum, turning: the oxide band, the spun face, the hub.
    let spin = b.add(parent, draw::frame("Drum", c.0 - r, c.1 - r, 2.0 * r, 2.0 * r));
    motion::add(b, spin, Motion::new(Target::Rotate).input(format!("fract(time * {})", rate())).range(0.0, 360.0).pivot(0.5, 0.5));
    let (k, kr) = (r, r);
    let oxide = draw::circle("oxide", k, k, kr, draw::conic(0.0, &[(0.0, "#4a3726"), (0.25, "#2e2117"), (0.5, "#56412d"), (0.75, "#2a1e15"), (1.0, "#4a3726")]));
    b.add(spin, draw::with_effects(oxide, vec![draw::inner_shadow("#00000090", 0.0, 0.0, 3.0)]));
    // The oxide's grain, so the turning shows.
    let grain: Vec<Vec<(f64, f64)>> = (0..36).map(|i| {
        let d = f64::from(i) * 10.0 + 3.0 * crate::reverb::jitter(f64::from(i));
        vec![draw::polar(k, k, kr - 4.0, d), draw::polar(k, k, kr - 1.0, d)]
    }).collect();
    b.add(spin, ui::strokes("grain", &grain, 0.7, draw::solid("#ffffff1c")));
    let face = draw::circle("spun face", k, k, kr * 0.74, draw::conic(20.0, &[(0.0, "#8c9298"), (0.12, "#e4e7ea"), (0.25, "#979da3"), (0.38, "#d6dadd"), (0.5, "#8a9096"), (0.62, "#e1e4e7"), (0.75, "#959ba1"), (0.88, "#d4d8db"), (1.0, "#8c9298")]));
    b.add(spin, draw::with_effects(face, vec![draw::drop_shadow("#00000080", 0.0, 1.0, 2.0)]));
    b.add(spin, draw::circle("hub", k, k, kr * 0.2, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#4a4d51"), (1.0, "#16171a")])));
    for i in 0..3 {
        let (sx, sy) = draw::polar(k, k, kr * 0.46, 120.0 * f64::from(i));
        b.add(spin, draw::circle("screw", sx, sy, if compact { 1.6 } else { 2.2 }, draw::radial_at(0.35, 0.3, 0.8, &[(0.0, "#e8eaec"), (1.0, "#5a5e63")])));
        let (ax, ay) = draw::polar(sx, sy, 1.5, 120.0 * f64::from(i) + 30.0);
        let (bx, by) = draw::polar(sx, sy, 1.5, 120.0 * f64::from(i) + 210.0);
        ui::line(b, spin, "slot", &[(ax, ay), (bx, by)], 0.6, "#2a2c2f");
    }
    // The marks the record head writes, carried round and fading as they
    // go: gone by the erase head at a short decay, nearly round at a long.
    let marks = if compact { 3 } else { 5 };
    for i in 0..marks {
        let at = 360.0 * f64::from(i) / f64::from(marks);
        let (mx, my) = draw::polar(k, k, kr - 2.5, at);
        let m = b.add(spin, ui::glowing(draw::circle("mark", mx, my, if compact { 2.0 } else { 2.6 }, draw::solid("#f1e8ff")), FLUX, 1.4, kit::GLOW));
        let age = format!("fract(time * {} + {:.4})", rate(), (at - RECORD).rem_euclid(360.0) / 360.0);
        let fade = format!("param({VERB}/on) * step({age}, {:.4}) * pow(1 - {age} / {:.4}, lerp(3.5, 0.6, norm({VERB}/decay)))", sweep / 360.0, sweep / 360.0);
        motion::add(b, m, Motion::new(Target::Opacity).input(fade).range(0.0, 1.0));
    }
    // The heads round it.
    head(b, parent, c, r, RECORD, "Rec", false, compact);
    for (i, d) in PLAYBACK.iter().enumerate() {
        head(b, parent, c, r, *d, &(i + 1).to_string(), true, compact);
    }
    head(b, parent, c, r, ERASE, "Erase", false, compact);
}

/// What the heads make, in (`x`, `y`, `w`, `h`): each head's tap, then
/// each repeat of all four, softer and smeared — spaced by the drum's
/// speed (the size), lasting with the decay.
fn taps(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64)) {
    let line = "#d8d2c6";
    ui::silk_left(b, parent, "Heads 1 · 2 · 3 · 4, and their repeats", x, y - 8.0, 5.5, line, 800.0);
    let base = y + h;
    ui::line(b, parent, "axis", &[(x, base), (x + w, base)], 0.8, &ui::alpha(line, 0.55));
    // The wash the repeats run into.
    kit::decay(b, parent, (x, y + h * 0.35, w, h * 0.65), &ui::alpha(FLUX, 0.5));
    let level = [0.95, 0.78, 0.86, 0.7];
    let reps = 5;
    let span = f64::from(4 * reps + 1);
    for r in 0..reps {
        for (hd, lv) in level.iter().enumerate() {
            let t = (hd as f64 + 1.0 + 4.0 * f64::from(r)) / span;
            let tall = (h - 6.0) * lv;
            let bw = 2.2 + 1.3 * f64::from(r);
            let bar = b.add(parent, draw::frame("tap", x - bw / 2.0, base - tall, bw, tall));
            let fill = ui::alpha(FLUX, 1.0 - 0.15 * f64::from(r));
            let body = draw::rect("line", 0.0, 0.0, bw, tall, draw::linear(180.0, &[(0.0, &fill), (1.0, &ui::alpha(FLUX, 0.35))]));
            b.add(bar, if r == 0 { ui::glowing(body, FLUX, 1.0, kit::GLOW) } else { body });
            motion::add(b, bar, Motion::new(Target::X).input(format!("{:.2} * (0.6 + 0.4 * norm({VERB}/size))", t * w)).range(0.0, 1.0).dynamics(Dynamics::spring(160.0, 24.0)));
            if r > 0 {
                motion::add(b, bar, Motion::new(Target::StretchY).input(format!("pow(0.2 + 0.72 * norm({VERB}/decay), {r})")).range(0.0, 1.0).pivot(0.5, 1.0).dynamics(Dynamics::spring(200.0, 26.0)));
            }
        }
    }
}

/// The Swell lever in a slot from `x0` to `x1` at `y`: a handle on the
/// decay.
fn swell(b: &mut Builder<'_>, parent: NodeId, (x0, x1): (f64, f64), y: f64) {
    ui::silk_right(b, parent, "Swell", x0 - 8.0, y, 6.0, "#d8d2c6", 800.0);
    b.add(parent, draw::with_effects(draw::rounded(draw::rect("slot", x0 - 4.0, y - 2.5, x1 - x0 + 8.0, 5.0, draw::solid("#050505")), 2.5), vec![draw::inner_shadow("#000000", 0.0, 1.0, 2.0), draw::drop_shadow("#ffffff18", 0.0, 1.0, 0.0)]));
    let area = b.add(parent, draw::frame("swell travel", x0, y - 9.0, x1 - x0, 18.0));
    let hw = 18.0;
    let g = b.add(area, draw::frame("Swell", 0.4 * (x1 - x0) - hw / 2.0, 0.0, hw, 18.0));
    b.add(g, draw::with_effects(draw::circle("lever", 9.0, 9.0, 7.0, draw::radial_at(0.38, 0.3, 0.85, &[(0.0, "#ffffff"), (0.45, "#c3c7cc"), (1.0, "#575b60")])), vec![draw::drop_shadow("#000000b0", 0.0, 2.0, 3.0)]));
    b.add(g, ui::glowing(draw::circle("index", 9.0, 9.0, 2.0, draw::solid(FLUX)), FLUX, 1.0, kit::GLOW));
    let kind = ControlKind::Handle {
        x: Some(ui::bind(&a("decay"))),
        y: None,
        wheel: None,
        x_map: HandleAxis::new(Axis::Linear { min: 0.0, max: 1.0 }),
        y_map: HandleAxis::new(Axis::Linear { min: 0.0, max: 1.0 }),
    };
    b.controls.insert(g, Control::new(kind).bound(ui::bind(&a("decay"))));
}

/// The picture in (`x`, `y`, `w`, `h`) on `panel`: the deck with its drum
/// and, where it is wide, the taps and the Swell lever from `top` down
/// (below a nameplate over the picture). Returns its well.
fn picture(b: &mut Builder<'_>, panel: NodeId, (x, y, w, h): (f64, f64, f64, f64), top: f64) -> NodeId {
    let mut well = draw::frame("Deck", x, y, w, h);
    well.fills = vec![draw::linear(180.0, &[(0.0, "#1b1917"), (1.0, "#0e0d0c")])];
    well.clips_content = true;
    well.corner_radii = document_proto::CornerRadii::uniform(6.0);
    well.effects = vec![draw::inner_shadow("#000000c0", 0.0, 2.0, 7.0)];
    let well = b.add(panel, well);
    let wide = w > 360.0;
    let dw = if wide { (h * 1.3).min(w * 0.4) } else { w };
    let r = dw.min(h) * if wide { 0.3 } else { 0.31 };
    let c = (dw / 2.0 + if wide { 4.0 } else { 0.0 }, h / 2.0 + 2.0);
    drum(b, well, c, r, !wide);
    if wide {
        ui::rule(b, well, dw + 6.0, 12.0, 1.0, h - 24.0, "#ffffff14");
        let rx = dw + 24.0;
        let rw = w - rx - 16.0;
        let ry = top.max(24.0);
        taps(b, well, (rx, ry, rw, h - ry - 32.0));
        swell(b, well, (rx + 34.0, rx + rw * 0.55), h - 13.0);
        ui::silk_right(b, well, "Drum · 4 heads", rx + rw, h - 13.0, 5.5, "#8f877c", 700.0);
    }
    kit::glass(b, well);
    well
}

/// The panel, `w` × `h`: enamel (or hammertone) with its stripe.
fn ground(b: &mut Builder<'_>, root: NodeId, (w, h): (f64, f64), l: &Look) -> NodeId {
    let mut panel = draw::frame("Panel", 0.0, 0.0, w, h);
    panel.fills = vec![draw::linear(180.0, &l.panel)];
    panel.corner_radii = document_proto::CornerRadii::uniform(8.0);
    let panel = b.add(root, draw::with_effects(panel, vec![draw::inner_shadow("#ffffff2a", 0.0, 1.0, 0.0), draw::inner_shadow("#00000050", 0.0, -2.0, 3.0)]));
    // The finish's speckle.
    let mut dots = String::new();
    for i in 0..((w * h) / 260.0) as i32 {
        let f = f64::from(i);
        let px = w * (0.5 + 0.5 * crate::reverb::jitter(f * 1.618 + 0.2));
        let py = h * (0.5 + 0.5 * crate::reverb::jitter(f * 2.414 + 0.7));
        dots.push_str(&draw::regular(px, py, 0.6 + 0.6 * crate::reverb::jitter(f).abs(), 5, 0.0));
        dots.push(' ');
    }
    b.add(panel, ui::area("speckle", dots, w, h, draw::solid(l.speckle)));
    // The stripe along the top, a chrome line under it.
    b.add(panel, draw::rect("stripe", 8.0, 3.0, w - 16.0, 3.0, draw::solid(&ui::alpha(l.stripe, 0.85))));
    b.add(panel, draw::rect("chrome", 8.0, 6.0, w - 16.0, 1.0, draw::linear(90.0, &[(0.0, "#9aa0a6"), (0.5, "#f0f2f4"), (1.0, "#9aa0a6")])));
    panel
}

/// The nameplate's title: tall capitals, spaced.
fn title(b: &mut Builder<'_>, t: NodeId, h: f64, size: f64, ink: &str) {
    let caps = draw::Lettering::family(draw::family::OSWALD).weight(600.0);
    crate::suite_studio_blues::parts::set(b, t, "MAGNETO", 44.0, h / 2.0, size, ink, &caps, 0.12);
}

#[must_use]
pub fn face(dark: bool) -> Face {
    face_at(dark, kit::Tier::Desktop)
}

/// The face at `tier`: the deck, its taps and the Swell lever beside the
/// controls. On a phone the picture is the drum alone and the knobs are
/// Size and Level.
#[must_use]
pub fn face_at(dark: bool, tier: kit::Tier) -> Face {
    let l = if dark { &DARK } else { &LIGHT };
    let small_room = matches!(tier, kit::Tier::Phone | kit::Tier::Strip);
    let w = tier.lane_w();
    let scene = kit::tier_lane(NAME, tier, |b, root| {
        let panel = ground(b, root, (w, kit::H), l);
        let sw = if small_room { kit::PHONE_PICTURE_R - 10.0 } else { 560.0 };
        let well = picture(b, panel, (10.0, 10.0, sw, kit::H - 20.0), 0.0);
        let lit = ui::alpha(kit::PURPLE_LAMP, 0.55);
        if small_room {
            kit::nameplate_top_right(b, panel, VERB, tier, (6.0, 30.0), TITLE_W, 210.0, 12.0, (l.accent, l.dim), |b, t| title(b, t, 30.0, 16.0, l.title));
            let (tx, ty, td) = kit::PHONE_TIME;
            kit::time_knob(b, panel, VERB, (tx, ty, td), 11.0, (l.ink, l.dim, &lit), |b, p, addr| {
                kit::knob3d_bare(b, p, addr, td, (tx, ty), &l.wheel);
            });
            let (xs, ky, kd) = kit::PHONE_KNOBS;
            for (i, (id, legend)) in [("size", "Size"), ("level", "Level")].iter().enumerate() {
                kit::knob3d(b, panel, &a(id), legend, kd, (xs[i], ky), &l.knob, l.ink, l.dim);
            }
        } else {
            kit::nameplate_top_right(b, panel, VERB, tier, (10.0, 28.0), TITLE_W, 200.0, 10.0, (l.accent, l.dim), |b, t| title(b, t, 28.0, 15.0, l.title));
            let x0 = 10.0 + sw + 20.0;
            let (tx, ty, td) = (x0 + 40.0, 78.0, 52.0);
            kit::time_knob(b, panel, VERB, (tx, ty, td), 8.5, (l.ink, l.dim, &lit), |b, p, addr| {
                kit::knob3d_bare(b, p, addr, td, (tx, ty), &l.wheel);
            });
            for (i, (id, legend)) in [("size", "Size"), ("predelay", "Pre-Delay"), ("level", "Level")].iter().enumerate() {
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

/// Its back at `tier`: the panel, no deck — the advanced controls across
/// it, and (standing alone) the nameplate, the bypass and the arrow.
#[must_use]
pub fn back_at(dark: bool, tier: kit::Tier) -> Face {
    let l = if dark { &DARK } else { &LIGHT };
    let small_room = matches!(tier, kit::Tier::Phone | kit::Tier::Strip);
    let w = tier.lane_w();
    let scene = kit::tier_lane(NAME, tier, |b, root| {
        let panel = ground(b, root, (w, kit::H), l);
        kit::back_knobs(b, panel, tier, &kit::VERB_MORE, l.ink, |b, p, id, legend, d, at| {
            kit::knob3d(b, p, &a(id), legend, d, at, &l.knob, l.ink, l.dim);
        });
        let (np, preset_w, size, ts) = if small_room { ((6.0, 30.0), 210.0, 12.0, 16.0) } else { ((10.0, 28.0), 200.0, 10.0, 15.0) };
        kit::head(b, panel, VERB, true, |b, head| {
            kit::nameplate_top_right(b, head, VERB, tier, np, TITLE_W, preset_w, size, (l.accent, l.dim), |b, t| title(b, t, np.1, ts, l.title));
            kit::face_bypass(b, head, VERB, tier, "purple", "On");
        });
        kit::flip_arrow(b, panel, VERB, tier, true, (l.ink, &ui::alpha(l.dim, 0.3)));
    });
    let mut f = kit::reverb_face(kit::tier_file(BACK, tier, dark), scene);
    f.params.extend(kit::nameplate_params(VERB));
    f.params.extend(kit::flip_params(VERB));
    f
}

/// Its tall face: the deck, its taps and the Swell lever across the top,
/// every control under them.
#[must_use]
pub fn tall(dark: bool) -> Face {
    let l = if dark { &DARK } else { &LIGHT };
    let scene = kit::tall_lane(NAME, |b, root| {
        let panel = ground(b, root, (kit::TALL_W, kit::TALL_H), l);
        picture(b, panel, (10.0, 10.0, kit::TALL_W - 20.0, ROWS.split - 16.0), 48.0);
        let t = kit::Tall {
            ns: VERB,
            main: &[("size", "Size"), ("predelay", "Pre-Delay"), ("damping", "HF Damp"), ("level", "Level")],
            inks: (l.ink, l.dim, &ui::alpha(kit::PURPLE_LAMP, 0.55), l.accent),
            scrim: l.scrim,
            bypass: ("purple", "On"),
            title_w: TITLE_W,
        };
        kit::tall_chrome_rows(b, panel, &t, ROWS, |b, p, addr, legend, d, at| {
            kit::knob3d(b, p, addr, legend, d, at, &l.knob, l.ink, l.dim);
        }, |b, p, addr, (cx, cy, d)| {
            kit::knob3d_bare(b, p, addr, d, (cx, cy), &l.wheel);
        }, |b, tn| title(b, tn, 30.0, 16.0, l.title));
    });
    let mut f = kit::reverb_face(kit::tall_file(FILE, dark), scene);
    f.params.extend(kit::tall_params(VERB));
    f
}
