//! The transformer: a line-output iron unit in Pultec blue-grey enamel.
//!
//! A glazed window into the chassis shows the part doing the work — a
//! frame-type output transformer, its E-I lamination stack seen three
//! quarters on (the sheets' edges striping its top and side), clamped
//! between two painted steel brackets bolted through the stack, and the
//! enamelled copper winding standing proud of it under a paper wrap that
//! carries its rating. The copper warms with DRIVE: the winding glows and
//! throws light on the chassis floor (bloom on the GPU stage). Its leads
//! run to a phenolic tag board. Beside it the output VU under glass,
//! lamp-lit; left of it the engraved badge, the pilot lamp and the
//! EMPHASIS key.
//!
//! One family of knobs — black skirted caps on knurled skirts, a white
//! index — DRIVE and OUTPUT the largest, in the same places as on every
//! saturation face.

use document_proto::NodeId;
use document_proto::motion::{Dynamics, Motion, Num, Target};
use surface_kit::{Place, draw, motion};
use surface_widgets::Builder;

use super::hw;
use crate::delay::art;
use crate::delay::parts::{self, Ink};
use crate::face::{Face, canvas};
use crate::lane::Shape;

const W: f64 = 960.0;
const H: f64 = 320.0;
const INK: &str = "#f1f4f6";
const DIM: &str = "#c9d5de";
/// The emphasis EQ's colour: the winding's copper, warm.
const COPPER: &str = "#e8804a";
/// The panel: Pultec blue-grey enamel, a shade deeper.
const PAINT: &[(f64, &str)] = &[(0.0, "#56768b"), (0.5, "#4a687c"), (1.0, "#3c5668")];

/// The knob family: a black moulded cap on a knurled skirt, white index.
const KNOB: (&str, &[&str]) = (
    "flat",
    &[
        "color=#17181a",
        "rough=0.24",
        "coat=0.85",
        "cap_r=0.74",
        "height=0.58",
        "dome=0.1",
        "edge=0.12",
        "skirt=knurled",
        "skirt_color=#1b1c1e",
        "skirt_r=1.0",
        "skirt_h=0.2",
        "skirt_teeth=44",
        "index=line",
        "pointer=#f3f1ea",
        "index_w=0.08",
        "index_in=0.18",
        "index_out=0.98",
        "spec=1.2",
        "shadow=1.3",
    ],
);

fn a(s: &str) -> String {
    format!("transformer/{s}")
}

/// How hot the copper runs: DRIVE, warming and cooling as a winding does.
fn heat(b: &mut Builder<'_>, id: NodeId, lo: f64, hi: f64) {
    let m = Motion::new(Target::Opacity).input(format!("norm({})", a("drive"))).range(lo, hi).dynamics(Dynamics::Ballistic { attack: Num::Value(0.4), release: Num::Value(1.2) });
    motion::add(b, id, m);
}

/// A filled polygon in the coordinates of a frame `w × h`.
fn poly(name: &str, (w, h): (f64, f64), pts: &[(f64, f64)], fill: document_proto::Paint) -> document_proto::Node {
    draw::path(name, 0.0, 0.0, w, h, art::poly_d(pts, false), fill)
}

/// Lines from `a + t·d` to `b + t·d` for `n` steps of `t` in 0..1: the
/// edges of a stack of sheets (or turns of wire) along `d`.
fn stack_lines((ax, ay): (f64, f64), (bx, by): (f64, f64), (dx, dy): (f64, f64), n: u32) -> Vec<Vec<(f64, f64)>> {
    (1..n)
        .map(|i| {
            let t = f64::from(i) / f64::from(n);
            vec![(ax + dx * t, ay + dy * t), (bx + dx * t, by + dy * t)]
        })
        .collect()
}

/// A hex bolt head seen from the front.
fn bolt(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), r: f64, (w, h): (f64, f64)) {
    let hex: Vec<(f64, f64)> = (0..6).map(|i| draw::polar(cx, cy, r, 30.0 + 60.0 * f64::from(i))).collect();
    let n = poly("bolt", (w, h), &hex, draw::linear(160.0, &[(0.0, "#f4f5f6"), (0.45, "#a9adb2"), (1.0, "#4b4e52")]));
    b.add(parent, draw::with_effects(draw::stroked(n, "#1a1b1d", 0.5), vec![draw::drop_shadow("#000000a0", 0.4, 1.0, 1.4)]));
    b.add(parent, draw::circle("bolt end", cx, cy, r * 0.45, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#d8dadd"), (1.0, "#6b6e72")])));
}

/// The output transformer, its art `214 × 140` from (`x`, `y`) of `win`.
/// Returns where its four leads leave the winding (window coordinates).
fn iron(b: &mut Builder<'_>, win: NodeId, (x, y): (f64, f64)) -> [(f64, f64); 4] {
    let (tw, th) = (214.0, 140.0);
    let root = b.add(win, draw::frame("transformer", x, y, tw, th));
    let sz = (tw, th);
    // Depth runs up and to the right.
    let (dx, dy) = (24.0, -17.0);
    let (fx, fy, fw, fh) = (10.0, 30.0, 150.0, 92.0);

    // Its shadow on the chassis floor.
    let shadow = draw::rounded(draw::rect("shadow", fx - 6.0, fy + fh - 6.0, fw + dx + 18.0, 16.0, draw::solid("#000000a0")), 8.0);
    b.add(root, draw::with_effects(shadow, vec![draw::drop_shadow("#000000c0", 4.0, 2.0, 10.0)]));

    // The stack: the top and right faces show every sheet's edge.
    let top = [(fx, fy), (fx + fw, fy), (fx + fw + dx, fy + dy), (fx + dx, fy + dy)];
    b.add(root, poly("stack top", sz, &top, draw::linear(180.0, &[(0.0, "#8e959c"), (1.0, "#b7bdc3")])));
    let side = [(fx + fw, fy), (fx + fw + dx, fy + dy), (fx + fw + dx, fy + fh + dy), (fx + fw, fy + fh)];
    b.add(root, poly("stack side", sz, &side, draw::linear(180.0, &[(0.0, "#4d535a"), (1.0, "#2a2e33")])));
    let n = 22;
    let sheets = stack_lines((fx, fy), (fx + fw, fy), (dx, dy), n);
    b.add(root, draw::with_opacity(parts::stroke_path("sheet edges", (0.0, 0.0, tw, th), &sheets, 0.6, "#24282c", false), 0.8));
    let lit = stack_lines((fx + 0.5, fy - 0.45), (fx + fw + 0.5, fy - 0.45), (dx, dy), n);
    b.add(root, draw::with_opacity(parts::stroke_path("sheet lips", (0.0, 0.0, tw, th), &lit, 0.4, "#ffffff", false), 0.5));
    let sheets = stack_lines((fx + fw, fy), (fx + fw, fy + fh), (dx, dy), n);
    b.add(root, draw::with_opacity(parts::stroke_path("sheet edges side", (0.0, 0.0, tw, th), &sheets, 0.6, "#0c0d0f", false), 0.85));
    let lit = stack_lines((fx + fw + 0.45, fy), (fx + fw + 0.45, fy + fh), (dx, dy), n);
    b.add(root, draw::with_opacity(parts::stroke_path("sheet lips side", (0.0, 0.0, tw, th), &lit, 0.35, "#c8d0d8", false), 0.35));

    // The front sheet: silicon steel, a faint blue temper.
    let face = draw::rect("stack face", fx, fy, fw, fh, draw::linear(165.0, &[(0.0, "#7c858e"), (0.45, "#5d656e"), (1.0, "#3d434a")]));
    b.add(root, draw::stroked(face, "#16181b", 0.8));
    b.add(root, draw::rect("temper", fx, fy, fw, fh, draw::linear(120.0, &[(0.0, "#6a7ca000"), (0.5, "#6a7ca026"), (1.0, "#a0786a1a")])));
    // The key light across the sheet: a soft diagonal sheen.
    b.add(root, draw::rect("sheen", fx, fy, fw, fh, draw::linear(115.0, &[(0.0, "#ffffff00"), (0.28, "#ffffff2e"), (0.42, "#ffffff00"), (1.0, "#00000030")])));
    // The E's back and the I's seam.
    parts::rule(b, root, fx, fx + fw, fy + fh * 0.8, 0.8, "#0e1012c0");
    parts::rule(b, root, fx, fx + fw, fy + fh * 0.8 + 0.9, 0.6, "#ffffff2a");

    // The winding, standing proud of the stack toward us.
    let (pdx, pdy) = (-dx * 0.42, -dy * 0.42);
    let (cw, ch) = (fw * 0.56, fh * 0.6);
    let (cx0, cy0) = (fx + fw / 2.0 - cw / 2.0 + pdx, fy + fh * 0.46 - ch / 2.0 + pdy);
    let coil_top = [(cx0 + 2.0, cy0), (cx0 + cw - 2.0, cy0), (cx0 + cw - 2.0 - pdx, cy0 - pdy), (cx0 + 2.0 - pdx, cy0 - pdy)];
    let coil_side = [(cx0 + cw, cy0 + 2.0), (cx0 + cw - pdx, cy0 + 2.0 - pdy), (cx0 + cw - pdx, cy0 + ch - 2.0 - pdy), (cx0 + cw, cy0 + ch - 2.0)];
    b.add(root, poly("coil top", sz, &coil_top, draw::linear(180.0, &[(0.0, "#b8683a"), (1.0, "#f0b07a")])));
    b.add(root, poly("coil side", sz, &coil_side, draw::linear(90.0, &[(0.0, "#7a3a18"), (1.0, "#3e1c0a")])));
    let turns = stack_lines((cx0 + 2.0, cy0), (cx0 + cw - 2.0, cy0), (-pdx, -pdy), 7);
    b.add(root, draw::with_opacity(parts::stroke_path("turns top", (0.0, 0.0, tw, th), &turns, 0.5, "#4a2008", false), 0.6));
    let front = draw::rounded(draw::rect("coil", cx0, cy0, cw, ch, draw::linear(180.0, &[(0.0, "#f6c08a"), (0.18, "#d98b4e"), (0.5, "#a85a28"), (0.82, "#7a3c18"), (1.0, "#5a2a10")])), 4.0);
    b.add(root, draw::with_effects(draw::stroked(front, "#2a1206", 0.7), vec![draw::drop_shadow("#00000080", -2.0, 2.0, 3.0)]));
    // The turns: round wire, each catching a line of light.
    let wire: Vec<Vec<(f64, f64)>> = (0..).map(|i| cy0 + 1.6 + 1.75 * f64::from(i)).take_while(|yy| *yy < cy0 + ch - 1.0).map(|yy| vec![(cx0 + 1.0, yy), (cx0 + cw - 1.0, yy)]).collect();
    b.add(root, draw::with_opacity(parts::stroke_path("turns", (0.0, 0.0, tw, th), &wire, 0.6, "#3a1806", false), 0.55));
    let glints: Vec<Vec<(f64, f64)>> = wire.iter().map(|l| l.iter().map(|(px, py)| (*px, py - 0.75)).collect()).collect();
    b.add(root, draw::with_opacity(parts::stroke_path("turn glints", (0.0, 0.0, tw, th), &glints, 0.35, "#ffe2c0", false), 0.4));
    // The curve of the winding round the leg: lit on the left, falling
    // away into the window at the right.
    b.add(root, draw::rounded(draw::rect("coil curve", cx0, cy0, cw, ch, draw::linear(90.0, &[(0.0, "#ffffff30"), (0.2, "#ffffff00"), (0.75, "#00000000"), (1.0, "#00000070")])), 4.0));

    // Heat: the copper glows with DRIVE, and throws light round it.
    let glow_d = art::poly_d(&coil_top, false) + &art::poly_d(&coil_side, false) + &draw::polygon(&[(cx0, cy0), (cx0 + cw, cy0), (cx0 + cw, cy0 + ch), (cx0, cy0 + ch)]);
    let hot = draw::path("hot copper", 0.0, 0.0, tw, th, glow_d, draw::radial_at(0.45, 0.4, 0.75, &[(0.0, "#ffc070b0"), (0.55, "#ff702878"), (1.0, "#d8381860")]));
    let hot = b.add(root, parts::emit(hot, "#ff7a2a", 1.8, 26.0));
    heat(b, hot, 0.04, 1.0);

    // The paper wrap and its rating.
    let (py0, ph) = (cy0 + ch * 0.32, ch * 0.4);
    let paper = draw::rect("paper", cx0 - 0.5, py0, cw + 1.0, ph, draw::linear(180.0, &[(0.0, "#efe4c8"), (0.5, "#e2d4b0"), (1.0, "#c9b890")]));
    b.add(root, draw::with_effects(paper, vec![draw::drop_shadow("#00000060", 0.0, 1.0, 1.5)]));
    parts::rule(b, root, cx0, cx0 + cw, py0 + 0.6, 0.5, "#ffffff80");
    parts::silk(b, root, "Out · 1 : 2", cx0 + cw / 2.0, py0 + ph * 0.36, Ink::new(6.4, "#3a2a18").weight(900.0).track(0.12));
    parts::silk(b, root, "600 Ω  ·  Ni-Fe", cx0 + cw / 2.0, py0 + ph * 0.74, Ink::new(4.8, "#5a4630").weight(800.0).track(0.14).plain());

    // The clamp: two painted steel brackets bolted through the stack, the
    // lower one turned out into mounting feet.
    let bracket = |name: &str, yy: f64| {
        let n = draw::rect(name, fx - 5.0, yy, fw + 10.0, 13.0, draw::linear(180.0, &[(0.0, "#55595f"), (0.12, "#2c2f33"), (0.85, "#1b1d20"), (1.0, "#0c0d0e")]));
        draw::with_effects(draw::stroked(n, "#000000", 0.6), vec![draw::inner_shadow("#ffffff40", 0.0, 1.0, 0.0), draw::drop_shadow("#000000a0", 0.0, 1.5, 2.5)])
    };
    b.add(root, bracket("upper bracket", fy - 3.0));
    let ytop = [(fx - 5.0, fy - 3.0), (fx + fw + 5.0, fy - 3.0), (fx + fw + 5.0 + dx * 0.22, fy - 3.0 + dy * 0.22), (fx - 5.0 + dx * 0.22, fy - 3.0 + dy * 0.22)];
    b.add(root, poly("upper bracket top", sz, &ytop, draw::linear(180.0, &[(0.0, "#2a2d31"), (1.0, "#6a6e74")])));
    let feet = draw::rounded(draw::rect("feet", fx - 20.0, fy + fh - 1.0, fw + 40.0, 6.0, draw::linear(180.0, &[(0.0, "#4a4e53"), (1.0, "#111214")])), 1.5);
    b.add(root, draw::with_effects(feet, vec![draw::drop_shadow("#000000c0", 0.0, 2.0, 3.0)]));
    b.add(root, bracket("lower bracket", fy + fh - 11.0));
    for (bx, by) in [(fx + 6.0, fy + 3.5), (fx + fw - 6.0, fy + 3.5), (fx + 6.0, fy + fh - 4.5), (fx + fw - 6.0, fy + fh - 4.5)] {
        bolt(b, root, (bx, by), 3.6, sz);
    }
    for sx in [fx - 13.0, fx + fw + 13.0] {
        art::screw(b, root, (sx, fy + fh + 2.0), 2.6, 25.0);
    }

    // Where the leads leave the winding (its top, toward the back).
    let (lx, ly) = (cx0 + cw * 0.5 - pdx * 0.5, cy0 - pdy * 0.5);
    [0.0, 1.0, 2.0, 3.0].map(|i| (x + lx - 12.0 + i * 8.0, y + ly))
}

/// The tag board the leads land on: phenolic, four turrets, their numbers.
fn tag_board(b: &mut Builder<'_>, win: NodeId, (x, y): (f64, f64), leads: [(f64, f64); 4]) {
    let (w, h) = (34.0, 112.0);
    let board = draw::rounded(draw::rect("tag board", x, y, w, h, draw::linear(90.0, &[(0.0, "#3e2413"), (0.5, "#6b4125"), (1.0, "#4a2c18")])), 2.5);
    b.add(win, draw::with_effects(draw::stroked(board, "#1a0e06", 0.6), vec![draw::drop_shadow("#000000b0", 1.0, 2.0, 4.0)]));
    let tags = [("#c8392b", "1"), ("#e9e6de", "2"), ("#2f6fd0", "5"), ("#1d1d1f", "6")];
    for (i, ((col, label), (x0, y0))) in tags.iter().zip(leads).enumerate() {
        let ty = y + 18.0 + i as f64 * 25.0;
        let (x1, y1) = (x + 9.0, ty);
        // Each lead: an arc up off the winding and down to its turret.
        let lift = 18.0 + i as f64 * 5.0;
        let pts: Vec<(f64, f64)> = (0..=20)
            .map(|k| {
                let t = f64::from(k) / 20.0;
                let u = 1.0 - t;
                let (c1, c2) = ((x0 + 4.0, y0 - lift), (x1 - 30.0, y1));
                (u * u * u * x0 + 3.0 * u * u * t * c1.0 + 3.0 * u * t * t * c2.0 + t * t * t * x1, u * u * u * y0 + 3.0 * u * u * t * c1.1 + 3.0 * u * t * t * c2.1 + t * t * t * y1)
            })
            .collect();
        parts::line(b, win, "lead shadow", &pts.iter().map(|(px, py)| (px + 1.0, py + 2.0)).collect::<Vec<_>>(), 2.6, "#00000070");
        parts::line(b, win, "lead", &pts, 2.4, col);
        parts::line(b, win, "lead sheen", &pts.iter().map(|(px, py)| (*px, py - 0.6)).collect::<Vec<_>>(), 0.6, "#ffffff55");
        b.add(win, draw::circle("turret", x1, y1, 3.6, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#fff4d0"), (0.45, "#c9a24a"), (1.0, "#6a4e1a")])));
        b.add(win, draw::circle("solder", x1, y1, 2.0, draw::radial_at(0.35, 0.3, 0.8, &[(0.0, "#ffffff"), (0.5, "#b9bcc0"), (1.0, "#6a6d72")])));
        parts::silk(b, win, label, x + 24.0, ty, Ink::new(6.0, "#ead8b8").weight(800.0).track(0.0));
    }
    for ry in [y + 5.0, y + h - 5.0] {
        art::screw(b, win, (x + w / 2.0, ry), 2.4, 60.0);
    }
}

/// The chassis window `(x, y, w, h)` on `pic`: the floor, the light the
/// winding throws on it, the transformer and its tag board.
fn chassis(b: &mut Builder<'_>, pic: NodeId, (x, y, w, h): (f64, f64, f64, f64)) {
    let win = hw::window(b, pic, "Chassis window", (x, y, w, h), &[(0.0, "#2c3036"), (1.0, "#15171a")]);
    // Pressed ribs in the floor.
    for i in 0..4 {
        let yy = 18.0 + f64::from(i) * 40.0;
        parts::rule(b, win, 0.0, w, yy, 1.0, "#0000004a");
        parts::rule(b, win, 0.0, w, yy + 1.0, 1.0, "#ffffff0c");
    }
    let art_w = 214.0 + 50.0;
    let ax = ((w - art_w) / 2.0).max(6.0);
    let ay = (h - 140.0) / 2.0 + 4.0;
    // The warm pool on the floor round the iron.
    let pool = draw::circle("heat pool", ax + 95.0, ay + 74.0, 120.0, draw::radial(&[(0.0, "#ff7a2a70"), (0.45, "#ff5a1a28"), (1.0, "#ff5a1a00")]));
    let pool = b.add(win, pool);
    heat(b, pool, 0.0, 1.0);
    let leads = iron(b, win, (ax, ay));
    tag_board(b, win, (ax + 228.0, ay + 18.0), leads);
    art::screw(b, win, (12.0, h - 12.0), 3.4, 30.0);
    art::screw(b, win, (w - 12.0, 12.0), 3.4, 70.0);
    hw::window_lip(b, win);
}

/// The front, `w × h`, its parts between `x0` and `x1`.
fn front(b: &mut Builder<'_>, panel: NodeId, (w, h): (f64, f64), (x0, x1): (f64, f64)) {
    // ── Badge, pilot, EMPHASIS ────────────────────────────────────────
    let wide = x1 - x0 > 800.0;
    let col = if wide { 196.0 } else { 168.0 };
    hw::badge(b, panel, (x0, 20.0, col, 46.0), ("Transformer", 13.5), "Line output stage");
    parts::silk_left(b, panel, "Nickel-iron · class A", x0 + 2.0, 82.0, Ink::new(6.4, DIM).weight(700.0).track(0.2));
    super::emphasis_key(b, panel, "transformer", (x0 + 33.0, 132.0), 66.0, "black", COPPER);
    parts::lamp(b, panel, (x0 + 100.0, 132.0), 10.0, "#ff4a36", true);
    parts::silk_left(b, panel, "Power", x0 + 113.0, 132.0, Ink::new(6.6, DIM).weight(800.0).track(0.22));

    // ── The chassis and the output VU; or the emphasis EQ ─────────────
    let pic = super::picture(b, panel, "transformer", (w, h));
    let (wx, vw) = (x0 + col + 16.0, if wide { 228.0 } else { 214.0 });
    let wr = x1 - vw - 14.0;
    chassis(b, pic, (wx, 16.0, wr - wx, 160.0));
    let vu = Place::new("Output").size(vw - 8.0).variant("black_rack").legend("VU").bind(parts::stream(&a("out")));
    let vu = parts::put(b, pic, "meter.vu_amber", &vu, x1 - vw / 2.0, 86.0);
    parts::glass(b, vu);
    let vh = parts::size_of(b, vu).1;
    parts::silk(b, pic, "Output", x1 - vw / 2.0, 86.0 + vh / 2.0 + 9.0, Ink::new(6.8, DIM).weight(800.0).track(0.24));
    super::emphasis_eq(b, panel, "transformer", (wx - 5.0, 11.0, x1 - wx + 5.0, 170.0), COPPER);

    // ── Knob row ──────────────────────────────────────────────────────
    parts::groove(b, panel, x0, x1, 190.0);
    let row = hw::Row { look: KNOB, ink: (INK, DIM), y: h - 86.0, legend_y: h - 24.0, value_y: h - 11.0 };
    let span = x1 - x0;
    let at = |f: f64| x0 + span * f;
    let knobs: [hw::Spec<'_>; 7] = [
        ("drive", "Drive", at(0.065), 76.0, parts::PCT_MARKS),
        ("core", "Core", at(0.215), 62.0, super::TRIM_MARKS),
        ("bloom", "Low Bloom", at(0.35), 60.0, super::TRIM_MARKS),
        ("hysteresis", "Hysteresis", at(0.48), 60.0, super::TRIM_MARKS),
        ("sag", "Sag", at(0.61), 60.0, super::TRIM_MARKS),
        ("mix", "Mix", at(0.775), 62.0, parts::PCT_MARKS),
        ("output", "Output", at(0.93), 72.0, super::OUT_MARKS),
    ];
    for (addr, name, x, d, marks) in knobs {
        hw::knob(b, panel, &row, (&a(addr), name, x, d, marks));
    }
    parts::vgroove(b, panel, at(0.69), 204.0, h - 14.0);
}

pub fn face() -> Face {
    let scene = canvas("Transformer", |b, root| {
        parts::fit_root(b, root, W, H);
        let panel = parts::panel(b, root, "panel.painted", "pultec", W, H, "ears");
        parts::repaint(b, panel, PAINT);
        front(b, panel, (W, H), (52.0, W - 52.0));
    });
    Face { file: "03-transformer", plugin: "saturate", scene, params: params(), streams: streams() }
}

fn params() -> Vec<surface_proto::ParamInfo> {
    vec![
        parts::percent(&a("drive"), "Drive", 30.0).featured(),
        parts::trim(&a("core"), "Core").featured(),
        parts::trim(&a("bloom"), "Low Bloom"),
        parts::trim(&a("hysteresis"), "Hysteresis"),
        parts::trim(&a("sag"), "Sag"),
        parts::percent(&a("mix"), "Mix", 100.0).featured(),
        super::output(&a("output")),
    ]
    .into_iter()
    .chain(super::emphasis_params("transformer"))
    .collect()
}

fn streams() -> Vec<surface_proto::StreamInfo> {
    vec![parts::level_stream(&a("out"), "Output Level", 2)]
}

/// The transformer alone in a lane ([`Shape::Tall`], 760 × 328): the same
/// front, narrower — badge, chassis window and VU across the top, the
/// seven knobs below.
pub fn tall_face() -> Face {
    let (w, h) = Shape::Tall.size();
    let scene = canvas("Transformer (tall)", |b, root| {
        parts::fit_root(b, root, w, h);
        let panel = parts::panel(b, root, "panel.painted", "pultec", w, h, "none");
        parts::repaint(b, panel, PAINT);
        for (x, y) in [(10.0, 10.0), (10.0, h - 10.0), (w - 10.0, 10.0), (w - 10.0, h - 10.0)] {
            art::screw(b, panel, (x, y), 4.0, 30.0);
        }
        front(b, panel, (w, h), (24.0, w - 24.0));
    });
    Face { file: Shape::Tall.file("03-transformer"), plugin: "saturate", scene, params: params(), streams: streams() }
}
