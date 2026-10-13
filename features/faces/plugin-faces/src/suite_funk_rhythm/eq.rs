//! `10-eq-desktop-1140x282`: the *Nine-Band Desk EQ*, a desktop graphic
//! EQ as wide as a rack face (1140 × 282, the reference's 4:1) in a
//! powder-coated case in three colourways (mauve, slate or blue:
//! `eq/color`, each a layer shown for its option) — big taupe dome knobs
//! for the high-pass and low-pass filters (their settings printed live
//! above; both open, as the reference shows them), nine lit white faders
//! from 65 Hz to 16 kHz with their gain printed above (set in the
//! reference's −12 / 0 / +12 pattern), a cream POWER button and its pilot
//! LED; three swatch keys at the bottom right pick the colourway.

use document_proto::{NodeId, TextAlign};
use surface_kit::materials::{Material, fill};
use surface_kit::{Place, TOUCH_MIN, draw};
use surface_proto::{ButtonMode, Control, ControlKind, Range, SlotRole, Unit, ValueFormat};
use surface_widgets::Builder;

use super::parts::{self, KnobLook, RACK_W, Type};
use super::pedals::PLUGIN;
use crate::face::{Face, canvas};
use crate::reverb::ui;

const BANDS: &[(&str, &str, f64)] = &[
    ("b65", "65 Hz", 272.0),
    ("b125", "125 Hz", 347.0),
    ("b250", "250 Hz", 420.0),
    ("b500", "500 Hz", 495.0),
    ("b1k", "1 kHz", 569.0),
    ("b2k", "2 kHz", 642.0),
    ("b4k", "4 kHz", 718.0),
    ("b8k", "8 kHz", 792.0),
    ("b16k", "16 kHz", 865.0),
];
/// The colourways: name and case paint.
const COLOURS: [(&str, &str); 3] = [("Mauve", "#9c8a8d"), ("Slate", "#3f4b53"), ("Blue", "#56799a")];
const INK: &str = "#f4efec";
/// The filters' ranges (Hz): each is off at its outer stop.
const HPF: (f64, f64) = (20.0, 1000.0);
const LPF: (f64, f64) = (1000.0, 20000.0);

/// A live value line: `prefix` printed, then the param's value in `format`.
fn value_line(b: &mut Builder<'_>, parent: NodeId, addr: &str, prefix: &str, (cx, cy): (f64, f64), w: f64, format: ValueFormat) {
    let t = Type::new(10.5, INK, 800.0).track(0.06);
    let r = if prefix.is_empty() {
        ui::lit_readout(b, parent, &format!("{addr} readout"), ui::bind(addr), cx - w / 2.0, cy, w, 10.5, INK, TextAlign::Center)
    } else {
        parts::words(b, parent, prefix, cx - w / 2.0, cy, w / 2.0 - 2.0, TextAlign::Right, t);
        ui::lit_readout(b, parent, &format!("{addr} readout"), ui::bind(addr), cx + 2.0, cy, w / 2.0, 10.5, INK, TextAlign::Left)
    };
    parts::format(b, r, format);
}

/// The filter's slope drawn over its knob.
fn slope(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), high: bool) {
    let pts = if high { vec![(cx - 9.0, cy + 4.0), (cx - 4.0, cy - 2.0), (cx + 9.0, cy - 2.0)] } else { vec![(cx - 9.0, cy - 2.0), (cx + 4.0, cy - 2.0), (cx + 9.0, cy + 4.0)] };
    parts::art(b, parent, &[pts], 2.0, INK);
}

/// The case in one colourway: powder coat, lit edges, a slot rebate round
/// each fader.
fn case(b: &mut Builder<'_>, layer: NodeId, (w, h): (f64, f64), color: &str) {
    let mut shell = draw::rounded(draw::frame("case", 0.0, 0.0, w, h), 12.0);
    shell.effects = vec![draw::drop_shadow("#00000080", 0.0, 6.0, 12.0)];
    let shell = b.add(layer, shell);
    fill(b, shell, (0.0, 0.0, w, h), 12.0, &Material::Powder { color: color.into(), coarse: false });
    for n in surface_kit::materials::light::edge("case edge", (0.0, 0.0, w, h), 12.0, 8.0, 1.0, true) {
        b.add(shell, n);
    }
    // The case's front lip, darker along the bottom.
    b.add(shell, draw::rect("lip", 0.0, h - 14.0, w, 14.0, draw::linear(180.0, &[(0.0, "#00000000"), (1.0, "#00000040")])));
}

/// The colourway keys: a swatch per colourway centred on (`cx`, `cy`),
/// the chosen one ringed.
fn colour_keys(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64)) {
    let n = COLOURS.len();
    for (i, (name, color)) in COLOURS.iter().enumerate() {
        let x = cx + 18.0 * (i as f64 - (n as f64 - 1.0) / 2.0);
        let key = b.add(parent, draw::frame(&format!("Colour {name}"), x - 8.0, cy - 8.0, 16.0, 16.0));
        b.add(key, draw::stroked(draw::circle("swatch", 8.0, 8.0, 4.6, draw::solid(color)), "#00000099", 0.8));
        let ring = b.add(key, draw::stroked(draw::circle("chosen", 8.0, 8.0, 6.6, draw::solid("#00000000")), INK, 1.1));
        let mut c = Control::new(ControlKind::Button { mode: ButtonMode::Select { index: i as u32, count: n as u32 } }).slot(SlotRole::Led, ring.0);
        c.binding = Some(ui::bind("eq/color"));
        b.controls.insert(key, c);
    }
}

pub(super) fn face() -> Face {
    let (w, h) = (RACK_W, 282.0);
    let scene = canvas("Nine-Band Desk EQ", |b, root| {
        parts::page(b, root, w, h);
        for (i, (_, color)) in COLOURS.iter().enumerate() {
            let layer = parts::variant_layer(b, root, "eq/color", i, (0.0, 0.0, w, h));
            case(b, layer, (w, h), color);
        }
        let case = b.add(root, draw::frame("controls", 0.0, 0.0, w, h));
        // Filters: big taupe domes.
        let look = KnobLook { cap: "domed", opts: &["color=#a8958e", "pointer=#1a1716", "index=dot", "index_w=0.07", "index_in=0.55", "index_out=0.55", "cap_r=0.94", "dome=0.24", "rough=0.3", "ticks=none"] };
        // The filters read OFF at their stops, as the reference prints them.
        for (addr, prefix, x, high, stop) in [("eq/hpf", "HPF :", 174.0, true, HPF.0), ("eq/lpf", "LPF :", 967.0, false, LPF.1)] {
            value_line(b, case, addr, prefix, (x, 36.0), 150.0, ValueFormat::default().mark(stop, "OFF"));
            slope(b, case, (x, 72.0), high);
            parts::knob3d(b, case, &look, addr, prefix, 88.0, (x, 139.0), INK);
        }
        // Bands.
        for (id, hz, x) in BANDS {
            let addr = format!("eq/{id}");
            // Gains signed, a flat band a plain 0 (the reference's).
            value_line(b, case, &addr, "", (*x, 36.0), 70.0, ValueFormat::decimals(1).signed().mark(0.0, "0 dB"));
            let p = Place::new(*hz).size(150.0).bind(ui::bind(&addr)).touch(TOUCH_MIN);
            let f = surface_kit::place(b, case, "lever.lit_fader", &p).unwrap_or_else(|e| panic!("{e}"));
            parts::centre_on(b, f, (*x, 141.0));
            parts::centred(b, case, hz, (*x, 251.0), Type::new(10.5, INK, 800.0).track(0.06));
        }
        // Power.
        let p = Place::new("Power").size(30.0).variant("cream").bind(ui::bind("eq/power")).options(&["nolamp"]).legend("").touch(TOUCH_MIN);
        let k = surface_kit::place(b, case, "button.round_cap", &p).unwrap_or_else(|e| panic!("{e}"));
        ui::strip_legend(b, k);
        parts::centre_on(b, k, (174.0, 229.0));
        parts::centred(b, case, "POWER", (174.0, 259.0), Type::new(8.5, INK, 800.0).track(0.2));
        parts::status_led(b, case, "eq/power", 7.0, (964.0, 232.0), "#eef6ff", "chrome", None);
        colour_keys(b, case, (1072.0, 256.0));
    });
    let mut params = vec![
        parts::pick("eq/color", "Colour", &COLOURS.map(|c| c.0), 0),
        parts::hz("eq/hpf", "HPF", HPF.0, HPF.1, HPF.0),
        parts::hz("eq/lpf", "LPF", LPF.0, LPF.1, LPF.1),
        parts::switch("eq/power", "Power", true),
    ];
    // The reference's bands: −12, 0, +12, repeated.
    for (i, (id, hz, _)) in BANDS.iter().enumerate() {
        let v = [-12.0, 0.0, 12.0][i % 3];
        params.push(ui::param(&format!("eq/{id}"), hz, Range::linear(-12.0, 12.0, Unit::Db), v).featured().bipolar());
    }
    ui::formatted(Face { file: "10-eq-desktop-1140x282", plugin: PLUGIN, scene, params, streams: Vec::new() })
}
