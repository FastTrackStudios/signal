//! Pre modulation: the chorus on the board, in front of the amp — the
//! Pre FX's fourth unit, beside its tremolo.
//!
//! Modulation's light blue, and unlike the post chorus' clean picture it
//! shows the machine: a bucket-brigade line, its cells lit as the clock
//! sweeps the delay through them (the ripple at the rate, as deep as the
//! depth), over the two voices' sweep, left and right. The engine is the
//! chorus block's (`engine`, CE-2 by default — the classic in front of an
//! amp), read out on its selector.

use document_proto::motion::{Dynamics, Motion, Target};
use document_proto::{CornerRadii, ResizeX};
use surface_kit::draw::{self, Lettering, family};
use surface_kit::motion;
use surface_proto::{ParamInfo, Range, Unit};
use surface_widgets::Builder;

use super::PLUGIN;
use crate::face::{Face, canvas};
use crate::reverb::ui;
use crate::rig_mod::{MOD_ENGINES, SKY};
use crate::rig_time::kit;
use crate::suite_studio_blues::parts::set;

const NS: &str = "premod";
const INK: &str = "#e8f4fb";
const DIM: &str = "#7f93a0";
pub const W: f64 = 400.0;
pub const H: f64 = 300.0;

fn a(id: &str) -> String {
    format!("{NS}/{id}")
}

fn knob(b: &mut Builder<'_>, p: document_proto::NodeId, id: &str, legend: &str, d: f64, at: (f64, f64)) -> document_proto::NodeId {
    let opts = ["color=#16202a", "pointer=#7dd3fc", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#081018"];
    kit::knob3d(b, p, &a(id), legend, d, at, &opts, INK, DIM)
}

#[must_use]
pub fn premod_unit() -> Face {
    let scene = canvas("Pre Mod", |b, root| {
        ui::fit_root(b, root, W, H);
        let mut p = draw::frame("Panel", 0.0, 0.0, W, H);
        p.fills = vec![draw::linear(180.0, &[(0.0, "#121b23"), (1.0, "#0a1016")])];
        let p = b.add(root, draw::with_effects(p, vec![draw::inner_shadow("#ffffff14", 0.0, 1.0, 0.0)]));
        let head = Lettering::family(family::INTER).weight(800.0);
        set(b, p, "PRE MOD", 44.0, 18.0, 12.0, SKY, &head, 0.3);
        ui::silk_left(b, p, "Into the amp", 130.0, 18.0, 6.5, DIM, 700.0);
        kit::bypass(b, p, NS, 104.0, 18.0, 18.0, "blue", "On");

        // ── The line ────────────────────────────────────────────────────
        let (sx, sy, sw, sh) = (12.0, 32.0, W - 24.0, 160.0);
        let mut s = draw::frame("Line", sx, sy, sw, sh);
        s.fills = vec![draw::linear(180.0, &[(0.0, "#071017"), (1.0, "#050a0f")])];
        s.clips_content = true;
        s.corner_radii = CornerRadii::uniform(6.0);
        s.effects = vec![draw::inner_shadow("#000000c0", 0.0, 2.0, 6.0)];
        s.layout_child.resize_x = Some(ResizeX::Stretch);
        let s = b.add(p, s);
        // The bucket brigade: a row of cells, each lit as the sweep passes
        // — a wave travelling at the rate, its swing the depth.
        let cells = 24;
        let (bx, by, bw, bh) = (14.0, 22.0, sw - 28.0, 34.0);
        let row = b.add(s, draw::frame("buckets", bx, by, bw, bh));
        if let Some(n) = b.scene.node_mut(row) {
            n.layout_child.resize_x = Some(ResizeX::Stretch);
        }
        let cw = bw / f64::from(cells);
        for i in 0..cells {
            let x = f64::from(i) * cw;
            b.add(row, draw::stroked(draw::rounded(draw::rect("bucket", x + 1.5, 0.0, cw - 3.0, bh, draw::solid("#0b1720")), 3.0), "#1b2c38", 1.0));
            let lit = b.add(row, draw::frame("charge", x + 1.5, 0.0, cw - 3.0, bh));
            b.add(lit, ui::glowing(draw::rounded(draw::rect("glow", 0.0, 0.0, cw - 3.0, bh, draw::solid(SKY)), 3.0), SKY, 0.8, 6.0));
            let phase = f64::from(i) / f64::from(cells) * std::f64::consts::TAU;
            let wave = format!("(0.5 + 0.5 * sin(time * 6.2832 * param({NS}/rate) - {phase:.3})) * (0.15 + 0.85 * norm({NS}/depth)) * param({NS}/on)");
            motion::add(b, lit, Motion::new(Target::Opacity).input(wave).range(0.04, 0.95));
        }
        ui::silk_left(b, s, "IN", 14.0, by + bh + 12.0, 6.5, DIM, 800.0);
        ui::silk_right(b, s, "OUT", sw - 14.0, by + bh + 12.0, 6.5, DIM, 800.0);
        // The sweep: the two voices' delay over time, left and right a
        // quarter-turn apart, scrolling — as tall as the depth.
        let (vx, vy, vw, vh) = (14.0, by + bh + 26.0, sw - 28.0, sh - (by + bh + 26.0) - 12.0);
        let view = b.add(s, draw::frame("sweep", vx, vy, vw, vh));
        if let Some(n) = b.scene.node_mut(view) {
            n.layout_child.resize_x = Some(ResizeX::Stretch);
        }
        b.add(view, ui::strokes("centre", &[vec![(0.0, vh / 2.0), (vw, vh / 2.0)]], 1.0, draw::solid("#ffffff14")));
        for (name, color, shift) in [("left", SKY, 0.0), ("right", "#c4b5fd", std::f64::consts::FRAC_PI_2)] {
            // Two periods drawn, twice the view wide, sliding left a period
            // a cycle of the LFO.
            let g = b.add(view, draw::frame(name, 0.0, 0.0, vw * 2.0, vh));
            let pts: Vec<(f64, f64)> = (0..=160)
                .map(|i| {
                    let t = f64::from(i) / 160.0;
                    (t * vw * 2.0, vh / 2.0 - (t * 4.0 * std::f64::consts::TAU / 2.0 + shift).sin() * (vh / 2.0 - 6.0))
                })
                .collect();
            let wave = b.add(g, draw::frame("wave", 0.0, 0.0, vw * 2.0, vh));
            b.add(wave, ui::glowing(ui::strokes("trace", &[pts], 1.8, draw::solid(color)), color, 0.7, 5.0));
            motion::add(b, wave, Motion::new(Target::StretchY).input(format!("0.1 + 0.9 * norm({NS}/depth)")).range(0.0, 1.0).pivot(0.5, 0.5).dynamics(Dynamics::spring(260.0, 26.0)));
            motion::add(b, g, Motion::new(Target::X).input(format!("fract(time * param({NS}/rate) * 0.5)")).range(0.0, -vw));
            motion::add(b, g, Motion::new(Target::Opacity).input(format!("0.25 + 0.75 * param({NS}/on)")).range(0.0, 1.0));
        }

        // ── Its knobs, and the engine ───────────────────────────────────
        let ky = 236.0;
        for (i, (id, legend)) in [("rate", "Rate"), ("depth", "Depth"), ("mix", "Mix")].iter().enumerate() {
            knob(b, p, id, legend, 34.0, (40.0 + 66.0 * i as f64, ky));
        }
        // The engine: a stepped knob, its name read under it.
        knob(b, p, "engine", "Engine", 34.0, (W - 60.0, ky));
        kit::responsive(b, p, &[s]);
    });
    let mut engine = ParamInfo::new(a("engine"), "Engine", Range::linear(0.0, (MOD_ENGINES.len() - 1) as f64, Unit::None)).choice(&MOD_ENGINES);
    engine.default = 5.0 / (MOD_ENGINES.len() - 1) as f64;
    let params = vec![
        ui::param(&a("rate"), "Rate", Range::log(0.05, 10.0, Unit::Hz), 0.6).featured(),
        ui::param(&a("depth"), "Depth", Range::linear(0.0, 1.0, Unit::None), 0.4).featured(),
        ui::param(&a("mix"), "Mix", Range::linear(0.0, 1.0, Unit::None), 0.5),
        engine,
        ui::toggle(&a("on"), "Pre Mod", false),
    ];
    Face { file: "38-pre-mod", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

fn params() -> Vec<ParamInfo> {
    let mut engine = ParamInfo::new(a("engine"), "Engine", Range::linear(0.0, (MOD_ENGINES.len() - 1) as f64, Unit::None)).choice(&MOD_ENGINES);
    engine.default = 5.0 / (MOD_ENGINES.len() - 1) as f64;
    let mut v = vec![
        ui::param(&a("rate"), "Rate", Range::log(0.05, 10.0, Unit::Hz), 0.6).featured(),
        ui::param(&a("depth"), "Depth", Range::linear(0.0, 1.0, Unit::None), 0.4).featured(),
        ui::param(&a("mix"), "Mix", Range::linear(0.0, 1.0, Unit::None), 0.5),
        ui::param(&a("width"), "Width", Range::linear(0.0, 1.0, Unit::None), 1.0),
        ui::param(&a("color"), "Color", Range::linear(0.0, 1.0, Unit::None), 0.5),
        ui::param(&a("feedback"), "Feedback", Range::linear(0.0, 1.0, Unit::None), 0.0),
        engine,
        ui::toggle(&a("on"), "Pre Mod", false),
    ];
    v.extend(kit::nameplate_params(NS));
    v.extend(kit::flip_params(NS));
    v
}

const KNOB: [&str; 6] = ["color=#16202a", "pointer=#7dd3fc", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#081018"];

/// A phone lane's panel, the head where every Time face has it: the
/// nameplate (the preset ‹ ›, PRE MOD, a press opening its engines), the
/// bypass and the arrow.
fn lane(tier: kit::Tier, back: bool, build: impl FnOnce(&mut Builder<'_>, document_proto::NodeId)) -> document_proto::Scene {
    let w = tier.lane_w();
    canvas("Pre Mod", move |b, root| {
        ui::fit_root(b, root, w, kit::H);
        let mut p = draw::frame("Panel", 0.0, 0.0, w, kit::H);
        p.fills = vec![draw::linear(180.0, &[(0.0, "#121b23"), (1.0, "#0a1016")])];
        let p = b.add(root, draw::with_effects(p, vec![draw::inner_shadow("#ffffff14", 0.0, 1.0, 0.0)]));
        build(b, p);
        let head = Lettering::family(family::INTER).weight(800.0);
        let preset_w = if tier == kit::Tier::Ipad { 150.0 } else { 210.0 };
        kit::head(b, p, NS, back, |b, layer| {
            kit::nameplate_top_right(b, layer, NS, tier, (8.0, 30.0), 110.0, preset_w, 12.0, (SKY, DIM), |b, t| {
                set(b, t, "PRE MOD", 48.0, 15.0, 15.0, SKY, &head, 0.25);
            });
        });
        kit::face_bypass(b, p, NS, tier, "blue", "On");
        kit::flip_arrow(b, p, NS, tier, back, (INK, "#ffffff18"));
    })
}

/// The bucket line and the sweep in a window `vw × vh` (a phone lane's
/// picture): the same machine as the unit's, smaller.
fn picture(b: &mut Builder<'_>, v: document_proto::NodeId, vw: f64, vh: f64) {
    let cells = 12;
    let (bx, by, bw, bh) = (10.0, 12.0, vw - 20.0, 30.0);
    let row = b.add(v, draw::frame("buckets", bx, by, bw, bh));
    // The line and the sweep take the window's width as it widens.
    if let Some(n) = b.scene.node_mut(row) {
        n.layout_child.resize_x = Some(ResizeX::Stretch);
    }
    let cw = bw / f64::from(cells);
    for i in 0..cells {
        let x = f64::from(i) * cw;
        b.add(row, draw::stroked(draw::rounded(draw::rect("bucket", x + 1.5, 0.0, cw - 3.0, bh, draw::solid("#0b1720")), 3.0), "#1b2c38", 1.0));
        let lit = b.add(row, draw::frame("charge", x + 1.5, 0.0, cw - 3.0, bh));
        b.add(lit, ui::glowing(draw::rounded(draw::rect("glow", 0.0, 0.0, cw - 3.0, bh, draw::solid(SKY)), 3.0), SKY, 0.8, 6.0));
        let phase = f64::from(i) / f64::from(cells) * std::f64::consts::TAU;
        let wave = format!("(0.5 + 0.5 * sin(time * 6.2832 * param({NS}/rate) - {phase:.3})) * (0.15 + 0.85 * norm({NS}/depth)) * param({NS}/on)");
        motion::add(b, lit, Motion::new(Target::Opacity).input(wave).range(0.04, 0.95));
    }
    let (sy, sh) = (by + bh + 12.0, vh - (by + bh + 12.0) - 10.0);
    let mut sweep = draw::frame("sweep", 10.0, sy, vw - 20.0, sh);
    sweep.clips_content = true;
    sweep.layout_child.resize_x = Some(ResizeX::Stretch);
    let view = b.add(v, sweep);
    let sw = vw - 20.0;
    b.add(view, ui::strokes("centre", &[vec![(0.0, sh / 2.0), (sw, sh / 2.0)]], 1.0, draw::solid("#ffffff14")));
    for (name, color, shift) in [("left", SKY, 0.0), ("right", "#c4b5fd", std::f64::consts::FRAC_PI_2)] {
        let g = b.add(view, draw::frame(name, 0.0, 0.0, sw * 2.0, sh));
        let pts: Vec<(f64, f64)> = (0..=120)
            .map(|i| {
                let t = f64::from(i) / 120.0;
                (t * sw * 2.0, sh / 2.0 - (t * 4.0 * std::f64::consts::TAU / 2.0 + shift).sin() * (sh / 2.0 - 4.0))
            })
            .collect();
        let wave = b.add(g, draw::frame("wave", 0.0, 0.0, sw * 2.0, sh));
        b.add(wave, ui::glowing(ui::strokes("trace", &[pts], 1.6, draw::solid(color)), color, 0.7, 5.0));
        motion::add(b, wave, Motion::new(Target::StretchY).input(format!("0.1 + 0.9 * norm({NS}/depth)")).range(0.0, 1.0).pivot(0.5, 0.5).dynamics(Dynamics::spring(260.0, 26.0)));
        motion::add(b, g, Motion::new(Target::X).input(format!("fract(time * param({NS}/rate) * 0.5)")).range(0.0, -sw));
    }
}

/// Pre Mod at `tier`: on a phone a lane in the Time faces' form — the
/// machine at the left, the Rate as the Time knob (TIME | BEAT), Depth,
/// Width and Mix in the knob row, the head at the right; elsewhere the
/// unit.
#[must_use]
pub fn premod_at(tier: kit::Tier) -> Face {
    if !matches!(tier, kit::Tier::Phone | kit::Tier::Strip | kit::Tier::Ipad) {
        return premod_unit();
    }
    let scene = lane(tier, false, |b, p| {
        let (vx, vy, vw, vh) = (10.0, 10.0, kit::picture_r(tier) - 10.0, kit::H - 20.0);
        let mut v = draw::frame("Line", vx, vy, vw, vh);
        v.fills = vec![draw::linear(180.0, &[(0.0, "#071017"), (1.0, "#050a0f")])];
        v.clips_content = true;
        v.corner_radii = CornerRadii::uniform(6.0);
        v.effects = vec![draw::inner_shadow("#000000c0", 0.0, 2.0, 6.0)];
        let v = b.add(p, v);
        picture(b, v, vw, vh);
        let (tx, ty, td) = kit::time_spot(tier);
        let ts = if tier == kit::Tier::Ipad { 10.0 } else { 11.0 };
        kit::time_knob(b, p, NS, (tx, ty, td), ts, (INK, DIM, &ui::alpha(SKY, 0.45)), |b, pp, addr| {
            kit::knob3d_bare(b, pp, addr, td, (tx, ty), &KNOB);
        });
        let (xs, ky, kd) = kit::knob_row(tier);
        for (i, (id, legend)) in [("depth", "Depth"), ("width", "Width"), ("mix", "Mix")].iter().enumerate() {
            kit::knob3d(b, p, &a(id), legend, kd, (xs[i], ky), &KNOB, INK, DIM);
        }
        kit::responsive(b, p, &[v]);
    });
    let mut params = params();
    params.extend(kit::time_params(NS));
    Face { file: kit::tier_file("38-pre-mod", tier, false), plugin: PLUGIN, scene, params, streams: Vec::new() }
}

/// Pre Mod's back on a phone: Color and Feedback.
#[must_use]
pub fn premod_back(tier: kit::Tier) -> Face {
    let scene = lane(tier, true, |b, p| {
        kit::back_knobs(b, p, kit::Tier::Phone, &[("color", "Color"), ("feedback", "Feedback")], INK, |b, pp, id, legend, d, at| {
            kit::knob3d(b, pp, &a(id), legend, d, at, &KNOB, INK, DIM);
        });
    });
    Face { file: kit::tier_file("38-pre-mod-back", tier, false), plugin: PLUGIN, scene, params: params(), streams: Vec::new() }
}
