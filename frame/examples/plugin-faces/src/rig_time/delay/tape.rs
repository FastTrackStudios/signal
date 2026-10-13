//! Tape: the boutique suite's pale grey tape echo (`suite_boutique_clean`'s
//! *ECHO* rack) laid out as a lane of the Time module, in the same four
//! bays: the tall E C H O over the printed VU and the time display; the
//! active-head lamps over the big white dial (here the delay time); the
//! black and orange knurled knobs; the brush mark, the emblem and the
//! orange POWER.
//!
//! The VU's needle kicks with every repeat and falls back, harder the more
//! feedback there is; the head lamps flash on their repeats — the n-th
//! head n times the time after the note, dimmer as the repeats die away.

use document_proto::motion::{Motion, Target};
use document_proto::{NodeId, TextAlign};
use surface_kit::meters::scale::Card;
use surface_kit::{Place, TOUCH_MIN, draw, motion};
use surface_widgets::Builder;

use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, DLY};
use crate::suite_boutique_clean::look::{self, KnobLook};
use crate::suite_funk_rhythm::parts::{self, Type};

/// The accent: delay blue where the original is orange.
const ORANGE: &str = kit::BLUE;
const INK: &str = "#1f2023";
const LAMP: &str = "#e2eeff";

fn a(id: &str) -> String {
    format!("{DLY}/{id}")
}

/// How loud the repeats are now, 0…1: a kick on every repeat falling
/// away, bigger with more feedback, still when it's off.
fn repeats() -> String {
    format!("pow(1 - fract(time * 1000 / max(param({DLY}/time), 20)), 3) * (0.35 + 0.65 * param({DLY}/feedback) / 0.95) * param({DLY}/on)")
}

fn bay(b: &mut Builder<'_>, parent: NodeId, name: &str, (x, y, w, h): (f64, f64, f64, f64), fill: &[(f64, &str)]) -> NodeId {
    let p = look::slab(name, (x, y, w, h), 3.0, vec![draw::linear(180.0, fill), draw::linear(90.0, &[(0.0, "#ffffff00"), (0.5, "#ffffff10"), (1.0, "#ffffff00")])]);
    b.add(parent, draw::with_effects(p, vec![draw::inner_shadow("#ffffff80", 0.0, 1.0, 0.5), draw::drop_shadow("#00000040", 0.0, 1.0, 2.0)]))
}

fn screws(b: &mut Builder<'_>, parent: NodeId, pts: &[(f64, f64)]) {
    for (x, y) in pts {
        b.add(parent, draw::circle("screw", *x, *y, 4.0, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#f4f4f4"), (1.0, "#8a8b8d")])));
        b.add(parent, draw::rect("cross", x - 2.6, y - 0.5, 5.2, 1.0, draw::solid("#3a3a3c")));
        b.add(parent, draw::rect("cross", x - 0.5, y - 2.6, 1.0, 5.2, draw::solid("#3a3a3c")));
    }
}

/// The VU printed on the panel, needle pivoting at (`px`, `py`), scale
/// radius `r` — a real VU card's law (−20 crowded left, 0 VU at about
/// 70 %), orange past 0. The needle rides the repeats.
fn vu(b: &mut Builder<'_>, parent: NodeId, (px, py): (f64, f64), r: f64, ink: &str) {
    let card = Card::Vu;
    let sweep = 100.0;
    let (floor, ceil) = card.range();
    let side = 2.0 * (r + 20.0);
    let c = side / 2.0;
    let root = b.add(parent, draw::frame("VU", px - c, py - c, side, side));
    let at = |db: f64| (card.fraction(db) - 0.5) * sweep;
    let arc: Vec<(f64, f64)> = (0..=60).map(|i| draw::polar(c, c, r, at(floor + (ceil - floor) * f64::from(i) / 60.0))).collect();
    b.add(root, ui::strokes("arc", &[arc], 1.1, draw::solid(ink)));
    let hot: Vec<(f64, f64)> = (0..=16).map(|i| draw::polar(c, c, r + 2.5, at(3.0 * f64::from(i) / 16.0))).collect();
    b.add(root, ui::strokes("over", &[hot], 5.0, draw::solid(ORANGE)));
    let numeral = Type::new(7.5, ink, 400.0).family(draw::family::SAIRA);
    for (db, s) in [(-20.0, "-20"), (-10.0, "10"), (-5.0, "5"), (-3.0, "3"), (0.0, "0"), (1.0, "1"), (3.0, "3+")] {
        let a = at(db);
        let col = if db >= 0.0 { ORANGE } else { ink };
        b.add(root, ui::strokes("tick", &[vec![draw::polar(c, c, r, a), draw::polar(c, c, r + 5.5, a)]], 1.2, draw::solid(col)));
        let (x, y) = draw::polar(c, c, r + 12.0, a);
        parts::centred(b, root, s, (x, y), Type { color: col, ..numeral });
    }
    parts::centred(b, root, "VU", (c, c - r * 0.48), Type::new(14.0, ink, 400.0).family(draw::family::SAIRA).width(85.0));
    // The needle, drawn upright, turned by the repeats from the rest.
    let needle = b.add(root, draw::frame("needle", 0.0, 0.0, side, side));
    b.add(needle, draw::with_effects(draw::rect("blade", c - 0.8, c - r - 2.0, 1.6, r + 2.0, draw::solid("#1a1a1c")), vec![draw::drop_shadow("#00000040", 1.5, 2.0, 1.5)]));
    let (rest, top) = (at(floor), at(1.0));
    motion::add(b, needle, Motion::new(Target::Rotate).input(repeats()).range(rest, top).pivot(0.5, 0.5));
    let base = draw::rounded(draw::rect("pivot cover", c - 9.0, c - 10.0, 18.0, 26.0, draw::linear(90.0, &[(0.0, "#2a2a2c"), (0.5, "#3c3c40"), (1.0, "#141416")])), 7.0);
    b.add(root, draw::with_effects(base, vec![draw::drop_shadow("#00000060", 0.0, 2.0, 3.0)]));
}

fn glow_key(b: &mut Builder<'_>, parent: NodeId, addr: &str, d: f64, at: (f64, f64), color: &str) -> NodeId {
    let opts = vec![format!("color={color}")];
    let p = Place { options: Some(opts), ..Place::new(addr).size(d).bind(ui::bind(addr)).touch(TOUCH_MIN) };
    parts::part(b, parent, "button.glow_key", p, &["bezel"], at)
}

#[must_use]
pub fn face(dark: bool) -> Face {
    face_at(dark, kit::Tier::Desktop)
}

/// The face at `tier`: one design — the pale grey (or graphite) unit in
/// its bays, the VU, the head lamps, the white dial, the knurled knobs —
/// laid out for the room. The smaller tiers keep the VU and the lamps as
/// the picture, the white dial as the Time knob and Repeats, Tape-Mod and
/// Level, at a size a finger can use. The nameplate is at the top right,
/// the power key at the Time faces' bypass spot.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn face_at(dark: bool, tier: kit::Tier) -> Face {
    // Light: the original's pale grey. Dark: the same unit in graphite,
    // lettered in white.
    let (ink, grey, unit_fill) = if dark {
        ("#e8e9eb", [(0.0, "#3a3c41"), (1.0, "#2b2d31")], [(0.0, "#1f2023"), (1.0, "#17181a")])
    } else {
        (INK, [(0.0, "#e6e7e8"), (1.0, "#cfd0d2")], [(0.0, "#bfc0c2"), (1.0, "#a9aaac")])
    };
    let small_room = tier.small();
    let w = tier.lane_w();
    let scene = kit::tier_lane("Tape", tier, |b, root| {
        let h = kit::H;
        let unit = bay(b, root, "Unit", (0.0, 0.0, w, h), &unit_fill);
        let (y, bh) = (6.0, h - 12.0);
        let black = KnobLook { preset: "knurled", opts: &["color=#1c1c1e", "pointer=#3b82f6", "index=dot", "cap_r=0.9", "height=0.6", "dome=0.08", "edge=0.1", "ribs=48", "rib_depth=0.03", "knurl_band=0.7", "index_w=0.09", "index_in=0.66", "index_out=0.66", "coat=0.5", "rough=0.25", "ticks=none"] };
        let orange = KnobLook { preset: "knurled", opts: &["color=#3b82f6", "pointer=#dbe8ff", "index=dot", "cap_r=0.9", "height=0.6", "dome=0.05", "edge=0.08", "ribs=24", "rib_depth=0.06", "knurl_band=1", "index_w=0.08", "index_in=0.66", "index_out=0.66", "coat=0.4", "rough=0.3", "ticks=none"] };
        let dial = KnobLook { preset: "white_knurl", opts: &["color=#eeede9", "index=none", "diamond=0", "ribs=90", "knurl_band=0.5", "rib_depth=0.02", "cap_r=0.97", "height=0.5", "dome=0.03", "edge=0.1", "ticks=none", "fallback=knurled"] };
        let lit = ui::alpha(ORANGE, 0.45);
        // The head lamps, each flashing on its head's repeat.
        let lamps = |b: &mut Builder<'_>, parent: NodeId, xs: [f64; 3], ly: f64| {
            for (n, x) in (1..=3).zip(xs) {
                b.add(parent, draw::with_effects(draw::circle("lamp socket", x, ly, 4.8, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#d8d9db"), (1.0, "#8e9094")])), vec![draw::inner_shadow("#00000060", 0.0, 1.0, 1.5)]));
                let lamp = b.add(parent, draw::emissive(draw::circle("head lamp", x, ly, 3.4, draw::solid(LAMP)), draw::hex(LAMP), 1.2, kit::BLOOM));
                let flash = format!("pow(1 - fract(time * 1000 / ({n} * max(param({DLY}/time), 20))), 4) * pow(param({DLY}/feedback), {}) * param({DLY}/on)", n - 1);
                motion::add(b, lamp, Motion::new(Target::Opacity).input(flash).range(0.05, 1.0));
            }
        };
        let picture;
        if small_room {
            // The bays cut round the controls: the dial's centred on the
            // Time knob, the VU's up to it, the knobs' clear of their tick
            // rings.
            let (tx, ty, td) = kit::time_spot(tier);
            // The dial's bay: the TIME | BEAT pill under the knob its width.
            let pane = tier == kit::Tier::Ipad;
            let half = if pane { 42.0 } else { 78.0 };
            let (b2x, b3x) = (tx - half, tx + half + 2.0);
            // The picture: ECHO, a slim VU, the heads under it — the
            // controls come first on a phone.
            let b1w = b2x - 2.0 - 6.0;
            let bay1 = bay(b, unit, "Bay 1", (6.0, y, b1w, bh), &grey);
            // On a pane: the letters smaller, the VU and its lamps closer.
            let (letter, lx, top, step, vr, lamp_dx) = if pane { (16.0, 16.0, 30.0, 26.0, 28.0, 20.0) } else { (18.0, 18.0, 28.0, 28.0, 34.0, 24.0) };
            for (i, ch) in "ECHO".chars().enumerate() {
                parts::centred(b, bay1, &ch.to_string(), (lx, top + step * i as f64), Type::new(letter, ink, 300.0).family(draw::family::SAIRA).width(88.0));
            }
            let vx = lx + (b1w - lx) / 2.0 + 4.0;
            vu(b, bay1, (vx, 72.0), vr, ink);
            lamps(b, bay1, [vx - lamp_dx, vx, vx + lamp_dx], 134.0);
            picture = bay1;
            // The white dial: Time, in its own bay.
            let bay2 = bay(b, unit, "Bay 2", (b2x, y, b3x - 2.0 - b2x, bh), &grey);
            kit::time_knob(b, bay2, DLY, (tx - b2x, ty - y, td), if tier == kit::Tier::Ipad { 10.0 } else { 11.0 }, (ink, "#6b6e74", &lit), |b, p, addr| {
                look::place_knob(b, p, &dial, addr, "", td, (tx - b2x, ty - y), ink);
            });
            // The knobs that matter on stage, in the phone's knob row.
            let bay3 = bay(b, unit, "Bay 3", (b3x, y, w - 6.0 - b3x, bh), &grey);
            let (xs, ky, kd) = kit::knob_row(tier);
            let ls = kit::legend_size(tier);
            for (i, (id, legend, warm)) in [("feedback", "Repeats", false), ("mod_depth", "Tape-Mod", false), ("level", "Level", true)].into_iter().enumerate() {
                let (x, cy) = (xs[i] - b3x, ky - y);
                let k = look::knob(b, bay3, if warm { &orange } else { &black }, &a(id), kd, (x, cy), 11, ink, false);
                ui::add_value(b, k, ls - 0.5, "#6b6e74");
                parts::centred(b, bay3, legend, (x, cy + kd / 2.0 + if pane { 22.0 } else { 30.0 }), Type::new(ls, ink, 600.0).track(0.06).upper());
            }
        } else {
            // Bay 1: the letters, the VU, the display.
            let bay1 = bay(b, unit, "Bay 1", (6.0, y, 330.0, bh), &grey);
            for (i, ch) in "ECHO".chars().enumerate() {
                parts::centred(b, bay1, &ch.to_string(), (30.0, 24.0 + 33.0 * i as f64), Type::new(30.0, ink, 300.0).family(draw::family::SAIRA).width(88.0));
            }
            vu(b, bay1, (194.0, 84.0), 54.0, ink);
            let disp = ui::display(b, bay1, "Display", 76.0, 114.0, 236.0, 26.0, "#9aa0a8", "glass");
            ui::lit_text(b, disp, "TIME", 8.0, 13.0, 30.0, 7.5, "#c9ccd2", TextAlign::Left, 500.0);
            ui::lit_readout(b, disp, "Delay time", ui::bind(&a("time_text")), 38.0, 13.0, 100.0, 8.0, "#e8eaee", TextAlign::Left);
            ui::lit_text(b, disp, "FEEDBACK", 140.0, 13.0, 50.0, 7.5, "#c9ccd2", TextAlign::Left, 500.0);
            ui::lit_readout(b, disp, "Feedback", ui::bind(&a("feedback")), 186.0, 13.0, 44.0, 8.0, ORANGE, TextAlign::Right);
            ui::glass(b, disp);
            screws(b, bay1, &[(9.0, 9.0), (9.0, bh - 9.0)]);
            picture = bay1;

            // Bay 2: the heads and the big white dial — the Time knob.
            let bw2 = 170.0;
            let bay2 = bay(b, unit, "Bay 2", (338.0, y, bw2, bh), &grey);
            let cx = bw2 / 2.0;
            parts::centred(b, bay2, "Active Heads", (cx, 13.0), Type::new(6.5, ink, 500.0).track(0.08).upper());
            parts::art(b, bay2, &[vec![(cx - 44.0, 22.0), (cx - 44.0, 13.0), (cx - 36.0, 13.0)], vec![(cx + 44.0, 22.0), (cx + 44.0, 13.0), (cx + 36.0, 13.0)]], 0.9, ink);
            lamps(b, bay2, [cx - 36.0, cx, cx + 36.0], 27.0);
            let dy = 76.0;
            let arc: Vec<(f64, f64)> = (0..=30).map(|i| draw::polar(cx, dy, 42.0, -135.0 + 270.0 * f64::from(i) / 30.0)).collect();
            parts::art(b, bay2, &[arc], 1.0, ink);
            for i in 0..5 {
                let (x, yy) = draw::polar(cx, dy, 47.0, -135.0 + 67.5 * f64::from(i));
                b.add(bay2, draw::circle("detent", x, yy, 2.2, draw::solid(ink)));
            }
            kit::time_knob(b, bay2, DLY, (cx, dy, 68.0), 8.5, (ink, "#6b6e74", &lit), |b, p, addr| {
                look::place_knob(b, p, &dial, addr, "", 68.0, (cx, dy), ink);
            });
            // Its pointer: an index at the rim, turning with the time.
            let turn = b.add(bay2, draw::frame("time index", cx - 34.0, dy - 34.0, 68.0, 68.0));
            b.add(turn, draw::circle("index", 34.0, 7.0, 3.0, draw::solid(ORANGE)));
            motion::add(b, turn, Motion::new(Target::Rotate).input(format!("param({DLY}/time_knob)")).range(-135.0, 135.0).pivot(0.5, 0.5));

            // Bay 3: the knobs, under the nameplate.
            let bw3 = 318.0;
            let bay3 = bay(b, unit, "Bay 3", (510.0, y, bw3, bh), &grey);
            let knobs = [
                ("feedback", "Feedback", false, 0, 0),
                ("mod_depth", "Tape-Mod", false, 1, 0),
                ("level", "Level", false, 2, 0),
                ("mod_rate", "Wow Rate", false, 0, 1),
                ("high_pass", "Bass", true, 1, 1),
                ("high_cut", "Treble", true, 2, 1),
            ];
            for (id, legend, warm, col, row) in knobs {
                let (x, cy) = (58.0 + 101.0 * f64::from(col), 58.0 + 56.0 * f64::from(row));
                look::knob(b, bay3, if warm { &orange } else { &black }, &a(id), 30.0, (x, cy), 11, ink, false);
                parts::centred(b, bay3, legend, (x, cy + 26.0), Type::new(7.0, ink, 500.0).track(0.08).upper());
            }

            // Bay 4: the mark and the emblem (the power key is at the
            // bypass spot).
            let bay4 = bay(b, unit, "Bay 4", (830.0, y, w - 836.0, bh), &grey);
            look::brush_mark(b, bay4, (40.0, 70.0), 14.0, "#8a8c90");
            look::emblem(b, bay4, (40.0, 116.0), 11.0, ink);
            screws(b, bay4, &[(w - 836.0 - 9.0, bh - 9.0)]);
        }
        // The nameplate, top right: the preset, then the machine.
        let (np_y, np_h, title_w, preset_w, size) = if small_room { kit::small_nameplate(tier) } else { (8.0, 26.0, 66.0, 196.0, 10.0) };
        kit::nameplate_top_right(b, unit, DLY, tier, (np_y, np_h), title_w, preset_w, size, (if dark { kit::BLUE_LIGHT } else { kit::BLUE_DEEP }, "#6b6e74"), |b, t| {
            parts::centred(b, t, "Tape", (title_w / 2.0 - 6.0, np_h / 2.0), Type::new(if tier == kit::Tier::Ipad { 18.0 } else if small_room { 22.0 } else { 18.0 }, ink, 400.0).family(draw::family::SAIRA));
        });
        // The power key at the bypass spot.
        let (bx, by, bd) = kit::bypass_spot(tier);
        glow_key(b, unit, &a("on"), bd, (bx, by), ORANGE);
        kit::flip_arrow(b, unit, DLY, tier, false, (ink, "#00000022"));
        // Responsive: the picture takes the width.
        kit::responsive(b, unit, &[picture]);
    });
    let mut f = kit::delay_face(kit::tier_file("60-dly-tape", tier, dark), scene);
    f.params.extend(kit::nameplate_params(DLY));
    f.params.extend(kit::time_params(DLY));
    f.params.extend(kit::flip_params(DLY));
    f
}

/// Its back at `tier`: the grey unit in one bay, no VU or dial — the
/// advanced controls across it in the unit's knurled knobs, the
/// nameplate, the power key and the arrow where they are on the front.
#[must_use]
pub fn back_at(dark: bool, tier: kit::Tier) -> Face {
    let (ink, grey, unit_fill) = if dark {
        ("#e8e9eb", [(0.0, "#3a3c41"), (1.0, "#2b2d31")], [(0.0, "#1f2023"), (1.0, "#17181a")])
    } else {
        (INK, [(0.0, "#e6e7e8"), (1.0, "#cfd0d2")], [(0.0, "#bfc0c2"), (1.0, "#a9aaac")])
    };
    let small_room = tier.small();
    let w = tier.lane_w();
    let scene = kit::tier_lane("Tape", tier, |b, root| {
        let h = kit::H;
        let unit = bay(b, root, "Unit", (0.0, 0.0, w, h), &unit_fill);
        let face = bay(b, unit, "Back", (6.0, 6.0, w - 12.0, h - 12.0), &grey);
        let black = KnobLook { preset: "knurled", opts: &["color=#1c1c1e", "pointer=#3b82f6", "index=dot", "cap_r=0.9", "height=0.6", "dome=0.08", "edge=0.1", "ribs=48", "rib_depth=0.03", "knurl_band=0.7", "index_w=0.09", "index_in=0.66", "index_out=0.66", "coat=0.5", "rough=0.25", "ticks=none"] };
        let knobs = [("mod_rate", "Wow Rate"), ("high_pass", "Bass"), ("high_cut", "Treble"), ("duck_sens", "Duck"), ("duck_release", "Duck Rel"), ("pan", "Pan"), ("repeat_dyn", "Dynamics"), ("dry", "Dry")];
        let legend = if small_room { kit::legend_size(tier) } else { 7.0 };
        kit::back_knobs(b, unit, tier, &knobs, ink, |b, p, id, name, d, (cx, cy)| {
            let k = look::knob(b, p, &black, &a(id), d, (cx, cy), 11, ink, false);
            ui::add_value(b, k, if small_room { legend - 0.5 } else { 7.5 }, "#6b6e74");
            parts::centred(b, p, name, (cx, cy + d / 2.0 + if tier == kit::Tier::Ipad { 24.0 } else if small_room { 30.0 } else { 24.0 }), Type::new(legend, ink, 600.0).track(0.06).upper());
        });
        let _ = face;
        let (np_y, np_h, title_w, preset_w, size) = if small_room { kit::small_nameplate(tier) } else { (8.0, 26.0, 66.0, 196.0, 10.0) };
        kit::head(b, unit, DLY, true, |b, head| {
            kit::nameplate_top_right(b, head, DLY, tier, (np_y, np_h), title_w, preset_w, size, (if dark { kit::BLUE_LIGHT } else { kit::BLUE_DEEP }, "#6b6e74"), |b, t| {
                parts::centred(b, t, "Tape", (title_w / 2.0 - 6.0, np_h / 2.0), Type::new(if tier == kit::Tier::Ipad { 18.0 } else if small_room { 22.0 } else { 18.0 }, ink, 400.0).family(draw::family::SAIRA));
            });
            let (bx, by, bd) = kit::bypass_spot(tier);
            glow_key(b, head, &a("on"), bd, (bx, by), ORANGE);
        });
        kit::flip_arrow(b, unit, DLY, tier, true, (ink, "#00000022"));
    });
    let mut f = kit::delay_face(kit::tier_file("60-dly-tape-back", tier, dark), scene);
    f.params.extend(kit::nameplate_params(DLY));
    f.params.extend(kit::flip_params(DLY));
    f
}

/// Its tall face: the unit's picture bay — ECHO, the VU, the display and
/// the heads' lamps — the whole top row, every control below it in the
/// unit's knurled knobs round the big white Time dial.
#[must_use]
pub fn tall(dark: bool) -> Face {
    let (ink, grey, unit_fill) = if dark {
        ("#e8e9eb", [(0.0, "#3a3c41"), (1.0, "#2b2d31")], [(0.0, "#1f2023"), (1.0, "#17181a")])
    } else {
        (INK, [(0.0, "#e6e7e8"), (1.0, "#cfd0d2")], [(0.0, "#bfc0c2"), (1.0, "#a9aaac")])
    };
    let scene = kit::tall_lane("Tape", |b, root| {
        let (w, h) = (kit::TALL_W, kit::TALL_H);
        let unit = bay(b, root, "Unit", (0.0, 0.0, w, h), &unit_fill);
        let top = bay(b, unit, "Picture", (6.0, 6.0, w - 12.0, kit::H - 8.0), &grey);
        bay(b, unit, "Controls", (6.0, kit::H + 4.0, w - 12.0, kit::H - 10.0), &grey);
        for (i, ch) in "ECHO".chars().enumerate() {
            parts::centred(b, top, &ch.to_string(), (34.0, 26.0 + 34.0 * i as f64), Type::new(30.0, ink, 300.0).family(draw::family::SAIRA).width(88.0));
        }
        vu(b, top, (176.0, 92.0), 58.0, ink);
        let disp = ui::display(b, top, "Display", 286.0, 56.0, 250.0, 30.0, "#9aa0a8", "glass");
        ui::lit_text(b, disp, "TIME", 8.0, 15.0, 30.0, 7.5, "#c9ccd2", TextAlign::Left, 500.0);
        ui::lit_readout(b, disp, "Delay time", ui::bind(&a("time_text")), 38.0, 15.0, 100.0, 8.0, "#e8eaee", TextAlign::Left);
        ui::lit_text(b, disp, "FEEDBACK", 146.0, 15.0, 50.0, 7.5, "#c9ccd2", TextAlign::Left, 500.0);
        ui::lit_readout(b, disp, "Feedback", ui::bind(&a("feedback")), 192.0, 15.0, 50.0, 8.0, ORANGE, TextAlign::Right);
        ui::glass(b, disp);
        // The heads, their lamps flashing on their repeats.
        parts::centred(b, top, "Active Heads", (410.0, 108.0), Type::new(6.5, ink, 500.0).track(0.08).upper());
        for (n, x) in (1..=3).zip([370.0, 410.0, 450.0]) {
            b.add(top, draw::with_effects(draw::circle("lamp socket", x, 126.0, 4.8, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#d8d9db"), (1.0, "#8e9094")])), vec![draw::inner_shadow("#00000060", 0.0, 1.0, 1.5)]));
            let lamp = b.add(top, draw::emissive(draw::circle("head lamp", x, 126.0, 3.4, draw::solid(LAMP)), draw::hex(LAMP), 1.2, kit::BLOOM));
            let flash = format!("pow(1 - fract(time * 1000 / ({n} * max(param({DLY}/time), 20))), 4) * pow(param({DLY}/feedback), {}) * param({DLY}/on)", n - 1);
            motion::add(b, lamp, Motion::new(Target::Opacity).input(flash).range(0.05, 1.0));
        }
        look::brush_mark(b, top, (w - 110.0, 110.0), 14.0, "#8a8c90");
        look::emblem(b, top, (w - 60.0, 110.0), 11.0, ink);
        screws(b, top, &[(9.0, 9.0), (9.0, kit::H - 17.0)]);
        let black = KnobLook { preset: "knurled", opts: &["color=#1c1c1e", "pointer=#3b82f6", "index=dot", "cap_r=0.9", "height=0.6", "dome=0.08", "edge=0.1", "ribs=48", "rib_depth=0.03", "knurl_band=0.7", "index_w=0.09", "index_in=0.66", "index_out=0.66", "coat=0.5", "rough=0.25", "ticks=none"] };
        let dial = KnobLook { preset: "white_knurl", opts: &["color=#eeede9", "index=none", "diamond=0", "ribs=90", "knurl_band=0.5", "rib_depth=0.02", "cap_r=0.97", "height=0.5", "dome=0.03", "edge=0.1", "ticks=none", "fallback=knurled"] };
        let t = kit::Tall { ns: DLY, main: &[("feedback", "Repeats"), ("mod_depth", "Tape-Mod"), ("level", "Level"), ("mod_rate", "Wow Rate")], inks: (ink, "#6b6e74", &ui::alpha(ORANGE, 0.45), if dark { kit::BLUE_LIGHT } else { kit::BLUE_DEEP }), scrim: if dark { "#2b2d31e6" } else { "#dcdddee6" }, bypass: ("blue", "On"), title_w: 74.0 };
        kit::tall_chrome(b, unit, &t, |b, p, addr, legend, d, (cx, cy)| {
            let k = look::knob(b, p, &black, addr, d, (cx, cy), 11, ink, false);
            ui::add_value(b, k, if d > 34.0 { 8.0 } else { 7.0 }, "#6b6e74");
            parts::centred(b, p, legend, (cx, cy + d / 2.0 + 20.0), Type::new(if d > 34.0 { 8.0 } else { 7.0 }, ink, 600.0).track(0.06).upper());
        }, |b, p, addr, (cx, cy, d)| {
            look::place_knob(b, p, &dial, addr, "", d, (cx, cy), ink);
        }, |b, tn| {
            parts::centred(b, tn, "Tape", (74.0 / 2.0 - 6.0, 15.0), Type::new(22.0, ink, 400.0).family(draw::family::SAIRA));
        });
    });
    let mut f = kit::delay_face(kit::tall_file("60-dly-tape", dark), scene);
    f.params.extend(kit::tall_params(DLY));
    f
}

/// The routing grid's block: the VU kicking on the repeats under the head
/// lamps flashing on theirs, on the graphite unit.
#[must_use]
pub fn block() -> Face {
    use crate::rig_cells::S;
    let ink = "#e8e9eb";
    let scene = crate::rig_cells::block("Tape", &[(0.0, "#3a3c41"), (1.0, "#1f2023")], |b, v, _| {
        vu(b, v, (S / 2.0, S * 0.6), S * 0.29, ink);
        for (n, x) in (1..=3).zip([S / 2.0 - 26.0, S / 2.0, S / 2.0 + 26.0]) {
            b.add(v, draw::circle("lamp socket", x, 26.0, 6.0, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#d8d9db"), (1.0, "#8e9094")])));
            let lamp = b.add(v, draw::emissive(draw::circle("head lamp", x, 26.0, 4.4, draw::solid(LAMP)), draw::hex(LAMP), 1.0, 9.0));
            let flash = format!("pow(1 - fract(time * 1000 / ({n} * max(param({DLY}/time), 20))), 4) * pow(param({DLY}/feedback), {}) * param({DLY}/on)", n - 1);
            motion::add(b, lamp, Motion::new(Target::Opacity).input(flash).range(0.05, 1.0));
        }
    });
    kit::delay_face("60-dly-tape-block", scene)
}
