//! `09-eq-rack-1140x286`: the nine-band graphic EQ in its desk case (1140
//! wide, the case's own 4:1 aspect), in the three colourways the rig ships
//! (`eq/color`, each a layer shown for its option: blush-pink plastic,
//! brushed gold, or cream) — HPF and LPF on dark scalloped knobs with
//! brass lines (both open, as the reference shows them), a brass bypass
//! toggle, nine wide faders (deep rounded slots, dark domed caps with a
//! cream index) over tick scales with the gain printed above each and the
//! band below, and the teal pilot. HPF and LPF read OFF at their open
//! stops and the gains read signed; three swatches under the pilot pick
//! the colourway.

use frame::document::proto::{NodeId, TextAlign};
use frame::surface::kit::draw;
use frame::surface::kit::materials::{Dir, Material, fill};
use frame::surface::proto::{ButtonMode, Control, ControlKind, Orientation, SlotRole, ValueFormat};
use frame::surface::widgets::Builder;

use super::look::KnobLook;
use super::look::{self, TEAL};
use crate::face::{Face, canvas};
use crate::reverb::ui;
use crate::suite_funk_rhythm::parts::{self, RACK_W, Type};

const INK: &str = "#1e1719";
const BANDS: [(&str, &str); 9] = [
    ("65", "65 Hz"),
    ("125", "125 Hz"),
    ("250", "250 Hz"),
    ("500", "500 Hz"),
    ("1k", "1 kHz"),
    ("2k", "2 kHz"),
    ("4k", "4 kHz"),
    ("8k", "8 kHz"),
    ("16k", "16 kHz"),
];
/// The fader travel: top of the lane and its length.
const LANE: (f64, f64) = (58.0, 150.0);

/// A plain printed readout of `addr` (`Value` slot), centred at `at`.
pub(super) fn readout(
    b: &mut Builder<'_>,
    parent: NodeId,
    addr: &str,
    at: (f64, f64),
    w: f64,
    t: Type<'_>,
    align: TextAlign,
) -> NodeId {
    let h = (t.size * 1.6).ceil();
    let x = match align {
        TextAlign::Left => at.0,
        TextAlign::Right => at.0 - w,
        _ => at.0 - w / 2.0,
    };
    let root = b.add(
        parent,
        draw::frame(&format!("{addr} readout"), x, at.1 - h / 2.0, w, h),
    );
    let v = parts::words(b, root, "", 0.0, h / 2.0, w, align, t);
    let mut c = Control::new(ControlKind::Readout).slot(SlotRole::Value, v.0);
    c.binding = Some(ui::bind(addr));
    b.controls.insert(root, c);
    root
}

/// A wide EQ fader centred on `x`: a deep rounded slot between tick
/// scales and a dark domed cap with a cream index, the cap riding a lane
/// `LANE.1` long (a vertical `Fader` whose `Thumb` is the cap).
fn fader(b: &mut Builder<'_>, parent: NodeId, addr: &str, name: &str, x: f64) -> NodeId {
    let (top, travel) = LANE;
    let (cap_w, cap_h) = (34.0, 20.0);
    let w = 62.0;
    let root = b.add(
        parent,
        draw::frame(name, x - w / 2.0, top - cap_h / 2.0, w, travel + cap_h),
    );
    let c = w / 2.0;
    // Tick scales either side.
    let ticks: Vec<Vec<(f64, f64)>> = (0..=12)
        .flat_map(|i| {
            let y = cap_h / 2.0 + travel * f64::from(i) / 12.0;
            let len = if i % 6 == 0 { 9.0 } else { 6.0 };
            [
                vec![(c - 13.0 - len, y), (c - 13.0, y)],
                vec![(c + 13.0, y), (c + 13.0 + len, y)],
            ]
        })
        .collect();
    b.add(
        root,
        ui::strokes("ticks", &ticks, 0.9, draw::solid("#1e171966")),
    );
    let slot = draw::rounded(
        draw::rect(
            "slot",
            c - 7.0,
            cap_h / 2.0 - 6.0,
            14.0,
            travel + 12.0,
            draw::linear(
                90.0,
                &[(0.0, "#050405"), (0.5, "#221b1d"), (1.0, "#0a0809")],
            ),
        ),
        7.0,
    );
    b.add(
        root,
        draw::with_effects(slot, vec![draw::drop_shadow("#ffffff80", 0.0, 1.0, 0.0)]),
    );
    let lane = b.add(
        root,
        draw::frame("lane", c - cap_w / 2.0, 0.0, cap_w, travel + cap_h),
    );
    let cap = b.add(lane, draw::frame("cap", 0.0, 0.0, cap_w, cap_h));
    let body = draw::rounded(
        draw::rect(
            "cap body",
            0.0,
            0.0,
            cap_w,
            cap_h,
            draw::linear(
                180.0,
                &[(0.0, "#5a5456"), (0.35, "#2a2426"), (1.0, "#0d0a0b")],
            ),
        ),
        cap_h / 2.0,
    );
    b.add(
        cap,
        draw::with_effects(body, vec![draw::drop_shadow("#000000a0", 1.0, 4.0, 5.0)]),
    );
    b.add(
        cap,
        draw::rounded(
            draw::rect(
                "cap sheen",
                4.0,
                2.0,
                cap_w - 8.0,
                cap_h * 0.35,
                draw::solid("#ffffff24"),
            ),
            cap_h * 0.2,
        ),
    );
    b.add(
        cap,
        draw::rounded(
            draw::rect(
                "index",
                5.0,
                cap_h / 2.0 - 1.2,
                cap_w - 10.0,
                2.4,
                draw::solid("#efe0c4"),
            ),
            1.2,
        ),
    );
    let mut ctl = Control::new(ControlKind::Fader {
        orientation: Orientation::Vertical,
    })
    .slot(SlotRole::Thumb, cap.0);
    ctl.binding = Some(ui::bind(addr));
    b.controls.insert(root, ctl);
    root
}

/// The filters' ranges: each reads OFF at its open stop.
const HPF: (f64, f64) = (20.0, 500.0);
const LPF: (f64, f64) = (2000.0, 20000.0);

/// The colourways, in `eq/color`'s order: name and swatch.
const COLOURWAYS: [(&str, &str); 3] = [
    ("Blush", "#eec7ca"),
    ("Gold", "#c79a4e"),
    ("Cream", "#f0ece5"),
];

/// The on-face colourway selector: a row of three swatch buttons centred
/// on `(cx, cy)`, each a `Select` over `eq/color`, the chosen one ringed
/// in brass.
fn colour_selector(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64)) {
    let (r, pitch) = (4.5, 15.0);
    let n = COLOURWAYS.len();
    for (i, (name, swatch)) in COLOURWAYS.iter().enumerate() {
        let x = cx + (i as f64 - (n - 1) as f64 / 2.0) * pitch;
        let s = pitch;
        let root = b.add(
            parent,
            draw::frame(&format!("Colour {name}"), x - s / 2.0, cy - s / 2.0, s, s),
        );
        let dot = draw::with_effects(
            draw::circle("swatch", s / 2.0, s / 2.0, r, draw::solid(swatch)),
            vec![draw::drop_shadow("#00000070", 0.0, 0.8, 1.5)],
        );
        b.add(root, draw::stroked(dot, "#1e171955", 0.6));
        let ring = b.add(
            root,
            draw::stroked(
                draw::circle(
                    "chosen",
                    s / 2.0,
                    s / 2.0,
                    r + 2.2,
                    draw::solid("#00000000"),
                ),
                "#b8893c",
                1.4,
            ),
        );
        let mut c = Control::new(ControlKind::Button {
            mode: ButtonMode::Select {
                index: i as u32,
                count: n as u32,
            },
        })
        .slot(SlotRole::Led, ring.0);
        c.binding = Some(ui::bind("eq/color"));
        b.controls.insert(root, c);
    }
}

/// The case in colourway `i`: blush paint, brushed gold, cream paint.
fn case(b: &mut Builder<'_>, layer: NodeId, (w, h): (f64, f64), i: usize) {
    let mut shell = draw::rounded(draw::frame("Case", 0.0, 0.0, w, h), 16.0);
    shell.effects = vec![draw::drop_shadow("#00000066", 0.0, 8.0, 16.0)];
    let shell = b.add(layer, shell);
    let m = match i {
        0 => Material::Powder {
            color: "#eec7ca".into(),
            coarse: false,
        },
        1 => Material::Brushed {
            color: "#c79a4e".into(),
            dir: Dir::Horizontal,
        },
        _ => Material::Powder {
            color: "#f0ece5".into(),
            coarse: false,
        },
    };
    fill(b, shell, (0.0, 0.0, w, h), 16.0, &m);
    for n in frame::surface::kit::materials::light::edge(
        "case edge",
        (0.0, 0.0, w, h),
        16.0,
        10.0,
        1.0,
        true,
    ) {
        b.add(shell, n);
    }
}

pub(super) fn face() -> Face {
    let (w, h) = (RACK_W, 286.0);
    let scene = canvas("Graphic EQ", |b, root| {
        parts::page(b, root, w, h);
        for x in [120.0, w - 170.0] {
            b.add(
                root,
                draw::rounded(
                    draw::rect("foot", x, h - 14.0, 50.0, 14.0, draw::solid("#1b1b1d")),
                    4.0,
                ),
            );
        }
        for i in 0..3 {
            let layer = parts::variant_layer(b, root, "eq/color", i, (0.0, 0.0, w, h - 12.0));
            case(b, layer, (w, h - 12.0), i);
        }
        let case = b.add(root, draw::frame("Controls", 0.0, 0.0, w, h - 12.0));
        let lab = Type::new(10.5, INK, 700.0);
        let knob = KnobLook {
            preset: "scalloped",
            opts: &[
                "color=#1e1413",
                "pointer=#dcaa4c",
                "ribs=9",
                "rib_depth=0.13",
                "coat=0.85",
                "rough=0.14",
                "height=0.56",
                "index_in=0.05",
                "index_out=0.72",
                "index_w=0.07",
                "ticks=none",
                "fallback=fluted",
            ],
        };
        // HPF.
        parts::words(b, case, "HPF :", 100.0, 35.0, 44.0, TextAlign::Right, lab);
        let hpf = readout(b, case, "eq/hpf", (148.0, 35.0), 60.0, lab, TextAlign::Left);
        look::set_format(b, hpf, ValueFormat::default().mark(HPF.0, "OFF"));
        look::place_knob(b, case, &knob, "eq/hpf", "HPF", 62.0, (153.0, 116.0), INK);
        parts::art(
            b,
            case,
            &[vec![(142.0, 176.0), (146.0, 169.0), (158.0, 169.0)]],
            1.8,
            INK,
        );
        look::brass_toggle(b, case, "eq/on", "Bypass", 20.0, (150.0, 218.0));
        // LPF.
        parts::words(b, case, "LPF :", 944.0, 35.0, 44.0, TextAlign::Right, lab);
        let lpf = readout(b, case, "eq/lpf", (992.0, 35.0), 70.0, lab, TextAlign::Left);
        look::set_format(b, lpf, ValueFormat::default().mark(LPF.1, "OFF"));
        look::place_knob(b, case, &knob, "eq/lpf", "LPF", 62.0, (990.0, 116.0), INK);
        parts::art(
            b,
            case,
            &[vec![(980.0, 169.0), (992.0, 169.0), (996.0, 176.0)]],
            1.8,
            INK,
        );
        parts::status_led(b, case, "eq/on", 12.0, (990.0, 218.0), TEAL, "light", None);
        // The nine faders.
        for (i, (id, band)) in BANDS.iter().enumerate() {
            let x = 262.0 + 76.4 * i as f64;
            let addr = format!("eq/g{id}");
            fader(b, case, &addr, band, x);
            let gain = readout(b, case, &addr, (x, 35.0), 70.0, lab, TextAlign::Center);
            look::set_format(b, gain, ValueFormat::decimals(1).signed());
            parts::centred(b, case, band, (x, 238.0), lab);
        }
        colour_selector(b, case, (990.0, 252.0));
    });
    let mut params = vec![
        parts::pick("eq/color", "Colour", &COLOURWAYS.map(|(n, _)| n), 0),
        parts::hz("eq/hpf", "HPF", HPF.0, HPF.1, HPF.0),
        parts::hz("eq/lpf", "LPF", LPF.0, LPF.1, LPF.1),
        parts::switch("eq/on", "Bypass", true),
    ];
    // The reference's faders: top, middle, bottom, repeated.
    for (i, (id, band)) in BANDS.iter().enumerate() {
        params.push(parts::db(
            &format!("eq/g{id}"),
            band,
            -12.0,
            12.0,
            [12.0, 0.0, -12.0][i % 3],
        ));
    }
    look::face("09-eq-rack-1140x286", scene, params)
}
