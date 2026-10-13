//! The pitch stage's two utility units, either side of the voices:
//!
//! - **Transpose**: the first block of the chain. A key readout and a
//!   chromatic strip two octaves wide — the open E marked, and a light on
//!   the note it now sounds — and the step it shifts by, in whole
//!   semitones, named by the key a standard-tuned guitar lands in.
//! - **Dive bomb**: a whammy. A treadle on the interval, and a screen that
//!   draws the pitch over time — a held note's line, dropping as far as the
//!   foot takes it (or rising) — with the target read out.
//!
//! Both drive the rig's pitch engine (`semitones`, `cents`); the shifted
//! signal alone.

use document_proto::motion::{Dynamics, Motion, Target};
use document_proto::{CornerRadii, NodeId, ResizeX};
use surface_kit::draw::{self, Lettering, family};
use surface_kit::motion;
use surface_proto::{ParamInfo, Range, Unit};
use surface_widgets::Builder;

use super::PLUGIN;
use super::drives::H;
use crate::face::{Face, canvas};
use crate::reverb::ui;
use crate::rig_time::kit;
use crate::suite_studio_blues::parts::set;

const INK: &str = "#ece7dc";
const DIM: &str = "#8b8f98";
/// The pitch family's yellow (the voices unit's), and the dive's hotter
/// orange.
const KEY: &str = "#facc15";
const DIVE: &str = "#fb923c";

/// Where an open E lands `s` semitones away — the key a standard-tuned
/// guitar plays in.
fn key_of(s: i32) -> &'static str {
    const KEYS: [&str; 12] = ["E", "F", "F♯", "G", "A♭", "A", "B♭", "B", "C", "C♯", "D", "E♭"];
    KEYS[s.rem_euclid(12) as usize]
}

/// The transposer's step: a choice of 25, −12…+12, each named for its key.
fn step_param(addr: &str) -> ParamInfo {
    let labels: Vec<String> = (-12..=12)
        .map(|s: i32| match s {
            0 => "E · Concert".to_string(),
            -12 => "E · Octave ↓".to_string(),
            12 => "E · Octave ↑".to_string(),
            s => format!("{} · {s:+}", key_of(s)),
        })
        .collect();
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let mut p = ParamInfo::new(addr, "Transpose", Range::linear(-12.0, 12.0, Unit::None)).choice(&refs);
    p.default = 0.5;
    p.featured = true;
    p
}

/// A readout of `addr` as its display text (a choice's name), lit, in the
/// panel's lettering, centred on `cy` across `w`.
fn name_readout(b: &mut Builder<'_>, parent: NodeId, addr: &str, (x, cy, w): (f64, f64, f64), size: f64, color: &str) -> NodeId {
    use document_proto::{NodeKind, TextAlign, TextData};
    let h = (size * 1.3).ceil();
    let root = b.add(parent, draw::frame(&format!("{addr} readout"), x, cy - h / 2.0, w, h));
    if let Some(n) = b.scene.node_mut(root) {
        n.layout_child.resize_x = Some(ResizeX::Stretch);
    }
    let mut t = draw::text("name", "", 0.0, 0.0, w, size, color, TextAlign::Center);
    if let NodeKind::Text(d) = &mut t.kind {
        *d = TextData { characters: String::new(), font_family: Some(family::INTER.into()), font_size: size, font_weight: Some(800.0), align: TextAlign::Center, ..TextData::default() };
    }
    let lit = b.add(root, draw::emissive(t, draw::hex(color), 0.7, size * 0.4));
    let c = surface_proto::Control::new(surface_proto::ControlKind::Readout).slot(surface_proto::SlotRole::Value, lit.0).bound(ui::bind(addr));
    b.controls.insert(root, c);
    root
}

/// A dark glass screen in `p`, stretching with the unit.
fn screen(b: &mut Builder<'_>, p: NodeId, name: &str, (x, y, w, h): (f64, f64, f64, f64)) -> NodeId {
    let mut s = draw::frame(name, x, y, w, h);
    s.fills = vec![draw::linear(180.0, &[(0.0, "#0c0d10"), (1.0, "#08090b")])];
    s.clips_content = true;
    s.corner_radii = CornerRadii::uniform(6.0);
    s.effects = vec![draw::inner_shadow("#000000c0", 0.0, 2.0, 6.0)];
    s.layout_child.resize_x = Some(ResizeX::Stretch);
    b.add(p, s)
}

fn panel(b: &mut Builder<'_>, root: NodeId, w: f64, top: &str, bottom: &str) -> NodeId {
    let mut p = draw::frame("Panel", 0.0, 0.0, w, H);
    p.fills = vec![draw::linear(180.0, &[(0.0, top), (1.0, bottom)])];
    b.add(root, draw::with_effects(p, vec![draw::inner_shadow("#ffffff14", 0.0, 1.0, 0.0)]))
}

fn knob(b: &mut Builder<'_>, p: NodeId, addr: &str, legend: &str, d: f64, at: (f64, f64), pointer: &str) -> NodeId {
    let ptr = format!("pointer={pointer}");
    let opts = ["color=#1c1d21", ptr.as_str(), "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#0b0b0c"];
    kit::knob3d(b, p, addr, legend, d, at, &opts, INK, DIM)
}

const TNS: &str = "transpose";
const DNS: &str = "dive";

/// The transposer's screen, `sw × sh` in `s`: the key it plays in, lit,
/// over the chromatic strip. `k` scales its lettering.
fn key_screen(b: &mut Builder<'_>, s: NodeId, sw: f64, sh: f64, k: f64) {
    ui::silk(b, s, "PLAYS IN", sw / 2.0, 14.0 * k, 6.5 * k, DIM, 800.0);
    name_readout(b, s, &format!("{TNS}/semitones"), (6.0, 40.0 * k, sw - 12.0), 21.0 * k, KEY);
    // The chromatic strip, −12…+12: a cell a semitone, the naturals
    // light, the sharps dark, the octaves and the open E marked.
    let cells = 25.0;
    let (kx, ky, kw, kh) = (10.0, sh - 56.0 * k, sw - 20.0, 30.0 * k);
    let strip = b.add(s, draw::frame("strip", kx, ky, kw, kh));
    if let Some(n) = b.scene.node_mut(strip) {
        n.layout_child.resize_x = Some(ResizeX::Stretch);
    }
    let cw = kw / cells;
    for i in 0..25 {
        let semis = i - 12;
        let sharp = matches!((semis as i32).rem_euclid(12), 2 | 4 | 7 | 9 | 11);
        let fill = if sharp { "#1d1f24" } else { "#2c2f36" };
        b.add(strip, draw::rounded(draw::rect("key", f64::from(i) * cw + 0.5, 0.0, cw - 1.0, kh, draw::solid(fill)), 2.0));
    }
    // The open E, where the strip starts from.
    b.add(strip, draw::rect("open E", 12.0 * cw + cw / 2.0 - 0.75, -4.0, 1.5, kh + 8.0, draw::solid("#ffffff60")));
    for (semis, label) in [(-12, "−OCT"), (0, "E"), (12, "+OCT")] {
        ui::silk(b, s, label, kx + (f64::from(semis) + 12.5) * cw, ky + kh + 10.0 * k, 6.5 * k, DIM, 800.0);
    }
    // The light on the note it sounds: from the open E, a cell a step.
    let light = b.add(strip, draw::frame("sounding", 12.0 * cw, 0.0, cw, kh));
    b.add(light, ui::glowing(draw::rounded(draw::rect("lit", 0.5, 2.0, cw - 1.0, kh - 4.0, draw::solid(KEY)), 2.0), KEY, 0.9, 8.0));
    motion::add(b, light, Motion::new(Target::X).input(format!("clamp(param({TNS}/semitones) / 12, -1, 1) * 0.5 + 0.5")).range(-12.0 * cw, 12.0 * cw).dynamics(Dynamics::spring(320.0, 30.0)));
    motion::add(b, light, Motion::new(Target::Opacity).input(format!("0.35 + 0.65 * param({TNS}/on)")).range(0.0, 1.0));
}

/// The dive bomb's screen, `sw × sh` in `s`: a held note's line, bending
/// as far as the treadle takes it, and where it is now. `k` scales its
/// lettering.
fn pitch_screen(b: &mut Builder<'_>, s: NodeId, sw: f64, sh: f64, k: f64) {
    let mid = sh / 2.0;
    let y_of = |semis: f64| mid - semis / 24.0 * (sh / 2.0 - 12.0);
    for (semis, label) in [(24, "+2 OCT"), (12, "+OCT"), (0, "UNISON"), (-12, "−OCT"), (-24, "−2 OCT")] {
        let y = y_of(f64::from(semis));
        // Twice the screen long: it clips them, and a wider one stays ruled.
        let mut line = ui::strokes("grid", &[vec![(0.0, y), (sw * 2.0, y)]], 1.0, draw::solid(if semis == 0 { "#ffffff30" } else { "#ffffff14" }));
        line.layout_child.resize_x = Some(ResizeX::Stretch);
        b.add(s, line);
        ui::silk_left(b, s, label, 6.0, y - 6.0 * k, 6.0 * k, DIM, 800.0);
    }
    // The held note: flat for the first third, then a curve that
    // bends to where the treadle is — drawn at a full dive, its depth
    // following `semitones` (scaled about the unison line).
    let x0 = sw * 0.32;
    let full = y_of(-24.0) - mid;
    let bend: Vec<(f64, f64)> = (0..=40)
        .map(|i| {
            let t = f64::from(i) / 40.0;
            let ease = 1.0 - (1.0 - t).powi(3);
            (x0 + t * (sw - x0 - 10.0), mid + ease * full)
        })
        .collect();
    b.add(s, ui::glowing(ui::strokes("held", &[vec![(10.0, mid), (x0, mid)]], 2.4, draw::solid("#f7f5ef")), "#ffffff", 0.5, 4.0));
    b.add(s, ui::strokes("unbent", &[vec![(x0, mid), (sw * 2.0, mid)]], 1.4, draw::solid("#ffffff30")));
    let curve = b.add(s, draw::frame("bend", 0.0, 0.0, sw, sh));
    if let Some(n) = b.scene.node_mut(curve) {
        n.layout_child.resize_x = Some(ResizeX::Stretch);
    }
    b.add(curve, ui::glowing(ui::strokes("dive", &[bend], 2.4, draw::solid(DIVE)), DIVE, 0.9, 8.0));
    // The depth: −24 is the curve as drawn, +24 its mirror.
    motion::add(b, curve, Motion::new(Target::StretchY).input(format!("0.5 + 0.5 * clamp(-param({DNS}/semitones) / 24, -1, 1)")).range(-1.0, 1.0).pivot(0.5, mid / sh).dynamics(Dynamics::spring(300.0, 28.0)));
    motion::add(b, curve, Motion::new(Target::Opacity).input(format!("0.3 + 0.7 * param({DNS}/on)")).range(0.0, 1.0));
    // Where it is now, in semitones.
    kit::seg_readout(b, s, &format!("{DNS}/semitones"), (sw - 74.0 * k, 22.0 * k, 64.0 * k), 20.0 * k, 3, 0, DIVE);
}

/// The transposer.
#[must_use]
pub fn transpose_unit() -> Face {
    const NS: &str = "transpose";
    const W: f64 = 200.0;
    let scene = canvas("Transpose", |b, root| {
        ui::fit_root(b, root, W, H);
        let p = panel(b, root, W, "#1b1a16", "#11100d");
        let head = Lettering::family(family::INTER).weight(800.0);
        set(b, p, "TRANSPOSE", 56.0, 18.0, 11.0, KEY, &head, 0.2);
        // Its On key low, by the knobs: the top-right corner is the host's.
        kit::bypass(b, p, NS, W - 30.0, 178.0, 18.0, "amber", "On");

        // ── The key: its readout, and the strip ─────────────────────────
        let (sx, sy, sw, sh) = (12.0, 32.0, W - 24.0, 126.0);
        let s = screen(b, p, "Key", (sx, sy, sw, sh));
        key_screen(b, s, sw, sh, 1.0);

        // ── Its knobs ───────────────────────────────────────────────────
        let k = knob(b, p, &format!("{NS}/semitones"), "Key", 46.0, (W * 0.3, 222.0), KEY);
        let _ = k;
        knob(b, p, &format!("{NS}/cents"), "Fine", 30.0, (W * 0.76, 232.0), "#f7f5ef");
        kit::responsive(b, p, &[s]);
    });
    let params = vec![
        step_param(&format!("{NS}/semitones")),
        ui::param(&format!("{NS}/cents"), "Fine", Range::linear(-100.0, 100.0, Unit::None), 0.0),
        ui::toggle(&format!("{NS}/on"), "Transpose", false),
    ];
    Face { file: "36-transpose", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

/// The dive bomb.
#[must_use]
pub fn dive_unit() -> Face {
    const NS: &str = "dive";
    const W: f64 = 240.0;
    let scene = canvas("Dive Bomb", |b, root| {
        ui::fit_root(b, root, W, H);
        let p = panel(b, root, W, "#1d1612", "#120d0a");
        let head = Lettering::family(family::INTER).weight(800.0);
        set(b, p, "DIVE BOMB", 52.0, 18.0, 11.0, DIVE, &head, 0.2);


        // ── The pitch over time ─────────────────────────────────────────
        // A held note's line across the screen: flat at unison until the
        // foot moves, then bending to the interval it is at, ±2 octaves
        // full height.
        let (sx, sy, sw, sh) = (12.0, 32.0, W - 84.0, H - 44.0);
        let s = screen(b, p, "Pitch", (sx, sy, sw, sh));
        pitch_screen(b, s, sw, sh, 1.0);

        // ── The treadle ─────────────────────────────────────────────────
        // Up is up: toe down dives, heel down rises — a fader the length
        // of the screen.
        let tx = W - 38.0;
        ui::silk(b, p, "TREADLE", tx, 38.0, 6.5, DIM, 800.0);
        let len = H - 100.0;
        b.add(p, draw::rounded(draw::rect("slot", tx - 3.0, 50.0, 6.0, len, draw::solid("#07080a")), 3.0));
        crate::rig_amp::fader(b, p, &format!("{NS}/semitones"), (tx, 44.0), len + 12.0, (40.0, 22.0), |b, c, cw, ch| {
            b.add(c, draw::stroked(draw::rounded(draw::rect("tread", 0.0, 0.0, cw, ch, draw::linear(180.0, &[(0.0, "#3a2c22"), (1.0, "#1b140f")])), 4.0), "#5b4434", 1.0));
            for i in 0..4 {
                b.add(c, draw::rect("rib", 6.0, 4.0 + f64::from(i) * 4.5, cw - 12.0, 1.5, draw::solid("#00000080")));
            }
            b.add(c, ui::glowing(draw::rect("edge", 4.0, ch - 3.0, cw - 8.0, 1.5, draw::solid(DIVE)), DIVE, 0.7, 4.0));
        });
        // Its On key under the treadle: the top-right corner is the host's.
        kit::bypass(b, p, NS, tx, H - 26.0, 22.0, "amber", "On");
        kit::responsive(b, p, &[s]);
    });
    let mut bend = ui::param(&format!("{NS}/semitones"), "Dive", Range::linear(-24.0, 24.0, Unit::None), 0.0).featured();
    bend.bipolar = true;
    let params = vec![bend, ui::param(&format!("{NS}/cents"), "Fine", Range::linear(-100.0, 100.0, Unit::None), 0.0), ui::toggle(&format!("{NS}/on"), "Dive Bomb", false)];
    Face { file: "37-dive-bomb", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

/// A lane's panel on a phone.
fn lane_panel(b: &mut Builder<'_>, root: NodeId, w: f64, top: &str, bottom: &str) -> NodeId {
    let mut p = draw::frame("Panel", 0.0, 0.0, w, kit::H);
    p.fills = vec![draw::linear(180.0, &[(0.0, top), (1.0, bottom)])];
    b.add(root, draw::with_effects(p, vec![draw::inner_shadow("#ffffff14", 0.0, 1.0, 0.0)]))
}

/// The transposer as a phone lane: its screen wide — the key it plays in
/// over the chromatic strip — then the Key, big, and Fine; the nameplate
/// (the preset ‹ ›, TRANSPOSE) and its On.
#[must_use]
pub fn transpose_phone() -> Face {
    let tier = kit::Tier::Phone;
    let w = tier.lane_w();
    let scene = canvas("Transpose", |b, root| {
        ui::fit_root(b, root, w, kit::H);
        let p = lane_panel(b, root, w, "#1b1a16", "#11100d");
        let (sw, sh) = (330.0, kit::H - 20.0);
        let s = screen(b, p, "Key", (10.0, 10.0, sw, sh));
        key_screen(b, s, sw, sh, 1.35);
        knob(b, p, &format!("{TNS}/semitones"), "Key", 80.0, (430.0, 72.0), KEY);
        knob(b, p, &format!("{TNS}/cents"), "Fine", 54.0, (555.0, 84.0), "#f7f5ef");
        let head = Lettering::family(family::INTER).weight(800.0);
        kit::nameplate_top_right(b, p, TNS, tier, (8.0, 30.0), 150.0, 200.0, 12.0, (KEY, DIM), |b, t| {
            set(b, t, "TRANSPOSE", 68.0, 15.0, 14.0, KEY, &head, 0.2);
        });
        kit::face_bypass(b, p, TNS, tier, "amber", "On");
        kit::responsive(b, p, &[s]);
    });
    let mut params = vec![
        step_param(&format!("{TNS}/semitones")),
        ui::param(&format!("{TNS}/cents"), "Fine", Range::linear(-100.0, 100.0, Unit::None), 0.0),
        ui::toggle(&format!("{TNS}/on"), "Transpose", false),
    ];
    params.extend(kit::nameplate_params(TNS));
    Face { file: "36-transpose-phone", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

/// The dive bomb as a phone lane: the pitch over time wide, the treadle
/// stood beside it the lane's height (toe down dives), the nameplate (the
/// preset ‹ ›, DIVE BOMB) and its On.
#[must_use]
pub fn dive_phone() -> Face {
    let tier = kit::Tier::Phone;
    let w = tier.lane_w();
    let scene = canvas("Dive Bomb", |b, root| {
        ui::fit_root(b, root, w, kit::H);
        let p = lane_panel(b, root, w, "#1d1612", "#120d0a");
        let (sw, sh) = (440.0, kit::H - 20.0);
        let s = screen(b, p, "Pitch", (10.0, 10.0, sw, sh));
        pitch_screen(b, s, sw, sh, 1.3);
        // The treadle: up is up — a fader the lane's height.
        let tx = 510.0;
        let len = kit::H - 40.0;
        ui::silk(b, p, "TREADLE", tx, 12.0, 8.0, DIM, 800.0);
        b.add(p, draw::rounded(draw::rect("slot", tx - 4.0, 26.0, 8.0, len - 6.0, draw::solid("#07080a")), 4.0));
        crate::rig_amp::fader(b, p, &format!("{DNS}/semitones"), (tx, 20.0), len + 4.0, (64.0, 32.0), |b, c, cw, ch| {
            b.add(c, draw::stroked(draw::rounded(draw::rect("tread", 0.0, 0.0, cw, ch, draw::linear(180.0, &[(0.0, "#3a2c22"), (1.0, "#1b140f")])), 5.0), "#5b4434", 1.0));
            for i in 0..5 {
                b.add(c, draw::rect("rib", 8.0, 5.0 + f64::from(i) * 4.8, cw - 16.0, 1.6, draw::solid("#00000080")));
            }
            b.add(c, ui::glowing(draw::rect("edge", 5.0, ch - 3.5, cw - 10.0, 2.0, draw::solid(DIVE)), DIVE, 0.7, 4.0));
        });
        let head = Lettering::family(family::INTER).weight(800.0);
        kit::nameplate_top_right(b, p, DNS, tier, (8.0, 30.0), 150.0, 170.0, 12.0, (DIVE, DIM), |b, t| {
            set(b, t, "DIVE BOMB", 66.0, 15.0, 14.0, DIVE, &head, 0.2);
        });
        kit::face_bypass(b, p, DNS, tier, "amber", "On");
        kit::responsive(b, p, &[s]);
    });
    let mut bend = ui::param(&format!("{DNS}/semitones"), "Dive", Range::linear(-24.0, 24.0, Unit::None), 0.0).featured();
    bend.bipolar = true;
    let mut params = vec![bend, ui::param(&format!("{DNS}/cents"), "Fine", Range::linear(-100.0, 100.0, Unit::None), 0.0), ui::toggle(&format!("{DNS}/on"), "Dive Bomb", false)];
    params.extend(kit::nameplate_params(DNS));
    Face { file: "37-dive-bomb-phone", plugin: PLUGIN, scene, params, streams: Vec::new() }
}
