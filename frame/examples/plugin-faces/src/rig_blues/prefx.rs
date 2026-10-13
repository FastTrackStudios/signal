//! The Pre FX module as lanes, in the Time module's form: two half-height
//! lanes stacked — the delay on top, the reverb under it — beside the
//! tremolo.
//!
//! - Aqua-Puss (`17-pedal-aqua-puss`): the teal delay, its knobs and name
//!   at the left, its repeats on a screen across the rest.
//! - Spring (`19-spring-reverb`): a purple spring tank — three coils hung
//!   between their transducers, shivering as the reverb rings, longer with
//!   the dwell, the drip glinting — and its knobs.

use document_proto::motion::{Motion, Target};
use document_proto::{CornerRadii, ResizeX};
use surface_kit::draw::{self, Lettering, family};
use surface_kit::motion;
use surface_proto::{Range, Unit};

use super::PLUGIN;
use super::filters::repeats_screen;
use crate::face::{Face, canvas};
use crate::reverb::ui;
use crate::rig_time::kit;
use crate::suite_studio_blues::parts::set;

/// A lane: the Time module's proportions.
pub const LANE: (f64, f64) = (720.0, 164.0);

/// A coil spring along `y` from `x0` to `x1`: `turns` loops, `r` high.
fn coil(x0: f64, x1: f64, y: f64, turns: usize, r: f64) -> Vec<(f64, f64)> {
    let n = turns * 12;
    (0..=n)
        .map(|i| {
            let t = i as f64 / n as f64;
            let ang = t * turns as f64 * std::f64::consts::TAU;
            (x0 + (x1 - x0) * t + ang.cos() * r * 0.35, y + ang.sin() * r)
        })
        .collect()
}

/// Where a Pre FX lane's parts go at `tier`, on a lane `w` wide: the
/// picture's right edge, the Time knob (centre, size), the knob row
/// (centres, line, size), the readout's size — the Time faces' places on a
/// phone, the same order in a desktop lane's 720.
struct Spots {
    w: f64,
    picture_r: f64,
    time: (f64, f64, f64),
    knobs: ([f64; 3], f64, f64),
    text: f64,
    small: bool,
}

impl Spots {
    fn at(tier: kit::Tier) -> Self {
        if matches!(tier, kit::Tier::Phone | kit::Tier::Strip) {
            Self { w: tier.lane_w(), picture_r: kit::PHONE_PICTURE_R, time: kit::PHONE_TIME, knobs: kit::PHONE_KNOBS, text: 11.0, small: true }
        } else if tier == kit::Tier::Ipad {
            // A pane (an iPad's FX row): the phone's lane, closer together.
            Self { w: tier.lane_w(), picture_r: kit::picture_r(tier), time: kit::time_spot(tier), knobs: kit::knob_row(tier), text: 10.0, small: true }
        } else {
            Self { w: LANE.0, picture_r: 300.0, time: (350.0, 70.0, 56.0), knobs: ([440.0, 516.0, 592.0], 82.0, 40.0), text: 8.5, small: false }
        }
    }
}

/// A Pre FX lane's panel `w` wide in its fill.
fn lane_panel(b: &mut surface_widgets::Builder<'_>, root: document_proto::NodeId, w: f64, fill: document_proto::Paint, sheen: &str) -> document_proto::NodeId {
    let mut p = draw::frame("Panel", 0.0, 0.0, w, LANE.1);
    p.fills = vec![fill];
    b.add(root, draw::with_effects(p, vec![draw::inner_shadow(sheen, 0.0, 1.5, 0.0)]))
}

const AQUA: &str = "aquapuss";
const AQUA_INK: &str = "#0b2a33";
const AQUA_KNOB: [&str; 6] = ["color=#f2f1ec", "pointer=#0b2a33", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#d6d4cc"];

fn aqua_fill() -> document_proto::Paint {
    draw::linear(160.0, &[(0.0, "#5fd0e2"), (0.5, "#3fb4c8"), (1.0, "#2c95a8")])
}

/// The Aqua-Puss's head where every Time face has it: the nameplate (its
/// presets, its name in its bubbly hand), the bypass and the arrow.
fn aqua_head(b: &mut surface_widgets::Builder<'_>, p: document_proto::NodeId, tier: kit::Tier, sp: &Spots, back: bool) {
    let (h, size, fs) = if sp.small { (30.0, 12.0, 22.0) } else { (26.0, 10.0, 19.0) };
    let tw = if sp.small { 140.0 } else { 120.0 };
    kit::head(b, p, AQUA, back, |b, head| {
        kit::nameplate_right(b, head, AQUA, sp.w, (8.0, h), tw, if sp.small { 210.0 } else { 180.0 }, size, ("#06323d", "#0b4a58"), |b, t| {
            set(b, t, "Aqua-Puss", (tw - 14.0) / 2.0, h / 2.0, fs, "#f4fbfd", &Lettering::family(family::KNEWAVE), 0.0);
        });
    });
    kit::face_bypass_w(b, p, AQUA, tier, sp.w, "blue", "On");
    kit::flip_arrow_w(b, p, AQUA, tier, sp.w, back, (AQUA_INK, "#ffffff40"));
}

fn aqua_params() -> Vec<surface_proto::ParamInfo> {
    let mut v = kit::delay_params_in(AQUA);
    v.extend(kit::nameplate_params(AQUA));
    v.extend(kit::flip_params(AQUA));
    v
}

/// The Aqua-Puss as a lane.
#[must_use]
pub fn aqua_lane() -> Face {
    aqua_at(kit::Tier::Desktop)
}

/// The Aqua-Puss at `tier`, in the Time faces' form: its repeats on the
/// screen at the left, the Delay as the Time knob (TIME | BEAT), Feedback,
/// Blend and Tone in the knob row, the head at the right.
#[must_use]
pub fn aqua_at(tier: kit::Tier) -> Face {
    let sp = Spots::at(tier);
    let scene = canvas("Aqua-Puss", |b, root| {
        ui::fit_root(b, root, sp.w, LANE.1);
        let p = lane_panel(b, root, sp.w, aqua_fill(), "#ffffff40");
        let screen = repeats_screen(b, p, (10.0, 10.0, sp.picture_r - 10.0, LANE.1 - 20.0), AQUA);
        let (tx, ty, td) = sp.time;
        kit::time_knob(b, p, AQUA, (tx, ty, td), sp.text, (AQUA_INK, "#0b4a58", "#ffffffa0"), |b, pp, addr| {
            kit::knob3d_bare(b, pp, addr, td, (tx, ty), &AQUA_KNOB);
        });
        let (xs, ky, kd) = sp.knobs;
        for (i, (id, legend)) in [("feedback", "Feedback"), ("level", "Blend"), ("high_cut", "Tone")].iter().enumerate() {
            kit::knob3d(b, p, &format!("{AQUA}/{id}"), legend, kd, (xs[i], ky), &AQUA_KNOB, AQUA_INK, AQUA_INK);
        }
        aqua_head(b, p, tier, &sp, false);
        kit::responsive(b, p, &[screen]);
    });
    let mut params = aqua_params();
    params.extend(kit::time_params(AQUA));
    Face { file: kit::tier_file("17-pedal-aqua-puss", tier, false), plugin: PLUGIN, scene, params, streams: Vec::new() }
}

/// The Aqua-Puss's back: its teal, no screen — the advanced controls.
#[must_use]
pub fn aqua_back(tier: kit::Tier) -> Face {
    let sp = Spots::at(tier);
    let scene = canvas("Aqua-Puss", |b, root| {
        ui::fit_root(b, root, sp.w, LANE.1);
        let p = lane_panel(b, root, sp.w, aqua_fill(), "#ffffff40");
        let knobs = [("mod_depth", "Mod"), ("mod_rate", "Mod Rate"), ("high_pass", "Low Cut"), ("duck_sens", "Duck"), ("duck_release", "Duck Rel"), ("pan", "Pan"), ("dry", "Dry")];
        kit::back_knobs_w(b, p, tier, sp.w, &knobs, AQUA_INK, |b, pp, id, legend, d, at| {
            kit::knob3d(b, pp, &format!("{AQUA}/{id}"), legend, d, at, &AQUA_KNOB, AQUA_INK, AQUA_INK);
        });
        aqua_head(b, p, tier, &sp, true);
    });
    Face { file: kit::tier_file("17-pedal-aqua-puss-back", tier, false), plugin: PLUGIN, scene, params: aqua_params(), streams: Vec::new() }
}

const SPRING: &str = "spring";
const SPRING_INK: &str = "#efe7ff";
const SPRING_DIM: &str = "#a898cc";
const SPRING_KNOB: [&str; 6] = ["color=#241640", "pointer=#c4b5fd", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#100820"];

fn spring_fill() -> document_proto::Paint {
    draw::linear(180.0, &[(0.0, "#2c1850"), (1.0, "#180c30")])
}

/// The spring tank in `p` at (`tx`, `ty`), `tw × th`: three coils hung
/// between their transducers, shivering as the reverb rings, the drip
/// glinting. Returns the tank (it takes a lane's width).
fn tank(b: &mut surface_widgets::Builder<'_>, p: document_proto::NodeId, (tx, ty, tw, th): (f64, f64, f64, f64)) -> document_proto::NodeId {
    const NS: &str = SPRING;
    const COIL: &str = "#c9b8ff";
    let mut t = draw::frame("Tank", tx, ty, tw, th);
    t.fills = vec![draw::linear(180.0, &[(0.0, "#140a26"), (1.0, "#0b0616")])];
    t.clips_content = true;
    t.corner_radii = CornerRadii::uniform(6.0);
    t.effects = vec![draw::inner_shadow("#000000c0", 0.0, 2.0, 6.0)];
    t.layout_child.resize_x = Some(ResizeX::Stretch);
    let t = b.add(p, t);
    let mut art = draw::frame("springs", 0.0, 0.0, tw, th);
    art.layout_child.resize_x = Some(ResizeX::Scale);
    let art = b.add(t, art);
    for x in [10.0, tw - 26.0] {
        b.add(art, draw::with_effects(draw::rounded(draw::rect("transducer", x, 18.0, 16.0, th - 36.0, draw::linear(90.0, &[(0.0, "#3a2a5c"), (0.5, "#6b58a0"), (1.0, "#2a1d45")])), 3.0), vec![draw::drop_shadow("#00000080", 0.0, 1.5, 3.0)]));
    }
    let turns = ((tw - 52.0) / 11.0).max(8.0) as usize;
    for (i, y) in [th * 0.3, th * 0.5, th * 0.7].iter().enumerate() {
        let g = b.add(art, draw::frame("spring", 0.0, 0.0, tw, th));
        b.add(g, ui::glowing(ui::strokes("coil", &[coil(26.0, tw - 26.0, *y, turns, 5.0)], 1.3, draw::solid(COIL)), "#a78bfa", 0.5, 4.0));
        let ring = format!("sin(time * {} + {i}) * pow(1 - fract(time * lerp(0.9, 0.25, norm({NS}/decay)) + {}), 2) * param({NS}/on)", 38.0 + 7.0 * i as f64, i as f64 * 0.21);
        motion::add(b, g, Motion::new(Target::Y).input(format!("0.5 + 0.5 * {ring}")).range(-3.0, 3.0));
    }
    for k in 0..8 {
        let f = f64::from(k);
        let gl = b.add(art, draw::emissive(draw::circle("drip", 30.0 + (f * 53.0) % (tw - 60.0).max(1.0), th * (0.3 + 0.2 * f64::from(k % 3)), 1.6, draw::solid("#f3eaff")), draw::hex("#c4b5fd"), 1.0, 5.0));
        motion::add(b, gl, Motion::new(Target::Opacity).input(format!("pow(0.5 + 0.5 * sin(time * {:.2} + {:.2}), 10) * norm({NS}/modulation) * param({NS}/on)", 1.3 + 0.4 * f, f * 1.9)).range(0.0, 1.0));
    }
    t
}

fn spring_head(b: &mut surface_widgets::Builder<'_>, p: document_proto::NodeId, tier: kit::Tier, sp: &Spots, back: bool) {
    let (h, size, fs) = if sp.small { (30.0, 12.0, 22.0) } else { (26.0, 10.0, 19.0) };
    let tw = if sp.small { 104.0 } else { 92.0 };
    let serif = Lettering::family(family::PLAYFAIR_DISPLAY).weight(700.0);
    kit::head(b, p, SPRING, back, |b, head| {
        kit::nameplate_right(b, head, SPRING, sp.w, (8.0, h), tw, if sp.small { 210.0 } else { 180.0 }, size, (kit::PURPLE_LAMP, SPRING_DIM), |b, t| {
            set(b, t, "Spring", (tw - 14.0) / 2.0, h / 2.0, fs, SPRING_INK, &serif, 0.02);
        });
    });
    kit::face_bypass_w(b, p, SPRING, tier, sp.w, "purple", "On");
    kit::flip_arrow_w(b, p, SPRING, tier, sp.w, back, (SPRING_INK, "#ffffff1c"));
}

fn spring_params() -> Vec<surface_proto::ParamInfo> {
    let mut v = kit::reverb_params_in(SPRING);
    v.extend(kit::nameplate_params(SPRING));
    v.extend(kit::flip_params(SPRING));
    v
}

/// The spring reverb as a lane, in purple.
#[must_use]
pub fn spring_lane() -> Face {
    spring_at(kit::Tier::Desktop)
}

/// The spring at `tier`, in the Time faces' form: the tank at the left, the
/// Dwell as the Time knob (TIME | BEAT), Tone, Drip and Mix in the knob
/// row, the head at the right.
#[must_use]
pub fn spring_at(tier: kit::Tier) -> Face {
    let sp = Spots::at(tier);
    let scene = canvas("Spring", |b, root| {
        ui::fit_root(b, root, sp.w, LANE.1);
        let p = lane_panel(b, root, sp.w, spring_fill(), "#ffffff1c");
        let t = tank(b, p, (10.0, 10.0, sp.picture_r - 10.0, LANE.1 - 20.0));
        let (tx, ty, td) = sp.time;
        kit::time_knob(b, p, SPRING, (tx, ty, td), sp.text, (SPRING_INK, SPRING_DIM, &ui::alpha(kit::PURPLE_LAMP, 0.55)), |b, pp, addr| {
            kit::knob3d_bare(b, pp, addr, td, (tx, ty), &SPRING_KNOB);
        });
        let (xs, ky, kd) = sp.knobs;
        for (i, (id, legend)) in [("tone", "Tone"), ("modulation", "Drip"), ("level", "Mix")].iter().enumerate() {
            kit::knob3d(b, p, &format!("{SPRING}/{id}"), legend, kd, (xs[i], ky), &SPRING_KNOB, SPRING_INK, SPRING_DIM);
        }
        spring_head(b, p, tier, &sp, false);
        kit::responsive(b, p, &[t]);
    });
    let mut params = spring_params();
    params.extend(kit::time_params(SPRING));
    Face { file: kit::tier_file("19-spring-reverb", tier, false), plugin: PLUGIN, scene, params, streams: Vec::new() }
}

/// The spring's back: its purple, no tank — the advanced controls.
#[must_use]
pub fn spring_back(tier: kit::Tier) -> Face {
    let sp = Spots::at(tier);
    let scene = canvas("Spring", |b, root| {
        ui::fit_root(b, root, sp.w, LANE.1);
        let p = lane_panel(b, root, sp.w, spring_fill(), "#ffffff1c");
        let knobs = [("damping", "Damping"), ("low_cut", "Low Cut"), ("high_cut", "High Cut"), ("predelay", "Pre-Delay"), ("duck", "Duck"), ("duck_threshold", "Duck Thr"), ("duck_release", "Duck Rel"), ("pan_a", "Pan")];
        kit::back_knobs_w(b, p, tier, sp.w, &knobs, SPRING_INK, |b, pp, id, legend, d, at| {
            kit::knob3d(b, pp, &format!("{SPRING}/{id}"), legend, d, at, &SPRING_KNOB, SPRING_INK, SPRING_DIM);
        });
        spring_head(b, p, tier, &sp, true);
    });
    Face { file: kit::tier_file("19-spring-reverb-back", tier, false), plugin: PLUGIN, scene, params: spring_params(), streams: Vec::new() }
}

const TREM: &str = "trem";

/// The Gravity Tank's tremolo as a phone lane, in the Time faces' form
/// and the unit's own parts: its powder-coat navy and copper edge, the
/// woven grille in its copper surround as the picture (a warm glow behind
/// it, pulsing with the volume — as deep as the intensity, as fast as the
/// speed), the cream knobs — the Speed as the Time knob (TIME | BEAT),
/// Intensity and Mix — its ON / OFF toggle at the bypass spot, and its
/// name in the unit's lettering on the nameplate.
#[must_use]
pub fn tank_trem_phone() -> Face {
    use crate::suite_studio_blues::parts::{Cap, knob as cap_knob, print, toggle};
    use surface_kit::materials::{light, paint};
    const INK: &str = "#e2d2c6";
    let tier = kit::Tier::Phone;
    let w = tier.lane_w();
    let h = LANE.1;
    let scene = canvas("Gravity Tank", |b, root| {
        ui::fit_root(b, root, w, h);
        let mut case = draw::frame("Case", 0.0, 0.0, w, h);
        case.corner_radii = CornerRadii::uniform(14.0);
        case.stroke = document_proto::Stroke { paints: vec![draw::linear(90.0, &[(0.0, "#a8664a"), (0.5, "#5a3a34"), (1.0, "#a8664a")])], weight: 3.0, ..document_proto::Stroke::default() };
        let case = b.add(root, case);
        paint::fill(b, case, (0.0, 0.0, w, h), 14.0, &paint::Material::Powder { color: "#30334a".into(), coarse: false });
        for e in light::edge("case edge", (0.0, 0.0, w, h), 14.0, 4.0, 1.1, true) {
            b.add(case, e);
        }
        // The grille in its copper surround.
        let (mx, my, mw, mh) = (14.0, 14.0, kit::PHONE_PICTURE_R - 18.0, h - 28.0);
        let mut surround = draw::with_effects(draw::rounded(draw::rect("surround", mx - 4.0, my - 4.0, mw + 8.0, mh + 8.0, draw::linear(180.0, &[(0.0, "#f2c7a6"), (0.5, "#c98a66"), (1.0, "#f0c09c")])), 12.0), vec![draw::drop_shadow("#00000080", 0.0, 2.0, 4.0)]);
        surround.layout_child.resize_x = Some(ResizeX::Stretch);
        b.add(case, surround);
        let mut win = draw::frame("Grille", mx, my, mw, mh);
        win.clips_content = true;
        win.corner_radii = CornerRadii::uniform(9.0);
        win.layout_child.resize_x = Some(ResizeX::Stretch);
        let win = b.add(case, win);
        // The cloth wider than its window (the window clips it), so it fills
        // the window however wide the lane is drawn.
        let mesh = surface_kit::Place { options: Some(vec![format!("w={}", mw * 2.0), format!("h={mh}"), "r=9".into()]), ..surface_kit::Place::new("Mesh").variant("mesh") };
        let m = ui::put_at(b, win, "panel.amp_grille", &mesh, 0.0, 0.0);
        if let Some(n) = b.scene.node_mut(m) {
            n.effects.clear();
        }
        // The valve's warm glow through the cloth, with the volume.
        let mut glow = draw::frame("glow", 0.0, 0.0, mw, mh);
        glow.layout_child.resize_x = Some(ResizeX::Stretch);
        let glow = b.add(win, glow);
        let lit = b.add(glow, draw::emissive(draw::circle("warmth", mw / 2.0, mh / 2.0, mh * 0.42, draw::radial_at(0.5, 0.5, 0.5, &[(0.0, "#ffcf8a90"), (0.7, "#e08a3c30"), (1.0, "#e08a3c00")])), draw::hex("#ffb060"), 0.8, 18.0));
        let pulse = format!("(1 - norm({TREM}/depth) * (0.5 - 0.5 * cos(6.2832 * time * param({TREM}/rate)))) * param({TREM}/on)");
        motion::add(b, lit, Motion::new(Target::Opacity).input(pulse.clone()).range(0.0, 1.0));
        for mut e in light::edge("mesh recess", (mx, my, mw, mh), 9.0, 3.0, 1.2, false) {
            e.layout_child.resize_x = Some(ResizeX::Stretch);
            b.add(case, e);
        }
        // Speed: the Time knob, a cream cap.
        let (tx, ty, td) = kit::PHONE_TIME;
        kit::time_knob(b, case, TREM, (tx, ty, td), 11.0, (INK, "#a9a7bd", "#c98a6699"), |b, pp, addr| {
            cap_knob(b, pp, Cap::Cream, addr, "SPEED", td * 0.92, tx, ty);
        });
        // The cream knobs, a little higher: the unit's bracket and name run
        // under them.
        let (xs, _, kd) = kit::PHONE_KNOBS;
        let ky = 72.0;
        for (i, (id, legend)) in [("depth", "INTENSITY"), ("mix", "MIX")].iter().enumerate() {
            cap_knob(b, case, Cap::Cream, &format!("{TREM}/{id}"), legend, kd, xs[i], ky);
            print(b, case, legend, xs[i], ky + kd / 2.0 + 15.0, 10.5, INK, 500.0, 0.12);
        }
        // Its ON / OFF toggle at the bypass spot, the lamp in its bezel
        // beside it, pulsing with the volume.
        let (bx, _, _) = kit::bypass_spot(tier);
        let by = ky;
        print(b, case, "ON", bx, by - 30.0, 10.0, INK, 500.0, 0.08);
        toggle(b, case, &format!("{TREM}/on"), "Tremolo", 24.0, bx, by);
        print(b, case, "OFF", bx, by + 30.0, 10.0, INK, 500.0, 0.08);
        let (lx, lr) = (bx + 40.0, 8.0);
        b.add(case, draw::with_effects(draw::circle("lamp bezel", lx, by, lr + 3.0, draw::linear(180.0, &[(0.0, "#d9d9de"), (0.5, "#7d7f88"), (1.0, "#3a3c44")])), vec![draw::drop_shadow("#00000090", 0.0, 1.5, 2.0)]));
        b.add(case, draw::circle("lamp lens", lx, by, lr, draw::radial_at(0.4, 0.35, 0.9, &[(0.0, "#2a4a3a"), (1.0, "#0c1a12")])));
        let lamp = b.add(case, draw::emissive(draw::circle("lamp", lx, by, lr, draw::radial_at(0.4, 0.35, 0.9, &[(0.0, "#e8fff4"), (0.4, "#3cf0a0"), (1.0, "#159060")])), draw::hex("#3cf0a0"), 1.0, 12.0));
        motion::add(b, lamp, Motion::new(Target::Opacity).input(pulse).range(0.05, 1.0));
        // The unit's bracket under its controls, and its name.
        let (x0, x1, yb) = (xs[0] - 50.0, lx + 18.0, h - 26.0);
        let line = vec![(x0, yb - 8.0), (x0, yb), (x1, yb), (x1, yb - 8.0)];
        b.add(case, ui::strokes("bracket", &[line], 1.4, draw::solid(INK)));
        print(b, case, "Harmonic Tremolo", (x0 + x1) / 2.0, h - 13.0, 12.5, INK, 500.0, 0.03);
        // The nameplate: the preset, then the maker's name.
        kit::nameplate_top_right(b, case, TREM, tier, (8.0, 30.0), 140.0, 190.0, 12.0, ("#f0c09c", "#a9a7bd"), |b, t| {
            print(b, t, "GRAVITY TANK", 63.0, 15.0, 12.0, INK, 600.0, 0.16);
        });
        kit::responsive(b, case, &[win]);
    });
    let mut params = vec![
        ui::param("trem/rate", "Speed", Range::log(0.05, 12.0, Unit::Hz), 4.0).featured(),
        ui::param("trem/depth", "Intensity", Range::linear(0.0, 1.0, Unit::None), 0.5).featured(),
        ui::param("trem/mix", "Mix", Range::linear(0.0, 1.0, Unit::None), 1.0),
        ui::toggle("trem/on", "Tremolo", true),
    ];
    params.extend(kit::nameplate_params(TREM));
    params.extend(kit::time_params(TREM));
    Face { file: "18-gravity-tank-trem-phone", plugin: PLUGIN, scene, params, streams: Vec::new() }
}
