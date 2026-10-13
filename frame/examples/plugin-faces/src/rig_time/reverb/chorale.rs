//! Chorale: a reverb that sings — its tail is shaped by vocal formants,
//! so the space answers like a choir holding a vowel. Drawn as the
//! phonetician draws a vowel: the vowel chart (close at the top, open at
//! the bottom, front at the left, back at the right), and on it the
//! choir — a cluster of voices that drift about their vowel as they hold
//! it. The tone moves the choir through the vowels, dark "oo" to bright
//! "ee"; the space spreads the voices wider; the modulation sets them
//! drifting.
//!
//! Beside the chart, where there is room, the vowel's formants (the
//! resonances the choir sings through, sliding as the vowel changes), and
//! the held chord: four parts, soprano to bass, sustained under a
//! fermata. The fermata is the decay — drag it out and the choir holds
//! longer.
//!
//! Two finishes, as a choir loft is: light, limestone and gold leaf; dark,
//! carved walnut and gold.

use document_proto::NodeId;
use document_proto::motion::{Dynamics, Motion, Target};
use surface_kit::{draw, motion};
use surface_proto::{Axis, Control, ControlKind, HandleAxis};
use surface_widgets::Builder;

use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, VERB};

const NAME: &str = "Chorale";
const FILE: &str = "87-verb-chorale";
const BACK: &str = "87-verb-chorale-back";
/// The tall face's rows: the picture a little under half, big knobs below.
const ROWS: kit::TallRows = kit::TallRows { split: 134.0, main: (30.0, 50.0), more: (126.0, 40.0) };
/// The voices' light: the section's purple, and their gold chart.
const VOICE: &str = kit::PURPLE_LAMP;
const GOLD: &str = "#d9b56a";
/// The nameplate's title width.
const TITLE_W: f64 = 96.0;

/// One finish's colours.
struct Look {
    ink: &'static str,
    dim: &'static str,
    panel: [(f64, &'static str); 3],
    tracery: &'static str,
    well: [(f64, &'static str); 2],
    knob: [&'static str; 6],
    wheel: [&'static str; 6],
    accent: &'static str,
    title: &'static str,
    scrim: &'static str,
}

const LIGHT: Look = Look {
    ink: "#2d2116",
    dim: "#7a6a52",
    panel: [(0.0, "#f2ebdc"), (0.55, "#e6dbc4"), (1.0, "#d6c7a8")],
    tracery: "#8a6a2c30",
    well: [(0.0, "#161c38"), (1.0, "#0b0f22")],
    knob: ["color=#f5efe2", "pointer=#3b1d6e", "index=line", "index_w=0.12", "skirt=plain", "skirt_color=#b08a3e"],
    wheel: ["color=#f5efe2", "pointer=#3b1d6e", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#9a7631"],
    accent: "#6a3fc4",
    title: "#5a2a14",
    scrim: "#e6dbc4e6",
};

const DARK: Look = Look {
    ink: "#efe3cf",
    dim: "#a8977d",
    panel: [(0.0, "#2e2119"), (0.55, "#22170f"), (1.0, "#170f0a")],
    tracery: "#d9b56a24",
    well: [(0.0, "#121733"), (1.0, "#080b1a")],
    knob: ["color=#3a2a1f", "pointer=#e9dcc0", "index=line", "index_w=0.12", "skirt=plain", "skirt_color=#b08a3e"],
    wheel: ["color=#35261c", "pointer=#e9dcc0", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#9a7631"],
    accent: kit::PURPLE_LAMP,
    title: GOLD,
    scrim: "#22170fe6",
};

fn a(id: &str) -> String {
    format!("{VERB}/{id}")
}

/// The chart's corners in (`x`, `y`, `w`, `h`): close front, close back,
/// open back, open front (the open front drawn in, as the chart is).
fn corners((x, y, w, h): (f64, f64, f64, f64)) -> [(f64, f64); 4] {
    [(x, y), (x + w, y), (x + w, y + h), (x + w * 0.38, y + h)]
}

/// The vowel chart in `at`, and the choir on it.
#[allow(clippy::too_many_lines)]
fn chart(b: &mut Builder<'_>, parent: NodeId, at: (f64, f64, f64, f64), compact: bool) {
    let (x, y, w, h) = at;
    let c = corners(at);
    let mut edge: Vec<(f64, f64)> = c.to_vec();
    edge.push(c[0]);
    ui::line(b, parent, "chart", &edge, 1.0, &ui::alpha(GOLD, 0.75));
    // The chart's inner lines: close-mid and open-mid, and the centre.
    let lerp = |p: (f64, f64), q: (f64, f64), t: f64| (p.0 + (q.0 - p.0) * t, p.1 + (q.1 - p.1) * t);
    let mut inner = Vec::new();
    for t in [1.0 / 3.0, 2.0 / 3.0] {
        inner.push(vec![lerp(c[0], c[3], t), lerp(c[1], c[2], t)]);
    }
    inner.push(vec![lerp(c[0], c[1], 0.5), lerp(c[3], c[2], 0.5)]);
    b.add(parent, ui::strokes("chart lines", &inner, 0.6, draw::solid(&ui::alpha(GOLD, 0.3))));
    // The vowels at their places.
    let size = if compact { 7.0 } else { 8.5 };
    let serif = draw::Lettering::family(draw::family::PLAYFAIR_DISPLAY).weight(400.0);
    let vowels = [("i", lerp(c[0], c[1], 0.0), (-7.0, -1.0)), ("u", c[1], (7.0, -1.0)), ("e", lerp(c[0], c[3], 0.5), (-7.0, 0.0)), ("o", lerp(c[1], c[2], 0.5), (7.0, 0.0)), ("a", c[3], (-6.0, 4.0)), ("ɑ", c[2], (7.0, 4.0))];
    for (v, p, off) in vowels {
        crate::suite_studio_blues::parts::set(b, parent, v, p.0 + off.0, p.1 + off.1, size, &ui::alpha(GOLD, 0.95), &serif, 0.0);
    }
    if !compact {
        ui::silk_left(b, parent, "Front", x, y + h + 9.0, 5.0, &ui::alpha(GOLD, 0.6), 800.0);
        ui::silk_right(b, parent, "Back", x + w, y + h + 9.0, 5.0, &ui::alpha(GOLD, 0.6), 800.0);
    }

    // Where the choir is: the tone carries it from "oo" (back, close) through
    // "ah" (open) to "ee" (front, close).
    let tone = format!("param({VERB}/tone)");
    let cx = format!("({:.1} - ({tone} + 1) / 2 * {:.1})", x + w * 0.86, w * 0.74);
    let cy = format!("({:.1} + {:.1} * (1 - {tone} * {tone}))", y + h * 0.2, h * 0.44);
    let spread = format!("(0.45 + 0.55 * norm({VERB}/size))");
    let reach = w.min(h) * 0.42;
    // Its aura: the field its voices fill.
    let r = reach * 1.2;
    let aura = b.add(parent, draw::circle("aura", r, r, r, draw::radial_at(0.5, 0.5, 0.5, &[(0.0, &ui::alpha(VOICE, 0.32)), (0.6, &ui::alpha(VOICE, 0.1)), (1.0, &ui::alpha(VOICE, 0.0))])));
    let follow = |b: &mut Builder<'_>, id: NodeId, dx: &str, dy: &str| {
        motion::add(b, id, Motion::new(Target::X).input(format!("{cx} + {dx}")).range(0.0, 1.0).dynamics(Dynamics::spring(120.0, 18.0)));
        motion::add(b, id, Motion::new(Target::Y).input(format!("{cy} + {dy}")).range(0.0, 1.0).dynamics(Dynamics::spring(120.0, 18.0)));
    };
    follow(b, aura, &format!("{:.1}", -r), &format!("{:.1}", -r));
    motion::add(b, aura, Motion::new(Target::Scale).input(format!("{spread} * (0.5 + 0.5 * param({VERB}/on))")).range(0.0, 1.0).pivot(0.5, 0.5).dynamics(Dynamics::spring(160.0, 22.0)));
    // The voices: each its own place in the cluster, drifting about it.
    let n = if compact { 9 } else { 14 };
    for i in 0..n {
        let f = f64::from(i);
        let (jx, jy) = (crate::reverb::jitter(f * 1.91 + 0.3), crate::reverb::jitter(f * 2.73 + 1.1));
        let rv = if i % 3 == 0 { 2.8 } else { 2.0 };
        let v = draw::circle("voice", 0.0, 0.0, rv, draw::radial_at(0.5, 0.5, 0.5, &[(0.0, "#ffffff"), (0.5, "#e6d8ff"), (1.0, VOICE)]));
        let v = b.add(parent, ui::glowing(v, VOICE, 1.0, kit::GLOW));
        let wob = |k: f64| format!("{:.1} * norm({VERB}/modulation) * sin(time * {:.2} + {:.2})", reach * 0.22, 0.6 + 0.5 * crate::reverb::jitter(f * k).abs(), f * k);
        follow(b, v, &format!("{:.2} * {spread} + {}", jx * reach, wob(1.3)), &format!("{:.2} * {spread} + {}", jy * reach * 0.7, wob(2.9)));
        motion::add(b, v, Motion::new(Target::Opacity).input(format!("0.35 + 0.65 * param({VERB}/on)")).range(0.0, 1.0));
    }
}

/// The formants in (`x`, `y`, `w`, `h`): the vowel's resonances, sliding
/// from "oo" through "ah" to "ee" with the tone.
fn formants(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64)) {
    ui::silk_left(b, parent, "Formants", x, y - 8.0, 5.5, &ui::alpha(GOLD, 0.85), 800.0);
    // The voice's harmonics, a comb under them.
    let teeth: Vec<Vec<(f64, f64)>> = (1..28).map(|k| {
        let tx = w * f64::from(k) / 28.0;
        vec![(x + tx, y + h), (x + tx, y + h - h * 0.9 * (1.0 - f64::from(k) / 34.0))]
    }).collect();
    b.add(parent, ui::strokes("harmonics", &teeth, 0.6, draw::solid(&ui::alpha(GOLD, 0.16))));
    ui::line(b, parent, "axis", &[(x, y + h), (x + w, y + h)], 0.8, &ui::alpha(GOLD, 0.6));
    // Each vowel's first three formants (centre, height) across the band.
    let vowels: [[(f64, f64); 3]; 3] = [
        [(0.1, 0.95), (0.2, 0.6), (0.56, 0.25)],
        [(0.28, 1.0), (0.42, 0.85), (0.62, 0.35)],
        [(0.08, 0.8), (0.64, 0.75), (0.78, 0.5)],
    ];
    let curve = |fs: &[(f64, f64); 3]| -> String {
        let mut d = String::new();
        for i in 0..=60 {
            let u = f64::from(i) / 60.0;
            let e: f64 = fs.iter().map(|(c, a)| a * (-((u - c) / 0.055).powi(2)).exp()).sum::<f64>().min(1.0);
            d += &format!("{} {:.2} {:.2} ", if i == 0 { "M" } else { "L" }, u * w, h - (h - 3.0) * (0.06 + 0.94 * e));
        }
        d
    };
    let curves: Vec<String> = vowels.iter().map(curve).collect();
    let areas: Vec<String> = curves.iter().map(|c| format!("M 0 {h:.2} L{} L {w:.2} {h:.2} Z", &c[1..])).collect();
    let morph = |b: &mut Builder<'_>, id: NodeId, shapes: Vec<String>| {
        motion::add(b, id, Motion::new(Target::Morph { shapes }).input(format!("param({VERB}/tone) + 1")).range(0.0, 1.0).dynamics(Dynamics::spring(160.0, 22.0)));
    };
    let fill = b.add(parent, draw::path("formant fill", x, y, w, h, areas[1].clone(), draw::linear(180.0, &[(0.0, &ui::alpha(VOICE, 0.55)), (1.0, &ui::alpha(VOICE, 0.06))])));
    morph(b, fill, areas);
    let mut edge = ui::strokes("formants", &[vec![(0.0, 0.0), (w, h)]], 1.5, draw::solid(VOICE));
    edge.transform = document_proto::Transform::translate(x, y);
    edge.size = document_proto::Size { width: w, height: h };
    edge.fill_geometry = vec![curves[1].clone()];
    let edge = b.add(parent, ui::glowing(edge, VOICE, 0.9, kit::GLOW));
    morph(b, edge, curves);
}

/// The held chord in (`x`, `y`, `w`, `h`) on `parent` (`ph` high): four
/// parts sustained under a fermata, a handle on the decay.
fn hold(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64), ph: f64) {
    ui::silk_left(b, parent, "Held", x, y - 8.0, 5.5, &ui::alpha(GOLD, 0.85), 800.0);
    let lx = x + 12.0;
    let lw = w - 12.0;
    let parts = ["S", "A", "T", "B"];
    let g = b.add(parent, draw::frame("Chord", lx, y, lw, h));
    for (i, p) in parts.iter().enumerate() {
        let py = h * (0.2 + 0.2 * i as f64) + 6.0;
        ui::silk(b, parent, p, x + 4.0, y + py, 5.5, &ui::alpha(GOLD, 0.8), 800.0);
        let line = draw::rounded(draw::rect(&format!("part {p}"), 0.0, py - 1.5, lw, 3.0, draw::linear(90.0, &[(0.0, "#ffffff"), (0.08, VOICE), (0.75, &ui::alpha(VOICE, 0.7)), (1.0, &ui::alpha(VOICE, 0.1))])), 1.5);
        b.add(g, ui::glowing(line, VOICE, 0.9, kit::GLOW));
    }
    motion::add(b, g, Motion::new(Target::StretchX).input(format!("norm({VERB}/decay)")).range(0.3, 1.0).pivot(0.0, 0.5).dynamics(Dynamics::spring(220.0, 26.0)));
    // The choir breathes as it holds.
    motion::add(b, g, Motion::new(Target::Opacity).input(format!("(0.8 + 0.2 * sin(time * 1.1)) * (0.4 + 0.6 * param({VERB}/on))")).range(0.0, 1.0));
    // The fermata over the barline where the hold ends.
    let (x0, x1) = (lx + lw * 0.3, lx + lw);
    let area = b.add(parent, draw::frame("hold travel", x0, 0.0, x1 - x0, ph));
    let hw = 26.0;
    let f = b.add(area, draw::frame("Fermata", 0.4 * (x1 - x0) - hw / 2.0, 0.0, hw, ph));
    let c = hw / 2.0;
    b.add(f, draw::rect("barline", c - 0.8, y + 2.0, 1.6, h - 2.0, draw::solid(&ui::alpha(GOLD, 0.9))));
    let arc: Vec<(f64, f64)> = (0..=16).map(|i| {
        let t = std::f64::consts::PI * f64::from(i) / 16.0;
        (c - 8.0 * t.cos(), y - 2.0 - 7.0 * t.sin())
    }).collect();
    b.add(f, ui::glowing(ui::line_node("fermata", &arc, 1.8, GOLD), GOLD, 0.6, kit::GLOW));
    b.add(f, draw::circle("fermata dot", c, y - 3.5, 1.6, draw::solid(GOLD)));
    let kind = ControlKind::Handle {
        x: Some(ui::bind(&a("decay"))),
        y: None,
        wheel: None,
        x_map: HandleAxis::new(Axis::Linear { min: 0.0, max: 1.0 }),
        y_map: HandleAxis::new(Axis::Linear { min: 0.0, max: 1.0 }),
    };
    b.controls.insert(f, Control::new(kind).bound(ui::bind(&a("decay"))));
}

/// The picture in (`x`, `y`, `w`, `h`) on `panel`: the chart and, where
/// it is wide, the formants and the held chord from `top` down (below a
/// nameplate over the picture). Returns its well.
fn picture(b: &mut Builder<'_>, panel: NodeId, (x, y, w, h): (f64, f64, f64, f64), top: f64, l: &Look) -> NodeId {
    let mut well = draw::frame("Vowel chart", x, y, w, h);
    well.fills = vec![draw::linear(180.0, &l.well)];
    well.clips_content = true;
    well.corner_radii = document_proto::CornerRadii::uniform(6.0);
    well.effects = vec![draw::inner_shadow("#000000c0", 0.0, 2.0, 7.0)];
    let well = b.add(panel, well);
    let wide = w > 360.0;
    let fw = if wide { (h * 1.45).min(w * 0.42) } else { w };
    let m = if wide { 18.0 } else { 16.0 };
    chart(b, well, (m + 6.0, m + 2.0, fw - 2.0 * m - 10.0, h - 2.0 * m - 8.0), !wide);
    if wide {
        let rx = fw + 18.0;
        let rw = w - rx - 16.0;
        let sy = top.max(24.0);
        let bh = h - sy - 16.0;
        ui::rule(b, well, fw + 2.0, 12.0, 1.0, h - 24.0, &ui::alpha(GOLD, 0.25));
        formants(b, well, (rx, sy, rw * 0.5 - 10.0, bh));
        hold(b, well, (rx + rw * 0.5 + 10.0, sy, rw * 0.5 - 10.0, bh), h);
    }
    kit::glass(b, well);
    well
}

/// The panel, `w` × `h`: stone (or walnut), an arcade of pointed arches
/// carved along its foot.
fn ground(b: &mut Builder<'_>, root: NodeId, (w, h): (f64, f64), l: &Look, foot: bool) -> NodeId {
    let mut panel = draw::frame("Panel", 0.0, 0.0, w, h);
    panel.fills = vec![draw::linear(180.0, &l.panel)];
    panel.corner_radii = document_proto::CornerRadii::uniform(8.0);
    let panel = b.add(root, draw::with_effects(panel, vec![draw::inner_shadow("#ffffff26", 0.0, 1.0, 0.0), draw::inner_shadow("#00000050", 0.0, -2.0, 3.0)]));
    // (Not on a phone: its knobs' values sit along the foot.)
    if !foot {
        return panel;
    }
    let mut arches = Vec::new();
    let (y1, y0, pitch) = (h - 3.0, h - 9.0, 12.0);
    let mut x = 14.0;
    while x + pitch < w - 10.0 {
        let arc: Vec<(f64, f64)> = (0..=8).map(|i| {
            let t = f64::from(i) / 8.0;
            // A lancet: two arcs meeting at a point.
            (x + pitch * t, y1 - (y1 - y0) * (1.0 - (2.0 * t - 1.0).abs().powf(1.6)).sqrt())
        }).collect();
        arches.push(arc);
        x += pitch;
    }
    b.add(panel, ui::strokes("arcade", &arches, 0.7, draw::solid(l.tracery)));
    panel
}

/// The nameplate's title: a choirbook's blackletter.
fn title(b: &mut Builder<'_>, t: NodeId, h: f64, size: f64, ink: &str) {
    let gothic = draw::Lettering::family(draw::family::UNIFRAKTUR_COOK).weight(700.0);
    crate::suite_studio_blues::parts::set(b, t, NAME, 40.0, h / 2.0, size, ink, &gothic, 0.0);
}

#[must_use]
pub fn face(dark: bool) -> Face {
    face_at(dark, kit::Tier::Desktop)
}

/// The face at `tier`: the chart, its formants and the held chord beside
/// the controls. On a phone the picture is the chart alone and the knobs
/// are Size, Vowel and Level.
#[must_use]
pub fn face_at(dark: bool, tier: kit::Tier) -> Face {
    let l = if dark { &DARK } else { &LIGHT };
    let small_room = matches!(tier, kit::Tier::Phone | kit::Tier::Strip);
    let w = tier.lane_w();
    let scene = kit::tier_lane(NAME, tier, |b, root| {
        let panel = ground(b, root, (w, kit::H), l, !small_room);
        let sw = if small_room { kit::PHONE_PICTURE_R - 10.0 } else { 560.0 };
        let well = picture(b, panel, (10.0, 10.0, sw, kit::H - 20.0), 0.0, l);
        let lit = ui::alpha(kit::PURPLE_LAMP, 0.55);
        if small_room {
            kit::nameplate_top_right(b, panel, VERB, tier, (6.0, 30.0), TITLE_W, 210.0, 12.0, (l.accent, l.dim), |b, t| title(b, t, 30.0, 21.0, l.title));
            let (tx, ty, td) = kit::PHONE_TIME;
            kit::time_knob(b, panel, VERB, (tx, ty, td), 11.0, (l.ink, l.dim, &lit), |b, p, addr| {
                kit::knob3d_bare(b, p, addr, td, (tx, ty), &l.wheel);
            });
            let (xs, ky, kd) = kit::PHONE_KNOBS;
            for (i, (id, legend)) in [("size", "Size"), ("tone", "Vowel"), ("level", "Level")].iter().enumerate() {
                kit::knob3d(b, panel, &a(id), legend, kd, (xs[i], ky), &l.knob, l.ink, l.dim);
            }
        } else {
            kit::nameplate_top_right(b, panel, VERB, tier, (10.0, 28.0), TITLE_W, 200.0, 10.0, (l.accent, l.dim), |b, t| title(b, t, 28.0, 19.0, l.title));
            let x0 = 10.0 + sw + 20.0;
            let (tx, ty, td) = (x0 + 40.0, 78.0, 52.0);
            kit::time_knob(b, panel, VERB, (tx, ty, td), 8.5, (l.ink, l.dim, &lit), |b, p, addr| {
                kit::knob3d_bare(b, p, addr, td, (tx, ty), &l.wheel);
            });
            for (i, (id, legend)) in [("size", "Size"), ("tone", "Vowel"), ("level", "Level")].iter().enumerate() {
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

/// Its back at `tier`: the panel, no chart — the advanced controls across
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
        let (np, preset_w, size, ts) = if small_room { ((6.0, 30.0), 210.0, 12.0, 21.0) } else { ((10.0, 28.0), 200.0, 10.0, 19.0) };
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

/// Its tall face: the chart, the formants and the held chord across the
/// top, every control under them.
#[must_use]
pub fn tall(dark: bool) -> Face {
    let l = if dark { &DARK } else { &LIGHT };
    let scene = kit::tall_lane(NAME, |b, root| {
        let panel = ground(b, root, (kit::TALL_W, kit::TALL_H), l, true);
        picture(b, panel, (10.0, 10.0, kit::TALL_W - 20.0, ROWS.split - 16.0), 50.0, l);
        let t = kit::Tall {
            ns: VERB,
            main: &[("size", "Size"), ("tone", "Vowel"), ("modulation", "Mod"), ("level", "Level")],
            inks: (l.ink, l.dim, &ui::alpha(kit::PURPLE_LAMP, 0.55), l.accent),
            scrim: l.scrim,
            bypass: ("purple", "On"),
            title_w: TITLE_W,
        };
        kit::tall_chrome_rows(b, panel, &t, ROWS, |b, p, addr, legend, d, at| {
            kit::knob3d(b, p, addr, legend, d, at, &l.knob, l.ink, l.dim);
        }, |b, p, addr, (cx, cy, d)| {
            kit::knob3d_bare(b, p, addr, d, (cx, cy), &l.wheel);
        }, |b, tn| title(b, tn, 30.0, 21.0, l.title));
    });
    let mut f = kit::reverb_face(kit::tall_file(FILE, dark), scene);
    f.params.extend(kit::tall_params(VERB));
    f
}
