//! Modern Rhythm post-FX: a twin-switch delay in the BigBox class
//! (404 × 286 — the render's 1.22 : 1 box pressed to the shared pedal
//! height) and a reverb in the Standard class (156 × 286).
//!
//! - **Delay 2**: slate blue brushed lid; MIX and FEEDBACK over an LCD that
//!   reads the mode line ("FREE / DAW / TAP · P-P: OFF / ON") and the time
//!   and tempo; SYNC and PING-PONG bat toggles; low cut, TIME, MOD, high cut;
//!   ENGAGE and TAP TEMPO stomps with purple pin-point LEDs beside them.
//! - **Reverb**: dusty rose; MIX and DECAY over low and high cut (printed as
//!   slope glyphs), the shared title, band and stomp.

use document_proto::{NodeId, TextAlign};
use surface_proto::ValueFormat;
use surface_widgets::Builder;

use crate::delay::parts::{self, Screen};
use crate::face::{Face, canvas};
use crate::rigkit::{self as rk, Enclosure, Letter};

use super::PLUGIN;
use super::common::{self, legend};
use super::pre::{self, XL, XR, Y1};

pub fn faces() -> Vec<Face> {
    vec![delay_2(), reverb()]
}

/// A slope glyph: a low cut rising to a flat top, or a flat top falling.
fn slope(b: &mut Builder<'_>, p: NodeId, (cx, cy): (f64, f64), low: bool, color: &str) {
    let pts = if low { vec![(cx - 7.0, cy + 4.0), (cx - 3.0, cy - 1.0), (cx + 7.0, cy - 1.0)] } else { vec![(cx - 7.0, cy - 1.0), (cx + 3.0, cy - 1.0), (cx + 7.0, cy + 4.0)] };
    rk::line(b, p, &pts, 1.4, color);
}

// ── 11 · Delay 2 ─────────────────────────────────────────────────────────

/// Delay 2 — Pedal, BigBox (404 × 286).
fn delay_2() -> Face {
    let a = |s: &str| format!("mrdl2/{s}");
    const INK: &str = "#f3f6f7";
    let scene = canvas("Delay 2", |b, root| {
        let e = Enclosure::new("bigbox", "#5e7f89", "satin").base("#1f2022").radius(11.0);
        let p = rk::pedal_body(b, root, &e);
        let (w, h) = e.size();
        common::brushing(b, p, (8.0, 3.0, w - 16.0, 268.0), 0xde12);
        let knob = pre::knob(INK).size(40.0);
        let lbl = pre::leg(INK);
        // Top row: MIX and FEEDBACK, the toggles, the LCD.
        for (addr, name, x) in [("mix", "MIX", 54.0), ("feedback", "FEEDBACK", 350.0)] {
            let k = pre::place(b, p, &knob, &a(addr), (x, 64.0));
            legend(b, k, name, (x, 28.0), lbl);
        }
        for (addr, name, x, dots) in [("sync", "SYNC", 124.0, 3), ("pingpong", "PING-PONG", 280.0, 2)] {
            let t = pre::toggle(b, p, name, &a(addr), (x, 34.0));
            legend(b, t, name, (x, 9.0), lbl);
            for i in 0..dots {
                let dx = (f64::from(i) - f64::from(dots - 1) / 2.0) * 13.0;
                b.add(p, surface_kit::draw::circle("dot", x + dx, 51.0, 1.3, surface_kit::draw::solid(INK)));
            }
        }
        let win = parts::screen(b, p, "Display", (112.0, 57.0, 180.0, 36.0), Screen { glass: ("#34493f", "#40584c"), ink: "#dcebe1", mesh: false });
        rk::letter_aligned(b, win, "FREE / DAW / TAP", (6.0, 8.0), Letter::new(6.2, 640.0, "#dcebe1").track(0.02), TextAlign::Left);
        rk::letter_aligned(b, win, "P-P: OFF / ON", (174.0, 8.0), Letter::new(6.2, 640.0, "#dcebe1").track(0.02), TextAlign::Right);
        // Fourteen-segment readings, as the LCD shows them.
        let seg = rk::lcd::seg14(10.5, "#dcebe1").track(0.06);
        let t = rk::lcd::readout(b, win, "Time readout", rk::bind(&a("time")), (4.0, 24.0, 86.0), seg, TextAlign::Left, None, ValueFormat { decimals: Some(0), ..ValueFormat::default() });
        let bpm = rk::lcd::readout(b, win, "Tempo readout", rk::bind(&a("bpm")), (86.0, 24.0, 90.0), seg, TextAlign::Right, None, ValueFormat { decimals: Some(1), ..ValueFormat::default() });
        common::capitals(b, t);
        common::capitals(b, bpm);
        // Second row: low cut, TIME, MOD, high cut.
        let row2 = [("low_cut", "", 56.0), ("time", "TIME", 154.0), ("mod", "MOD", 251.0), ("high_cut", "", 349.0)];
        for (addr, name, x) in row2 {
            let k = pre::place(b, p, &knob, &a(addr), (x, 142.0));
            if name.is_empty() {
                slope(b, p, (x, 107.0), addr == "low_cut", INK);
            } else {
                legend(b, k, name, (x, 106.0), lbl);
            }
        }
        common::arrows(b, p, (w, h), 186.0, 20.0, INK);
        // The two stomps, their LEDs inboard of them.
        let on = common::stomp(b, p, "Engage", &a("on"), (54.0, 240.0), 38.0, (40.0, 2.0), false);
        legend(b, on, "ENGAGE", (54.0, 206.0), lbl);
        let tap = common::stomp(b, p, "Tap Tempo", &a("tap"), (350.0, 240.0), 38.0, (-40.0, 2.0), true);
        legend(b, tap, "TAP TEMPO", (350.0, 206.0), lbl);
        rk::letter_fit(b, p, "DELAY-2", (w / 2.0, 214.0), 104.0, Letter::new(18.0, 380.0, INK));
        common::vdc(b, p, (w, h), (w / 2.0, 250.0), INK);
    });
    let params = vec![
        rk::percent(&a("mix"), "Mix", 30.0).featured(),
        rk::percent(&a("feedback"), "Feedback", 40.0).featured(),
        rk::toggle(&a("sync"), "Sync", false),
        rk::toggle(&a("pingpong"), "Ping-Pong", false),
        rk::ms(&a("time"), "Time", 20.0, 2000.0, 1500.0).featured(),
        crate::delay::parts::param(&a("bpm"), "Tempo", surface_proto::Range::linear(40.0, 300.0, surface_proto::Unit::Bpm), 120.0),
        rk::percent(&a("mod"), "Modulation", 20.0),
        rk::hz(&a("low_cut"), "Low Cut", 20.0, 1000.0, 20.0),
        rk::hz(&a("high_cut"), "High Cut", 1000.0, 20000.0, 20000.0),
        rk::toggle(&a("on"), "Engage", true).featured(),
        rk::toggle(&a("tap"), "Tap Tempo", false),
    ];
    Face { file: "11-post-bigbox-delay-two", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

// ── 12 · Reverb ──────────────────────────────────────────────────────────

/// Reverb — Pedal, Standard (156 × 286).
fn reverb() -> Face {
    let a = |s: &str| format!("mrverb/{s}");
    const INK: &str = "#fbf2f3";
    let scene = canvas("Reverb", |b, root| {
        let p = pre::lid(b, root, "#b3616f", INK, "REVERB", &a("on"));
        pre::dial(b, p, INK, &a("mix"), "MIX", (XL, Y1), true);
        pre::dial(b, p, INK, &a("decay"), "DECAY", (XR, Y1), true);
        for (addr, x, low) in [("low_cut", XL, true), ("high_cut", XR, false)] {
            pre::place(b, p, &pre::knob(INK), &a(addr), (x, 104.0));
            slope(b, p, (x, 134.0), low, INK);
        }
    });
    let params = vec![
        rk::percent(&a("mix"), "Mix", 25.0).featured(),
        crate::delay::parts::param(&a("decay"), "Decay", surface_proto::Range::log(0.2, 20.0, surface_proto::Unit::Seconds), 2.5).featured(),
        rk::hz(&a("low_cut"), "Low Cut", 20.0, 1000.0, 100.0),
        rk::hz(&a("high_cut"), "High Cut", 1000.0, 20000.0, 8000.0),
        rk::toggle(&a("on"), "Bypass", true).featured(),
    ];
    Face { file: "12-post-standard-reverb", plugin: PLUGIN, scene, params, streams: Vec::new() }
}
