//! Modern Rhythm pre-FX: four pedals of the Standard 125B class
//! (156 × 286), all one height, brushed anodised lids over a black base,
//! jacks on the sides.
//!
//! What they share, as the suite draws them: tall knurled knobs with grey
//! satin caps and a white dot, each inside a printed ring of fine dots; bold
//! small legends; a mini bat toggle between the knobs; the model name large
//! in a thin geometric sans ("OVERDRIVE- 1"); printed ▶ arrows at the jacks;
//! a purple pin-point LED over a stomp switch in the bottom band, and the
//! 9VDC glyph beside it.

use crate::face::{Face, canvas};
use crate::rigkit::{self as rk, Enclosure, KnobStyle, Letter};

use super::PLUGIN;
use super::common::{self, legend};

pub fn faces() -> Vec<Face> {
    vec![compressor(), overdrive_1(), delay_1(), overdrive_2()]
}

/// Knob centres (the two columns) and the rows, from the reference.
pub(super) const XL: f64 = 33.0;
pub(super) const XR: f64 = 123.0;
pub(super) const Y1: f64 = 47.0;

/// The pedals' knob: a tall grey cap, knurled round its foot, a domed
/// satin top with a white dot (`knob.3d`'s knurled dome, recoloured). Its
/// ring of 21 dots is printed by [`place`].
pub(super) const fn knob(ink: &'static str) -> KnobStyle<'static> {
    KnobStyle::new(super::knob_id("knob.3d"), "knurled_dome", 36.0, ink).opts(KNOB_LOOK)
}

const KNOB_LOOK: &[&str] = &[
    "ticks=none",
    "color=#4d4f53",
    "metal=0.35",
    "rough=0.34",
    "coat=0.3",
    "aniso=0.4",
    "ribs=30",
    "knurl_band=0.55",
    "rib_depth=0.065",
    "height=0.72",
    "dome=0.07",
    "edge=0.1",
    "cap_r=0.92",
    "index=dot",
    "pointer=#f4f4f2",
    "index_w=0.07",
    "index_in=0.76",
    "index_out=0.76",
    "fallback=knurled",
    "fallback_color=#4a4b4f",
];

/// A pedal knob `s` with its shaft at `at`, inside the lid's printed ring of
/// 21 fine dots; no legend (the caller cuts it).
pub(super) fn place(
    b: &mut surface_widgets::Builder<'_>,
    p: document_proto::NodeId,
    s: &KnobStyle<'_>,
    addr: &str,
    (cx, cy): (f64, f64),
) -> document_proto::NodeId {
    let r = s.d / 2.0 * 1.34;
    let dot = (s.d * 0.022).max(0.55);
    let n = 21;
    let d: String = (0..n)
        .map(|i| {
            let a = (f64::from(i) / f64::from(n - 1) - 0.5) * 300.0;
            let (x, y) = surface_kit::draw::polar(cx, cy, r, a);
            format!("M {:.3} {y:.3} A {dot:.3} {dot:.3} 0 1 1 {:.3} {y:.3} A {dot:.3} {dot:.3} 0 1 1 {:.3} {y:.3} Z ", x - dot, x + dot, x - dot)
        })
        .collect();
    let (w, h) = b
        .scene
        .node(p)
        .map_or((cx * 2.0, cy * 2.0), |n| (n.size.width, n.size.height));
    b.add(
        p,
        surface_kit::draw::path(
            "scale dots",
            0.0,
            0.0,
            w,
            h,
            d,
            surface_kit::draw::solid(s.ink),
        ),
    );
    rk::knob(
        b,
        p,
        s,
        addr,
        "",
        (cx, cy),
        0.0,
        crate::delay::parts::Ink::new(6.0, s.ink),
    )
}

/// A legend's lettering: bold, small, as the lids print it.
pub(super) fn leg(color: &str) -> Letter<'_> {
    Letter::new(6.2, 720.0, color).track(0.04)
}

/// The shared lid and printing; `ink` is the silkscreen colour.
pub(super) fn lid<'a>(
    b: &mut surface_widgets::Builder<'_>,
    root: document_proto::NodeId,
    color: &'a str,
    ink: &'a str,
    title: &str,
    bypass: &str,
) -> document_proto::NodeId {
    let e = Enclosure::new("standard", color, "satin")
        .base("#1f2022")
        .radius(9.0);
    let p = rk::pedal_body(b, root, &e);
    let (w, h) = e.size();
    common::brushing(
        b,
        p,
        (8.0, 3.0, w - 16.0, 268.0),
        color.len() as u64 * 7919 + title.len() as u64,
    );
    rk::letter_fit(
        b,
        p,
        title,
        (w / 2.0, 176.0),
        128.0f64.min(title.chars().count() as f64 * 13.6),
        Letter::new(18.0, 380.0, ink).track(0.0),
    );
    common::arrows(b, p, (w, h), 210.0, 13.0, ink);
    common::vdc(b, p, (w, h), (w - 21.0, 246.0), ink);
    common::band(b, p, w, bypass);
    p
}

/// A knob with its legend above (`above`) or below it.
#[allow(clippy::too_many_arguments)]
pub(super) fn dial(
    b: &mut surface_widgets::Builder<'_>,
    p: document_proto::NodeId,
    ink: &'static str,
    addr: &str,
    name: &str,
    at: (f64, f64),
    above: bool,
) {
    let k = place(b, p, &knob(ink), addr, at);
    let ly = if above { at.1 - 34.5 } else { at.1 + 29.0 };
    legend(b, k, name, (at.0, ly), leg(ink));
}

/// A mini bat toggle whose nut sits at (`cx`, `cy`).
pub(super) fn toggle(
    b: &mut surface_widgets::Builder<'_>,
    p: document_proto::NodeId,
    name: &str,
    addr: &str,
    (cx, cy): (f64, f64),
) -> document_proto::NodeId {
    // The kit's toggle keeps a caption strip over it: its nut sits 6 px
    // under its box's centre.
    rk::mini_toggle(b, p, name, addr, (cx, cy - 6.0), 11.0, "chrome")
}

// ── 01 · Compressor ──────────────────────────────────────────────────────

/// Compressor — Pedal, Standard (156 × 286). Brushed silver, dark ink:
/// COMP and LEVEL, the ATTACK toggle (SMOOTH / SNAPPY) between them.
fn compressor() -> Face {
    let a = |s: &str| format!("mrcomp/{s}");
    const INK: &str = "#1b1c1e";
    let scene = canvas("Compressor", |b, root| {
        let p = lid(b, root, "#c3c5c7", INK, "COMPRESSOR", &a("on"));
        dial(b, p, INK, &a("comp"), "COMP", (XL, Y1), true);
        dial(b, p, INK, &a("level"), "LEVEL", (XR, Y1), true);
        let t = toggle(b, p, "Attack", &a("attack"), (78.0, 94.0));
        legend(b, t, "ATTACK", (78.0, 73.0), leg(INK));
        rk::letter(b, p, "SMOOTH", (46.0, 94.0), leg(INK));
        rk::letter(b, p, "SNAPPY", (110.0, 94.0), leg(INK));
    });
    let params = vec![
        rk::percent(&a("comp"), "Comp", 50.0).featured(),
        rk::db(&a("level"), "Level", -12.0, 12.0, 0.0).featured(),
        rk::toggle(&a("attack"), "Attack Snappy", false),
        rk::toggle(&a("on"), "Bypass", true).featured(),
    ];
    Face {
        file: "01-pre-standard-compressor",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

// ── 02 · Overdrive 1 ─────────────────────────────────────────────────────

/// Overdrive 1 — Pedal, Standard (156 × 286). Brushed teal, white ink:
/// DRIVE and LEVEL, TONE under them, the clipping toggle between (soft
/// knee over it, hard line under it).
fn overdrive_1() -> Face {
    let a = |s: &str| format!("mrod1/{s}");
    const INK: &str = "#f4f6f5";
    let scene = canvas("Overdrive 1", |b, root| {
        let p = lid(b, root, "#3e8a79", INK, "OVERDRIVE- 1", &a("on"));
        dial(b, p, INK, &a("drive"), "DRIVE", (XL, Y1), true);
        dial(b, p, INK, &a("level"), "LEVEL", (XR, Y1), true);
        dial(b, p, INK, &a("tone"), "TONE", (78.0, 106.0), false);
        toggle(b, p, "Clip", &a("clip"), (78.0, 61.5));
        rk::line(
            b,
            p,
            &[(71.0, 48.0), (75.0, 48.0), (79.5, 43.0), (85.0, 43.0)],
            1.0,
            INK,
        );
        rk::line(b, p, &[(72.0, 83.0), (84.0, 83.0)], 1.2, INK);
    });
    let params = vec![
        rk::percent(&a("drive"), "Drive", 50.0).featured(),
        rk::db(&a("level"), "Level", -12.0, 12.0, 0.0).featured(),
        rk::percent(&a("tone"), "Tone", 50.0).featured(),
        rk::toggle(&a("clip"), "Soft Clip", true),
        rk::toggle(&a("on"), "Bypass", true).featured(),
    ];
    Face {
        file: "02-pre-standard-overdrive-one",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

// ── 03 · Delay 1 ─────────────────────────────────────────────────────────

/// Delay 1 — Pedal, Standard (156 × 286). Brushed purple, white ink: an LCD
/// reading the time ("500 ms"), MIX and FBK, TIME and TONE, the NOTE / MS
/// toggle between them.
fn delay_1() -> Face {
    let a = |s: &str| format!("mrdl1/{s}");
    const INK: &str = "#f6f3fa";
    let scene = canvas("Delay 1", |b, root| {
        let p = lid(b, root, "#7a58a3", INK, "DELAY- 1", &a("on"));
        common::lcd(
            b,
            p,
            "Time readout",
            &a("time"),
            (53.0, 9.0, 50.0, 18.0),
            9.5,
            None,
        );
        dial(b, p, INK, &a("mix"), "MIX", (XL, Y1), true);
        dial(b, p, INK, &a("feedback"), "FBK", (XR, Y1), true);
        dial(b, p, INK, &a("time"), "TIME", (XL, 97.0), false);
        dial(b, p, INK, &a("tone"), "TONE", (XR, 97.0), false);
        let t = toggle(b, p, "Time mode", &a("sync"), (78.0, 72.0));
        legend(b, t, "NOTE", (78.0, 51.5), leg(INK));
        rk::letter(b, p, "MS", (78.0, 93.5), leg(INK));
    });
    let params = vec![
        rk::percent(&a("mix"), "Mix", 30.0).featured(),
        rk::percent(&a("feedback"), "Feedback", 35.0),
        rk::ms(&a("time"), "Time", 20.0, 2000.0, 500.0).featured(),
        rk::percent(&a("tone"), "Tone", 60.0),
        rk::toggle(&a("sync"), "Note Sync", false),
        rk::toggle(&a("on"), "Bypass", true).featured(),
    ];
    Face {
        file: "03-pre-standard-delay-one",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

// ── 04 · Overdrive 2 ─────────────────────────────────────────────────────

/// Overdrive 2 — Pedal, Standard (156 × 286). Brushed orange, white ink:
/// GAIN and LEVEL over BASS and TREBLE.
fn overdrive_2() -> Face {
    let a = |s: &str| format!("mrod2/{s}");
    const INK: &str = "#fbf4ef";
    let scene = canvas("Overdrive 2", |b, root| {
        let p = lid(b, root, "#cf5f36", INK, "OVERDRIVE- 2", &a("on"));
        dial(b, p, INK, &a("gain"), "GAIN", (XL, Y1), true);
        dial(b, p, INK, &a("level"), "LEVEL", (XR, Y1), true);
        dial(b, p, INK, &a("bass"), "BASS", (XL, 106.0), false);
        dial(b, p, INK, &a("treble"), "TREBLE", (XR, 106.0), false);
    });
    let params = vec![
        rk::percent(&a("gain"), "Gain", 55.0).featured(),
        rk::db(&a("level"), "Level", -12.0, 12.0, 0.0).featured(),
        rk::db(&a("bass"), "Bass", -12.0, 12.0, 0.0),
        rk::db(&a("treble"), "Treble", -12.0, 12.0, 0.0),
        rk::toggle(&a("on"), "Bypass", true).featured(),
    ];
    Face {
        file: "04-pre-standard-overdrive-two",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}
