//! Blackout Gain pre-FX: three Standard (125B, 156 × 286) boxes in black
//! with white line-art, jacks on top. Every box is framed by the same
//! notched border, carries the maker's badge at its foot, and puts its LED
//! and plain stomp switch in the bottom band (the lower 30 %), the switch
//! in a printed ring.

use document_proto::NodeId;
use surface_widgets::Builder;

use super::PLUGIN;
use super::art::{self, BLACK, GREEN, INK, Put, RED};
use crate::delay::parts::Ink;
use crate::face::{Face, canvas};
use crate::rigkit::{self as rk, Enclosure, KnobStyle};

pub fn faces() -> Vec<Face> {
    vec![gate(), drive(), boost()]
}

const BIG: KnobStyle<'static> = super::pedal_knob(48.0);
const SMALL: KnobStyle<'static> = super::pedal_knob(40.0);

fn enclosure() -> Enclosure<'static> {
    Enclosure::new("standard", BLACK, "satin").base("#0b0b0b").jacks("top")
}

/// The border, the badge and the switch with its LED: what every box shares.
fn furniture(b: &mut Builder<'_>, p: NodeId, size: (f64, f64), addr: &str, led: &str, supply: &str) {
    art::border(b, p, size, 7.0, (15.0, 272.0), supply);
    let (w, _) = size;
    art::switch_ring(b, p, (w / 2.0, 235.0), 21.0);
    art::stomp(b, p, "Bypass", addr, (w / 2.0, 235.0), 34.0, led, (0.0, -31.0), 17.0, false);
    art::badge(b, p, size, (w / 2.0, 265.0), 100.0);
}

// ── 01 · Ward (noise gate) ───────────────────────────────────────────────

/// Ward — Pedal, Standard (156 × 286). A noise gate: one big knob in a
/// petal rosette, knots in the top corners, two skulls looking out with
/// braided locks between them, the name in hollow block capitals, a green
/// LED.
fn gate() -> Face {
    let ns = "bggate";
    let a = |s: &str| format!("{ns}/{s}");
    let scene = canvas("Ward", |b, root| {
        let e = enclosure();
        let p = rk::pedal_body(b, root, &e);
        let size = e.size();
        let (w, _) = size;
        art::knot(b, p, size, (27.0, 36.0), 26.0);
        art::knot(b, p, size, (w - 27.0, 36.0), 26.0);
        art::flourish(b, p, size, (24.0, 58.0), 26.0, false);
        art::flourish(b, p, size, (w - 24.0, 58.0), 26.0, true);
        art::rosette(b, p, size, (w / 2.0, 68.0), 54.0, 7);
        art::skull(b, p, size, &Put::new(0.0, 86.0, 70.0, true));
        art::skull(b, p, size, &Put::new(w - 70.0, 86.0, 70.0, false));
        art::locks(b, p, size, (w / 2.0 - 15.0, 94.0), 64.0, 4, 3.4, 1.3);
        art::locks(b, p, size, (w / 2.0 + 15.0, 94.0), 64.0, 4, 3.4, 1.3);
        art::hollow_title(b, p, "WARD", (w / 2.0, 172.0), (118.0, 36.0), 1.5);
        rk::knob(b, p, &BIG, &a("threshold"), "", (w / 2.0, 68.0), 0.0, Ink::new(6.0, INK));
        furniture(b, p, size, &a("on"), GREEN, "9V DC");
    });
    let params = vec![rk::db(&a("threshold"), "Threshold", -96.0, 0.0, -60.0).featured(), rk::toggle(&a("on"), "Bypass", true).featured()];
    Face { file: "01-pre-standard-ward-gate", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

// ── 02 · Omen (drive) ────────────────────────────────────────────────────

/// Omen — Pedal, Standard (156 × 286). A three-knob drive: DRIVE and LEVEL
/// high in the corners, TONE between and below them, two skulls face to
/// face, the name in tall condensed capitals, the red LED in a hexagram.
fn drive() -> Face {
    let ns = "bgdrive";
    let a = |s: &str| format!("{ns}/{s}");
    let scene = canvas("Omen", |b, root| {
        let e = enclosure();
        let p = rk::pedal_body(b, root, &e);
        let size = e.size();
        let (w, _) = size;
        art::skull(b, p, size, &Put::new(-2.0, 94.0, 70.0, false));
        art::skull(b, p, size, &Put::new(w - 68.0, 94.0, 70.0, true));
        art::locks(b, p, size, (12.0, 150.0), 44.0, 2, 4.0, 1.0);
        art::locks(b, p, size, (w - 12.0, 150.0), 44.0, 2, 4.0, 1.0);
        let none = Ink::new(6.0, INK);
        for (addr, name, at, ly) in [("drive", "DRIVE", (34.0, 40.0), 72.0), ("level", "LEVEL", (w - 34.0, 40.0), 72.0), ("tone", "TONE", (w / 2.0, 72.0), 36.0)] {
            let k = rk::knob(b, p, &SMALL, &a(addr), "", at, 0.0, none);
            art::label_letter(b, k, name, (at.0, ly), art::knob_letter(10.0));
        }
        art::title(b, p, "OMEN", (w / 2.0, 176.0), 74.0);
        hexagram(b, p, size, (w / 2.0, 204.0), 15.0);
        furniture(b, p, size, &a("on"), RED, "9-32V DC");
    });
    let params = vec![
        rk::percent(&a("drive"), "Drive", 50.0).featured(),
        rk::percent(&a("tone"), "Tone", 50.0),
        rk::db(&a("level"), "Level", -24.0, 12.0, 0.0).featured(),
        rk::toggle(&a("on"), "Bypass", true).featured(),
    ];
    Face { file: "02-pre-standard-omen-drive", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

/// Two interlaced triangles, the upper one taller (the drive's sigil).
fn hexagram(b: &mut Builder<'_>, p: NodeId, size: (f64, f64), (cx, cy): (f64, f64), r: f64) {
    let up = format!("M {cx} {} L {} {} L {} {} Z", cy - r * 1.35, cx + r, cy + r * 0.55, cx - r, cy + r * 0.55);
    let down = format!("M {cx} {} L {} {} L {} {} Z", cy + r * 1.1, cx + r, cy - r * 0.55, cx - r, cy - r * 0.55);
    for d in [up, down] {
        let mut n = surface_kit::draw::path("sigil", 0.0, 0.0, size.0, size.1, d, surface_kit::draw::solid(RED));
        n.fills = Vec::new();
        n.stroke = document_proto::Stroke { paints: vec![surface_kit::draw::solid("#ff6a5c")], weight: 1.0, ..document_proto::Stroke::default() };
        b.add(p, n);
    }
}

// ── 03 · Maul (boost) ────────────────────────────────────────────────────

/// Maul — Pedal, Standard (156 × 286). A one-knob boost: the knob over a
/// lattice of knots and crossed bars, skulls either side, a star
/// medallion round the green LED, the name in wide capitals.
fn boost() -> Face {
    let ns = "bgboost";
    let a = |s: &str| format!("{ns}/{s}");
    let scene = canvas("Maul", |b, root| {
        let e = enclosure();
        let p = rk::pedal_body(b, root, &e);
        let size = e.size();
        let (w, _) = size;
        // The lattice: two crossed bars and knots at their ends.
        for (x0, x1) in [(24.0, w - 38.0), (w - 24.0, 38.0)] {
            rk::line(b, p, &[(x0, 28.0), (x1, 132.0)], 4.0, INK);
            rk::line(b, p, &[(x0, 28.0), (x1, 132.0)], 1.4, BLACK);
        }
        art::knot(b, p, size, (28.0, 32.0), 22.0);
        art::knot(b, p, size, (w - 28.0, 32.0), 22.0);
        art::knot(b, p, size, (w / 2.0, 118.0), 26.0);
        art::skull(b, p, size, &Put::new(2.0, 50.0, 50.0, true));
        art::skull(b, p, size, &Put::new(w - 52.0, 50.0, 50.0, false));
        rk::knob(b, p, &BIG, &a("level"), "", (w / 2.0, 72.0), 0.0, Ink::new(6.0, INK));
        art::flourish(b, p, size, (30.0, 150.0), 34.0, false);
        art::flourish(b, p, size, (w - 30.0, 150.0), 34.0, true);
        rk::letter_fit(b, p, "MAUL", (w / 2.0, 176.0), 112.0, rk::Letter::new(20.0, 400.0, INK).family(surface_kit::draw::family::PIRATA_ONE).track(0.22));
        medallion(b, p, size, (w / 2.0, 204.0), 13.0);
        furniture(b, p, size, &a("on"), GREEN, "9V DC");
    });
    let params = vec![rk::db(&a("level"), "Boost", 0.0, 20.0, 10.0).featured(), rk::toggle(&a("on"), "Bypass", true).featured()];
    Face { file: "03-pre-standard-maul-boost", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

/// An eight-point star in a ring (the boost's medallion round its LED).
fn medallion(b: &mut Builder<'_>, p: NodeId, size: (f64, f64), (cx, cy): (f64, f64), r: f64) {
    let pts: Vec<(f64, f64)> = (0..16).map(|i| surface_kit::draw::polar(cx, cy, if i % 2 == 0 { r * 1.25 } else { r * 0.9 }, 22.5 * f64::from(i))).collect();
    b.add(p, surface_kit::draw::path("star", 0.0, 0.0, size.0, size.1, surface_kit::draw::polygon(&pts), surface_kit::draw::solid(INK)));
    b.add(p, surface_kit::draw::circle("star heart", cx, cy, r * 0.8, surface_kit::draw::solid(BLACK)));
}
