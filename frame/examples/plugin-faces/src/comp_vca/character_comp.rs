//! The digitally-controlled analogue character compressor — laid out from
//! photographs of the Empirical Labs Distressor (EL8): a black 1U front with
//! white hairline section frames.
//!
//! Left: the name, the IN key with its lamp, the 16-lamp gain-reduction
//! ladder (numbers printed *above* the lamps, lit right to left, green →
//! yellow → orange → red as reduction deepens), and the ratio row, whose
//! coloured lamps are the switch (click one to pick it; NUKE at the end).
//! Then the unlit REDLINE / 1% THD pair, and the DETECTOR and AUDIO lamp
//! columns over their grey keys. Right: four big cream dials — INPUT,
//! ATTACK, RELEASE, OUTPUT — whose 0–10 numerals are engraved on the
//! skirt and turn past a fixed index on the header line, and the POWER
//! rocker.

use document_proto::{Node, NodeId, TextAlign};
use surface_kit::{Place, TOUCH_MIN, draw};
use surface_proto::{Binding, ButtonMode, Control, ControlKind, SlotRole, ValueFormat};
use surface_widgets::Builder;

use super::logo;
use super::ui::{self, bind};
use crate::face::{Face, canvas};

pub mod lane;

const W: f64 = 1500.0;
const H: f64 = 136.0;
const INK: &str = "#eceef0";
const LINE: &str = "#e8eaecd9";
/// The panel's black, for knocking lines out behind print.
const PAINT: &str = "#1b1c1e";

/// Gain-reduction lamps in the order they light: 1 dB first (at the right).
const LADDER: &[&str] = &["1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "12", "14", "17", "20", "23", "26"];
/// The ladder's lamp colours in the same order (photographed).
const LADDER_COLORS: &str = "colors=#46e05a,#46e05a,#46e05a,#46e05a,#46e05a,#46e05a,#f6d43c,#f6d43c,#f6d43c,#f6d43c,#ff8a24,#ff8a24,#ff8a24,#ff3b2f,#ff3b2f,#ff3b2f";
const RATIOS: &[&str] = &["1:1", "2:1", "3:1", "4:1", "6:1", "10:1", "20:1", "Nuke"];
const RATIO_COLORS: &[&str] = &["#ff3b2f", "#46e05a", "#46e05a", "#46e05a", "#f6d43c", "#ff8a24", "#ff3b2f", "#dfe6ee"];
const DIAL_ROW: f64 = 73.0;
const DIAL: f64 = 110.0;
const HEADER: f64 = 12.0;

/// A position across the front, as a fraction of its width (measured off
/// the photograph).
fn fx(f: f64) -> f64 {
    W * f
}

/// A patch of panel paint, knocking a frame line out behind print.
fn knock(b: &mut Builder<'_>, panel: NodeId, cx: f64, cy: f64, w: f64, h: f64) {
    b.add(panel, draw::rect("knockout", cx - w / 2.0, cy - h / 2.0, w, h, draw::solid(PAINT)));
}

/// A header set on the frame line.
fn header(b: &mut Builder<'_>, panel: NodeId, s: &str, cx: f64, cy: f64, size: f64) {
    knock(b, panel, cx, cy, ui::text_w(s, size) + size * 1.2, size * 1.5);
    ui::silk(b, panel, s, cx, cy, size, INK, 800.0);
}

/// A lamp cell: lamp centre, legend centre, cell box (x, y, w, h).
type Cell = ((f64, f64), (f64, f64), (f64, f64, f64, f64));

/// Radio lamps over a choice param: each option a lamp in its own colour
/// with its legend, a `Select` button whose `Led` is the lit lens.
/// `cells` = (lamp centre, legend centre, cell box).
fn lamp_select(b: &mut Builder<'_>, panel: NodeId, addr: &str, opts: &[&str], colors: &[&str], cells: &[Cell], size: f64) {
    let n = opts.len() as u32;
    for (i, (label, ((lx, ly), (tx, ty), (x, y, w, h)))) in opts.iter().zip(cells).enumerate() {
        let cell = b.add(panel, draw::frame(&format!("{addr} {label}"), *x, *y, *w, *h));
        let d = 8.0;
        b.add(cell, draw::circle("bezel", lx - x, ly - y, d / 2.0 + 1.4, draw::linear(180.0, &[(0.0, "#2a2b2e"), (1.0, "#060607")])));
        b.add(cell, ui::dark_lens(lx - x, ly - y, d / 2.0, colors[i]));
        let lit = b.add(cell, ui::lens(lx - x, ly - y, d / 2.0, colors[i]));
        let tw = ui::text_w(label, size) + size;
        let t = b.add(cell, ui::text("legend", label, tx - x - tw / 2.0, ty - y, tw, size, INK, TextAlign::Center, 800.0, false));
        let mut c = Control::new(ControlKind::Button { mode: ButtonMode::Select { index: i as u32, count: n } }).slot(SlotRole::Led, lit.0).slot(SlotRole::Label, t.0);
        c.binding = Some(bind(addr));
        b.controls.insert(cell, c);
    }
}

/// A light-grey key with the pill-shaped dimple the unit's keys have —
/// decoration: the lamps above it are the switch.
fn grey_key(b: &mut Builder<'_>, panel: NodeId, cx: f64, cy: f64, w: f64, h: f64, dark: bool) -> Node {
    let (c0, c1, c2) = if dark { ("#56585c", "#3a3c40", "#26282b") } else { ("#f0f0ee", "#cfd0cf", "#a9aaa9") };
    let cap = draw::rect("key", cx - w / 2.0, cy - h / 2.0, w, h, draw::linear(180.0, &[(0.0, c0), (0.5, c1), (1.0, c2)]));
    let cap = draw::with_effects(
        draw::stroked(draw::rounded(cap, 2.0), "#000000c0", 1.0),
        vec![draw::drop_shadow("#000000c0", 0.0, 2.0, 3.0), draw::inner_shadow("#ffffff70", 0.0, 1.0, 0.0)],
    );
    b.add(panel, cap);
    let dimple = draw::rect(
        "dimple",
        cx - w * 0.36,
        cy - h * 0.28,
        w * 0.72,
        h * 0.56,
        draw::linear(180.0, &[(0.0, if dark { "#2c2e31" } else { "#b8b9b8" }), (1.0, if dark { "#4a4c50" } else { "#e6e6e4" })]),
    );
    draw::rounded(dimple, h * 0.28)
}

/// A big cream dial under its header, with a fixed index on the line.
fn dial(b: &mut Builder<'_>, panel: NodeId, addr: &str, legend: &str, format: Option<ValueFormat>, cx: f64) {
    let mut p = Place::new(legend).size(DIAL).variant("cream").touch(TOUCH_MIN).bind(bind(addr));
    p.format = format;
    let k = ui::put(b, panel, "knob.numbered_cream", &p, cx, DIAL_ROW);
    ui::strip_legend(b, k);
    // The kit's index triangle stands above a 2U panel's dial; on 1U the
    // index is a short white tick down from the header line.
    for id in b.scene.walk(k) {
        if let Some(n) = b.scene.node_mut(id)
            && n.name == "panel index"
        {
            n.visible = false;
        }
    }
    ui::rule(b, panel, cx - 1.0, HEADER, 2.0, DIAL_ROW - DIAL / 2.0 - HEADER - 1.0, INK);
}

pub fn face() -> Face {
    let scene = unit("charcomp", "Character", "charcomp/input", "charcomp/output", "charcomp/in");
    let params = vec![
        ui::param("charcomp/input", "Input", surface_proto::Range::linear(0.0, 10.0, surface_proto::Unit::None), 5.0).featured(),
        ui::ms("charcomp/attack", "Attack", 0.05, 50.0, 5.0).featured(),
        ui::ms("charcomp/release", "Release", 50.0, 3500.0, 400.0).featured(),
        ui::db("charcomp/output", "Output", -12.0, 24.0, 0.0).featured(),
        ui::choice("charcomp/ratio", "Ratio", RATIOS, 3).featured(),
        ui::choice("charcomp/detector", "Detector", &["Off", "HP", "HP+"], 0),
        ui::choice("charcomp/audio", "Audio", &["Clean", "Dist 2", "Dist 3"], 0),
        ui::toggle("charcomp/in", "In", true),
    ];
    let streams = vec![ui::gr_stream("charcomp/gr", "Gain Reduction")];
    Face { file: "10-character-comp", plugin: "comp", scene, params, streams }
}

/// The unit's size (a 1U front).
pub const SIZE: (f64, f64) = (W, H);

/// The ratio lamps' order, for a host mapping them to its own ratios.
pub const RATIO_NAMES: &[&str] = RATIOS;

/// The unit's front, its controls under `ns/` (`attack`, `release`,
/// `ratio`, `detector`, `audio`, `in`, the `gr` stream), its logotype
/// `name`, its INPUT and OUTPUT dials bound to `input` and `output` (a
/// host may drive a threshold and a level with them) and its IN key to
/// `on`.
pub fn unit(ns: &str, name: &str, input: &str, output: &str, on: &str) -> document_proto::Scene {
    let a = |id: &str| format!("{ns}/{id}");
    canvas("Character Compressor", |b, root| {
        ui::fit_root(b, root, W, H);
        let panel = ui::panel(b, root, "panel.painted", "black", W, H, "ears");
        if let Some(n) = b.scene.node_mut(panel) {
            n.fills = vec![draw::linear(178.0, &[(0.0, "#232427"), (0.5, "#1b1c1e"), (1.0, "#141517")])];
        }

        // ── The frames ─────────────────────────────────────────────────
        ui::section(b, panel, fx(0.055), HEADER, fx(0.363) - fx(0.055), 112.0, LINE, 1.6);
        ui::section(b, panel, fx(0.363), HEADER, fx(0.937) - fx(0.363), 112.0, LINE, 1.6);
        // The ladder stands in the left frame's top line.
        knock(b, panel, (fx(0.105) + fx(0.360)) / 2.0, HEADER, fx(0.360) - fx(0.105), 8.0);

        // ── The name, set as the unit's logotype (big, heavy, slanted
        // white), and the IN key ─────────────────────────────────────
        let script = logo::Style { size: 34.0, weight: 800.0, italic: true, tracking: -0.02 };
        logo::mark(b, panel, name, fx(0.057), 62.0, 200.0, script, "#f2f2f0", TextAlign::Left);
        let dimple = grey_key(b, panel, fx(0.2), 67.0, 44.0, 20.0, true);
        b.add(panel, dimple);
        lamp_toggle(b, panel, on, fx(0.2) - 24.0, fx(0.245), 67.0);

        // ── Gain reduction: 16 lamps, numbers above ────────────────────
        let ladder = Place::new("Gain Reduction").size(23.7 * LADDER.len() as f64).scale(LADDER).options(&["gr", LADDER_COLORS]).reversed().bind(Binding::stream(a("gr")));
        let lad = ui::put(b, panel, "meter.led_ladder", &ladder, fx(0.2325), 30.0);
        for id in b.scene.walk(lad) {
            if let Some(n) = b.scene.node_mut(id)
                && n.name == "threshold"
            {
                n.visible = false;
            }
        }
        let (x0, pitch) = (fx(0.2325) - 23.7 * 8.0 + 23.7 / 2.0, 23.7);
        for (i, l) in LADDER.iter().rev().enumerate() {
            ui::silk(b, panel, l, x0 + i as f64 * pitch, HEADER, 8.5, INK, 800.0);
        }
        ui::silk(b, panel, "Gain Reduction", fx(0.231), 44.0, 10.0, INK, 800.0);

        // ── Ratio: the lamps are the switch ────────────────────────────
        let cells: Vec<_> = (0..RATIOS.len())
            .map(|i| {
                let x = fx(0.1385) + i as f64 * 35.5;
                ((x, 110.0), (x, 97.0), (x - 17.0, 89.0, 34.0, 28.0))
            })
            .collect();
        lamp_select(b, panel, &a("ratio"), RATIOS, RATIO_COLORS, &cells, 8.0);
        header(b, panel, "Ratio", fx(0.216), 124.0, 11.0);
        // The unlit REDLINE and 1% THD lamps, by their sine.
        let sx = fx(0.297);
        let sine: String = (0..=24)
            .map(|i| {
                let t = f64::from(i) / 24.0;
                format!("{} {:.2} {:.2} ", if i == 0 { "M" } else { "L" }, t * 30.0, 8.0 - 7.0 * (t * std::f64::consts::TAU).sin())
            })
            .collect();
        let mut s = draw::stroked(draw::path("sine", sx - 15.0, 48.0, 30.0, 16.0, sine.trim_end().to_string(), draw::solid("#00000000")), INK, 1.6);
        s.fills.clear();
        b.add(panel, s);
        ui::rule(b, panel, sx - 17.0, 56.0, 34.0, 1.0, INK);
        ui::lamp(b, panel, fx(0.320), 46.0, 7.0, "#ff3b2f", false);
        ui::silk_left(b, panel, "Redline", fx(0.320) + 7.0, 46.0, 8.5, INK, 800.0);
        ui::lamp(b, panel, fx(0.320), 66.0, 7.0, "#f6d43c", false);
        ui::silk_left(b, panel, "1% THD", fx(0.320) + 7.0, 66.0, 8.5, INK, 800.0);
        let d = grey_key(b, panel, fx(0.3375), 94.0, 36.0, 20.0, false);
        b.add(panel, d);
        ui::silk(b, panel, "Ratio", fx(0.3375), 114.0, 10.5, INK, 800.0);

        // ── Detector and audio: lamp columns over grey keys ────────────
        for (addr, opts, colors, keyx, legend) in [
            (a("detector"), &["Off", "HP", "HP+"], &["#46e05a", "#f6d43c", "#ff3b2f"], fx(0.391), "Detector"),
            (a("audio"), &["Clean", "Dist 2", "Dist 3"], &["#46e05a", "#f6d43c", "#ff3b2f"], fx(0.443), "Audio"),
        ] {
            let lx = keyx + 18.0;
            let cells: Vec<_> = (0..3)
                .map(|i| {
                    let y = 26.0 + i as f64 * 20.0;
                    ((lx, y), (lx - 26.0, y), (lx - 48.0, y - 10.0, 56.0, 20.0))
                })
                .collect();
            lamp_select(b, panel, &addr, opts, colors, &cells, 8.5);
            let d = grey_key(b, panel, keyx, 94.0, 36.0, 20.0, false);
            b.add(panel, d);
            header(b, panel, legend, keyx, 114.0, 10.5);
        }

        // ── The dials ──────────────────────────────────────────────────
        for (addr, legend, format, f, hx) in [
            (input.to_string(), "Input", Some(ValueFormat::decimals(1)), 0.517, 0.4875),
            (a("attack"), "Attack", Some(ValueFormat::decimals(1)), 0.622, 0.592),
            (a("release"), "Release", None, 0.727, 0.696),
            (output.to_string(), "Output", Some(ValueFormat::decimals(1)), 0.833, 0.803),
        ] {
            header(b, panel, legend, fx(hx), HEADER, 11.5);
            dial(b, panel, &addr, legend, format, fx(f));
        }

        // ── Power ──────────────────────────────────────────────────────
        header(b, panel, "Power", fx(0.91), HEADER, 11.5);
        let (px, py) = (fx(0.9075), 70.0);
        let bezel = draw::rect("rocker bezel", px - 17.0, py - 30.0, 34.0, 60.0, draw::linear(180.0, &[(0.0, "#0b0b0c"), (1.0, "#1c1d1f")]));
        b.add(
            panel,
            draw::with_effects(
                draw::stroked(draw::rounded(bezel, 2.0), "#000000", 1.0),
                vec![draw::drop_shadow("#ffffff22", 0.0, 1.0, 0.0), draw::inner_shadow("#000000", 0.0, 2.0, 3.0)],
            ),
        );
        let rocker = draw::rect("rocker", px - 12.0, py - 25.0, 24.0, 50.0, draw::linear(180.0, &[(0.0, "#3a3b3e"), (0.48, "#1d1e20"), (0.52, "#0d0d0e"), (1.0, "#1a1b1d")]));
        b.add(panel, draw::with_effects(draw::rounded(rocker, 1.5), vec![draw::inner_shadow("#ffffff26", 0.0, 1.0, 0.0)]));
    })
}

/// The IN key's lamp and legend: a toggle whose `Led` is the lamp, over the
/// dark key to its left.
fn lamp_toggle(b: &mut Builder<'_>, panel: NodeId, addr: &str, x0: f64, cx: f64, cy: f64) {
    let h = TOUCH_MIN;
    let cell = b.add(panel, draw::frame("In", x0, cy - h / 2.0, cx - x0 + 36.0, h));
    let (lx, ly) = (cx - x0, h / 2.0);
    b.add(cell, draw::circle("bezel", lx, ly, 5.4, draw::linear(180.0, &[(0.0, "#2a2b2e"), (1.0, "#060607")])));
    b.add(cell, ui::dark_lens(lx, ly, 4.0, "#ff3b2f"));
    let lit = b.add(cell, ui::lens(lx, ly, 4.0, "#ff3b2f"));
    let t = b.add(cell, ui::text("legend", "In", lx + 8.0, ly, 40.0, 10.5, INK, TextAlign::Left, 800.0, true));
    let mut c = Control::new(ControlKind::Button { mode: ButtonMode::Toggle }).slot(SlotRole::Led, lit.0).slot(SlotRole::Label, t.0);
    c.binding = Some(bind(addr));
    b.controls.insert(cell, c);
}

/// A big cream dial `d` across at (`cx`, `cy`), its header on the frame
/// line at `line` with the index tick down to the dial.
#[allow(clippy::too_many_arguments)]
fn dial_at(b: &mut Builder<'_>, panel: NodeId, addr: &str, legend: &str, format: Option<ValueFormat>, (cx, cy): (f64, f64), d: f64, line: f64) {
    let mut p = Place::new(legend).size(d).variant("cream").touch(TOUCH_MIN).bind(bind(addr));
    p.format = format;
    let k = ui::put(b, panel, "knob.numbered_cream", &p, cx, cy);
    ui::strip_legend(b, k);
    for id in b.scene.walk(k) {
        if let Some(n) = b.scene.node_mut(id)
            && n.name == "panel index"
        {
            n.visible = false;
        }
    }
    header(b, panel, legend, cx, line, (d * 0.105).clamp(10.0, 15.0));
    ui::rule(b, panel, cx - 1.0, line + 6.0, 2.0, (cy - d / 2.0 - line - 7.0).max(2.0), INK);
}

/// The same unit, not a rackspace: its front laid out to fill a `w` × `h`
/// box (a panel of a rig's row), staying with the photographed unit. Its
/// top half is the unit's left and middle, as they are on the front: the
/// gain-reduction ladder across the top with its numbers above, the name
/// with the BY PASS key and lamp beside it, the sine with REDLINE and 1%
/// THD, the ratio lamps (NUKE blue, "Opto" under 10:1) over the RATIO
/// bracket with the RATIO key, then the DETECTOR and AUDIO lamp columns
/// over their keys. Its bottom half is the unit's right: INPUT, ATTACK,
/// RELEASE and OUTPUT on their header line — INPUT the big numbered dial —
/// and POWER with the maker's mark. Addresses as [`unit`].
#[allow(clippy::too_many_lines)]
pub fn unit_box(ns: &str, name: &str, input: &str, output: &str, on: &str, w: f64, h: f64) -> document_proto::Scene {
    let a = |id: &str| format!("{ns}/{id}");
    canvas("Character Compressor", |b, root| {
        ui::fit_root(b, root, w, h);
        let panel = ui::panel(b, root, "panel.painted", "black", w, h, "none");
        if let Some(n) = b.scene.node_mut(panel) {
            n.fills = vec![draw::linear(178.0, &[(0.0, "#232427"), (0.5, "#1b1c1e"), (1.0, "#141517")])];
            n.corner_radii = document_proto::CornerRadii::uniform(6.0);
        }
        let pad = (h * 0.045).max(10.0);
        let mid = h * 0.5;
        // The unit's scale here: its 1U front is 136 tall; each half of
        // this box is about that.
        let k = ((mid - pad) / 136.0).clamp(0.75, 1.6);

        // ── Top half: the meter section and the switch section, at the
        // unit's own proportions, centred ────────────────────────────────
        let (at, ab) = (pad + 6.0, mid - pad * 0.6);
        let ah = ab - at;
        let gap = pad * 0.8;
        let room = w - 2.0 * pad - gap;
        let aw = (room * 0.68).min(ah * 5.4);
        let bw = (room - aw).min(ah * 2.4);
        let ax0 = pad + (room - aw - bw) / 2.0;
        let ax1 = ax0 + aw;
        ui::section(b, panel, ax0, at, aw, ah, LINE, 1.6);
        let row = |f: f64| at + ah * f;

        // The ladder across the top, its numbers on the frame line above,
        // lit right to left.
        let (lx0, lx1) = (ax0 + aw * 0.3, ax1 - aw * 0.03);
        let pitch = (lx1 - lx0) / LADDER.len() as f64;
        let ly = row(0.2);
        let ladder = Place::new("Gain Reduction").size(pitch * LADDER.len() as f64).scale(LADDER).options(&["gr", LADDER_COLORS]).reversed().bind(Binding::stream(a("gr")));
        let lad = ui::put(b, panel, "meter.led_ladder", &ladder, (lx0 + lx1) / 2.0, ly);
        for id in b.scene.walk(lad) {
            if let Some(n) = b.scene.node_mut(id)
                && n.name == "threshold"
            {
                n.visible = false;
            }
        }
        let numbers = (pitch * 0.34).clamp(7.5, 12.0);
        knock(b, panel, (lx0 + lx1) / 2.0, at, lx1 - lx0 + 6.0, numbers * 1.4);
        for (i, l) in LADDER.iter().rev().enumerate() {
            ui::silk(b, panel, l, lx0 + pitch * (i as f64 + 0.5), at, numbers, INK, 800.0);
        }
        ui::silk(b, panel, "Gain Reduction", lx0 + (lx1 - lx0) * 0.62, row(0.34), (10.5 * k).min(pitch * 0.5), INK, 800.0);

        // The name, heavy and slanted; BY PASS beside it.
        let size = (ah * 0.24).min(aw * 0.075);
        let script = logo::Style { size, weight: 800.0, italic: true, tracking: -0.02 };
        logo::mark(b, panel, name, ax0 + aw * 0.03, row(0.5), aw * 0.34, script, "#f2f2f0", TextAlign::Left);
        // Clear of the name, however narrow the box.
        let bx = (ax0 + aw * 0.44).max(ax0 + aw * 0.03 + size * 5.3 + 24.0 * k);
        let key = grey_key(b, panel, bx, row(0.45), 40.0 * k, 18.0 * k, true);
        b.add(panel, key);
        lamp_toggle(b, panel, on, bx + 24.0 * k, bx + 30.0 * k, row(0.45));
        ui::silk(b, panel, "By Pass", bx, row(0.45) + 14.0 * k, 8.0 * k, INK, 800.0);

        // The sine, with REDLINE and 1% THD.
        let sx = (ax0 + aw * 0.66).max(bx + 76.0 * k);
        let sy = row(0.53);
        let sine: String = (0..=24)
            .map(|i| {
                let t = f64::from(i) / 24.0;
                format!("{} {:.2} {:.2} ", if i == 0 { "M" } else { "L" }, t * 28.0 * k, 7.0 * k - 6.0 * k * (t * std::f64::consts::TAU).sin())
            })
            .collect();
        let mut sn = draw::stroked(draw::path("sine", sx - 14.0 * k, sy - 7.0 * k, 28.0 * k, 14.0 * k, sine.trim_end().to_string(), draw::solid("#00000000")), INK, 1.5);
        sn.fills.clear();
        b.add(panel, sn);
        ui::rule(b, panel, sx - 16.0 * k, sy, 32.0 * k, 1.0, INK);
        let rx = sx + 26.0 * k;
        ui::lamp(b, panel, rx, sy - 9.0 * k, 6.5 * k, "#ff3b2f", false);
        ui::silk_left(b, panel, "Redline", rx + 7.0 * k, sy - 9.0 * k, 8.0 * k, INK, 800.0);
        ui::lamp(b, panel, rx, sy + 9.0 * k, 6.5 * k, "#f6d43c", false);
        ui::silk_left(b, panel, "1% THD", rx + 7.0 * k, sy + 9.0 * k, 8.0 * k, INK, 800.0);

        // The ratio lamps, legends above ("Opto" under 10:1), over the
        // RATIO bracket; the RATIO key at the row's end.
        let (rx0, rx1) = (ax0 + aw * 0.05, ax0 + aw * 0.82);
        let rp = (rx1 - rx0) / RATIOS.len() as f64;
        let ry = row(0.82);
        let label = (8.5 * k).clamp(7.5, 11.0).min(rp * 0.3);
        let cells: Vec<_> = (0..RATIOS.len())
            .map(|i| {
                let x = rx0 + rp * (i as f64 + 0.5);
                ((x, ry), (x, row(0.69)), (x - rp / 2.0, row(0.64), rp, ry - row(0.64) + 8.0 * k))
            })
            .collect();
        lamp_select(b, panel, &a("ratio"), RATIOS, RATIO_COLORS, &cells, label);
        ui::silk(b, panel, "Opto", rx0 + rp * 5.5, ry + 11.0 * k, 7.5 * k, INK, 700.0);
        header(b, panel, "Ratio", (rx0 + rx1) / 2.0, ab, 11.0 * k);
        let kx = ax0 + aw * 0.91;
        let rk = grey_key(b, panel, kx, row(0.72), 36.0 * k, 20.0 * k, false);
        b.add(panel, rk);
        ui::silk(b, panel, "Ratio", kx, row(0.72) + 18.0 * k, 9.0 * k, INK, 800.0);

        // ── The switch section: DETECTOR and AUDIO lamp columns over their
        // keys ───────────────────────────────────────────────────────────
        let bx0 = ax1 + gap;
        ui::section(b, panel, bx0, at, bw, ah, LINE, 1.6);
        for (gi, (addr, opts, colors, legend)) in [
            (a("detector"), ["HP", "Band", "Link"], ["#46e05a", "#f6d43c", "#ff8a24"], "Detector"),
            (a("audio"), ["HP", "Dist 2", "Dist 3"], ["#46e05a", "#f6d43c", "#ff3b2f"], "Audio"),
        ]
        .iter()
        .enumerate()
        {
            let cx = bx0 + bw * (0.27 + 0.46 * gi as f64);
            let step = ah * 0.16;
            let cells: Vec<_> = (0..3)
                .map(|i| {
                    let y = row(0.16) + step * i as f64;
                    ((cx - 12.0 * k, y), (cx + 10.0 * k, y), (cx - 26.0 * k, y - step / 2.0, 64.0 * k, step))
                })
                .collect();
            lamp_select(b, panel, addr, &opts[..], &colors[..], &cells, (8.5 * k).min(10.0));
            let key = grey_key(b, panel, cx, row(0.74), 36.0 * k, 20.0 * k, false);
            b.add(panel, key);
            header(b, panel, legend, cx, ab, 10.0 * k);
        }

        // ── Bottom half: the dials on their header line, and POWER ─────
        let line = mid + pad * 1.2;
        let room = h - pad - line - 8.0;
        let big = room.min((w - 2.0 * pad) / 5.4);
        let span = (w - 2.0 * pad - 130.0 * k).min(big * 5.2);
        let (dx0, dx1) = (pad + (w - 2.0 * pad - 130.0 * k - span) / 2.0, pad + (w - 2.0 * pad - 130.0 * k - span) / 2.0 + span);
        ui::rule(b, panel, dx0, line, dx1 - dx0, 1.6, LINE);
        for (i, (addr, legend, format, d, note)) in [
            (input.to_string(), "Input", Some(ValueFormat::decimals(1)), big, ""),
            (a("attack"), "Attack", Some(ValueFormat::decimals(1)), big * 0.88, "Stereo Image Link"),
            (a("release"), "Release", None, big * 0.88, "British Mode (1:1)"),
            (output.to_string(), "Output", Some(ValueFormat::decimals(1)), big * 0.88, ""),
        ]
        .iter()
        .enumerate()
        {
            let cx = dx0 + (dx1 - dx0) * (0.125 + 0.25 * i as f64);
            let cy = line + 6.0 + room / 2.0;
            dial_at(b, panel, addr, legend, format.clone(), (cx, cy), *d, line);
            if !note.is_empty() && (dx1 - dx0) / 4.0 > d * 1.7 {
                ui::silk_left(b, panel, note, cx + d * 0.46, line + 13.0 * k, 7.0 * k, "#e8c547", 700.0);
            }
        }
        // POWER, and the maker's mark beside it.
        let px = dx1 + 65.0 * k;
        let py = line + 6.0 + room / 2.0;
        header(b, panel, "Power", px, line, 11.0 * k);
        let bezel = draw::rect("rocker bezel", px - 17.0 * k, py - 30.0 * k, 34.0 * k, 60.0 * k, draw::linear(180.0, &[(0.0, "#0b0b0c"), (1.0, "#1c1d1f")]));
        b.add(
            panel,
            draw::with_effects(draw::stroked(draw::rounded(bezel, 2.0), "#000000", 1.0), vec![draw::drop_shadow("#ffffff22", 0.0, 1.0, 0.0), draw::inner_shadow("#000000", 0.0, 2.0, 3.0)]),
        );
        let rocker = draw::rect("rocker", px - 12.0 * k, py - 25.0 * k, 24.0 * k, 50.0 * k, draw::linear(180.0, &[(0.0, "#3a3b3e"), (0.48, "#1d1e20"), (0.52, "#0d0d0e"), (1.0, "#1a1b1d")]));
        b.add(panel, draw::with_effects(draw::rounded(rocker, 1.5), vec![draw::inner_shadow("#ffffff26", 0.0, 1.0, 0.0)]));
        ui::silk(b, panel, "Empirical", px, py + 42.0 * k, 9.0 * k, "#e8c547", 800.0);
        ui::silk(b, panel, "Labs", px, py + 54.0 * k, 9.0 * k, "#e8c547", 800.0);
        crate::reverb::ui::flush(b, root, w, h);
    })
}

/// The unit as a phone lane (`w` × `h`), in the Time faces' form and its
/// own parts: a picture of its left end — the gain-reduction ladder with
/// its numbers, the name, the ratio lamps (a 2 × 4 of them, each the
/// switch) — then INPUT as the big dial, ATTACK, RELEASE and OUTPUT in the
/// knob row, its IN key at the bypass spot and the More arrow. `back`:
/// the switch section instead — the DETECTOR and AUDIO lamps, side by
/// side. Addresses as [`unit`].
#[allow(clippy::too_many_arguments)]
pub fn unit_phone(ns: &str, name: &str, input: &str, output: &str, on: &str, back: bool) -> document_proto::Scene {
    use crate::rig_time::kit;
    use document_proto::ResizeX;
    let a = |id: &str| format!("{ns}/{id}");
    let tier = kit::Tier::Phone;
    let (w, h) = (tier.lane_w(), kit::H);
    canvas("Character Compressor", |b, root| {
        ui::fit_root(b, root, w, h);
        let panel = ui::panel(b, root, "panel.painted", "black", w, h, "none");
        if let Some(n) = b.scene.node_mut(panel) {
            n.fills = vec![draw::linear(178.0, &[(0.0, "#232427"), (0.5, "#1b1c1e"), (1.0, "#141517")])];
            n.corner_radii = document_proto::CornerRadii::uniform(6.0);
        }
        // A wider picture than the Time faces': the ladder needs its room.
        let (px, pw) = (12.0, 236.0);
        let mut picture = None;
        if back {
            // The switch section, its lamp columns side by side.
            ui::silk_left(b, panel, "Advanced", 16.0, 20.0, 10.0, INK, 800.0);
            for (gi, (addr, opts, colors, legend)) in [
                (a("detector"), ["HP", "Band", "Link"], ["#46e05a", "#f6d43c", "#ff8a24"], "Detector"),
                (a("audio"), ["HP", "Dist 2", "Dist 3"], ["#46e05a", "#f6d43c", "#ff3b2f"], "Audio"),
            ]
            .iter()
            .enumerate()
            {
                let x0 = 30.0 + 250.0 * gi as f64;
                let (sx, sy, sw, sh) = (x0, 40.0, 230.0, h - 62.0);
                ui::section(b, panel, sx, sy, sw, sh, LINE, 1.6);
                header(b, panel, legend, sx + sw / 2.0, sy + sh, 11.0);
                let step = sw / 3.0;
                let cells: Vec<_> = (0..3)
                    .map(|i| {
                        let cx = sx + step * (i as f64 + 0.5);
                        ((cx, sy + sh * 0.6), (cx, sy + sh * 0.3), (cx - step / 2.0, sy + 6.0, step, sh - 12.0))
                    })
                    .collect();
                lamp_select(b, panel, addr, &opts[..], &colors[..], &cells, 12.0);
            }
        } else {
            // The picture: the unit's left end, framed.
            let (py, ph) = (14.0, h - 28.0);
            let mut sec = draw::frame("Picture", px, py, pw, ph);
            sec.layout_child.resize_x = Some(ResizeX::Stretch);
            let sec = b.add(panel, sec);
            picture = Some(sec);
            let mut frame = draw::stroked(draw::rounded(draw::rect("section", 0.0, 0.0, pw, ph, draw::solid("#00000000")), 2.0), LINE, 1.6);
            frame.fills.clear();
            frame.layout_child.resize_x = Some(ResizeX::Stretch);
            b.add(sec, frame);
            // The ladder, its numbers above, lit right to left.
            let (lx0, lx1) = (18.0, pw - 18.0);
            let ladder = Place::new("Gain Reduction").size(lx1 - lx0).scale(LADDER).options(&["gr", LADDER_COLORS]).reversed().bind(Binding::stream(a("gr")));
            let lad = ui::put(b, sec, "meter.led_ladder", &ladder, (lx0 + lx1) / 2.0, 30.0);
            // Its own numbers, each over its lamp (the unit prints them
            // above).
            for id in b.scene.walk(lad) {
                if let Some(n) = b.scene.node_mut(id)
                    && n.name == "threshold"
                {
                    n.transform.ty -= 26.0;
                }
            }
            // The name.
            let script = logo::Style { size: 30.0, weight: 800.0, italic: true, tracking: -0.02 };
            logo::mark(b, sec, name, 10.0, 54.0, pw - 20.0, script, "#f2f2f0", TextAlign::Left);
            // The ratio lamps, two rows of four, the switch.
            let (gx0, gy0) = (4.0, 72.0);
            let (cw, chh) = ((pw - 8.0) / 4.0, (ph - gy0 - 4.0) / 2.0);
            let cells: Vec<_> = (0..RATIOS.len())
                .map(|i| {
                    let (c, r) = ((i % 4) as f64, (i / 4) as f64);
                    let (x, y) = (gx0 + cw * c, gy0 + chh * r);
                    ((x + cw / 2.0, y + chh * 0.66), (x + cw / 2.0, y + chh * 0.28), (x, y, cw, chh))
                })
                .collect();
            lamp_select(b, sec, &a("ratio"), RATIOS, RATIO_COLORS, &cells, 9.5);

            // The four dials, one size, on their header line, as the unit
            // has them: INPUT, ATTACK, RELEASE, OUTPUT.
            let (bx, _, _) = kit::bypass_spot(tier);
            let (dx0, dx1) = (px + pw + 14.0, bx - 44.0);
            let line = 48.0;
            let pitch = (dx1 - dx0) / 4.0;
            let d = (pitch - 8.0).min(h - line - 18.0);
            let cy = line + 8.0 + (h - line - 8.0) / 2.0;
            ui::rule(b, panel, dx0, line - 0.8, dx1 - dx0, 1.6, LINE);
            for (i, (addr, legend)) in [(input.to_string(), "Input"), (a("attack"), "Attack"), (a("release"), "Release"), (output.to_string(), "Output")].iter().enumerate() {
                let cx = dx0 + pitch * (i as f64 + 0.5);
                let format = if *legend == "Release" { None } else { Some(ValueFormat::decimals(1)) };
                dial_at(b, panel, addr, legend, format, (cx, cy), d, line);
            }
        }
        // Its IN key at the bypass spot, the lamp over it.
        let (bx, by, _) = kit::bypass_spot(tier);
        let key = grey_key(b, panel, bx, by + 8.0, 44.0, 22.0, true);
        b.add(panel, key);
        lamp_toggle(b, panel, on, bx - 16.0, bx - 6.0, by - 18.0);
        ui::silk(b, panel, "By Pass", bx, by + 30.0, 8.0, INK, 800.0);
        kit::flip_arrow(b, panel, ns, tier, back, (INK, "#ffffff14"));
        kit::head(b, panel, ns, back, |b, head| {
            kit::nameplate_top_right(b, head, ns, tier, (8.0, 30.0), 150.0, 190.0, 12.0, ("#e8c547", "#9a9ca0"), |b, t| {
                ui::silk(b, t, "Empirical Labs", 75.0, 15.0, 11.0, "#e8c547", 800.0);
            });
        });
        // The picture takes a wider lane; on the back, the room between
        // the switch section and the key.
        let stretch = picture.unwrap_or_else(|| b.add(panel, draw::frame("room", 520.0, 0.0, kit::bypass_spot(tier).0 - 60.0 - 520.0, 1.0)));
        kit::responsive(b, panel, &[stretch]);
        crate::reverb::ui::flush(b, root, w, h);
    })
}
