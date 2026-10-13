//! The desktop VCA compressor — laid out from photographs of the original
//! dbx 160 (1976): a gloss-black front with its THRESHOLD lamps, a big
//! COMPRESSION knob and the decibel meter, here built as a premium
//! reissue would be — the front set into a dark anodised chassis (milled
//! end caps held by socket screws, a brushed graphite rail along its top
//! and foot) in place of the original's walnut cheeks and bright trim.
//!
//! Left to right as the unit has them: THRESHOLD (printed in the unit's
//! input volts, .01 → 3 V) with its BELOW / ABOVE lamps — a two-lamp ladder
//! on the gain-reduction stream: dark with no reduction, BELOW lit as the
//! signal reaches threshold (half a dB), ABOVE once it is over (1 dB) —
//! COMPRESSION from 1:1 to ∞:1 on the unit's own uneven scale (the biggest
//! knob), and OUTPUT GAIN ±20 dB. The knobs are black and knurled with a
//! polished aluminium insert. Under them, the ivory POWER key and the
//! interlocked METER keys (input, output, gain change). The big meter at
//! the right is a pale card printed in blue with a plain decibel scale,
//! recessed behind glass, fed by whichever signal the METER keys pick.
//!
//! The rack face and the host's lane shapes (see [`lane`]) are one layout
//! read at three sets of [`Spots`], so they share every material.

use document_proto::{NodeId, TextAlign};
use surface_kit::{Ink, Place, TOUCH_MIN, draw};
use surface_proto::{Binding, ParamInfo, StreamInfo, Unit};
use surface_widgets::Builder;

use super::logo;
use super::ui::{self, bind};
use crate::face::{Face, canvas};

pub mod lane;

/// The silkscreen: a warm white, not paper-white, so it sits in the gloss.
const INK: &str = "#e8e6df";
/// The printed scales round the knobs, a step quieter than the headers.
const SCALE_INK: &str = "#c4c2bb";

/// Threshold as printed: the input level (volts) where compression starts,
/// evenly spaced in dB — .01 V at the stop is -38 dBu, 3 V is +12 dBu.
const THRESHOLDS: &[&str] = &[".01", ".03", ".1", ".3", "1", "3"];
/// The compression dial's own uneven printing, 1:1 to limiting.
const RATIOS: &[&str] = &["1", "1.5", "2", "3", "4", "6", "10", "20", "∞"];
/// The real ratio at each printed mark; ∞ stands in as 60:1.
const RATIO_VALUES: &[f64] = &[1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 10.0, 20.0, 60.0];
const OUTPUTS: &[&str] = &["-20", "-10", "0", "+10", "+20"];

/// Where a form puts things, panel px.
struct Spots {
    size: (f64, f64),
    /// The chassis: end-cap width, top and foot rails, screw size.
    caps: f64,
    rails: (f64, f64),
    screw: f64,
    /// THRESHOLD, COMPRESSION, OUTPUT GAIN: shaft x and diameter.
    knobs: [(f64, f64); 3],
    /// The knob row's shaft line, and the headers' line.
    row: f64,
    header: f64,
    /// Header and numeral type sizes.
    head: f64,
    numerals: f64,
    /// OUTPUT GAIN set on two lines.
    stacked: bool,
    /// The lamps' line and the ladder's length.
    lamps: (f64, f64),
    /// The key row's line and the keys' cap width — none on the lane.
    keys: Option<(f64, f64)>,
    /// The meter's centre and width.
    meter: (f64, f64, f64),
    /// The logotype's centre and scale.
    logo: (f64, f64, f64),
}

/// The rack face: 860 × 360.
const RACK: Spots = Spots {
    size: (860.0, 360.0),
    caps: 26.0,
    rails: (14.0, 18.0),
    screw: 10.0,
    knobs: [(112.0, 80.0), (270.0, 100.0), (456.0, 80.0)],
    row: 160.0,
    header: 52.0,
    head: 13.0,
    numerals: 11.0,
    stacked: false,
    lamps: (78.0, 56.0),
    keys: Some((290.0, 66.0)),
    meter: (690.0, 138.0, 260.0),
    logo: (690.0, 286.0, 0.92),
};

/// The rack face: gloss black in its dark anodised chassis.
pub fn face() -> Face {
    let scene = canvas("VCA Compressor", |b, root| build(b, root, &RACK));
    Face { file: "09-vca-comp", plugin: "comp", scene, params: params(true), streams: streams() }
}

/// The catalog entries; the METER keys only where the face has them.
fn params(keys: bool) -> Vec<ParamInfo> {
    let mut v = vec![
        ui::db("vcacomp/threshold", "Threshold", -40.0, 0.0, -20.0).featured(),
        ui::param("vcacomp/ratio", "Compression", ui::printed(RATIO_VALUES, Unit::Ratio), 4.0).featured(),
        ui::db("vcacomp/output", "Output Gain", -20.0, 20.0, 0.0).featured(),
    ];
    if keys {
        v.push(ui::choice("vcacomp/meter", "Meter", &["Input", "Output", "Gain Change"], 2));
    }
    v
}

fn streams() -> Vec<StreamInfo> {
    vec![ui::gr_stream("vcacomp/gr", "Gain Reduction"), ui::level_stream("vcacomp/meter_level", "Meter (METER select)", 1)]
}

/// Build the face at `s` under `root`.
fn build(b: &mut Builder<'_>, root: NodeId, s: &Spots) {
    let (w, h) = s.size;
    ui::fit_root(b, root, w, h);
    let panel = ui::panel(b, root, "panel.desktop", "black", w, h, "none");
    // Gloss black: a deep, even black with a broad reflection across its
    // upper half (the plate light adds the softbox on the GPU stage).
    b.add(panel, draw::rect("gloss", 0.0, 0.0, w, h, draw::linear(172.0, &[(0.0, "#ffffff10"), (0.32, "#ffffff04"), (0.5, "#ffffff00"), (1.0, "#0000002e")])));
    chassis(b, panel, s);

    // ── Threshold, with its below/above lamps ──────────────────────────
    let (tx, td) = s.knobs[0];
    ui::silk(b, panel, "Threshold", tx, s.header, s.head, INK, 700.0);
    lamps(b, panel, s, tx);
    knob(b, panel, ("vcacomp/threshold", "Threshold"), THRESHOLDS, (tx, s.row, td), s.numerals);

    // ── Compression: the biggest knob ──────────────────────────────────
    let (cx, cd) = s.knobs[1];
    ui::silk(b, panel, "Compression", cx, s.header, s.head, INK, 700.0);
    knob(b, panel, ("vcacomp/ratio", "Compression"), RATIOS, (cx, s.row, cd), s.numerals);

    // ── Output gain ────────────────────────────────────────────────────
    let (ox, od) = s.knobs[2];
    if s.stacked {
        ui::silk(b, panel, "Output", ox, s.header - s.head * 1.25, s.head, INK, 700.0);
        ui::silk(b, panel, "Gain", ox, s.header, s.head, INK, 700.0);
    } else {
        ui::silk(b, panel, "Output Gain", ox, s.header, s.head, INK, 700.0);
    }
    knob(b, panel, ("vcacomp/output", "Output Gain"), OUTPUTS, (ox, s.row, od), s.numerals);

    if let Some((y, cap)) = s.keys {
        keys(b, panel, s, y, cap);
    }

    // ── The meter, and the name set as the unit's logotype ─────────────
    let (mx, my, mw) = s.meter;
    let meter = Place::new("Decibels").size(mw).variant("slim").legend("DECIBELS").options(&["glass"]).bind(Binding::stream("vcacomp/meter_level"));
    let m = ui::put(b, panel, "meter.vu_decibel_white", &meter, mx, my);
    surround(b, panel, m, s.caps * 0.3);
    logotype(b, panel, s.logo);
}

/// A machined graphite surround the meter is set into, `t` wide — drawn
/// under the meter (moved back to the top afterwards).
fn surround(b: &mut Builder<'_>, panel: NodeId, meter: NodeId, t: f64) {
    let Some(n) = b.scene.node(meter) else { return };
    let (x, y, w, h) = (n.transform.tx - t, n.transform.ty - t, n.size.width + 2.0 * t, n.size.height + 2.0 * t);
    let ring = draw::rect("meter surround", x, y, w, h, draw::linear(180.0, &[(0.0, "#46494e"), (0.12, "#2c2e32"), (0.85, "#1c1d20"), (1.0, "#33353a")]));
    b.add(panel, draw::with_effects(draw::stroked(draw::rounded(ring, t * 0.9 + 2.0), "#000000b0", 1.0), vec![draw::drop_shadow("#00000099", 0.0, 2.0, 6.0), draw::inner_shadow("#ffffff26", 0.0, 1.0, 0.0)]));
    if let Some(p) = b.scene.node_mut(panel) {
        p.children.retain(|c| *c != meter);
        p.children.push(meter);
    }
}

// ── the chassis ──────────────────────────────────────────────────────────

/// Horizontal hairlines every `period` px over (`x`, `y`, `w`, `h`), one path.
fn hairlines(x: f64, y: f64, w: f64, h: f64, period: f64, offset: f64) -> String {
    let mut d = String::new();
    let mut yy = y + offset;
    while yy < y + h {
        d.push_str(&format!("M {x:.2} {yy:.2} H {:.2} V {:.2} H {x:.2} Z ", x + w, (yy + 0.6).min(y + h)));
        yy += period;
    }
    d
}

/// A brushed graphite rail across the front: dark anodised, its edge
/// catching the light on the side that faces up.
fn rail(b: &mut Builder<'_>, panel: NodeId, (x, w): (f64, f64), y: f64, h: f64, top: bool) {
    let paint = if top {
        draw::linear(180.0, &[(0.0, "#4a4d52"), (0.2, "#36383d"), (0.7, "#26282c"), (1.0, "#1a1b1e")])
    } else {
        draw::linear(180.0, &[(0.0, "#3c3f44"), (0.25, "#2c2e32"), (0.75, "#1f2023"), (1.0, "#111214")])
    };
    b.add(panel, draw::rect("rail", x, y, w, h, paint));
    // The brushing, and the light lying along it.
    b.add(panel, draw::path("rail grain", 0.0, 0.0, 1.0, 1.0, hairlines(x, y, w, h, 2.0, 0.0), draw::solid("#ffffff0b")));
    b.add(panel, draw::path("rail grain", 0.0, 0.0, 1.0, 1.0, hairlines(x, y, w, h, 3.0, 1.0), draw::solid("#0000001a")));
    b.add(
        panel,
        draw::rect("rail sheen", x, y, w, h, draw::linear(90.0, &[(0.0, "#ffffff00"), (0.2, "#ffffff12"), (0.34, "#ffffff04"), (0.62, "#ffffff00"), (0.8, "#ffffff0c"), (1.0, "#ffffff00")])),
    );
    if top {
        // The top edge catches the light; the lip over the front throws a
        // shadow down onto the gloss.
        b.add(panel, draw::rect("rail edge", x, y, w, 1.0, draw::solid("#ffffff2e")));
        b.add(panel, draw::rect("rail lip", x, y + h - 1.0, w, 1.0, draw::solid("#000000c0")));
        b.add(panel, draw::rect("lip shadow", x, y + h, w, h.min(10.0), draw::linear(180.0, &[(0.0, "#00000080"), (1.0, "#00000000")])));
    } else {
        // The foot rail's chamfer faces up and takes the light.
        b.add(panel, draw::rect("rail chamfer", x, y, w, 1.0, draw::solid("#000000b0")));
        b.add(panel, draw::rect("rail chamfer", x, y + 1.0, w, 1.0, draw::solid("#ffffff24")));
        b.add(panel, draw::rect("rail edge", x, y + h - 1.0, w, 1.0, draw::solid("#00000099")));
    }
}

/// A socket-head screw, black, its hex socket dark.
fn screw(b: &mut Builder<'_>, panel: NodeId, cx: f64, cy: f64, d: f64) {
    let r = d / 2.0;
    b.add(panel, draw::with_effects(draw::circle("screw seat", cx, cy, r + 1.0, draw::solid("#00000000")), vec![draw::inner_shadow("#000000c0", 0.0, 1.0, 1.5)]));
    let head = draw::circle("screw", cx, cy, r, draw::radial_at(0.36, 0.3, 0.85, &[(0.0, "#5b5e63"), (0.45, "#2a2c30"), (1.0, "#0d0e10")]));
    b.add(panel, draw::with_effects(draw::stroked(head, "#00000099", 0.8), vec![draw::drop_shadow("#ffffff14", 0.0, 0.8, 0.0)]));
    let hex = draw::path("socket", 0.0, 0.0, 1.0, 1.0, draw::regular(cx, cy, r * 0.46, 6, 30.0), draw::solid("#060607"));
    b.add(panel, draw::with_effects(hex, vec![draw::inner_shadow("#ffffff1c", 0.0, -0.6, 0.0)]));
}

/// A milled end cap, black anodised, held by socket screws (three on the
/// rack face).
fn end_cap(b: &mut Builder<'_>, panel: NodeId, s: &Spots, left: bool) {
    let (w, h) = s.size;
    let cw = s.caps;
    let x = if left { 0.0 } else { w - cw };
    let paint = if left {
        draw::linear(90.0, &[(0.0, "#1a1b1e"), (0.3, "#383a3f"), (0.62, "#2b2d31"), (1.0, "#1e1f22")])
    } else {
        draw::linear(90.0, &[(0.0, "#1e1f22"), (0.38, "#2b2d31"), (0.7, "#34363b"), (1.0, "#16171a")])
    };
    b.add(panel, draw::rect("end cap", x, 0.0, cw, h, paint));
    // The milling: fine vertical cuts, barely there.
    let mut d = String::new();
    let mut gx = x + 1.5;
    while gx < x + cw - 1.0 {
        d.push_str(&format!("M {gx:.2} 0 h 0.6 V {h:.2} h -0.6 Z "));
        gx += 2.5;
    }
    b.add(panel, draw::path("end cap grain", 0.0, 0.0, 1.0, 1.0, d, draw::solid("#ffffff08")));
    // Lit along its top, a chamfer down the edge it meets the front with.
    b.add(panel, draw::rect("end cap top", x, 0.0, cw, 1.0, draw::solid("#ffffff30")));
    let inner = if left { x + cw - 2.0 } else { x };
    let chamfer: &[(f64, &str)] = if left { &[(0.0, "#ffffff1c"), (1.0, "#000000c0")] } else { &[(0.0, "#000000c0"), (1.0, "#ffffff14")] };
    b.add(panel, draw::rect("end cap chamfer", inner, 0.0, 2.0, h, draw::linear(90.0, chamfer)));
    let pad = s.screw * 0.5 + s.rails.0.max(cw * 0.45);
    let mut ys = vec![pad, h - pad];
    if h > 300.0 {
        ys.insert(1, h / 2.0);
    }
    for y in ys {
        screw(b, panel, x + cw / 2.0, y, s.screw);
    }
}

/// The chassis round the gloss front: the two rails, then the end caps over
/// their ends.
fn chassis(b: &mut Builder<'_>, panel: NodeId, s: &Spots) {
    let (w, h) = s.size;
    let (top, foot) = s.rails;
    rail(b, panel, (0.0, w), 0.0, top, true);
    rail(b, panel, (0.0, w), h - foot, foot, false);
    end_cap(b, panel, s, true);
    end_cap(b, panel, s, false);
}

// ── the parts ────────────────────────────────────────────────────────────

/// The BELOW / ABOVE lamps: the ladder on the gain-reduction stream, its
/// bezels made deeper collars and its printed thresholds hidden (the unit
/// prints what the lamps mean).
fn lamps(b: &mut Builder<'_>, panel: NodeId, s: &Spots, tx: f64) {
    let (ly, len) = s.lamps;
    let p = Place::new("Threshold lamps").size(len).scale(&["0.5", "1"]).options(&["gr", "colors=#ffc23a,#ff4a36"]).bind(Binding::stream("vcacomp/gr"));
    let ladder = ui::put(b, panel, "meter.led_ladder", &p, tx, ly);
    let mut d = 0.0;
    let mut lamp_y = 0.0;
    for id in b.scene.walk(ladder) {
        let Some(n) = b.scene.node_mut(id) else { continue };
        match n.name.as_str() {
            "threshold" => n.visible = false,
            "bezel" => {
                // A deeper collar: dark chrome, lit from above.
                let grow = n.size.width * 0.14;
                n.transform.tx -= grow;
                n.transform.ty -= grow;
                n.size.width += 2.0 * grow;
                n.size.height += 2.0 * grow;
                d = n.size.width;
                lamp_y = n.transform.ty + d / 2.0;
                n.fills = vec![draw::linear(180.0, &[(0.0, "#8a8d92"), (0.35, "#3a3c40"), (0.7, "#141517"), (1.0, "#4a4c51")])];
                n.effects = vec![draw::drop_shadow("#000000b0", 0.0, 1.0, 2.0), draw::inner_shadow("#00000099", 0.0, 0.0, 1.0)];
            }
            _ => {}
        }
    }
    let top = b.scene.node(ladder).map_or(0.0, |n| n.transform.ty);
    let size = (s.head * 0.62).max(7.5);
    // The ladder's lamps sit a quarter of its length either side of centre.
    let off = len * 0.25 + d * 0.5 + size * 0.4;
    ui::silk_right(b, panel, "Below", tx - off, top + lamp_y, size, SCALE_INK, 700.0);
    ui::silk_left(b, panel, "Above", tx + off, top + lamp_y, size, SCALE_INK, 700.0);
}

/// A knurled black knob with a tick ring and the unit's numerals round it,
/// its shaft at (`cx`, `cy`), `size` across.
fn knob(b: &mut Builder<'_>, panel: NodeId, (addr, name): (&str, &str), scale: &[&str], (cx, cy, size): (f64, f64, f64), numerals: f64) -> NodeId {
    let ticks = vec![""; scale.len()];
    let p = Place::new(name).size(size).scale(&ticks).ink(Ink::Color(SCALE_INK.into())).touch(TOUCH_MIN).bind(bind(addr));
    let k = ui::put(b, panel, "knob.knurled_insert", &p, cx, cy);
    ui::strip_legend(b, k);
    // The knurl catches the light as texture, not as a ring of white.
    for id in b.scene.walk(k) {
        if let Some(n) = b.scene.node_mut(id)
            && n.name == "knurl"
        {
            n.fills = vec![draw::solid("#ffffff12")];
        }
    }
    ui::scale_labels(b, panel, cx, cy, size * 0.68 + numerals, scale, 300.0, numerals, SCALE_INK);
    k
}

/// The POWER key: an ivory piano key in a black housing (the unit's mains
/// switch — decoration here, as a plugin has no mains).
fn power_key(b: &mut Builder<'_>, panel: NodeId, cx: f64, cy: f64, w: f64) {
    let h = w * 0.42;
    let pad = w * 0.07;
    let house = draw::rect("power housing", cx - w / 2.0 - pad, cy - h / 2.0 - pad, w + 2.0 * pad, h + 2.0 * pad, draw::linear(180.0, &[(0.0, "#050506"), (1.0, "#16171a")]));
    b.add(panel, draw::with_effects(draw::rounded(house, 2.0), vec![draw::inner_shadow("#000000", 0.0, 1.0, 2.0), draw::drop_shadow("#ffffff1c", 0.0, 1.0, 0.0)]));
    let cap = draw::rect("power key", cx - w / 2.0, cy - h / 2.0, w, h, draw::linear(180.0, &[(0.0, "#f1ecdf"), (0.18, "#e6e0d0"), (0.8, "#d3ccba"), (1.0, "#a8a292")]));
    b.add(
        panel,
        draw::with_effects(
            draw::stroked(draw::rounded(cap, 1.5), "#000000b0", 1.0),
            vec![draw::drop_shadow("#000000a0", 0.0, 1.5, 2.5), draw::inner_shadow("#ffffff70", 0.0, 1.0, 0.0)],
        ),
    );
}

/// The key row: POWER under THRESHOLD, the interlocked METER keys under
/// COMPRESSION and OUTPUT GAIN, bracketed and titled.
fn keys(b: &mut Builder<'_>, panel: NodeId, s: &Spots, py: f64, cap: f64) {
    let px = s.knobs[0].0;
    let small = s.head * 0.8;
    ui::silk(b, panel, "Power", px, py - cap * 0.42, small, INK, 700.0);
    power_key(b, panel, px, py, cap * 1.12);
    let mid = (s.knobs[1].0 + s.knobs[2].0) / 2.0;
    let sel = Place::new("Meter").size(cap).variant("cream").bind(bind("vcacomp/meter")).options(&["Input", "Output", "Gain Change"]).ink(Ink::Color(SCALE_INK.into())).opt_legend(small * 0.9);
    let bank = ui::put(b, panel, "button.piano_bank", &sel, mid, py - cap * 0.15);
    // The bracket under the row, METER set into it.
    let half = b.scene.node(bank).map_or(cap * 1.6, |n| n.size.width / 2.0) + 4.0;
    let by = py + cap * 0.42;
    let gap = ui::text_w("Meter", small) / 2.0 + small;
    let t = 1.2;
    ui::rule(b, panel, mid - half, by - 7.0, t, 7.0, SCALE_INK);
    ui::rule(b, panel, mid + half - t, by - 7.0, t, 7.0, SCALE_INK);
    ui::rule(b, panel, mid - half, by, half - gap, t, SCALE_INK);
    ui::rule(b, panel, mid + gap, by, half - gap, t, SCALE_INK);
    ui::silk(b, panel, "Meter", mid, by + 1.0, small, INK, 700.0);
}

/// The logotype: heavy, tightly set lowercase, a jewel-like dot, a bold
/// word, and the line under it — centred on (`cx`, `cy`) at scale `k`.
fn logotype(b: &mut Builder<'_>, panel: NodeId, (cx, cy, k): (f64, f64, f64)) {
    let heavy = logo::Style { size: 40.0 * k, weight: 900.0, italic: false, tracking: -0.04 };
    let bold = logo::Style { size: 30.0 * k, weight: 800.0, italic: false, tracking: -0.01 };
    let ink = "#e4e1d8";
    logo::mark(b, panel, "vca", cx - 150.0 * k, cy - 2.0 * k, 138.0 * k, heavy, ink, TextAlign::Right);
    b.add(
        panel,
        draw::with_effects(
            draw::circle("mark dot", cx - 1.0 * k, cy, 5.5 * k, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#c9473a"), (0.5, "#7a1d16"), (1.0, "#2a0806")])),
            vec![draw::drop_shadow("#ffffff26", 0.0, 1.0, 0.0), draw::inner_shadow("#00000080", 0.0, -1.0, 1.0)],
        ),
    );
    logo::mark(b, panel, "comp", cx + 12.0 * k, cy + 2.0 * k, 140.0 * k, bold, ink, TextAlign::Left);
    ui::silk(b, panel, "Compressor / Limiter", cx, cy + 32.0 * k, 11.5 * k, SCALE_INK, 600.0);
}

/// A legend size for a placed component configured by `legend=` option.
trait LegendOpt {
    fn opt_legend(self, size: f64) -> Self;
}

impl LegendOpt for Place {
    fn opt_legend(mut self, size: f64) -> Self {
        self.options.get_or_insert_with(Vec::new).push(format!("legend={size}"));
        self
    }
}
