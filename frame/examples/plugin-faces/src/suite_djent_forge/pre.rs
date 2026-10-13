//! Djent Forge pre-FX: five pedals in one row, all one height. Four are the
//! Standard 125B class (156 × 286), the last a Micro (92 × 286).
//!
//! What they share, as the plugin draws them: a powder-coated lid, glossy
//! black fluted knobs with a white line, bold semi-condensed legends, the
//! model name in wide engraved capitals across the middle, a chrome-rimmed
//! blue LED over a knurled stomp switch, and under the switch either the
//! maker's mark (invented, in heavy geometric capitals) or a thin printed
//! rule over a "designed in" line. Rows sit where the reference's do.

use document_proto::{NodeId, TextAlign};
use surface_kit::Place;
use surface_widgets::Builder;

use super::type_::{fine, legend, mark, name};
use super::{BLUE, LED_BLUE, PLUGIN, pedal_knob};
use crate::delay::parts;
use crate::face::{Face, canvas};
use crate::rigkit::{self as rk, Enclosure, KnobStyle, lcd};

pub fn faces() -> Vec<Face> {
    vec![compressor(), tape_drift(), twin_octave(), tight_drive(), havoc()]
}

/// The pedals' knob, 38 across (the Micro's is bigger).
const KNOB: KnobStyle<'static> = pedal_knob(38.0);

/// Where the reference's rows fall on a 286-tall box: the knob legends
/// under the top and lower rows, the name, the LED, the switch, and the
/// mark or rule under it — the LED and switch sit high enough that the
/// mark has the bottom band to itself.
const LEGEND_TOP: f64 = 62.0;
const LEGEND_LOW: f64 = 115.5;
const NAME_Y: f64 = 134.5;
const LED_Y: f64 = 157.0;
const SWITCH_Y: f64 = 190.0;
const MARK_Y: f64 = 232.0;
const RULE_Y: f64 = 233.0;

/// The maker's mark, as the silver and navy boxes print it: a heavy
/// geometric word over a second line spread letter by letter to its width.
fn maker_mark(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64, w: f64, color: &str) {
    let (_, size) = rk::letter_fit(b, parent, "AZIMUTH", (cx, cy), w, mark(20.0, color).track(0.01));
    let l = mark(5.0, color).weight(500.0);
    let chars: Vec<char> = "CIRCUITS".chars().collect();
    let n = chars.len() as f64;
    for (i, ch) in chars.iter().enumerate() {
        let x = cx - w / 2.0 + 2.5 + (w - 5.0) * i as f64 / (n - 1.0);
        rk::letter(b, parent, &ch.to_string(), (x, cy + size * 0.5 + 6.0), l);
    }
}

/// The LED over the stomp switch every box shares.
fn stomp_band(b: &mut Builder<'_>, parent: NodeId, w: f64, addr: &str, d: f64) {
    rk::stomp(b, parent, "Bypass", addr, (w / 2.0, SWITCH_Y), d, LED_BLUE, (0.0, LED_Y - SWITCH_Y), 17.0, "bezel", false);
}

/// The printed rule with a small notch over the "designed by" line (the
/// green and champagne boxes).
fn rule_and_line(b: &mut Builder<'_>, parent: NodeId, w: f64, y: f64, color: &str, notch_down: bool) {
    let (x0, x1, cx) = (w * 0.2, w * 0.8, w / 2.0);
    let k = if notch_down { 1.0 } else { -1.0 };
    rk::line(b, parent, &[(x0, y), (cx - 7.0, y), (cx, y + 5.0 * k), (cx + 7.0, y), (x1, y)], 0.8, color);
    rk::letter_fit(b, parent, "DESIGNED IN THE FORGE", (cx, y + 16.0), w * 0.7, fine(4.6, color));
}

// ── 01 · Skyline Compressor ──────────────────────────────────────────────

/// Skyline Compressor — Pedal, Standard (156 × 286). Silver powder coat,
/// blue lettering; DETAIL, MIX, VOLUME, COMP round an INTENSITY dome
/// button; a row of ten blue LEDs reads gain reduction (−1 … −20 dB, lit
/// from the right).
fn compressor() -> Face {
    let ns = "dfcomp";
    let a = |s: &str| format!("{ns}/{s}");
    let scene = canvas("Skyline Compressor", |b, root| {
        let e = Enclosure::new("standard", "#b9bbbe", "powder").base("#8e9094");
        let p = rk::pedal_body(b, root, &e);
        let (w, _) = e.size();
        let lg = legend(7.4, BLUE);
        let knob_at = [("detail", "Detail", (30.0, 35.0), LEGEND_TOP), ("mix", "Mix", (w - 30.0, 35.0), LEGEND_TOP), ("volume", "Volume", (30.0, 89.0), LEGEND_LOW), ("comp", "Comp", (w - 30.0, 89.0), LEGEND_LOW)];
        for (addr, label, at, ly) in knob_at {
            rk::knob_l(b, p, &KNOB, &a(addr), label, at, ly, lg);
        }
        // The GR meter: a black slot with ten lamps, printed thresholds over it.
        let (mx0, mx1, my) = (46.0, 112.0, 17.5);
        draw_slot(b, p, (mx0 - 2.0, my - 4.4, mx1 - mx0 + 4.0, 8.8));
        rk::gr_dots(b, p, "Compression", &a("gr"), (mx0, mx1, my), 4.8, &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 10.0, 15.0, 20.0], LED_BLUE, true);
        for (i, l) in ["-20", "-10", "-7", "-5", "-3", "-1"].iter().enumerate() {
            rk::letter(b, p, l, (mx0 + 4.0 + (mx1 - mx0 - 8.0) * i as f64 / 5.0, 8.6), legend(5.2, BLUE));
        }
        rk::letter(b, p, "COMPRESSION", (w / 2.0, 26.6), lg);
        let dome = Place::new("Intensity").size(22.0).variant("chrome").bind(rk::bind(&a("intensity"))).touch(surface_kit::TOUCH_MIN);
        let dome = parts::put(b, p, "switch.push_dome", &dome, w / 2.0, 62.0);
        rk::label_l(b, dome, "INTENSITY", (w / 2.0, 81.0), lg);

        rk::letter_fit(b, p, "COMPRESSOR", (w / 2.0, NAME_Y), 117.0, name(11.0, BLUE));
        stomp_band(b, p, w, &a("on"), 40.0);
        maker_mark(b, p, w / 2.0, MARK_Y, w * 0.76, BLUE);
    });
    let params = vec![
        rk::percent(&a("detail"), "Detail", 50.0),
        rk::percent(&a("mix"), "Mix", 50.0).featured(),
        rk::db(&a("volume"), "Volume", -12.0, 12.0, 0.0).featured(),
        rk::percent(&a("comp"), "Comp", 50.0).featured(),
        rk::toggle(&a("intensity"), "Intensity", false),
        rk::toggle(&a("on"), "Bypass", true).featured(),
    ];
    Face { file: "01-pre-standard-compressor", plugin: PLUGIN, scene, params, streams: vec![rk::gr_stream(&a("gr"), "Compression")] }
}

/// A recessed black slot (the GR meter's window).
fn draw_slot(b: &mut Builder<'_>, parent: NodeId, r: (f64, f64, f64, f64)) -> NodeId {
    let fill = surface_kit::draw::linear(180.0, &[(0.0, "#050506"), (1.0, "#1c1d20")]);
    parts::well(b, parent, "meter slot", r, fill, r.3 / 2.0)
}

/// The legend ink every box but the silver one prints in: white.
const WHITE: &str = "#f4f4f1";

/// The five-knob layout the green box uses and the others vary: two
/// knobs up top at the edges, a row of three below.
const TOP_L: (f64, f64) = (28.0, 34.0);
const TOP_R: (f64, f64) = (128.0, 34.0);
const LOW_L: (f64, f64) = (28.0, 88.0);
const LOW_M: (f64, f64) = (78.0, 88.0);
const LOW_R: (f64, f64) = (128.0, 88.0);

// ── 02 · Tape Drift ──────────────────────────────────────────────────────

/// Tape Drift — Pedal, Standard (156 × 286). A tape-echo box in sage green
/// powder coat: MIX and FEED up top either side of a small dark LCD that
/// reads the time ("2000 MS") in fourteen segments over their unlit
/// ghosts, a MS / NOTE mini toggle under it, TIME, MOD and TONE below; a
/// printed rule and "designed in" line under the switch.
fn tape_drift() -> Face {
    let ns = "dftape";
    let a = |s: &str| format!("{ns}/{s}");
    let scene = canvas("Tape Drift", |b, root| {
        let e = Enclosure::new("standard", "#77886a", "powder").base("#4f5c46");
        let p = rk::pedal_body(b, root, &e);
        let (w, _) = e.size();
        let lg = legend(7.4, WHITE);
        rk::knob_l(b, p, &KNOB, &a("mix"), "Mix", TOP_L, LEGEND_TOP, lg);
        rk::knob_l(b, p, &KNOB, &a("feedback"), "Feed", TOP_R, LEGEND_TOP, lg);
        rk::knob_l(b, p, &KNOB, &a("time"), "Time", LOW_L, LEGEND_LOW, lg);
        rk::knob_l(b, p, &KNOB.touch(38.0), &a("mod"), "Mod", LOW_M, LEGEND_LOW, lg);
        rk::knob_l(b, p, &KNOB, &a("tone"), "Tone", LOW_R, LEGEND_LOW, lg);
        // The time readout: a small dark LCD, 14-segment characters.
        let win = parts::screen(b, p, "Time display", (53.0, 9.5, 50.0, 19.0), parts::Screen { glass: ("#0a0b0c", "#17181a"), ink: "#9a9c9e", mesh: false });
        let seg = lcd::seg14(7.4, "#d2d4d6").track(0.04);
        lcd::readout(b, win, "Time", rk::bind(&a("time")), (1.0, 9.5, 48.0), seg, TextAlign::Center, None, surface_proto::ValueFormat::decimals(0).unit(" MS"));
        // MS / NOTE: the time runs free or locks to a note.
        let tm = Place::new("Time mode").size(9.0).variant("black").bind(rk::bind(&a("sync"))).legend("").touch(18.0);
        let tm = parts::put(b, p, "switch.toggle_nut", &tm, 78.0, 49.0);
        parts::strip_legend(b, tm);
        let small = legend(5.8, WHITE);
        rk::letter_aligned(b, p, "• MS", (87.0, 42.0), small, TextAlign::Left);
        rk::letter_aligned(b, p, "• NOTE", (87.0, 58.0), small, TextAlign::Left);
        rk::letter_fit(b, p, "TAPE DRIFT", (w / 2.0, NAME_Y), 96.0, name(11.0, WHITE));
        stomp_band(b, p, w, &a("on"), 40.0);
        rule_and_line(b, p, w, RULE_Y, "#eef0ea", false);
    });
    let params = vec![
        rk::percent(&a("mix"), "Mix", 50.0).featured(),
        rk::percent(&a("feedback"), "Feed", 50.0).featured(),
        parts::param(&a("time"), "Time", surface_proto::Range::linear(40.0, 3960.0, surface_proto::Unit::Milliseconds), 2000.0).featured(),
        rk::percent(&a("mod"), "Mod", 50.0),
        rk::percent(&a("tone"), "Tone", 50.0),
        rk::toggle(&a("sync"), "Note Sync", false),
        rk::toggle(&a("on"), "Bypass", true).featured(),
    ];
    Face { file: "02-pre-standard-tape-drift", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

// ── 03 · Twin Octave ─────────────────────────────────────────────────────

/// Twin Octave — Pedal, Standard (156 × 286). An octave box in champagne
/// powder coat: OCT−1 and OCT+1 up top at the edges, DIRECT alone below
/// between them; the same rule and line under the switch.
fn twin_octave() -> Face {
    let ns = "dfoct";
    let a = |s: &str| format!("{ns}/{s}");
    let scene = canvas("Twin Octave", |b, root| {
        let e = Enclosure::new("standard", "#bfb094", "powder").base("#8e836d");
        let p = rk::pedal_body(b, root, &e);
        let (w, _) = e.size();
        let lg = legend(7.4, WHITE);
        rk::knob_l(b, p, &KNOB, &a("sub"), "Oct-1", TOP_L, LEGEND_TOP, lg);
        rk::knob_l(b, p, &KNOB, &a("up"), "Oct+1", TOP_R, LEGEND_TOP, lg);
        rk::knob_l(b, p, &KNOB, &a("direct"), "Direct", LOW_M, LEGEND_LOW, lg);
        rk::letter_fit(b, p, "TWIN OCTAVE", (w / 2.0, NAME_Y), 128.0, name(11.0, WHITE));
        stomp_band(b, p, w, &a("on"), 40.0);
        rule_and_line(b, p, w, RULE_Y, "#f6f2ea", true);
    });
    let params = vec![
        rk::percent(&a("sub"), "Octave Down", 50.0).featured(),
        rk::percent(&a("up"), "Octave Up", 50.0).featured(),
        rk::percent(&a("direct"), "Direct", 50.0).featured(),
        rk::toggle(&a("on"), "Bypass", true).featured(),
    ];
    Face { file: "03-pre-standard-twin-octave", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

// ── 04 · Tight Drive ─────────────────────────────────────────────────────

/// Tight Drive — Pedal, Standard (156 × 286). A tight-bass overdrive in
/// navy sparkle: VOL and BRIGHT up top pulled in from the edges, ATTACK and
/// DRIVE lower at the edges, a small white GATE knob lit from inside
/// between them; the model name in two lines of wide blue capitals and the
/// maker's mark in blue.
fn tight_drive() -> Face {
    let ns = "dfdrive";
    let a = |s: &str| format!("{ns}/{s}");
    let scene = canvas("Tight Drive", |b, root| {
        let e = Enclosure::new("standard", "#23262e", "powder:coarse").base("#15171c");
        let p = rk::pedal_body(b, root, &e);
        let (w, _) = e.size();
        let ink = "#2a78c8";
        let lg = legend(7.4, ink);
        rk::knob_l(b, p, &KNOB, &a("volume"), "Vol", (51.0, 25.0), 53.0, lg);
        rk::knob_l(b, p, &KNOB, &a("bright"), "Bright", (103.0, 25.0), 53.0, lg);
        rk::knob_l(b, p, &KNOB, &a("attack"), "Attack", (29.0, 76.0), 104.0, lg);
        rk::knob_l(b, p, &KNOB, &a("drive"), "Drive", (126.0, 74.0), 104.0, lg);
        // The gate: a small translucent knob with a blue lamp under it.
        let glow = surface_kit::draw::circle("gate glow", 78.0, 88.5, 14.0, surface_kit::draw::radial(&[(0.0, "#8fd0ffcc"), (0.55, "#3f8fe066"), (1.0, "#3f8fe000")]));
        b.add(p, parts::emit(glow, "#6fb8ff", 0.8, 14.0));
        let gate = super::knob("fluted", 21.0, "#ffffff", &["color=#e9f1ff", "pointer=#1a2a44", "ticks=none", "ribs=9", "rib_depth=0.1", "cap_r=0.96", "height=0.6", "rough=0.3", "coat=0.5", "spec=1.4", "index=line", "index_in=0.3", "index_out=0.9"]);
        rk::knob_l(b, p, &gate, &a("gate"), "Gate", (78.0, 88.5), 71.0, lg);
        let title = name(9.6, ink).track(0.2);
        rk::letter(b, p, "TIGHT", (w / 2.0, 124.5), title);
        rk::letter(b, p, "DRIVE", (w / 2.0, 138.5), title);
        stomp_band(b, p, w, &a("on"), 40.0);
        maker_mark(b, p, w / 2.0, MARK_Y, w * 0.76, "#1f6ab8");
    });
    let params = vec![
        rk::db(&a("volume"), "Volume", -24.0, 24.0, 0.0).featured(),
        rk::percent(&a("bright"), "Bright", 50.0),
        rk::percent(&a("attack"), "Attack", 36.0),
        rk::percent(&a("drive"), "Drive", 42.0).featured(),
        rk::percent(&a("gate"), "Gate", 0.0),
        rk::toggle(&a("on"), "Bypass", true).featured(),
    ];
    Face { file: "04-pre-standard-tight-drive", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

// ── 05 · Havoc ───────────────────────────────────────────────────────────

/// Havoc — Pedal, Micro (92 × 286). A narrow oxblood box with one big MIX
/// knob, the name in wide capitals, and a printed jagged squiggle over a
/// two-line "designed in" credit under the switch.
fn havoc() -> Face {
    let ns = "dfhavoc";
    let a = |s: &str| format!("{ns}/{s}");
    let scene = canvas("Havoc", |b, root| {
        let e = Enclosure::new("micro", "#7a3531", "powder").base("#4f2320");
        let p = rk::pedal_body(b, root, &e);
        let (w, _) = e.size();
        rk::knob_l(b, p, &pedal_knob(58.0), &a("mix"), "Mix", (w / 2.0, 42.0), 94.0, legend(8.4, WHITE));
        rk::letter_fit(b, p, "HAVOC", (w / 2.0, NAME_Y), 58.0, name(11.0, WHITE));
        stomp_band(b, p, w, &a("on"), 36.0);
        // The squiggle: a flat line that breaks into a few jagged teeth.
        let y = RULE_Y;
        let mut pts = vec![(14.0, y), (34.0, y)];
        for (i, dx) in [0.0, 3.0, 6.0, 9.0, 12.0, 15.0, 18.0].iter().enumerate() {
            pts.push((36.0 + dx, if i % 2 == 0 { y - 4.5 } else { y + 0.5 }));
        }
        pts.extend([(57.0, y), (78.0, y)]);
        rk::line(b, p, &pts, 0.8, "#f1e8e4");
        let small = fine(4.3, "#f1e8e4").track(0.2);
        rk::letter(b, p, "DESIGNED", (w / 2.0, y + 12.0), small);
        rk::letter(b, p, "IN THE FORGE", (w / 2.0, y + 20.5), small);
    });
    let params = vec![rk::percent(&a("mix"), "Mix", 50.0).featured(), rk::toggle(&a("on"), "Bypass", true).featured()];
    Face { file: "05-pre-micro-havoc", plugin: PLUGIN, scene, params, streams: Vec::new() }
}
