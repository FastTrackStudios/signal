//! The nine-band graphic EQ — Tabletop, 1000 × 282 (the unit's 3.3 : 1
//! face plus its feet).
//!
//! A black anodised desktop box: a high-pass knob (its setting read over
//! it, "HPF" and the value), the slope glyph and the on/off bat toggle at
//! the left; nine long-throw faders whose caps are lit blue from inside,
//! each with its gain read over it and its band (65 Hz … 16 kHz) under it;
//! the low-pass knob and the invented maker's badge at the right.

use document_proto::{NodeId, TextAlign};
use surface_kit::{Place, TOUCH_MIN, draw};
use surface_proto::{Control, ControlKind, Orientation, SlotRole, ValueFormat};
use surface_widgets::Builder;

use super::PLUGIN;
use crate::delay::parts::{self, Ink};
use crate::face::{Face, canvas};
use crate::rigkit::{self as rk, Letter};

const W: f64 = 1000.0;
const H: f64 = 282.0;
const BODY_H: f64 = 263.0;
const PRINT: &str = "#e2e2e0";
const BLUE: &str = "#7fc4ff";
const BANDS: &[&str] = &[
    "65 Hz", "125 Hz", "250 Hz", "500 Hz", "1 kHz", "2 kHz", "4 kHz", "8 kHz", "16 kHz",
];
const XS: [f64; 9] = [
    233.0, 299.0, 366.0, 433.0, 499.0, 566.0, 633.0, 700.0, 767.0,
];

fn a(s: &str) -> String {
    format!("bgeq/{s}")
}

pub fn face() -> Face {
    let scene = canvas("Nine-Band EQ", |b, root| {
        parts::fit_root(b, root, W, H);
        for x in [40.0, W - 108.0] {
            let foot = draw::rounded(
                draw::rect(
                    "foot",
                    x,
                    BODY_H - 6.0,
                    68.0,
                    H - BODY_H + 6.0,
                    draw::linear(180.0, &[(0.0, "#1a1a1a"), (1.0, "#050505")]),
                ),
                4.0,
            );
            b.add(
                root,
                draw::with_effects(foot, vec![draw::drop_shadow("#000000a0", 0.0, 3.0, 4.0)]),
            );
        }
        let mut body = draw::frame("Body", 0.0, 0.0, W, BODY_H);
        body.fills = vec![draw::linear(
            180.0,
            &[
                (0.0, "#2a2a2c"),
                (0.04, "#161617"),
                (0.9, "#111112"),
                (1.0, "#060606"),
            ],
        )];
        let body = draw::with_effects(
            draw::stroked(draw::rounded(body, 14.0), "#000000", 1.5),
            vec![
                draw::inner_shadow("#ffffff30", 0.0, 1.5, 1.0),
                draw::drop_shadow("#000000b0", 0.0, 6.0, 14.0),
            ],
        );
        let body = b.add(root, body);
        // A faint horizontal brushing on the anodising.
        let mut d = String::new();
        let mut y = 3.0;
        while y < BODY_H {
            d.push_str(&format!("M 0 {y:.1} H {W} V {:.1} H 0 Z ", y + 0.6));
            y += 2.7;
        }
        b.add(
            body,
            draw::path("brushing", 0.0, 0.0, W, BODY_H, d, draw::solid("#ffffff04")),
        );

        let legend = Ink::new(10.0, PRINT).weight(700.0).track(0.02).plain();
        for (i, x) in XS.iter().enumerate() {
            fader(b, root, i, *x, legend);
        }
        filter_knob(b, root, "hpf", "HPF", 136.0, "/", legend, 20.0);
        filter_knob(b, root, "lpf", "LPF", 866.0, "\\", legend, 20_000.0);
        let t = rk::mini_toggle(b, root, "Power", &a("on"), (136.0, 204.0), 22.0, "chrome");
        let _ = t;
        badge(b, root, (864.0, 204.0));
    });
    let mut params: Vec<surface_proto::ParamInfo> = (1..=9)
        .map(|i| {
            rk::db(
                &a(&format!("b{i}")),
                &format!("{} Gain", BANDS[i - 1]),
                -12.0,
                12.0,
                0.0,
            )
            .featured()
        })
        .collect();
    params.push(rk::hz(&a("hpf"), "High-Pass", 20.0, 400.0, 20.0));
    params.push(rk::hz(&a("lpf"), "Low-Pass", 2000.0, 20000.0, 20000.0));
    params.push(rk::toggle(&a("on"), "On", true).featured());
    Face {
        file: "06-eq-tabletop-nine-band",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

/// One band: the slot, the lit cap (the `Thumb`), the gain read over it,
/// the band under it.
fn fader(b: &mut Builder<'_>, root: NodeId, i: usize, x: f64, legend: Ink<'_>) {
    let (top, bot) = (51.0, 221.0);
    let (cw, ch) = (16.0, 32.0);
    let travel = bot - top - ch;
    let w = TOUCH_MIN;
    let f = b.add(root, draw::frame(BANDS[i], x - w / 2.0, top, w, bot - top));
    let slot = draw::rounded(
        draw::rect(
            "slot",
            w / 2.0 - 5.5,
            0.0,
            11.0,
            bot - top,
            draw::linear(
                90.0,
                &[(0.0, "#000000"), (0.5, "#0a0a0a"), (1.0, "#1c1c1c")],
            ),
        ),
        5.5,
    );
    b.add(
        f,
        draw::with_effects(
            slot,
            vec![
                draw::inner_shadow("#000000", 0.0, 2.0, 3.0),
                draw::drop_shadow("#ffffff14", 0.0, 1.0, 0.0),
            ],
        ),
    );
    let lane = b.add(
        f,
        draw::frame("lane", w / 2.0 - cw / 2.0, 0.0, cw, travel + ch),
    );
    let cap = b.add(lane, draw::frame("cap", 0.0, 0.0, cw, ch));
    let body = draw::rounded(
        draw::rect(
            "cap body",
            0.0,
            0.0,
            cw,
            ch,
            draw::linear(
                180.0,
                &[(0.0, "#2a4a66"), (0.5, "#1c3a55"), (1.0, "#12283c")],
            ),
        ),
        2.5,
    );
    b.add(cap, draw::stroked(body, "#000000", 0.8));
    let glow = draw::rounded(
        draw::rect(
            "cap light",
            2.5,
            3.0,
            cw - 5.0,
            ch - 6.0,
            draw::linear(
                90.0,
                &[(0.0, "#7ab8f0"), (0.5, "#e6f4ff"), (1.0, "#7ab8f0")],
            ),
        ),
        2.0,
    );
    b.add(cap, draw::emissive(glow, draw::hex(BLUE), 0.8, 12.0));
    let mut c = Control::new(ControlKind::Fader {
        orientation: Orientation::Vertical,
    })
    .slot(SlotRole::Thumb, cap.0);
    c.binding = Some(rk::bind(&a(&format!("b{}", i + 1))));
    b.controls.insert(f, c);
    parts::label_at(b, f, BANDS[i], (x, 240.0), legend);
    // The gain, read over the slot.
    let r = parts::readout(
        b,
        root,
        &format!("{} readout", BANDS[i]),
        rk::bind(&a(&format!("b{}", i + 1))),
        (x - 34.0, 31.0, 68.0),
        10.0,
        PRINT,
        TextAlign::Center,
    );
    parts::format(b, r, ValueFormat::decimals(1).signed());
    unlit(b, r);
}

/// A readout's value doesn't glow on this unit (it is print).
fn unlit(b: &mut Builder<'_>, readout: NodeId) {
    let v = b
        .controls
        .get(&readout)
        .and_then(|c| c.slots_of(SlotRole::Value).next())
        .map(NodeId);
    if let Some(n) = v.and_then(|v| b.scene.node_mut(v)) {
        n.emission = None;
    }
}

/// A filter knob: the chrome-capped knob, its setting read over it and the
/// slope glyph under it.
#[allow(clippy::too_many_arguments)]
fn filter_knob(
    b: &mut Builder<'_>,
    root: NodeId,
    addr: &str,
    name: &str,
    x: f64,
    slope: &str,
    legend: Ink<'_>,
    stop: f64,
) {
    let mut p = Place::new(name)
        .size(54.0)
        .variant("spun_metal")
        .bind(rk::bind(&a(addr)))
        .touch(TOUCH_MIN);
    p.options = Some(super::EQ_KNOB.iter().map(ToString::to_string).collect());
    let k = parts::put(b, root, super::knob_id("knob.knurled_cap"), &p, x, 91.0);
    parts::strip_legend(b, k);
    parts::strip_scale(b, k);
    rk::silk(b, root, &format!("{name} :"), x - 22.0, 31.0, legend);
    let r = parts::readout(
        b,
        root,
        &format!("{name} readout"),
        rk::bind(&a(addr)),
        (x - 4.0, 31.0, 60.0),
        10.0,
        PRINT,
        TextAlign::Left,
    );
    // At its stop the filter is out: the readout says so.
    parts::format(b, r, ValueFormat::decimals(0).mark(stop, "OFF"));
    unlit(b, r);
    parts::label_at(b, k, name, (x, 31.0), legend.color("#00000000"));
    let g = if slope == "/" {
        vec![(x - 6.0, 152.0), (x - 1.0, 152.0), (x + 4.0, 144.0)]
    } else {
        vec![(x - 4.0, 144.0), (x + 1.0, 144.0), (x + 6.0, 152.0)]
    };
    rk::line(b, root, &g, 1.8, PRINT);
}

/// The maker's badge: a small outlined box with the invented name in
/// squared extended capitals.
fn badge(b: &mut Builder<'_>, root: NodeId, (cx, cy): (f64, f64)) {
    let (w, h) = (92.0, 22.0);
    let plate = draw::stroked(
        draw::rect(
            "badge",
            cx - w / 2.0,
            cy - h / 2.0,
            w,
            h,
            draw::solid("#0c0c0c"),
        ),
        "#cfcfcf",
        1.2,
    );
    b.add(root, plate);
    b.add(
        root,
        draw::stroked(
            draw::rect(
                "badge inner",
                cx - w / 2.0 + 2.5,
                cy - h / 2.0 + 2.5,
                w - 5.0,
                h - 5.0,
                draw::solid("#00000000"),
            ),
            "#cfcfcf80",
            0.6,
        ),
    );
    rk::letter_fit(
        b,
        root,
        "VANTOM",
        (cx, cy),
        w - 18.0,
        Letter::new(12.0, 600.0, "#e0e0e0").stretch(1.3).track(0.08),
    );
}
