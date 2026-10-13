//! The Scandinavian optical valve compressor — laid out from the maker's
//! front-panel photo of the blue 3U unit (Tube-Tech CL 1B), with no maker's
//! names on it.
//!
//! Royal-blue painted steel with slotted rack holes. The name block and
//! the IN toggle at the left; two rows of big glossy black seven-sided
//! knobs — GAIN, RATIO, THRESHOLD over ATTACK, RELEASE — each read
//! against a white dot per printed step; black bar selectors for METER
//! (input / compression / output), attack/release SELECT (fixed /
//! fix-man / manual) and sidechain BUS SELECT; the tan VU in a thin black
//! frame at the top right over the big red pilot jewel and the OFF/ON
//! power knob. White silkscreen, captions in capitals and positions in
//! lower case, as the unit prints them.

use document_proto::NodeId;
use surface_kit::{Place, draw};
use surface_proto::{MeterTap, Range, SourceSelect, StreamInfo, StreamKind, Unit};
use surface_widgets::Builder;

use super::ui::{self, Ink};
use crate::face::{Face, canvas};

pub mod lane;

/// The photographed panel: 1925 × 520 px from (35, 38) in the photo.
const W: f64 = 1000.0;
const H: f64 = 270.0;
const S: f64 = W / 1925.0;
const INK: &str = "#f4f6f8";
const CHROME: &str = "#d4d8dc";
/// The knobs' travel between their end dots, as printed.
const SWEEP: f64 = 240.0;
/// The selectors' three positions, 40° apart.
const SELECT: f64 = 80.0;

/// Where the panel prints GAIN's and THRESHOLD's marks (degrees from up)
/// and what they mean: not evenly — the pots' taper is printed as it
/// falls. "off" is the first mark (−20 dB of gain; a threshold of +10).
const GAIN: &[(f64, f64)] = &[(-120.0, -20.0), (-85.0, -10.0), (-55.0, 0.0), (4.0, 10.0), (44.0, 20.0), (120.0, 30.0)];
const THRESHOLD: &[(f64, f64)] = &[(-120.0, 10.0), (-94.0, 0.0), (-54.0, -10.0), (5.0, -20.0), (46.0, -30.0), (120.0, -40.0)];

/// A photo coordinate on this face.
const fn at(x: f64, y: f64) -> (f64, f64) {
    ((x - 35.0) * S, (y - 38.0) * S)
}

pub fn face() -> Face {
    let scene = canvas("Opto Compressor", build);
    let params = vec![
        // "off" at the start of travel: the threshold above anything.
        ui::param("optocomp/threshold", "Threshold", ui::printed_range(THRESHOLD, SWEEP, Unit::Db), -20.0).short("Thresh"),
        ui::param("optocomp/ratio", "Ratio", Range::linear(2.0, 10.0, Unit::Ratio), 4.0),
        ui::param("optocomp/attack", "Attack", Range::log(0.5, 300.0, Unit::Milliseconds), 10.0),
        ui::param("optocomp/release", "Release", Range::log(0.05, 10.0, Unit::Seconds), 0.5),
        ui::choice("optocomp/timing", "Attack / Release Select", &["Fixed", "Fix-Man", "Manual"], 2).short("Timing"),
        ui::param("optocomp/gain", "Gain", ui::printed_range(GAIN, SWEEP, Unit::Db), 0.0),
        ui::choice("optocomp/meter", "Meter", &["Input", "Compression", "Output"], 1),
        ui::choice("optocomp/sidechain", "Sidechain Bus Select", &["Off", "1", "2"], 0).short("SC Bus"),
        ui::toggle("optocomp/active", "In", true),
        ui::toggle("optocomp/power", "Power", true),
    ];
    let mut streams = ui::streams("optocomp", "Opto Compressor");
    streams.push(StreamInfo { id: surface_proto::EntryId::new("optocomp/in"), name: "Opto Compressor Input".into(), kind: StreamKind::Level { channels: 2 }, rate_hz: 30.0 });
    Face { file: "05-opto-comp", plugin: "comp", scene, params, streams }
}

fn build(b: &mut Builder<'_>, root: NodeId) {
    let panel = ui::put(b, root, "panel.painted", &Place::new("Opto compressor panel").variant("blue").options(&[&format!("w={W}"), &format!("h={H}"), "ends=none"]));
    ui::repaint(b, panel, &[(0.0, "#0f6aa6"), (0.5, "#0a5d95"), (1.0, "#084f80")]);

    // ── Rack holes and screws ────────────────────────────────────────
    for (x, y) in [(67.0, 185.0), (67.0, 410.0), (1927.0, 188.0), (1927.0, 418.0)] {
        ui::rack_slot(b, panel, at(x, y), 20.0, 13.0);
    }
    for (x, y) in [(180.0, 143.0), (178.0, 455.0), (1808.0, 147.0), (1808.0, 460.0)] {
        ui::phillips(b, panel, at(x, y), 15.0, CHROME);
    }

    // ── The name block and IN, left ──────────────────────────────────
    let x0 = at(66.0, 0.0).0;
    ui::silk_left(b, panel, "Opto-Comp", x0, at(0.0, 262.0).1, Ink::new(24.0, INK).weight(600.0).track(-0.02));
    let line = Ink::new(9.6, INK).weight(500.0).track(0.0);
    ui::silk_left(b, panel, "Compressor", x0, at(0.0, 310.0).1, line);
    ui::silk_right(b, panel, "Valve", at(298.0, 0.0).0, at(0.0, 310.0).1, line);
    ui::silk_left(b, panel, "Optical", x0, at(0.0, 350.0).1, line);
    ui::silk_right(b, panel, "Stereo", at(298.0, 0.0).0, at(0.0, 350.0).1, line);
    let (tx, ty) = at(255.0, 458.0);
    let active = ui::put(b, panel, "switch.toggle_bare", &Place::new("In").size(40.0).legend(" ").variant("black").options(&["in", ""]).keep_case().bind(ui::bind("optocomp/active")));
    super::opto_leveler::place_toggle(b, active, tx, ty);

    // ── The knobs ────────────────────────────────────────────────────
    let labels = Ink::new(9.0, INK).weight(500.0).track(0.02).plain();
    let caption = Ink::new(8.6, INK).weight(700.0).track(0.06);
    let knob = |b: &mut Builder<'_>, name: &str, addr: &str, marks: &[(f64, &str)], (x, y): (f64, f64), cap_y: f64| {
        let c = at(x, y);
        let k = ui::rotary_at(b, panel, "knob.heptagon", &Place::new(name).size(70.0).legend(" ").sweep(SWEEP).scale(&[]).touch(surface_kit::TOUCH_MIN).bind(ui::bind(addr)), c.0, c.1);
        ui::hide_ring(b, k);
        ui::marks_at(b, panel, c, 42.0, 47.0, marks, 4.4, labels);
        ui::silk(b, panel, name, c.0, at(0.0, cap_y).1, caption);
    };
    let printed = |m: &[(f64, f64)], words: [&'static str; 6]| -> Vec<(f64, &'static str)> { m.iter().zip(words).map(|((a, _), w)| (*a, w)).collect() };
    knob(b, "Gain", "optocomp/gain", &printed(GAIN, ["off", "−10", "0", "+10", "+20", "+30"]), (450.0, 175.0), 267.0);
    knob(b, "Ratio", "optocomp/ratio", &[(-120.0, "2:1"), (120.0, "10:1")], (727.0, 175.0), 268.0);
    knob(b, "Threshold", "optocomp/threshold", &printed(THRESHOLD, ["off", "0", "−10", "−20", "−30", "−40"]), (1007.0, 175.0), 268.0);
    knob(b, "Attack", "optocomp/attack", &[(-120.0, "fast"), (120.0, "slow")], (447.0, 425.0), 517.0);
    knob(b, "Release", "optocomp/release", &[(-120.0, "fast"), (120.0, "slow")], (725.0, 425.0), 519.0);

    // ── The selectors ────────────────────────────────────────────────
    let words = Ink::new(8.6, INK).weight(500.0).track(0.02).plain();
    let selector = |b: &mut Builder<'_>, name: &str, addr: &str, options: &[&str], (x, y): (f64, f64)| {
        let c = at(x, y);
        let k = ui::rotary_at(b, panel, "knob.bar_pointer", &Place::new(name).size(50.0).legend(" ").options(&["1", "2", "3"]).sweep(SELECT).touch(surface_kit::TOUCH_MIN).bind(ui::bind(addr)), c.0, c.1);
        ui::hide_ring(b, k);
        ui::dial_numerals(b, panel, c, 40.0, SELECT, options, words);
    };
    selector(b, "Meter", "optocomp/meter", &["input", "", "output"], (1230.0, 175.0));
    ui::silk(b, panel, "compres-", at(1230.0, 0.0).0, at(0.0, 80.0).1, words);
    ui::silk(b, panel, "sion", at(1230.0, 0.0).0, at(0.0, 98.0).1, words);
    ui::silk(b, panel, "Meter", at(1228.0, 0.0).0, at(0.0, 268.0).1, caption);
    selector(b, "Attack / Release Select", "optocomp/timing", &["fixed", "fix./man.", "manual"], (1007.0, 425.0));
    ui::silk(b, panel, "attack/release", at(1007.0, 0.0).0, at(0.0, 510.0).1, words);
    ui::silk(b, panel, "Select", at(1007.0, 0.0).0, at(0.0, 530.0).1, caption);
    selector(b, "Sidechain Bus Select", "optocomp/sidechain", &["off", "1", "2"], (1230.0, 425.0));
    ui::silk(b, panel, "sidechain", at(1230.0, 0.0).0, at(0.0, 511.0).1, words);
    ui::silk(b, panel, "Bus Select", at(1230.0, 0.0).0, at(0.0, 531.0).1, caption);

    // ── The meter, the jewel and power ───────────────────────────────
    let (mx, my) = at(1551.0, 180.0);
    let cw = 150.0;
    let (fw, fh) = (cw + 12.0, cw * 0.58 + 12.0);
    let frame = draw::rounded(draw::rect("meter frame", mx - fw / 2.0, my - fh / 2.0, fw, fh, draw::linear(180.0, &[(0.0, "#1f2222"), (1.0, "#101212")])), 2.0);
    b.add(panel, draw::with_effects(draw::stroked(frame, "#000000aa", 1.0), vec![draw::drop_shadow("#00000066", 0.0, 1.5, 3.0), draw::inner_shadow("#ffffff1f", 0.0, 1.0, 0.0)]));
    let vu = Place::new("Gain Reduction meter").size(cw).variant("open").legend("VU").options(&["glass"]).bind(ui::stream("optocomp/gr")).select(SourceSelect::new(
        "optocomp/meter",
        vec![Some(MeterTap::new("optocomp/in")), Some(MeterTap::new("optocomp/gr")), Some(MeterTap::new("optocomp/out"))],
    ));
    ui::centred_at(b, panel, "meter.vu_amber", &vu, mx, my);
    let ch = cw * 0.58;
    b.add(panel, draw::rounded(draw::rect("card tint", mx - cw / 2.0, my - ch / 2.0, cw, ch, draw::solid("#f3e2b833")), 3.0));

    ui::centred_at(b, panel, "button.jewel", &Place::new("Pilot").size(34.0).variant("red").bind(ui::bind("optocomp/power")), at(1450.0, 428.0).0, at(0.0, 428.0).1);
    let c = at(1650.0, 428.0);
    let power = ui::rotary_at(b, panel, "knob.bar_pointer", &Place::new("Power").size(40.0).legend(" ").options(&["Off", "On"]).sweep(40.0).touch(surface_kit::TOUCH_MIN).bind(ui::bind("optocomp/power")), c.0, c.1);
    ui::hide_ring(b, power);
    ui::silk(b, panel, "Off", at(1652.0, 0.0).0, at(0.0, 356.0).1, caption);
    ui::silk(b, panel, "On", at(1708.0, 0.0).0, at(0.0, 377.0).1, caption);

    ui::fit_root(b, root, panel);
}
