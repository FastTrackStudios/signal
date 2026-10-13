//! The analog delay: a bucket-brigade stompbox, pedal-shop green.
//!
//! A double-wide pedal seen from above. Top row: TIME and REPEATS, the red
//! LED readout (the time in ms and the note it is locked to), MIX. Second
//! row: the chip's own controls — how far the clock drifts (CLOCK), how
//! much top falls off the end of the chain (DEGRADE), WARBLE, TONE and DUCK.
//! At the foot, a window in the lid onto the board with the BBD chip on it,
//! between the two stomp switches: BYPASS (its LED is the on lamp) and TAP.

use document_proto::TextAlign;
use surface_kit::{Place, draw};

use super::art;
use super::parts::{self, Ink, Screen, bind};
use crate::face::{Face, canvas};

const W: f64 = 860.0;
const H: f64 = 520.0;
const INK: &str = "#f1ead2";
const DIM: &str = "#b5c7b3";
const LED: &str = "#ff3b2e";
const T_MIN: f64 = 20.0;
const T_MAX: f64 = 1000.0;

fn a(s: &str) -> String {
    format!("analog/{s}")
}

pub fn face() -> Face {
    let scene = canvas("Analog Delay", |b, root| {
        parts::fit_root(b, root, W, H);
        let lid = art::enclosure(b, root, (W, H), &[(0.0, "#3a6b56"), (0.5, "#2b5444"), (1.0, "#1d3b30")], (0.07, 0.035));

        // ── Title ──────────────────────────────────────────────────────
        parts::silk(b, lid, "Analog Delay", W / 2.0 - 4.0, 48.0, Ink::new(22.0, INK).weight(800.0).track(0.08));
        parts::silk(b, lid, "Bucket brigade · 4096 stages", W / 2.0 - 4.0, 74.0, Ink::new(7.5, DIM).weight(700.0).track(0.24));
        parts::silk_left(b, lid, "← Output", 40.0, 30.0, Ink::new(6.5, DIM).weight(700.0).track(0.2));
        parts::silk_right(b, lid, "Input →", W - 48.0, 30.0, Ink::new(6.5, DIM).weight(700.0).track(0.2));

        let scale = Ink::new(7.0, DIM).weight(700.0).track(0.0).plain();
        let legend = Ink::new(10.0, INK).weight(800.0).track(0.16);
        let value = Ink::new(7.5, DIM).weight(600.0).track(0.02).plain();

        // ── Top row: time, repeats | readout | mix ────────────────────
        let r1 = 156.0;
        let tm = parts::time_marks(T_MIN, T_MAX);
        let big: [(&str, &str, f64, &[(f64, &str)]); 3] = [("time", "Time", 150.0, &tm), ("feedback", "Repeats", 316.0, parts::FB_MARKS), ("mix", "Mix", 700.0, parts::PCT_MARKS)];
        for (addr, name, x, marks) in big {
            let k = parts::knob(b, lid, "knob.skirted", "black", &a(addr), name, (x, r1), 74.0);
            let sweep = parts::sweep_of(b, k);
            parts::marks(b, lid, (x, r1), 44.0, sweep, marks, 11, scale);
            parts::legend_at(b, k, name, r1 + 64.0, legend);
            if addr != "time" {
                parts::value_at(b, k, r1 + 80.0, value);
            }
        }
        let win = parts::screen(b, lid, "Readout", (432.0, r1 - 44.0, 176.0, 62.0), Screen { glass: ("#140202", "#250505"), ink: LED, mesh: false });
        parts::readout(b, win, "Delay time", bind(&a("time")), (6.0, 22.0, 164.0), 24.0, LED, TextAlign::Center);
        parts::note_picker(b, win, "Note", &a("div"), (54.0, 47.0, 68.0), 11.0, LED, "#7a1a14");
        let sync = Place::new("Sync").size(34.0).variant("chrome").legend("Sync").options(&["On", "Off"]).bind(bind(&a("sync")));
        let sw = parts::put(b, lid, "switch.toggle_bat", &sync, 520.0, r1 + 58.0);
        parts::caption_style(b, sw, Ink::new(7.0, INK).weight(800.0).track(0.14));

        // ── Second row: the chip's controls ────────────────────────────
        let r2 = 306.0;
        let small: [(&str, &str, f64); 5] = [("clock", "Clock", 140.0), ("degrade", "Degrade", 285.0), ("warble", "Warble", 430.0), ("tone", "Tone", 575.0), ("duck", "Duck", 720.0)];
        for (addr, name, x) in small {
            let k = parts::knob(b, lid, "knob.skirted", "black", &a(addr), name, (x, r2), 52.0);
            let sweep = parts::sweep_of(b, k);
            parts::marks(b, lid, (x, r2), 32.0, sweep, parts::PCT_MARKS, 11, scale);
            parts::legend_at(b, k, name, r2 + 50.0, legend.size(9.0));
        }

        // ── Foot: bypass | window onto the chip | tap ──────────────────
        let fy = 452.0;
        let foot = Ink::new(7.0, DIM).weight(700.0).track(0.2);
        let bypass = Place::new("Bypass").size(34.0).variant("red").bind(bind(&a("on")));
        let on = parts::footswitch(b, lid, &bypass, (150.0, fy));
        parts::label_at(b, on, "On", (180.0, fy - 36.0), foot);
        let tap = Place::new("Tap").size(34.0).variant("amber").options(&["momentary"]).bind(bind(&a("tap")));
        let tap = parts::footswitch(b, lid, &tap, (710.0, fy));
        parts::label_at(b, tap, "Tap", (740.0, fy - 36.0), foot);
        let board = parts::well(b, lid, "Window", (282.0, 386.0, 296.0, 96.0), draw::linear(160.0, &[(0.0, "#1f4a2a"), (1.0, "#12301b")]), 6.0);
        // Traces on the board, then the chip over them.
        let traces: Vec<Vec<(f64, f64)>> = (0..6)
            .map(|i| {
                let y = 14.0 + f64::from(i) * 13.0;
                vec![(0.0, y), (60.0, y), (74.0, y + 6.0), (230.0, y + 6.0), (244.0, y), (296.0, y)]
            })
            .collect();
        b.add(board, parts::stroke_path("traces", (0.0, 0.0, 296.0, 96.0), &traces, 1.6, "#c9a24a55", false));
        art::chip(b, board, (148.0, 48.0), (150.0, 44.0), 8, "BBD-4096", "Dual · low noise");
        for (x, y) in [(22.0, 20.0), (22.0, 76.0), (274.0, 20.0), (274.0, 76.0)] {
            b.add(board, draw::rounded(draw::rect("cap", x - 6.0, y - 4.0, 12.0, 8.0, draw::linear(180.0, &[(0.0, "#d9b25c"), (1.0, "#8a6a24")])), 2.0));
        }
    });

    let params = vec![
        parts::time_ms(&a("time"), "Time", T_MIN, T_MAX, 375.0).featured(),
        parts::toggle(&a("sync"), "Sync", false),
        parts::choice(&a("div"), "Note", parts::DIVISIONS, parts::DEFAULT_DIV),
        parts::feedback(&a("feedback"), "Repeats", 40.0).featured(),
        parts::percent(&a("mix"), "Mix", 35.0).featured(),
        parts::percent(&a("clock"), "Clock", 30.0),
        parts::percent(&a("degrade"), "Degrade", 40.0),
        parts::percent(&a("warble"), "Warble", 20.0),
        parts::percent(&a("tone"), "Tone", 50.0),
        parts::percent(&a("duck"), "Duck", 0.0),
        parts::toggle(&a("on"), "Bypass", true).featured(),
        parts::toggle(&a("tap"), "Tap Tempo", false),
    ];
    Face { file: "03-analog", plugin: "delay", scene, params, streams: Vec::new() }
}
