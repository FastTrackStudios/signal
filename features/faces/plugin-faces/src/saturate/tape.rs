//! The tape machine: a two-track mastering deck in warm charcoal enamel —
//! what you drive tape saturation with.
//!
//! Across the top, left to right: the transport behind glass — two NAB
//! aluminium reels (the light stays put on their spun flanges while they
//! turn at the rate SPEED sets; the emptier take-up turns faster), the tape
//! running down past the guides, the erase / record / play heads and the
//! capstan with its pinch roller; the meter bridge — two lamp-lit VUs
//! under glass reading the level going to tape, the RECORD lamp between
//! them; and the engraved badge over the EMPHASIS key and the SPEED switch
//! (7½, 15, 30 in/s; slower is more head bump and less top).
//!
//! Below, the record electronics in one family of knobs — spun aluminium
//! caps on black skirts, a black index: the tape FORMULATION switch
//! (standard or hot, the family's two profiles), RECORD LEVEL (drive into
//! the tape), BIAS (under- or over-biased from the calibrated point),
//! EMPHASIS, COMPRESSION, and MIX and OUTPUT last.

use frame::document::proto::NodeId;
use frame::surface::kit::{Place, draw, motion};
use frame::surface::proto::{Binding, EntryId, Source};
use frame::surface::widgets::Builder;

use super::hw;
use crate::delay::art::{self, circle_d, ring_d, sector_d};
use crate::delay::parts::{self, Ink};
use crate::face::{Face, canvas};
use crate::lane::Shape;

const W: f64 = 960.0;
const H: f64 = 330.0;
const INK: &str = "#f0ebe2";
const DIM: &str = "#c4b9a8";
const SPEEDS: &[&str] = &["7½", "15", "30"];
const SPEED_NAMES: &[&str] = &["7.5 in/s", "15 in/s", "30 in/s"];
const FORMULATIONS: &[&str] = &["Standard", "Hot"];
/// The emphasis EQ's colour: the VUs' lit ivory.
const LAMP: &str = "#f2c46a";
/// The panel: warm charcoal enamel.
const PAINT: &[(f64, &str)] = &[(0.0, "#4d4944"), (0.5, "#3d3a36"), (1.0, "#2c2a27")];

/// The knob family: a spun aluminium cap in a black skirt, black index.
const KNOB: (&str, &[&str]) = (
    "spun_metal",
    &[
        "cap_r=0.8",
        "height=0.5",
        "skirt_r=1.0",
        "skirt_h=0.22",
        "pointer=#0a0a0a",
        "index_w=0.085",
        "index_in=0.08",
        "index_out=0.96",
        "spec=1.0",
        "shadow=1.3",
    ],
);

fn a(s: &str) -> String {
    format!("tape_sat/{s}")
}

/// An NAB reel face-on, `r` in radius centred on (`cx`, `cy`) of `deck`, its
/// flange's windows at `rot`:
/// the oxide pack wound to `fill`, a spun aluminium flange with three
/// windows, the big NAB hub. The flange and hub turn (`spin`, turns a
/// second); the light on them — the stage's `reel lit` frame — does not.
fn reel(
    b: &mut Builder<'_>,
    deck: NodeId,
    (cx, cy, r): (f64, f64, f64),
    (fill, rot): (f64, f64),
    (spin, name): (&str, &str),
) {
    let side = 2.0 * r + 10.0;
    let c = side / 2.0;
    let root = b.add(deck, draw::frame("reel", cx - c, cy - c, side, side));
    b.add(
        root,
        draw::with_effects(
            draw::circle("reel shadow", c + 2.0, c + 4.0, r, draw::solid("#00000080")),
            vec![draw::drop_shadow("#000000a0", 0.0, 4.0, 9.0)],
        ),
    );
    let lit = b.add(root, draw::frame("reel lit", 0.0, 0.0, side, side));
    // The pack: brown oxide, its wound edge faintly banded.
    let rp = r * fill;
    let pack = draw::circle(
        "tape pack",
        c,
        c,
        rp,
        draw::radial_at(
            0.5,
            0.5,
            0.5,
            &[
                (0.0, "#1e120a"),
                (0.4, "#3a2416"),
                (0.93, "#5a3a24"),
                (1.0, "#24160c"),
            ],
        ),
    );
    b.add(lit, pack);
    let bands: String = (1..7)
        .map(|k| circle_d(c, c, r * 0.36 + (rp - r * 0.36) * f64::from(k) / 7.0, false))
        .collect();
    let mut bands = draw::stroked(
        draw::path(
            "pack bands",
            0.0,
            0.0,
            side,
            side,
            bands,
            draw::solid("#000000"),
        ),
        "#000000",
        0.6,
    );
    bands.fills = Vec::new();
    bands.stroke.align = frame::document::proto::StrokeAlign::Center;
    b.add(lit, draw::with_opacity(bands, 0.3));
    let turning = b.add(lit, draw::frame("reel turning", 0.0, 0.0, side, side));
    // The flange: aluminium, spun (the conic sheen), three windows.
    let mut d = circle_d(c, c, r, false);
    for i in 0..3 {
        let w0 = rot + 120.0 * f64::from(i);
        d.push_str(&sector_d(
            c,
            c,
            r * 0.42,
            r * 0.88,
            w0 + 14.0,
            w0 + 106.0,
            true,
        ));
    }
    d.push_str(&circle_d(c, c, r * 0.31, true));
    let spun = draw::conic(
        rot + 20.0,
        &[
            (0.0, "#eef0f2"),
            (0.12, "#a6aaaf"),
            (0.25, "#dfe2e5"),
            (0.38, "#8c9096"),
            (0.5, "#eef0f2"),
            (0.62, "#a6aaaf"),
            (0.75, "#dfe2e5"),
            (0.88, "#8c9096"),
            (1.0, "#eef0f2"),
        ],
    );
    b.add(
        turning,
        draw::with_effects(
            draw::path("flange", 0.0, 0.0, side, side, d, spun),
            vec![draw::drop_shadow("#00000070", 0.0, 1.0, 2.0)],
        ),
    );
    // Turned rings on the flange, and the lit edges of rim and windows.
    let mut rings = String::new();
    for k in [0.93, 0.36] {
        rings.push_str(&circle_d(c, c, r * k, false));
    }
    let mut rings = draw::stroked(
        draw::path(
            "turned rings",
            0.0,
            0.0,
            side,
            side,
            rings,
            draw::solid("#000000"),
        ),
        "#00000060",
        0.7,
    );
    rings.fills = Vec::new();
    rings.stroke.align = frame::document::proto::StrokeAlign::Center;
    b.add(turning, rings);
    let mut rim = ring_d(c, c, r - 1.1, r);
    for i in 0..3 {
        let w0 = rot + 120.0 * f64::from(i);
        rim.push_str(&sector_d(
            c,
            c,
            r * 0.88,
            r * 0.88 + 0.9,
            w0 + 14.0,
            w0 + 106.0,
            false,
        ));
    }
    b.add(
        turning,
        draw::with_opacity(
            draw::path(
                "flange rim",
                0.0,
                0.0,
                side,
                side,
                rim,
                draw::solid("#ffffff"),
            ),
            0.55,
        ),
    );
    // The NAB hub: a black centre with three keyways, the spindle cap.
    b.add(
        turning,
        draw::circle(
            "hub",
            c,
            c,
            r * 0.31,
            draw::radial_at(
                0.4,
                0.35,
                0.9,
                &[(0.0, "#4a4c50"), (0.7, "#1c1d1f"), (1.0, "#0b0b0c")],
            ),
        ),
    );
    let keys: Vec<Vec<(f64, f64)>> = (0..3)
        .map(|i| {
            let k = rot + 60.0 + 120.0 * f64::from(i);
            vec![
                draw::polar(c, c, r * 0.13, k),
                draw::polar(c, c, r * 0.27, k),
            ]
        })
        .collect();
    b.add(
        turning,
        parts::lines_node("keyways", &keys, r * 0.06, "#050505"),
    );
    b.add(
        turning,
        draw::circle(
            "spindle",
            c,
            c,
            r * 0.1,
            draw::radial_at(
                0.35,
                0.3,
                0.8,
                &[(0.0, "#ffffff"), (0.5, "#b8babd"), (1.0, "#4c4e52")],
            ),
        ),
    );
    motion::add(b, turning, motion::spin(spin).named(name));
}

/// The transport in the window `(x, y, w, h)` of `pic`: two reels, the
/// tape path, guides, heads, capstan and pinch roller.
fn transport(b: &mut Builder<'_>, pic: NodeId, (x, y, w, h): (f64, f64, f64, f64)) {
    let deck = hw::window(
        b,
        pic,
        "Deck window",
        (x, y, w, h),
        &[(0.0, "#36383b"), (1.0, "#1d1e20")],
    );
    // Brushed deck plate.
    let streaks: Vec<Vec<(f64, f64)>> = (0..)
        .map(|i| 1.5 + 2.1 * f64::from(i))
        .take_while(|yy| *yy < h)
        .map(|yy| vec![(0.0, yy), (w, yy)])
        .collect();
    b.add(
        deck,
        draw::with_opacity(
            parts::stroke_path(
                "brushing",
                (0.0, 0.0, w, h),
                &streaks,
                0.5,
                "#ffffff",
                false,
            ),
            0.05,
        ),
    );

    let r = (w * 0.19).min(52.0);
    let (ly, gy) = (r + 12.0, h - 30.0);
    let (lx, rx) = (w * 0.25, w * 0.75);
    // 7½ / 15 / 30 in/s: each step doubles the turn rate.
    let speed = "0.3 * pow(2, 2*norm(tape_sat/speed))";
    let (fs, ft) = (0.86, 0.58);
    // The tape: off each pack's outer edge, down round the guides, over
    // the heads and through the capstan.
    let (g1, g2) = ((14.0, gy), (w - 14.0, gy));
    let left = draw::polar(lx, ly, r * fs, 248.0);
    let right = draw::polar(rx, ly, r * ft, 112.0);
    art::tape(
        b,
        deck,
        &[
            left,
            (g1.0 - 4.0, g1.1 - 6.0),
            (g1.0 + 4.0, gy + 5.0),
            (g2.0 - 4.0, gy + 5.0),
            (g2.0 + 4.0, g2.1 - 6.0),
            right,
        ],
        2.6,
        "#4a2c18",
    );
    reel(
        b,
        deck,
        (lx, ly, r),
        (fs, 10.0),
        (&format!("{speed} / {fs}"), "supply"),
    );
    reel(
        b,
        deck,
        (rx, ly, r),
        (ft, 70.0),
        (&format!("{speed} / {ft}"), "take-up"),
    );
    // Guides at the corners, the head block between, capstan at the right.
    for g in [g1, g2] {
        art::post(b, deck, g, 4.2);
    }
    let (bx, bw) = (w * 0.3, w * 0.4);
    let block = draw::rounded(
        draw::rect(
            "head block",
            bx,
            gy + 4.0,
            bw,
            16.0,
            draw::linear(180.0, &[(0.0, "#55585c"), (1.0, "#202123")]),
        ),
        3.0,
    );
    b.add(
        deck,
        draw::with_effects(
            draw::stroked(block, "#000000a0", 0.6),
            vec![
                draw::inner_shadow("#ffffff30", 0.0, 1.0, 0.0),
                draw::drop_shadow("#000000a0", 0.0, 2.0, 3.0),
            ],
        ),
    );
    for (i, label) in ["E", "R", "P"].iter().enumerate() {
        let hx = bx + bw * (0.2 + 0.3 * i as f64);
        art::head(b, deck, (hx, gy + 11.0), 13.0, 12.0);
        parts::silk(
            b,
            deck,
            label,
            hx,
            gy + 22.0,
            Ink::new(5.0, "#b9b4aa").weight(800.0).track(0.0),
        );
    }
    let cap = w * 0.8;
    art::post(b, deck, (cap, gy + 5.0), 3.2);
    art::roller(b, deck, (cap + 2.0, gy + 14.0), 7.0);
    hw::window_lip(b, deck);
}

/// One VU on the bridge, reading channel `ch` of the output.
fn vu(
    b: &mut Builder<'_>,
    bridge: NodeId,
    ch: u8,
    (cx, cy, size): (f64, f64, f64),
    legend: &str,
) -> f64 {
    let binding = Binding {
        source: Source::Stream {
            id: EntryId::new(a("out")),
            channel: Some(ch),
        },
        ..Binding::stream(a("out"))
    };
    let p = Place::new(format!("VU {legend}"))
        .size(size)
        .variant("black_rack")
        .legend("VU")
        .bind(binding);
    let vu = parts::put(b, bridge, "meter.vu_amber", &p, cx, cy);
    parts::glass(b, vu);
    let vh = parts::size_of(b, vu).1;
    parts::silk(
        b,
        bridge,
        legend,
        cx,
        cy + vh / 2.0 + 9.0,
        Ink::new(6.6, DIM).weight(800.0).track(0.3),
    );
    vh
}

/// The front, `w × h`, its parts between `x0` and `x1`.
fn front(b: &mut Builder<'_>, panel: NodeId, (w, h): (f64, f64), (x0, x1): (f64, f64)) {
    let wide = x1 - x0 > 800.0;
    let (deck_w, col) = if wide { (330.0, 150.0) } else { (262.0, 128.0) };
    let (top, band) = (16.0, 160.0);

    // ── Transport and meter bridge; or the emphasis EQ ────────────────
    let pic = super::picture(b, panel, "tape_sat", (w, h));
    transport(b, pic, (x0 + 5.0, top, deck_w, band));
    let (bx0, bx1) = (x0 + deck_w + 22.0, x1 - col - 16.0);
    let size = ((bx1 - bx0) / 2.0 - 8.0).min(176.0);
    let cy = top + 62.0;
    let mut vh = 0.0;
    for (i, legend) in ["Left", "Right"].into_iter().enumerate() {
        let cx = bx0 + (bx1 - bx0) * (0.25 + 0.5 * i as f64);
        vh = vu(b, pic, i as u8, (cx, cy, size), legend);
    }
    // The bridge: a black housing round both movements.
    let housing = draw::rounded(
        draw::rect(
            "meter bridge",
            bx0 - 8.0,
            top - 4.0,
            bx1 - bx0 + 16.0,
            band + 8.0,
            draw::linear(180.0, &[(0.0, "#2a2826"), (1.0, "#141312")]),
        ),
        7.0,
    );
    let housing = draw::with_effects(
        draw::stroked(housing, "#000000c0", 1.0),
        vec![
            draw::inner_shadow("#ffffff22", 0.0, 1.0, 0.0),
            draw::drop_shadow("#00000090", 0.0, 2.0, 5.0),
        ],
    );
    let housing = b.add(pic, housing);
    if let Some(p) = b.scene.node_mut(pic) {
        p.children.retain(|c| *c != housing);
        p.children.insert(1, housing);
    }
    let mid = (bx0 + bx1) / 2.0;
    let ry = (cy + vh / 2.0 + 30.0).min(top + band - 6.0);
    parts::lamp(b, pic, (mid - 40.0, ry), 8.0, "#ff4a36", true);
    parts::silk_left(
        b,
        pic,
        "Record",
        mid - 30.0,
        ry,
        Ink::new(6.4, DIM).weight(800.0).track(0.24),
    );
    parts::silk_left(
        b,
        pic,
        "· Level to tape",
        mid + 6.0,
        ry,
        Ink::new(6.4, "#9a9184").weight(700.0).track(0.18),
    );
    super::emphasis_eq(
        b,
        panel,
        "tape_sat",
        (x0, top - 5.0, bx1 - x0, band + 10.0),
        LAMP,
    );

    // ── Badge, EMPHASIS, speed ────────────────────────────────────────
    let cx = x1 - col / 2.0;
    hw::badge(
        b,
        panel,
        (x1 - col, 18.0, col, 38.0),
        ("Tape Machine", 10.5),
        "Record electronics",
    );
    super::emphasis_key(b, panel, "tape_sat", (cx, 78.0), 62.0, "black", LAMP);
    let speed = hw_selector(b, panel, ("Speed", &a("speed")), (cx, 148.0), SPEEDS);
    parts::legend_at(
        b,
        speed,
        "Speed in/s",
        180.0,
        Ink::new(6.6, DIM).weight(800.0).track(0.2),
    );

    // ── Record electronics ────────────────────────────────────────────
    parts::groove(b, panel, x0, x1, 190.0);
    let row = hw::Row {
        look: KNOB,
        ink: (INK, DIM),
        y: h - 86.0,
        legend_y: h - 24.0,
        value_y: h - 11.0,
    };
    let span = x1 - x0;
    let at = |f: f64| x0 + span * f;
    let tape = hw_selector(
        b,
        panel,
        ("Tape", &a("formulation")),
        (at(0.07), row.y),
        FORMULATIONS,
    );
    parts::legend_at(
        b,
        tape,
        "Formulation",
        row.legend_y,
        Ink::new(8.2, INK).weight(800.0).track(0.16),
    );
    let knobs: [hw::Spec<'_>; 6] = [
        ("drive", "Record Level", at(0.215), 76.0, parts::PCT_MARKS),
        ("bias", "Bias", at(0.36), 62.0, super::TRIM_MARKS),
        ("emphasis", "Emphasis", at(0.48), 60.0, super::TRIM_MARKS),
        (
            "compression",
            "Compression",
            at(0.6),
            60.0,
            super::TRIM_MARKS,
        ),
        ("mix", "Mix", at(0.775), 62.0, parts::PCT_MARKS),
        ("output", "Output", at(0.93), 72.0, super::OUT_MARKS),
    ];
    for (addr, name, x, d, marks) in knobs {
        hw::knob(b, panel, &row, (&a(addr), name, x, d, marks));
    }
    parts::vgroove(b, panel, at(0.69), 204.0, h - 14.0);
}

/// A rotary selector of the family's moulding: a black chicken-head, its
/// positions printed round it.
fn hw_selector(
    b: &mut Builder<'_>,
    panel: NodeId,
    (name, addr): (&str, &str),
    (cx, cy): (f64, f64),
    labels: &[&str],
) -> NodeId {
    hw::selector(b, panel, (name, addr), (cx, cy, 50.0), labels, INK)
}

pub fn face() -> Face {
    let scene = canvas("Tape Machine", |b, root| {
        parts::fit_root(b, root, W, H);
        let panel = parts::panel(b, root, "panel.painted", "charcoal", W, H, "ears");
        parts::repaint(b, panel, PAINT);
        front(b, panel, (W, H), (52.0, W - 52.0));
    });

    Face {
        file: "02-tape",
        plugin: "saturate",
        scene,
        params: params(),
        streams: streams(),
    }
}

fn params() -> Vec<frame::surface::proto::ParamInfo> {
    vec![
        parts::percent(&a("drive"), "Record Level", 30.0).featured(),
        parts::trim(&a("bias"), "Bias").featured(),
        parts::trim(&a("emphasis"), "Emphasis"),
        parts::trim(&a("compression"), "Compression"),
        parts::percent(&a("mix"), "Mix", 100.0).featured(),
        super::output(&a("output")),
        parts::choice(&a("speed"), "Speed", SPEED_NAMES, 1).featured(),
        parts::choice(&a("formulation"), "Formulation", FORMULATIONS, 0),
    ]
    .into_iter()
    .chain(super::emphasis_params("tape_sat"))
    .collect()
}

fn streams() -> Vec<frame::surface::proto::StreamInfo> {
    vec![parts::level_stream(&a("out"), "Output Level", 2)]
}

/// The machine alone in a lane ([`Shape::Tall`], 760 × 328): the same
/// front, narrower — transport, meter bridge and badge across the top,
/// the record electronics below.
pub fn tall_face() -> Face {
    let (w, h) = Shape::Tall.size();
    let scene = canvas("Tape Machine (tall)", |b, root| {
        parts::fit_root(b, root, w, h);
        let panel = parts::panel(b, root, "panel.painted", "charcoal", w, h, "none");
        parts::repaint(b, panel, PAINT);
        for (x, y) in [
            (10.0, 10.0),
            (10.0, h - 10.0),
            (w - 10.0, 10.0),
            (w - 10.0, h - 10.0),
        ] {
            art::screw(b, panel, (x, y), 4.0, 30.0);
        }
        front(b, panel, (w, h), (24.0, w - 24.0));
    });
    Face {
        file: Shape::Tall.file("02-tape"),
        plugin: "saturate",
        scene,
        params: params(),
        streams: streams(),
    }
}
