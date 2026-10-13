//! Modern Rhythm EQ: a nine-band graphic equaliser as a tabletop unit —
//! Tabletop form at the manual render's aspect (1000 × 254, ≈ 3.9 : 1): a
//! spun HPF knob and the POWER bat over it at the left, nine faders with
//! lit caps between printed tick ladders, each band's gain printed over it
//! (−12.0 dB … +12.0 dB) and its frequency under it, the spun LPF knob and
//! a violet status lamp at the right.
//!
//! The unit comes in the manual's four colourways, a choice param
//! (`mreq/finish`) that repaints it: **Bronze** and **Black** (violet-lit
//! caps, the player's band set 75 Hz … 12 kHz), **Ivory** and **Silver**
//! (white caps, the original set 65 Hz … 16 kHz). Each finish is a layer
//! shown while the param picks it; four swatch keys under the case pick one.

use frame::document::proto::{NodeId, TextAlign};
use frame::surface::kit::{Place, draw};
use frame::surface::proto::{
    ButtonMode, Condition, Control, ControlKind, Orientation, SlotRole, ValueFormat,
};
use frame::surface::widgets::Builder;

use crate::delay::parts;
use crate::face::{Face, canvas};
use crate::rigkit::{self as rk, Letter};

use super::PLUGIN;
use super::common::legend;

const W: f64 = 1000.0;
const H: f64 = 254.0;
const INK: &str = "#ececed";
const VIOLET: &str = "#c38bff";
const NS: &str = "mreq";

/// The colourways: name, plate top / body / bottom, scuff strength, and
/// whether the caps are violet-lit (the player's set) or white (original).
const FINISHES: [(&str, [&str; 3], f64, bool); 4] = [
    ("Bronze", ["#9a8c7f", "#7d7166", "#5f554d"], 0.13, true),
    ("Ivory", ["#d8d3c6", "#c3bdae", "#a49e90"], 0.16, false),
    ("Black", ["#46474a", "#2a2b2e", "#1b1c1f"], 0.12, true),
    ("Silver", ["#a4a6a9", "#86888b", "#6a6c6f"], 0.14, false),
];
/// The player's band set and the original's.
const BANDS_MOD: [&str; 9] = [
    "75 Hz", "150 Hz", "250 Hz", "400 Hz", "800 Hz", "1.5 kHz", "4.5 kHz", "8 kHz", "12 kHz",
];
const BANDS_ORIG: [&str; 9] = [
    "65 Hz", "125 Hz", "250 Hz", "500 Hz", "1 kHz", "2 kHz", "4 kHz", "8 kHz", "16 kHz",
];

pub fn faces() -> Vec<Face> {
    vec![nine_band()]
}

fn finish_addr() -> String {
    format!("{NS}/finish")
}

/// A layer of `parent` (at `(x, y, w, h)`) drawn only while the finish is
/// one of `which`: a readout of the finish param, shown by condition.
fn layer(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    (x, y, w, h): (f64, f64, f64, f64),
    which: &[f64],
) -> NodeId {
    let id = b.add(parent, draw::frame(name, x, y, w, h));
    // Its value: the finish's name, carried but not printed.
    let mut t = draw::text(
        "value",
        "",
        0.0,
        0.0,
        w.min(40.0),
        4.0,
        "#00000000",
        TextAlign::Left,
    );
    t.opacity = 0.0;
    let value = b.add(id, t);
    let c = Control::new(ControlKind::Readout)
        .slot(SlotRole::Value, value.0)
        .bound(rk::bind(&finish_addr()))
        .shown_if(Condition::any_of(finish_addr(), which));
    b.controls.insert(id, c);
    id
}

/// The finishes whose caps are violet, and whose are white.
fn violet_finishes() -> Vec<f64> {
    FINISHES
        .iter()
        .enumerate()
        .filter(|(_, f)| f.3)
        .map(|(i, _)| i as f64)
        .collect()
}
fn white_finishes() -> Vec<f64> {
    FINISHES
        .iter()
        .enumerate()
        .filter(|(_, f)| !f.3)
        .map(|(i, _)| i as f64)
        .collect()
}

/// A graphic-EQ fader: a black slot, a cap that rides it (`Thumb`) — lit
/// violet or white by the finish — the band's gain printed live over it
/// (`Value`), both band sets' frequencies under it.
#[allow(clippy::too_many_arguments)]
fn fader(
    b: &mut Builder<'_>,
    p: NodeId,
    i: usize,
    addr: &str,
    (cx, y0, len): (f64, f64, f64),
    value_y: f64,
    freq_y: f64,
) -> NodeId {
    let w = 44.0;
    let root = b.add(
        p,
        draw::frame(&format!("Band {}", i + 1), cx - w / 2.0, y0, w, len),
    );
    let slot = draw::rounded(
        draw::rect(
            "slot",
            w / 2.0 - 6.0,
            0.0,
            12.0,
            len,
            draw::linear(
                90.0,
                &[(0.0, "#000000"), (0.5, "#151517"), (1.0, "#000000")],
            ),
        ),
        5.0,
    );
    b.add(
        root,
        draw::with_effects(
            draw::stroked(slot, "#3a3b3e", 1.0),
            vec![draw::inner_shadow("#000000", 0.0, 2.0, 3.0)],
        ),
    );
    let cap_h = 26.0;
    let lane = b.add(root, draw::frame("lane", 0.0, 0.0, w, len));
    let cap = b.add(lane, draw::frame("cap", w / 2.0 - 8.0, 0.0, 16.0, cap_h));
    let violet = layer(
        b,
        cap,
        "violet cap",
        (0.0, 0.0, 16.0, cap_h),
        &violet_finishes(),
    );
    let body = draw::rounded(
        draw::rect(
            "cap body",
            0.0,
            0.0,
            16.0,
            cap_h,
            draw::linear(
                180.0,
                &[
                    (0.0, "#1a1a1c"),
                    (0.14, "#3a3a3e"),
                    (0.2, "#8e6cc0"),
                    (0.5, "#f1e4ff"),
                    (0.8, "#8e6cc0"),
                    (0.86, "#3a3a3e"),
                    (1.0, "#1a1a1c"),
                ],
            ),
        ),
        2.0,
    );
    b.add(violet, draw::emissive(body, draw::hex(VIOLET), 0.8, 18.0));
    let white = layer(
        b,
        cap,
        "white cap",
        (0.0, 0.0, 16.0, cap_h),
        &white_finishes(),
    );
    let body = draw::rounded(
        draw::rect(
            "cap body",
            0.0,
            0.0,
            16.0,
            cap_h,
            draw::linear(
                180.0,
                &[
                    (0.0, "#1a1a1c"),
                    (0.14, "#5a5a5e"),
                    (0.22, "#d8d8d6"),
                    (0.5, "#ffffff"),
                    (0.78, "#c8c8c6"),
                    (0.86, "#4a4a4e"),
                    (1.0, "#1a1a1c"),
                ],
            ),
        ),
        2.0,
    );
    b.add(
        white,
        draw::with_effects(body, vec![draw::drop_shadow("#00000099", 0.0, 2.0, 3.0)]),
    );
    // The live gain over the fader.
    let size = 9.5;
    let mut t = parts::text_node(
        "",
        -30.0,
        value_y - y0,
        w + 60.0,
        &parts::Ink::new(size, INK).weight(700.0).track(0.0).plain(),
        TextAlign::Center,
    );
    t.name = "value".into();
    let value = b.add(root, t);
    let mut c = Control::new(ControlKind::Fader {
        orientation: Orientation::Vertical,
    })
    .slot(SlotRole::Thumb, cap.0)
    .slot(SlotRole::Value, value.0);
    c.binding = Some(rk::bind(addr));
    c.format = Some(ValueFormat::decimals(1).signed());
    b.controls.insert(root, c);
    // The frequencies: the player's set on the violet finishes, the
    // original's on the white (the one in the violet layer names the band).
    let l = Letter::new(9.0, 700.0, INK);
    let mod_set = layer(
        b,
        root,
        "player's band",
        (0.0, 0.0, w, len),
        &violet_finishes(),
    );
    let id = rk::letter(b, mod_set, BANDS_MOD[i], (w / 2.0, freq_y - y0), l);
    if let Some(c) = b.controls.get_mut(&root) {
        c.slots.push(frame::surface::proto::Slot {
            role: SlotRole::Label,
            node: id.0,
        });
    }
    let orig = layer(
        b,
        root,
        "original band",
        (0.0, 0.0, w, len),
        &white_finishes(),
    );
    rk::letter(b, orig, BANDS_ORIG[i], (w / 2.0, freq_y - y0), l);
    root
}

/// The printed tick ladder between two faders.
fn ladder(b: &mut Builder<'_>, p: NodeId, cx: f64, (y0, y1): (f64, f64)) {
    let n = 13;
    let lines: Vec<Vec<(f64, f64)>> = (0..n)
        .map(|i| {
            let y = y0 + (y1 - y0) * f64::from(i) / f64::from(n - 1);
            let hw = if i % 6 == 0 { 9.0 } else { 6.0 };
            vec![(cx - hw, y), (cx + hw, y)]
        })
        .collect();
    b.add(
        p,
        draw::with_opacity(
            parts::stroke_path("ladder", (0.0, 0.0, W, H), &lines, 1.0, INK, false),
            0.45,
        ),
    );
}

/// A spun filter knob with its readout over it ("HPF : 20 HZ").
#[allow(clippy::too_many_arguments)]
fn filter(
    b: &mut Builder<'_>,
    p: NodeId,
    name: &str,
    addr: &str,
    (cx, cy): (f64, f64),
    label: &str,
    glyph: bool,
    stop: f64,
) {
    // A dark spun cap in a darker rim, a white line for the index.
    let look = [
        "ticks=none",
        "color=#4a4b50",
        "skirt_color=#16171a",
        "pointer=#f4f4f2",
        "index_in=0.25",
        "index_out=0.92",
        "fallback_color=#55565b",
    ];
    let place = Place::new(name)
        .size(62.0)
        .variant("spun_metal")
        .bind(rk::bind(addr))
        .touch(frame::surface::kit::TOUCH_MIN)
        .options(&look);
    let k = parts::put(b, p, super::knob_id("knob.3d"), &place, cx, cy);
    parts::strip_legend(b, k);
    rk::letter_aligned(
        b,
        p,
        &format!("{label} :"),
        (cx - 2.0, 22.0),
        Letter::new(9.0, 700.0, INK),
        TextAlign::Right,
    );
    let mut t = parts::text_node(
        "",
        cx + 3.0,
        22.0,
        70.0,
        &parts::Ink::new(9.0, INK).weight(700.0).track(0.02),
        TextAlign::Left,
    );
    t.name = "value".into();
    let v = b.add(k, {
        let (tx, ty) = b
            .scene
            .node(k)
            .map_or((0.0, 0.0), |n| (n.transform.tx, n.transform.ty));
        let mut t = t;
        t.transform.tx -= tx;
        t.transform.ty -= ty;
        t
    });
    if let Some(c) = b.controls.get_mut(&k) {
        c.slots.push(frame::surface::proto::Slot {
            role: SlotRole::Value,
            node: v.0,
        });
        // At its stop the filter is out: the readout says so.
        c.format = Some(ValueFormat::decimals(0).mark(stop, "OFF"));
    }
    // The slope glyph under the readout: a low cut rises, a high cut falls.
    let y = 50.0;
    let pts = if glyph {
        vec![(cx - 7.0, y + 5.0), (cx - 2.0, y), (cx + 7.0, y)]
    } else {
        vec![(cx - 7.0, y), (cx + 2.0, y), (cx + 7.0, y + 5.0)]
    };
    rk::line(b, p, &pts, 1.6, INK);
}

/// One finish's case: the plate in its paint, scratched and scuffed.
fn plate(b: &mut Builder<'_>, p: NodeId, i: usize) {
    let (name, [top, body, bottom], scuff, _) = FINISHES[i];
    let l = layer(
        b,
        p,
        &format!("{name} case"),
        (0.0, 0.0, W, H - 12.0),
        &[i as f64],
    );
    let plate = draw::rounded(
        draw::rect(
            "case",
            0.0,
            0.0,
            W,
            H - 12.0,
            draw::linear(
                180.0,
                &[
                    (0.0, top),
                    (0.1, body),
                    (0.9, bottom),
                    (1.0, &parts::mixed(bottom, "#000000", 0.4)),
                ],
            ),
        ),
        8.0,
    );
    b.add(
        l,
        draw::with_effects(
            draw::stroked(plate, "#000000", 1.5),
            vec![draw::inner_shadow("#ffffff30", 0.0, 1.5, 1.0)],
        ),
    );
    // Scratched metal: pale scratches, scuffs and dark wear.
    let mut s: u64 = 0x9e11 + i as u64 * 7;
    let mut rnd = || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        (s >> 11) as f64 / (1u64 << 53) as f64
    };
    let mut d = String::new();
    for _ in 0..220 {
        let (px, py) = (8.0 + rnd() * (W - 16.0), 8.0 + rnd() * (H - 30.0));
        let (lx, ly) = (5.0 + rnd() * 34.0, (rnd() - 0.5) * 18.0);
        d.push_str(&format!(
            "M {px:.1} {py:.1} l {lx:.1} {ly:.1} l 0.4 0.5 l {:.1} {:.1} Z ",
            -lx, -ly
        ));
    }
    b.add(
        l,
        draw::with_opacity(
            draw::path("scratches", 0.0, 0.0, W, H, d, draw::solid("#ffffff")),
            scuff,
        ),
    );
    for (color, a) in [("#ffffff", scuff * 0.55), ("#000000", scuff * 0.8)] {
        let mut d = String::new();
        for _ in 0..40 {
            let (px, py) = (20.0 + rnd() * (W - 40.0), 15.0 + rnd() * (H - 45.0));
            let r = 6.0 + rnd() * 22.0;
            d.push_str(&format!(
                "M {px:.1} {py:.1} q {:.1} {:.1} {:.1} {:.1} q {:.1} {:.1} {:.1} {:.1} Z ",
                r,
                -r * 0.3,
                r * 1.6,
                r * 0.2,
                -r * 0.6,
                r * 0.5,
                -r * 1.6,
                -r * 0.2
            ));
        }
        b.add(
            l,
            draw::with_opacity(
                draw::path("scuffs", 0.0, 0.0, W, H, d, draw::solid(color)),
                a,
            ),
        );
    }
}

/// The four finish keys under the case: swatches, the chosen one ringed.
fn finish_keys(b: &mut Builder<'_>, p: NodeId) {
    let n = FINISHES.len();
    for (i, (name, [top, body, _], _, _)) in FINISHES.iter().enumerate() {
        let cx = W / 2.0 - 1.5 * 16.0 + 16.0 * i as f64;
        let key = b.add(
            p,
            draw::frame(&format!("Finish {name}"), cx - 7.0, H - 13.0, 14.0, 12.0),
        );
        b.add(
            key,
            draw::stroked(
                draw::circle(
                    "swatch",
                    7.0,
                    6.0,
                    4.0,
                    draw::linear(180.0, &[(0.0, top), (1.0, body)]),
                ),
                "#00000099",
                0.8,
            ),
        );
        let ring = b.add(
            key,
            draw::stroked(
                draw::circle("chosen", 7.0, 6.0, 5.6, draw::solid("#00000000")),
                INK,
                1.0,
            ),
        );
        let mut c = Control::new(ControlKind::Button {
            mode: ButtonMode::Select {
                index: i as u32,
                count: n as u32,
            },
        })
        .slot(SlotRole::Led, ring.0);
        c.binding = Some(rk::bind(&finish_addr()));
        b.controls.insert(key, c);
    }
}

/// The nine-band graphic EQ — Tabletop, 1000 × 254.
fn nine_band() -> Face {
    let a = |s: &str| format!("{NS}/{s}");
    let scene = canvas("Nine-Band Graphic", |b, root| {
        parts::fit_root(b, root, W, H);
        // Rubber feet under the case.
        for x in [110.0, 890.0] {
            b.add(
                root,
                draw::rounded(
                    draw::rect(
                        "foot",
                        x - 50.0,
                        H - 16.0,
                        100.0,
                        16.0,
                        draw::linear(180.0, &[(0.0, "#1c1c1e"), (1.0, "#050506")]),
                    ),
                    5.0,
                ),
            );
        }
        let shadow = draw::rounded(
            draw::rect("case shadow", 0.0, 0.0, W, H - 12.0, draw::solid("#111214")),
            8.0,
        );
        b.add(
            root,
            draw::with_effects(shadow, vec![draw::drop_shadow("#000000c0", 0.0, 6.0, 14.0)]),
        );
        for i in 0..FINISHES.len() {
            plate(b, root, i);
        }
        let xs: Vec<f64> = (0..9).map(|i| 237.0 + 65.5 * f64::from(i)).collect();
        for i in 0..8 {
            ladder(b, root, (xs[i] + xs[i + 1]) / 2.0, (58.0, 200.0));
        }
        for (i, x) in xs.iter().enumerate() {
            fader(
                b,
                root,
                i,
                &a(&format!("b{}", i + 1)),
                (*x, 50.0, 158.0),
                22.0,
                222.0,
            );
        }
        filter(b, root, "HPF", &a("hpf"), (128.0, 108.0), "HPF", true, 20.0);
        filter(
            b,
            root,
            "LPF",
            &a("lpf"),
            (872.0, 108.0),
            "LPF",
            false,
            20000.0,
        );
        let t = rk::mini_toggle(
            b,
            root,
            "Power",
            &a("power"),
            (128.0, 182.0 - 6.0),
            12.0,
            "chrome",
        );
        legend(
            b,
            t,
            "POWER",
            (128.0, 216.0),
            Letter::new(8.0, 760.0, INK).track(0.04),
        );
        rk::led(b, root, (872.0, 182.0), 9.0, VIOLET, true);
        finish_keys(b, root);
    });
    // The manual's demonstration setting: −12, 0, +12 across the bands.
    let demo = [-12.0, 0.0, 12.0];
    let mut params: Vec<frame::surface::proto::ParamInfo> = (1..=9)
        .map(|i| {
            rk::db(
                &a(&format!("b{i}")),
                &format!("Band {i} Gain"),
                -12.0,
                12.0,
                demo[(i - 1) % 3],
            )
        })
        .collect();
    params[0] = params[0].clone().featured();
    params.push(rk::hz(&a("hpf"), "High-Pass", 20.0, 1000.0, 20.0).featured());
    params.push(rk::hz(&a("lpf"), "Low-Pass", 2000.0, 20000.0, 20000.0).featured());
    params.push(rk::toggle(&a("power"), "Power", true).featured());
    params.push(rk::choice(
        &a("finish"),
        "Finish",
        &FINISHES.map(|f| f.0),
        2,
    ));
    Face {
        file: "10-eq-tabletop-nine-band",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}
