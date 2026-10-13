//! The 4×12 — Cab form, 900 × 900: a closed-back straight cab in black
//! leather, corner protectors, a two-tone baffle (lighter top half), four
//! twelves with ribbed paper cones, and the suite's cab controls:
//!
//! - two mics, A on the top-left speaker and B on the top-right, each a
//!   handle you drag across the cone — sideways is POSITION (cap → edge),
//!   down is DISTANCE (on the grille → back);
//! - a strip over the cab's foot (the featured-controls overlay): per mic
//!   the model, on/off, LEVEL, PAN and PHASE; ROOM on/off and its LEVEL in
//!   the middle.

use document_proto::{NodeId, TextAlign};
use surface_kit::{Place, TOUCH_MIN, draw};
use surface_proto::{Axis, ButtonMode, Control, ControlKind, HandleAxis, SlotRole, ValueFormat};
use surface_widgets::Builder;

use super::PLUGIN;
use super::art;
use crate::delay::parts::{self, Ink};
use crate::face::{Face, canvas};
use crate::rigkit::{self as rk, KnobStyle};

const S: f64 = 900.0;
const MICS: &[&str] = &[
    "Dynamic 1",
    "Dynamic 1 Off-Axis",
    "Dynamic 2",
    "Condenser 1",
    "Condenser 2",
    "Ribbon 1",
    "Ribbon 2",
];
const TEXT: &str = "#d6d6d4";
const LIT: &str = "#5dff8a";

fn a(s: &str) -> String {
    format!("bgcab/{s}")
}

pub fn face() -> Face {
    let scene = canvas("4x12 Cabinet", |b, root| {
        parts::fit_root(b, root, S, S);
        cabinet(b, root);
        let speakers = [
            (264.0, 256.0),
            (633.0, 256.0),
            (264.0, 637.0),
            (633.0, 637.0),
        ];
        for (i, (cx, cy)) in speakers.iter().enumerate() {
            speaker(b, root, (*cx, *cy), 147.0, i);
        }
        mic_handle(b, root, "Mic A", "a", speakers[0], 147.0);
        mic_handle(b, root, "Mic B", "b", speakers[1], 147.0);
        strip(b, root);
    });
    let mut params = Vec::new();
    for (m, on, model) in [("a", true, 0usize), ("b", true, 5)] {
        params.push(rk::toggle(
            &a(&format!("mic_{m}_on")),
            &format!("Mic {} On", m.to_uppercase()),
            on,
        ));
        params.push(rk::choice(
            &a(&format!("mic_{m}_model")),
            &format!("Mic {} Model", m.to_uppercase()),
            MICS,
            model,
        ));
        params.push(
            crate::delay::parts::param(
                &a(&format!("mic_{m}_position")),
                &format!("Mic {} Position", m.to_uppercase()),
                surface_proto::Range::linear(0.0, 1.0, surface_proto::Unit::None),
                0.5,
            )
            .featured(),
        );
        params.push(crate::delay::parts::param(
            &a(&format!("mic_{m}_distance")),
            &format!("Mic {} Distance", m.to_uppercase()),
            surface_proto::Range::linear(0.0, 1.0, surface_proto::Unit::None),
            0.0,
        ));
        params.push(
            rk::db(
                &a(&format!("mic_{m}_level")),
                &format!("Mic {} Level", m.to_uppercase()),
                -36.0,
                12.0,
                0.0,
            )
            .featured(),
        );
        params.push(
            crate::delay::parts::param(
                &a(&format!("mic_{m}_pan")),
                &format!("Mic {} Pan", m.to_uppercase()),
                surface_proto::Range::linear(-100.0, 100.0, surface_proto::Unit::Percent),
                0.0,
            )
            .bipolar(),
        );
        params.push(rk::toggle(
            &a(&format!("mic_{m}_phase")),
            &format!("Mic {} Phase", m.to_uppercase()),
            false,
        ));
    }
    params.push(rk::toggle(&a("room_on"), "Room Send", false));
    params.push(rk::db(&a("room_level"), "Room Level", -36.0, 12.0, 0.0));
    Face {
        file: "05-cab-4x12",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

/// The box: tolex, corner protectors, the frame round the baffle, the
/// two-tone baffle.
fn cabinet(b: &mut Builder<'_>, root: NodeId) {
    b.add(
        root,
        draw::with_effects(
            draw::rounded(
                draw::rect(
                    "cab shadow",
                    10.0,
                    14.0,
                    S - 20.0,
                    S - 16.0,
                    draw::solid("#000000"),
                ),
                16.0,
            ),
            vec![draw::drop_shadow("#000000c0", 0.0, 8.0, 16.0)],
        ),
    );
    let body = art::tolex(
        b,
        root,
        "Cabinet",
        (12.0, 4.0, S - 24.0, S - 10.0),
        14.0,
        0x4c12_cab5,
    );
    let (bw, bh) = (S - 24.0, S - 10.0);
    let edge = draw::stroked(
        draw::rounded(draw::frame("edge", 0.0, 0.0, bw, bh), 14.0),
        "#050505",
        3.0,
    );
    b.add(
        body,
        draw::with_effects(
            edge,
            vec![
                draw::inner_shadow("#000000e0", 0.0, 0.0, 12.0),
                draw::inner_shadow("#ffffff22", 0.0, 2.0, 1.0),
            ],
        ),
    );
    // The baffle frame (a strip of grey-stained wood) and the two halves.
    let (fx, fy, fw, fh) = (50.0, 54.0, bw - 100.0, bh - 110.0);
    let frame = draw::rect(
        "baffle frame",
        fx,
        fy,
        fw,
        fh,
        draw::linear(
            90.0,
            &[(0.0, "#2e2e2e"), (0.5, "#3a3a3a"), (1.0, "#2a2a2a")],
        ),
    );
    b.add(
        body,
        draw::with_effects(frame, vec![draw::inner_shadow("#000000", 0.0, 2.0, 4.0)]),
    );
    let half = (fh - 30.0) / 2.0;
    for (i, (y, c0, c1)) in [
        (fy + 10.0, "#383838", "#2c2c2c"),
        (fy + 20.0 + half, "#1e1e1e", "#171717"),
    ]
    .iter()
    .enumerate()
    {
        let panel = draw::rect(
            if i == 0 {
                "baffle top"
            } else {
                "baffle bottom"
            },
            fx + 12.0,
            *y,
            fw - 24.0,
            half,
            draw::linear(180.0, &[(0.0, c0), (1.0, c1)]),
        );
        b.add(
            body,
            draw::with_effects(panel, vec![draw::inner_shadow("#000000b0", 0.0, 2.0, 6.0)]),
        );
    }
    // Corner protectors.
    for (x, y, rot) in [
        (0.0, 0.0, 0.0),
        (bw - 40.0, 0.0, 90.0),
        (0.0, bh - 40.0, 270.0),
        (bw - 40.0, bh - 40.0, 180.0),
    ] {
        let d = match rot as i32 {
            0 => format!(
                "M {x} {} L {x} {} Q {x} {y} {} {y} L {} {y} L {} {} L {} {} L {} {} Z",
                y + 40.0,
                y + 10.0,
                x + 10.0,
                x + 40.0,
                x + 40.0,
                y + 14.0,
                x + 14.0,
                y + 14.0,
                x + 14.0,
                y + 40.0
            ),
            90 => format!(
                "M {x} {y} L {} {y} Q {} {y} {} {} L {} {} L {} {} L {} {} L {x} {} Z",
                x + 30.0,
                x + 40.0,
                x + 40.0,
                y + 10.0,
                x + 40.0,
                y + 40.0,
                x + 26.0,
                y + 40.0,
                x + 26.0,
                y + 14.0,
                y + 14.0
            ),
            270 => format!(
                "M {x} {y} L {} {y} L {} {} L {} {} L {} {} L {} {} Q {x} {} {x} {} Z",
                x + 14.0,
                x + 14.0,
                y + 26.0,
                x + 40.0,
                y + 26.0,
                x + 40.0,
                y + 40.0,
                x + 10.0,
                y + 40.0,
                y + 40.0,
                y + 30.0
            ),
            _ => format!(
                "M {} {y} L {} {y} L {} {} Q {} {} {} {} L {x} {} L {x} {} L {} {} Z",
                x + 26.0,
                x + 40.0,
                x + 40.0,
                y + 30.0,
                x + 40.0,
                y + 40.0,
                x + 30.0,
                y + 40.0,
                y + 40.0,
                y + 26.0,
                x + 26.0,
                y + 26.0
            ),
        };
        let n = draw::path(
            "corner",
            0.0,
            0.0,
            bw,
            bh,
            d,
            draw::linear(160.0, &[(0.0, "#3c3c3c"), (1.0, "#0e0e0e")]),
        );
        b.add(
            body,
            draw::with_effects(
                draw::stroked(n, "#000000", 1.0),
                vec![draw::drop_shadow("#00000099", 0.0, 2.0, 3.0)],
            ),
        );
    }
}

/// One twelve: the basket's rim with its bolts, the surround, the ribbed
/// cone, the dust cap.
fn speaker(b: &mut Builder<'_>, root: NodeId, (cx, cy): (f64, f64), r: f64, i: usize) {
    b.add(
        root,
        draw::circle(
            "speaker hole",
            cx,
            cy + 3.0,
            r + 4.0,
            draw::solid("#00000099"),
        ),
    );
    b.add(
        root,
        draw::circle(
            "basket",
            cx,
            cy,
            r,
            draw::linear(170.0, &[(0.0, "#2c2c2c"), (1.0, "#121212")]),
        ),
    );
    for k in 0..4 {
        let (bx, by) = draw::polar(cx, cy, r * 0.93, 45.0 + 90.0 * f64::from(k));
        b.add(
            root,
            draw::stroked(
                draw::circle("basket bolt", bx, by, 3.0, draw::solid("#262626")),
                "#555555",
                0.8,
            ),
        );
    }
    b.add(
        root,
        draw::circle("gasket", cx, cy, r * 0.87, draw::solid("#0d0d0d")),
    );
    b.add(
        root,
        draw::circle(
            "surround",
            cx,
            cy,
            r * 0.83,
            draw::radial(&[(0.0, "#1a1a1a"), (0.9, "#2b2b2b"), (1.0, "#101010")]),
        ),
    );
    let cone = draw::circle(
        "cone",
        cx,
        cy,
        r * 0.74,
        draw::radial_at(
            0.45,
            0.4,
            0.75,
            &[(0.0, "#3a3a3a"), (0.45, "#262626"), (1.0, "#141414")],
        ),
    );
    b.add(root, cone);
    // The cone's ribs: fine concentric rings.
    for k in 0..14 {
        let rr = r * (0.3 + 0.031 * f64::from(k));
        let ring = draw::stroked(
            draw::circle("cone rib", cx, cy, rr, draw::solid("#00000000")),
            if k % 2 == 0 { "#5c5c5c" } else { "#0a0a0a" },
            0.9,
        );
        b.add(root, draw::with_opacity(ring, 0.6));
    }
    b.add(
        root,
        draw::with_effects(
            draw::circle(
                "dust cap",
                cx,
                cy,
                r * 0.2,
                draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#4a4a4a"), (1.0, "#101010")]),
            ),
            vec![draw::drop_shadow("#000000b0", 0.0, 3.0, 6.0)],
        ),
    );
    b.add(
        root,
        draw::circle(
            "cone light",
            cx - r * 0.25,
            cy - r * 0.3,
            r * 0.5,
            draw::radial(&[(0.0, "#ffffff10"), (1.0, "#ffffff00")]),
        ),
    );
    let _ = i;
}

/// A mic over a speaker: a handle in the area from the cap out to the
/// rim (x = position) and down (y = distance), drawn as a dynamic's grille
/// and body seen end-on.
fn mic_handle(
    b: &mut Builder<'_>,
    root: NodeId,
    name: &str,
    m: &str,
    (cx, cy): (f64, f64),
    r: f64,
) {
    let area = b.add(
        root,
        draw::frame(
            &format!("{name} area"),
            cx - r * 0.1,
            cy - r * 0.55,
            r * 0.85,
            r * 1.1,
        ),
    );
    let h = b.add(area, draw::frame(name, 0.0, 0.0, 44.0, 44.0));
    let c = 22.0;
    b.add(
        h,
        draw::with_effects(
            draw::rotated(
                draw::rounded(
                    draw::rect(
                        "mic body",
                        c - 5.0,
                        c - 2.0,
                        10.0,
                        42.0,
                        draw::linear(
                            90.0,
                            &[(0.0, "#1a1a1a"), (0.5, "#555555"), (1.0, "#101010")],
                        ),
                    ),
                    3.0,
                ),
                35.0,
            ),
            vec![draw::drop_shadow("#000000c0", 3.0, 5.0, 6.0)],
        ),
    );
    b.add(
        h,
        draw::with_effects(
            draw::circle(
                "mic grille",
                c,
                c,
                9.0,
                draw::radial_at(
                    0.4,
                    0.35,
                    0.8,
                    &[(0.0, "#9a9a9a"), (0.6, "#4a4a4a"), (1.0, "#1a1a1a")],
                ),
            ),
            vec![draw::drop_shadow("#000000c0", 2.0, 4.0, 6.0)],
        ),
    );
    let mut mesh = String::new();
    for k in -3..=3 {
        let y = c + f64::from(k) * 2.2;
        let half = (81.0 - (y - c).powi(2)).max(0.0).sqrt();
        mesh.push_str(&format!(
            "M {:.2} {y:.2} L {:.2} {y:.2} ",
            c - half,
            c + half
        ));
    }
    let mut mn = draw::path(
        "mic mesh",
        0.0,
        0.0,
        44.0,
        44.0,
        mesh,
        draw::solid("#000000"),
    );
    mn.fills = Vec::new();
    mn.stroke = document_proto::Stroke {
        paints: vec![draw::solid("#2a2a2a")],
        weight: 0.6,
        ..document_proto::Stroke::default()
    };
    b.add(h, mn);
    let kind = ControlKind::Handle {
        x: Some(rk::bind(&a(&format!("mic_{m}_position")))),
        y: Some(rk::bind(&a(&format!("mic_{m}_distance")))),
        wheel: None,
        x_map: HandleAxis::new(Axis::Linear { min: 0.0, max: 1.0 }),
        y_map: HandleAxis::new(Axis::Linear { min: 1.0, max: 0.0 }),
    };
    // The handle is named for its position axis (a readout host shows it).
    b.controls.insert(
        h,
        Control::new(kind).bound(rk::bind(&a(&format!("mic_{m}_position")))),
    );
}

/// The featured-controls strip over the cab's foot.
fn strip(b: &mut Builder<'_>, root: NodeId) {
    let (x, y, w, h) = (24.0, 752.0, S - 48.0, 136.0);
    let mut n = draw::frame("Mic strip", x, y, w, h);
    n.fills = vec![draw::solid("#0b0b0bd9")];
    let n = draw::with_effects(
        draw::stroked(draw::rounded(n, 12.0), "#ffffff1f", 1.0),
        vec![draw::drop_shadow("#000000a0", 0.0, 4.0, 12.0)],
    );
    let s = b.add(root, n);
    let knob = KnobStyle::new(
        super::knob_id("knob.pedal_grip"),
        "knurled",
        36.0,
        "#8a8a88",
    )
    .opts(super::CAB_KNOB);
    let legend = Ink::new(8.0, TEXT).weight(600.0).track(0.06);
    let value = Ink::new(7.0, "#8a8a88").weight(500.0).track(0.02).plain();
    for (m, x0) in [("a", 14.0), ("b", w - 354.0)] {
        let up = m.to_uppercase();
        rk::silk(
            b,
            s,
            &format!("MIC {up}"),
            x0 + 28.0,
            28.0,
            Ink::new(11.0, TEXT).weight(700.0).track(0.1),
        );
        pill_select(
            b,
            s,
            &format!("Mic {up} model"),
            &a(&format!("mic_{m}_model")),
            (x0 + 60.0, 28.0 - 15.0, 170.0, 30.0),
        );
        pill_toggle(
            b,
            s,
            &format!("Mic {up} on"),
            &a(&format!("mic_{m}_on")),
            (x0 + 240.0, 13.0, 44.0, 30.0),
            "ON",
        );
        pill_toggle(
            b,
            s,
            &format!("Mic {up} phase"),
            &a(&format!("mic_{m}_phase")),
            (x0 + 292.0, 13.0, 44.0, 30.0),
            "Ø",
        );
        for (i, (p, name)) in [
            ("position", "Position"),
            ("distance", "Distance"),
            ("level", "Level"),
            ("pan", "Pan"),
        ]
        .iter()
        .enumerate()
        {
            let kx = x0 + 42.0 + 84.0 * i as f64;
            let k = rk::knob(
                b,
                s,
                &knob,
                &a(&format!("mic_{m}_{p}")),
                name,
                (kx, 78.0),
                118.0,
                legend,
            );
            parts::value_at(b, k, 128.0, value);
            match *p {
                "level" => parts::format(b, k, ValueFormat::decimals(1).signed()),
                "pan" => parts::format(b, k, ValueFormat::decimals(0).mark(0.0, "C")),
                _ => {}
            }
        }
    }
    let cx = w / 2.0;
    pill_toggle(
        b,
        s,
        "Room send",
        &a("room_on"),
        (cx - 24.0, 13.0, 48.0, 30.0),
        "ROOM",
    );
    let k = rk::knob(
        b,
        s,
        &knob,
        &a("room_level"),
        "Room",
        (cx, 78.0),
        118.0,
        legend,
    );
    parts::value_at(b, k, 128.0, value);
    parts::format(b, k, ValueFormat::decimals(1).signed());
}

/// A dark pill that is a toggle: its text lights green while on.
fn pill_toggle(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    addr: &str,
    (x, y, w, h): (f64, f64, f64, f64),
    text: &str,
) -> NodeId {
    let hh = h.max(TOUCH_MIN);
    let f = b.add(
        parent,
        draw::frame(name, x, y - (hh - h) / 2.0, w.max(TOUCH_MIN), hh),
    );
    let (fw, _) = (w.max(TOUCH_MIN), hh);
    b.add(
        f,
        draw::stroked(
            draw::rounded(
                draw::rect(
                    "pill",
                    (fw - w) / 2.0,
                    (hh - h) / 2.0,
                    w,
                    h,
                    draw::solid("#1c1c1c"),
                ),
                h / 2.0,
            ),
            "#ffffff30",
            1.0,
        ),
    );
    let ink = Ink::new(9.0, "#777775").weight(700.0).track(0.06);
    b.add(
        f,
        parts::text_node(text, 0.0, hh / 2.0, fw, &ink, TextAlign::Center),
    );
    let lit = parts::text_node(text, 0.0, hh / 2.0, fw, &ink.color(LIT), TextAlign::Center);
    let lit = b.add(f, parts::emit(lit, LIT, 0.5, 6.0));
    let mut c = Control::new(ControlKind::Button {
        mode: ButtonMode::Toggle,
    })
    .slot(SlotRole::Led, lit.0);
    c.binding = Some(rk::bind(addr));
    b.controls.insert(f, c);
    f
}

/// A dark pill reading a choice param (the mic model), with step arrows —
/// a `Selector` whose `Value` is the name.
fn pill_select(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    addr: &str,
    (x, y, w, h): (f64, f64, f64, f64),
) -> NodeId {
    let hh = h.max(TOUCH_MIN);
    let f = b.add(parent, draw::frame(name, x, y - (hh - h) / 2.0, w, hh));
    b.add(
        f,
        draw::stroked(
            draw::rounded(
                draw::rect("pill", 0.0, (hh - h) / 2.0, w, h, draw::solid("#000000")),
                6.0,
            ),
            "#ffffff30",
            1.0,
        ),
    );
    let ink = Ink::new(10.0, TEXT).weight(500.0).track(0.0).plain();
    let mut t = parts::text_node("", 10.0, hh / 2.0, w - 30.0, &ink, TextAlign::Left);
    t.name = "value".into();
    let v = b.add(f, t);
    let (ax, ay) = (w - 14.0, hh / 2.0);
    b.add(
        f,
        draw::path(
            "chevron",
            0.0,
            0.0,
            w,
            hh,
            draw::polygon(&[(ax - 4.0, ay - 2.0), (ax + 4.0, ay - 2.0), (ax, ay + 3.0)]),
            draw::solid(TEXT),
        ),
    );
    let mut c = Control::new(ControlKind::Selector).slot(SlotRole::Value, v.0);
    c.binding = Some(rk::bind(addr));
    c.format = Some(ValueFormat::labels(MICS));
    b.controls.insert(f, c);
    let _ = Place::new(name);
    f
}
