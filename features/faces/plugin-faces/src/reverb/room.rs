//! The room: a plain studio box whose character is its early reflections.
//!
//! The recessed plate at the top draws the room in perspective — back wall,
//! floor lines, the source against the back wall and the listener near the
//! front — with the first reflections folding off the walls, and beside it
//! those same reflections as a tap diagram over the first 80 ms. SIZE and
//! DECAY, the two a room is chosen by, are the big brushed knobs on the
//! right with the room keys under them; the rest run along the bottom.
//!
//! The drawing is the room the params make (motion + the `reverb.room`
//! shape provider): turn SIZE and the back wall recedes or comes forward
//! and every reflection path re-traces off the moved walls; DIFFUSION sets
//! how many reflections there are, DECAY adds the second-order paths and
//! slows the taps' fall, WALLS brightens them, and they light with the
//! output level.

use document_proto::motion::{Dynamics, Motion, Num, Target};
use document_proto::{Node, NodeId, Stroke, StrokeCap, StrokeJoin};
use surface_kit::{Place, draw, motion};
use surface_proto::Unit;
use surface_widgets::Builder;

use super::ui::{self, Dial, bind};
use crate::animate;
use crate::face::{Face, canvas};

const W: f64 = 960.0;
const H: f64 = 320.0;
const INK: &str = "#e3e8ee";
const DIM: &str = "#98a2ad";
const RAY: &str = "#86d69a";

/// The room's params as the `reverb.room` shape provider reads them —
/// smoothed by springs on the plate, so the walls glide when a knob turns.
const LIVE: &[(&str, &str)] = &[("size", "motion(size)"), ("decay", "motion(decay)"), ("diffusion", "motion(diffusion)")];

/// The plate's drawing: the room in perspective, its first reflections and
/// the tap diagram — every part that depends on the room redrawn by the
/// `reverb.room` provider from SIZE (the back wall recedes, the source goes
/// with it), DIFFUSION (how many reflections are traced) and DECAY (the
/// second-order paths, the taps' fall).
fn room_art(b: &mut Builder<'_>, plate: NodeId, w: f64, h: f64, rest: &[(&str, f64)]) {
    let line = ui::alpha(INK, 0.55);
    let faint = ui::alpha(INK, 0.16);
    for (name, addr) in [("size", "room/size"), ("decay", "room/decay"), ("diffusion", "room/diffusion")] {
        motion::add(b, plate, Motion::new(Target::None).named(name).input(format!("norm({addr})")).dynamics(Dynamics::spring(90.0, 19.0)));
    }
    let part = |name: &str, part: &str| animate::shape_node(name, "reverb.room", part, (w, h), &[], LIVE, rest);
    let fill = |mut n: Node, paint| {
        n.fills = vec![paint];
        n
    };
    let stroke = |mut n: Node, weight: f64, color: &str| {
        n.stroke = Stroke { paints: vec![draw::solid(color)], weight, cap: StrokeCap::Round, join: StrokeJoin::Round, ..Stroke::default() };
        n
    };
    // The box: front opening (fixed), back wall and the edges between.
    b.add(plate, fill(part("back wall", "back"), draw::linear(180.0, &[(0.0, "#ffffff08"), (1.0, "#ffffff12")])));
    b.add(plate, fill(part("floor", "floor"), draw::linear(180.0, &[(0.0, "#86d69a0a"), (1.0, "#86d69a1c")])));
    b.add(plate, stroke(part("edges", "edges"), 1.0, &line));
    b.add(plate, stroke(part("back wall line", "outline"), 1.2, &line));
    let (fx0, fy0, fx1, fy1) = (22.0, 14.0, 318.0, h - 14.0);
    ui::line(b, plate, "front", &[(fx0, fy0), (fx1, fy0), (fx1, fy1), (fx0, fy1), (fx0, fy0)], 1.0, &faint);
    b.add(plate, stroke(part("boards", "boards"), 0.6, &faint));
    // Source at the back wall, listener near the front.
    b.add(plate, fill(part("speaker", "speaker"), draw::linear(180.0, &[(0.0, "#4a5057"), (1.0, "#22262a")])));
    b.add(plate, fill(part("cone", "cone"), draw::solid("#0e1012")));
    let ear = (fx0 + (fx1 - fx0) * 0.6, fy1 - 16.0);
    b.add(plate, ui::glowing(draw::circle("listener", ear.0, ear.1, 4.0, draw::solid(RAY)), RAY, 0.8, 5.0));
    b.add(plate, stroke(part("direct", "direct"), 0.8, &ui::alpha(INK, 0.35)));
    // The reflections: first order bright, second order (DECAY) dim. Harder
    // WALLS reflect brighter; the paths light up with the output.
    let rays = b.add(plate, ui::glowing(stroke(part("rays", "rays"), 1.1, &ui::alpha(RAY, 0.8)), RAY, 0.8, 4.0));
    motion::add(b, rays, Motion::new(Target::Opacity).input("0.55 + 0.45*norm(room/walls)"));
    motion::add(b, rays, Motion::new(Target::Emission).input("0.6 + 1.4*level(room/out)").dynamics(Dynamics::Ballistic { attack: Num::Value(0.02), release: Num::Value(0.45) }));
    b.add(plate, stroke(part("echoes", "echoes"), 0.8, &ui::alpha(RAY, 0.3)));
    b.add(plate, ui::glowing(fill(part("bounces", "bounces"), draw::solid(RAY)), RAY, 0.8, 4.0));

    // The tap diagram: the same reflections over time.
    let (x0, x1, base) = (352.0, w - 20.0, h - 30.0);
    ui::silk_left(b, plate, "Early reflections", x0, 16.0, 6.5, DIM, 800.0);
    ui::line(b, plate, "axis", &[(x0, base), (x1, base)], 1.0, &line);
    for ms in [0.0, 20.0, 40.0, 60.0, 80.0] {
        let x = x0 + (x1 - x0) * ms / 80.0;
        ui::line(b, plate, "tick", &[(x, base), (x, base + 3.5)], 1.0, &line);
        let l = if ms >= 80.0 { "80 ms".to_string() } else { format!("{ms:.0}") };
        ui::plain(b, plate, &l, x, base + 11.0, 6.5, DIM, 600.0);
    }
    let mut taps = stroke(part("taps", "taps"), 2.2, RAY);
    taps.stroke.cap = StrokeCap::None;
    b.add(plate, ui::glowing(taps, RAY, 0.7, 3.0));
    b.add(plate, fill(part("tap heads", "tap_heads"), draw::solid(RAY)));
}

pub fn face() -> Face {
    let decay = ui::param("room/decay", "Decay", surface_proto::Range::log(0.2, 4.0, Unit::Seconds), 0.8).featured();
    // The room at rest: the params' defaults, normalized.
    let rest = [("size", 0.45), ("decay", decay.default), ("diffusion", 0.6)];
    let scene = canvas("Room Reverb", |b, root| {
        ui::fit_root(b, root, W, H);
        let panel = ui::panel(b, root, "panel.painted", "charcoal", W, H, "ears");

        ui::plain(b, panel, "Room", 80.0, 32.0, 17.0, INK, 800.0);
        ui::silk_left(b, panel, "Ambience processor", 118.0, 28.0, 7.5, INK, 800.0);
        ui::silk_left(b, panel, "Early reflections · short decay", 118.0, 40.0, 6.0, DIM, 700.0);

        // ── The plate: the room and its reflections ───────────────────
        let (px, py, pw, ph) = (50.0, 54.0, 560.0, 140.0);
        let mut plate = draw::frame("Room", px, py, pw, ph);
        plate.fills = vec![draw::linear(180.0, &[(0.0, "#16191c"), (1.0, "#202428")])];
        plate.corner_radii = document_proto::CornerRadii::uniform(6.0);
        plate.clips_content = true;
        plate.effects = vec![draw::inner_shadow("#000000c0", 0.0, 2.0, 7.0), draw::drop_shadow("#ffffff1a", 0.0, 1.0, 0.0)];
        let plate = b.add(panel, plate);
        room_art(b, plate, pw, ph, &rest);

        // ── SIZE and DECAY, with the room keys ─────────────────────────
        let big = Dial::new("knob.metal", "silver", 58.0, INK, DIM).legend(10.0);
        ui::dial(b, panel, "room/size", "Size", 704.0, 96.0, &big.scale(&["0", "", "", "", "", "100"]).legend_at(160.0));
        ui::dial(b, panel, "room/decay", "Decay", 842.0, 96.0, &big.scale(&["0.2", "", "", "", "", "4 s"]).legend_at(160.0));
        let keys = Place::new("Room").size(62.0).variant("green").bind(bind("room/type")).options(&["Room", "Chamber", "Studio"]);
        let k = ui::put(b, panel, "button.led_bank", &keys, 773.0, 240.0);
        ui::strip_legend(b, k);
        ui::silk(b, panel, "Space", 773.0, 272.0, 7.5, INK, 800.0);
        ui::rule(b, panel, 640.0, 40.0, 1.0, 250.0, "#00000066");
        ui::rule(b, panel, 641.0, 40.0, 1.0, 250.0, "#ffffff12");

        // ── The row ────────────────────────────────────────────────────
        let small = Dial::new("knob.metal", "black", 38.0, INK, DIM).legend_at(292.0);
        let row: &[(&str, &str)] = &[
            ("room/predelay", "Pre-Delay"),
            ("room/damping", "Damping"),
            ("room/tone", "Tone"),
            ("room/diffusion", "Diffusion"),
            ("room/walls", "Walls"),
            ("room/width", "Width"),
            ("room/mix", "Mix"),
        ];
        let pitch = (pw - 40.0) / (row.len() - 1) as f64;
        for (i, (addr, legend)) in row.iter().enumerate() {
            ui::dial(b, panel, addr, legend, px + 20.0 + pitch * i as f64, 238.0, &small);
        }
    });

    let params = vec![
        super::pct("room/size", "Size", 45.0).featured(),
        decay,
        ui::choice("room/type", "Room", &["Room", "Chamber", "Studio"], 0).featured(),
        super::predelay("room/predelay", "Pre-Delay", 5.0),
        super::pct("room/damping", "Damping", 35.0),
        super::pct("room/tone", "Tone", 50.0),
        super::pct("room/diffusion", "Diffusion", 60.0),
        super::pct("room/walls", "Walls", 50.0),
        super::width("room/width"),
        super::mix("room/mix", 25.0),
    ];
    Face { file: "04-room", plugin: "reverb", scene, params, streams: super::levels("room") }
}
