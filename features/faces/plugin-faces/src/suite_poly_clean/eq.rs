//! `08-eq-nine-band-tabletop` — a nine-band graphic EQ in a tabletop case,
//! 1100 × 316 design px (a 3.9:1 case on two feet): a big knurled
//! high-pass knob and its power lever at the left, nine faders with
//! glowing caps (65 Hz … 16 kHz, ±12 dB, each with its readout above), a
//! big low-pass knob and the white pilot at the right.
//!
//! The unit comes in the rig's three colourways, one per amp — **Pink**,
//! **White** and **Black** — a choice param (`eq/finish`) picked by the
//! three swatch keys on the case's front edge. Each colourway is a layer
//! (the case, its knobs, its print) shown while the param picks it; the
//! faders, the lever and the pilot are the same on all three.

use frame::document::proto::{CornerRadii, NodeId, TextAlign};
use frame::surface::kit::{Place, draw};
use frame::surface::proto::{
    ButtonMode, Condition, Control, ControlKind, ParamInfo, Range, SlotRole, Unit, ValueFormat,
};
use frame::surface::widgets::Builder;

use super::PLUGIN;
use super::parts::{self, Cap, knob};
use crate::face::{Face, canvas};
use crate::reverb::ui;

const W: f64 = 1100.0;
const H: f64 = 316.0;
const BODY_H: f64 = 284.0;
const NS: &str = "eq";
const BANDS: [(&str, &str); 9] = [
    ("65", "65Hz"),
    ("125", "125Hz"),
    ("250", "250Hz"),
    ("500", "500Hz"),
    ("1k", "1kHz"),
    ("2k", "2kHz"),
    ("4k", "4kHz"),
    ("8k", "8kHz"),
    ("16k", "16kHz"),
];
const XS: [f64; 9] = [
    256.0, 330.0, 403.0, 477.0, 550.0, 624.0, 697.0, 771.0, 845.0,
];
/// The high-pass and low-pass ranges: each prints OFF at its open end.
const HPF: (f64, f64) = (20.0, 1000.0);
const LPF: (f64, f64) = (1000.0, 20_000.0);

/// A colourway: name, the case's paint (top edge, top, face, bottom), its
/// lip (top, bottom), the print, and the knobs.
struct Finish {
    name: &'static str,
    case: [&'static str; 4],
    lip: [&'static str; 2],
    shade: &'static str,
    ink: &'static str,
    knob: Cap,
}

const FINISHES: [Finish; 3] = [
    Finish {
        name: "Pink",
        case: ["#dea3b6", "#d596ab", "#c98ba0", "#b87a8f"],
        lip: ["#a86a80", "#8d5569"],
        shade: "#6a334855",
        ink: "#4a2a35",
        knob: parts::EQ_PINK,
    },
    Finish {
        name: "White",
        case: ["#f7f7f6", "#eeeeec", "#e3e3e1", "#d2d2d0"],
        lip: ["#c4c4c2", "#a9a9a7"],
        shade: "#4a4a4a40",
        ink: "#3c3c3e",
        knob: parts::EQ_WHITE,
    },
    Finish {
        name: "Black",
        case: ["#3a3a3d", "#2c2c2f", "#222224", "#18181a"],
        lip: ["#141416", "#0b0b0c"],
        shade: "#00000080",
        ink: "#e9e9ea",
        knob: parts::EQ_BLACK,
    },
];

fn finish_addr() -> String {
    format!("{NS}/finish")
}

/// A frame of `parent` covering the case, drawn only while the finish is
/// `i`: a readout of the finish param, shown by condition.
fn layer(b: &mut Builder<'_>, parent: NodeId, i: usize) -> NodeId {
    let id = b.add(
        parent,
        draw::frame(&format!("{} finish", FINISHES[i].name), 0.0, 0.0, W, BODY_H),
    );
    let mut t = draw::text(
        "value",
        "",
        0.0,
        0.0,
        40.0,
        4.0,
        "#00000000",
        TextAlign::Left,
    );
    t.opacity = 0.0;
    let value = b.add(id, t);
    let c = Control::new(ControlKind::Readout)
        .slot(SlotRole::Value, value.0)
        .bound(ui::bind(&finish_addr()))
        .shown_if(Condition::any_of(finish_addr(), &[i as f64]));
    b.controls.insert(id, c);
    id
}

/// A plain live value (a `Readout`'s `Value` text) centred on (`cx`, `cy`).
#[allow(clippy::too_many_arguments)]
fn value(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    addr: &str,
    cx: f64,
    cy: f64,
    w: f64,
    size: f64,
    ink: &str,
    format: ValueFormat,
) -> NodeId {
    let root = b.add(
        parent,
        draw::frame(name, cx - w / 2.0, cy - size, w, size * 2.0),
    );
    let t = b.add(
        root,
        draw::lettered(
            ui::text(
                "value",
                "",
                0.0,
                size,
                w,
                size,
                ink,
                TextAlign::Center,
                500.0,
                false,
            ),
            &parts::Hand::Ui.lettering(500.0),
        ),
    );
    let mut c = Control::new(ControlKind::Readout).slot(SlotRole::Value, t.0);
    c.binding = Some(ui::bind(addr));
    c.format = Some(format);
    b.controls.insert(root, c);
    root
}

/// The power lever: a black capsule, its chrome ball at the right when on.
fn lever(b: &mut Builder<'_>, parent: NodeId, addr: &str, cx: f64, cy: f64) {
    let (w, h) = (48.0, 44.0);
    let root = b.add(
        parent,
        draw::frame("Power", cx - w / 2.0, cy - h / 2.0, w, h),
    );
    let cap = draw::rounded(
        draw::rect(
            "capsule",
            6.0,
            h / 2.0 - 8.0,
            w - 12.0,
            16.0,
            draw::linear(
                180.0,
                &[(0.0, "#2a2a2c"), (0.5, "#0b0b0c"), (1.0, "#1d1d1f")],
            ),
        ),
        8.0,
    );
    b.add(
        root,
        draw::with_effects(
            cap,
            vec![
                draw::drop_shadow("#00000080", 0.0, 2.0, 3.0),
                draw::inner_shadow("#ffffff26", 0.0, 1.0, 0.5),
            ],
        ),
    );
    b.add(
        root,
        draw::stroked(
            draw::circle("ring", 16.0, h / 2.0, 4.5, draw::solid("#00000000")),
            "#6a6a6c",
            1.2,
        ),
    );
    let mut poses = Vec::new();
    for (name, x) in [("off", 14.0), ("on", w - 12.0)] {
        let pose = b.add(root, draw::frame(name, 0.0, 0.0, w, h));
        b.add(
            pose,
            draw::circle(
                "ball",
                x,
                h / 2.0,
                5.5,
                draw::radial_at(
                    0.35,
                    0.3,
                    0.85,
                    &[(0.0, "#ffffff"), (0.6, "#b4b5b7"), (1.0, "#4a4b4d")],
                ),
            ),
        );
        poses.push(pose);
    }
    let mut c = Control::new(ControlKind::Button {
        mode: ButtonMode::Toggle,
    })
    .slot(SlotRole::Off, poses[0].0)
    .slot(SlotRole::On, poses[1].0);
    c.binding = Some(ui::bind(addr));
    b.controls.insert(root, c);
}

/// The white pilot that glows while the EQ is on.
fn pilot(b: &mut Builder<'_>, parent: NodeId, addr: &str, cx: f64, cy: f64) {
    let root = b.add(
        parent,
        draw::frame("Pilot", cx - 20.0, cy - 8.0, 40.0, 16.0),
    );
    b.add(
        root,
        draw::rounded(
            draw::rect("well", 2.0, 3.0, 36.0, 10.0, draw::solid("#00000040")),
            5.0,
        ),
    );
    let lens = draw::rounded(
        draw::rect(
            "lens",
            3.0,
            3.5,
            34.0,
            9.0,
            draw::linear(
                180.0,
                &[(0.0, "#ffffff"), (0.5, "#dfe3ea"), (1.0, "#ffffff")],
            ),
        ),
        4.5,
    );
    let lens = draw::with_effects(lens, vec![draw::drop_shadow("#ffffffb0", 0.0, 0.0, 5.0)]);
    let lens = b.add(root, draw::emissive(lens, draw::hex("#f2f4ff"), 0.5, 8.0));
    let mut c = Control::new(ControlKind::Readout).slot(SlotRole::Glow, lens.0);
    c.binding = Some(ui::bind(addr));
    b.controls.insert(root, c);
}

/// One colourway: the case in its paint, the filter knobs, and the print
/// (readouts, knees, band labels) in its ink.
fn skin(b: &mut Builder<'_>, body: NodeId, i: usize) {
    let f = &FINISHES[i];
    let l = layer(b, body, i);
    let [edge, top, face, bottom] = f.case;
    let mut case = draw::rect(
        "case",
        0.0,
        0.0,
        W,
        BODY_H,
        draw::linear(
            180.0,
            &[(0.0, edge), (0.08, top), (0.6, face), (1.0, bottom)],
        ),
    );
    case.fills.push(draw::radial_at(
        0.5,
        0.35,
        0.7,
        &[(0.0, "#ffffff14"), (1.0, "#ffffff00")],
    ));
    case.corner_radii = CornerRadii {
        top_left: 10.0,
        top_right: 10.0,
        bottom_left: 4.0,
        bottom_right: 4.0,
    };
    case.effects = vec![
        draw::inner_shadow("#ffffff66", 0.0, 2.0, 1.5),
        draw::inner_shadow(f.shade, 0.0, -3.0, 4.0),
    ];
    b.add(l, case);
    b.add(
        l,
        draw::rect(
            "bottom lip",
            0.0,
            BODY_H - 6.0,
            W,
            6.0,
            draw::linear(180.0, &[(0.0, f.lip[0]), (1.0, f.lip[1])]),
        ),
    );
    // High-pass: its readout (OFF at the bottom of its travel), the knee,
    // the big knob; low-pass the same, OFF at the top.
    value(
        b,
        l,
        "HPF readout",
        "eq/hpf",
        146.0,
        32.0,
        90.0,
        10.0,
        f.ink,
        ValueFormat::decimals(0).mark(HPF.0, "OFF"),
    );
    parts::cut_glyph(b, l, 146.0, 60.0, 26.0, true, f.ink);
    knob(
        b,
        l,
        &f.knob,
        "eq/hpf",
        "High Pass",
        98.0,
        146.0,
        139.0,
        f.ink,
    );
    value(
        b,
        l,
        "LPF readout",
        "eq/lpf",
        951.0,
        32.0,
        90.0,
        10.0,
        f.ink,
        ValueFormat::default().mark(LPF.1, "OFF"),
    );
    parts::cut_glyph(b, l, 951.0, 60.0, 26.0, false, f.ink);
    knob(
        b, l, &f.knob, "eq/lpf", "Low Pass", 98.0, 951.0, 139.0, f.ink,
    );
    for ((id, label), x) in BANDS.iter().zip(XS) {
        value(
            b,
            l,
            &format!("{label} readout"),
            &format!("eq/g{id}"),
            x,
            32.0,
            64.0,
            10.0,
            f.ink,
            ValueFormat::decimals(1).unit("db").signed(),
        );
        parts::print(b, l, label, x, 247.0, 10.0, f.ink, 500.0, 0.08);
    }
}

/// The three colourway keys on the case's front edge: swatches, the
/// chosen one ringed.
fn finish_keys(b: &mut Builder<'_>, body: NodeId) {
    let n = FINISHES.len();
    for (i, f) in FINISHES.iter().enumerate() {
        let cx = W / 2.0 + 18.0 * (i as f64 - 1.0);
        let key = b.add(
            body,
            draw::frame(&format!("Finish {}", f.name), cx - 8.0, 259.0, 16.0, 16.0),
        );
        let swatch = draw::circle(
            "swatch",
            8.0,
            8.0,
            4.5,
            draw::linear(180.0, &[(0.0, f.case[0]), (1.0, f.case[2])]),
        );
        b.add(key, draw::stroked(swatch, "#00000080", 0.8));
        let ring = b.add(
            key,
            draw::stroked(
                draw::circle("chosen", 8.0, 8.0, 6.6, draw::solid("#00000000")),
                "#f4f4f4",
                1.1,
            ),
        );
        let mut c = Control::new(ControlKind::Button {
            mode: ButtonMode::Select {
                index: i as u32,
                count: n as u32,
            },
        })
        .slot(SlotRole::Led, ring.0);
        c.binding = Some(ui::bind(&finish_addr()));
        b.controls.insert(key, c);
    }
}

pub fn face() -> Face {
    let scene = canvas("Nine Band EQ (tabletop, 1100×316)", |b, root| {
        ui::fit_root(b, root, W, H);
        // Feet, then the case: a box, its top face catching the light, in
        // the chosen colourway.
        for fx in [0.1, 0.9] {
            let foot = draw::rounded(
                draw::rect(
                    "foot",
                    W * fx - 36.0,
                    BODY_H - 10.0,
                    72.0,
                    H - BODY_H + 10.0,
                    draw::linear(
                        90.0,
                        &[(0.0, "#050505"), (0.4, "#2b2b2d"), (1.0, "#070707")],
                    ),
                ),
                6.0,
            );
            b.add(root, foot);
        }
        let mut body = draw::frame("Case", 0.0, 0.0, W, BODY_H);
        body.corner_radii = CornerRadii {
            top_left: 10.0,
            top_right: 10.0,
            bottom_left: 4.0,
            bottom_right: 4.0,
        };
        body.effects = vec![draw::drop_shadow("#000000a0", 0.0, 10.0, 16.0)];
        body.fills = vec![draw::solid("#1a1a1c")];
        let body = b.add(root, body);
        for i in 0..FINISHES.len() {
            skin(b, body, i);
        }
        lever(b, body, "eq/on", 146.0, 230.0);
        pilot(b, body, "eq/on", 951.0, 230.0);
        for ((id, label), x) in BANDS.iter().zip(XS) {
            let p = Place::new(format!("{label} gain"))
                .size(145.0)
                .bind(ui::bind(&format!("eq/g{id}")));
            let f = frame::surface::kit::place(b, body, "lever.glow_cap", &p)
                .unwrap_or_else(|e| panic!("{e}"));
            let fw = b.scene.node(f).map_or(0.0, |n| n.size.width);
            ui::move_to(b, f, x - fw / 2.0, 44.0);
        }
        finish_keys(b, body);
    });
    let mut params: Vec<ParamInfo> = BANDS
        .iter()
        .map(|(id, label)| {
            ui::param(
                &format!("{NS}/g{id}"),
                &format!("{label} Gain"),
                Range::linear(-12.0, 12.0, Unit::Db),
                0.0,
            )
            .bipolar()
        })
        .collect();
    params[0] = params[0].clone().featured();
    params.push(parts::hz(NS, "hpf", "High Pass", HPF.0, HPF.1, HPF.0));
    params.push(parts::hz(NS, "lpf", "Low Pass", LPF.0, LPF.1, LPF.1));
    params.push(parts::on(NS, "on", "Power", true).featured());
    params.push(parts::pick(
        NS,
        "finish",
        "Finish",
        &FINISHES.map(|f| f.name),
        0,
    ));
    Face {
        file: "08-eq-nine-band-tabletop",
        plugin: PLUGIN,
        scene,
        params,
        streams: vec![],
    }
}
