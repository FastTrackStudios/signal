//! The pitch-shifting delay: cold and electronic, because the effect is
//! not natural.
//!
//! A violet rack unit with a graphic display that draws the effect
//! literally — each repeat a step up the staircase by the interval — and
//! reads the interval in semitones beside the time (ms and note). INTERVAL
//! is the big knob in the middle of the row, stepped in semitones with the
//! fifth and the octave marked; SPREAD detunes the two sides against each
//! other in cents. The voicing keys pick plain pitch repeats or shimmer
//! (each repeat an octave up, fed back into the next).

use frame::document::proto::{NodeId, TextAlign};
use frame::surface::kit::Place;
use frame::surface::proto::{Range, Unit};
use frame::surface::widgets::Builder;

use super::art;
use super::parts::{self, Ink, Screen, bind};
use crate::face::{Face, canvas};

const W: f64 = 960.0;
const H: f64 = 300.0;
const INK: &str = "#ece7fa";
const DIM: &str = "#9d95bd";
const LCD: &str = "#8fb2ff";
const LCD_DIM: &str = "#44578a";
const MODES: &[&str] = &["Pitch", "Shimmer"];
const T_MIN: f64 = 1.0;
const T_MAX: f64 = 4000.0;

fn a(s: &str) -> String {
    format!("pitch/{s}")
}

/// The staircase: repeats climbing a fifth at a time.
fn staircase(b: &mut Builder<'_>, win: NodeId, (x, y, w, h): (f64, f64, f64, f64)) {
    art::graticule(b, win, (x, y, w, h), (8, 4), "#8fb2ff14");
    let n = 7;
    let step_w = w / f64::from(n);
    let rise = h / f64::from(n + 1);
    let mut pts = Vec::new();
    for i in 0..n {
        let level = 0.78_f64.powi(i);
        let bx = x + f64::from(i) * step_w;
        let by = y + h - rise * f64::from(i + 1);
        art::bar(
            b,
            win,
            (bx + 3.0, by - 3.0, step_w - 6.0, 6.0),
            LCD,
            0.3 + 0.7 * level,
        );
        pts.push((bx + step_w / 2.0, by));
    }
    // The dry note, on the floor.
    let floor = y + h - 2.0;
    parts::rule(b, win, x, x + w, floor, 1.0, LCD_DIM);
    art::tag(b, win, "+7 st per repeat", (x + 2.0, y + 8.0), 6.5, LCD_DIM);
}

pub fn face() -> Face {
    let scene = canvas("Pitch Delay", |b, root| {
        parts::fit_root(b, root, W, H);
        let panel = parts::panel(b, root, "panel.brushed", "black", W, H, "ears");
        parts::repaint(
            b,
            panel,
            &[(0.0, "#2b2540"), (0.5, "#1f1a2f"), (1.0, "#141120")],
        );

        // ── Model line ─────────────────────────────────────────────────
        parts::silk_left(
            b,
            panel,
            "Pitch Echo",
            50.0,
            34.0,
            Ink::new(15.0, INK).weight(800.0).track(0.08),
        );
        parts::rule(b, panel, 50.0, 214.0, 47.0, 1.0, "#ece7fa38");
        parts::silk_left(
            b,
            panel,
            "Harmonic delay · ±12 semitones",
            50.0,
            60.0,
            Ink::new(6.8, DIM).weight(600.0).track(0.16),
        );
        let modes = Place::new("Voicing")
            .size(46.0)
            .variant("blue")
            .bind(bind(&a("mode")))
            .options(MODES);
        parts::put(b, panel, "button.illuminated_bank", &modes, 132.0, 98.0);
        parts::silk(
            b,
            panel,
            "Voicing",
            132.0,
            136.0,
            Ink::new(6.5, DIM).weight(700.0).track(0.2),
        );

        // ── Display ────────────────────────────────────────────────────
        let win = parts::screen(
            b,
            panel,
            "Display",
            (262.0, 24.0, 440.0, 112.0),
            Screen {
                glass: ("#070a16", "#0c1124"),
                ink: LCD,
                mesh: false,
            },
        );
        parts::shader(b, win, "crt", &[("strength", 0.35), ("scale", 3.0)]);
        art::tag(b, win, "Interval", (16.0, 16.0), 7.0, LCD_DIM);
        parts::readout(
            b,
            win,
            "Interval",
            bind(&a("interval")),
            (12.0, 46.0, 126.0),
            30.0,
            LCD,
            TextAlign::Left,
        );
        art::tag(b, win, "Spread", (16.0, 76.0), 7.0, LCD_DIM);
        parts::readout(
            b,
            win,
            "Spread",
            bind(&a("spread")),
            (70.0, 76.0, 70.0),
            11.0,
            LCD,
            TextAlign::Left,
        );
        art::tag(b, win, "Time", (150.0, 16.0), 7.0, LCD_DIM);
        parts::readout(
            b,
            win,
            "Time",
            bind(&a("time")),
            (146.0, 46.0, 116.0),
            22.0,
            LCD,
            TextAlign::Left,
        );
        parts::note_picker(
            b,
            win,
            "Note",
            &a("div"),
            (150.0, 78.0, 66.0),
            11.0,
            LCD,
            LCD_DIM,
        );
        parts::readout(
            b,
            win,
            "Time mode",
            bind(&a("sync")),
            (222.0, 78.0, 40.0),
            9.0,
            LCD,
            TextAlign::Left,
        );
        staircase(b, win, (272.0, 14.0, 156.0, 84.0));

        let sync = Place::new("Time mode")
            .size(40.0)
            .bind(bind(&a("sync")))
            .options(parts::TIME_MODES);
        parts::put(b, panel, "button.square_bank", &sync, 790.0, 60.0);
        let tap = Place::new("Tap")
            .size(40.0)
            .variant("blue")
            .legend("Tap")
            .options(&["momentary"])
            .bind(bind(&a("tap")));
        parts::put(b, panel, "button.illuminated", &tap, 790.0, 110.0);
        for (i, x) in [874.0, 890.0].into_iter().enumerate() {
            parts::led_column(
                b,
                panel,
                if i == 0 { "Wet L" } else { "Wet R" },
                &a("wet"),
                i as u8,
                (x, 28.0, 12.0, 100.0),
                13,
                (-36.0, 3.0),
            );
        }
        parts::silk(
            b,
            panel,
            "Wet",
            888.0,
            140.0,
            Ink::new(6.5, DIM).weight(700.0).track(0.2),
        );

        // ── Knob row ───────────────────────────────────────────────────
        parts::groove(b, panel, 40.0, W - 40.0, 156.0);
        let row = 212.0;
        let legend = Ink::new(8.0, INK).weight(700.0).track(0.14);
        let value = Ink::new(7.5, DIM).weight(600.0).track(0.02).plain();
        let scale = Ink::new(6.5, DIM).weight(600.0).track(0.0).plain();
        let tm = parts::time_marks(T_MIN, T_MAX);
        let iv: [(f64, &str); 7] = [
            (0.0, "-12"),
            (5.0 / 24.0, "-7"),
            (7.0 / 24.0, "-5"),
            (0.5, "0"),
            (17.0 / 24.0, "+5"),
            (19.0 / 24.0, "+7"),
            (1.0, "+12"),
        ];
        let knobs: [(&str, &str, f64, f64, &[(f64, &str)], usize); 8] = [
            ("time", "Time", 96.0, 54.0, &tm, 11),
            ("feedback", "Feedback", 200.0, 54.0, parts::FB_MARKS, 11),
            ("interval", "Interval", 330.0, 64.0, &iv, 25),
            (
                "spread",
                "Spread",
                456.0,
                44.0,
                &[(0.0, "0"), (1.0, "50")],
                11,
            ),
            ("tone", "Tone", 546.0, 44.0, parts::PCT_MARKS, 11),
            ("drive", "Drive", 636.0, 44.0, parts::PCT_MARKS, 11),
            ("duck", "Duck", 726.0, 44.0, parts::PCT_MARKS, 11),
            ("mix", "Mix", 852.0, 50.0, parts::PCT_MARKS, 11),
        ];
        for (addr, name, x, d, marks, minors) in knobs {
            let k = parts::knob(b, panel, "knob.silver_top", "", &a(addr), name, (x, row), d);
            let sweep = parts::sweep_of(b, k);
            parts::marks(
                b,
                panel,
                (x, row),
                d / 2.0 + 5.0,
                sweep,
                marks,
                minors,
                scale,
            );
            parts::legend_at(b, k, name, 266.0, legend);
            if !matches!(addr, "time" | "interval" | "spread") {
                parts::value_at(b, k, 280.0, value);
            }
        }
        parts::vgroove(b, panel, 400.0, 170.0, 286.0);
        parts::vgroove(b, panel, 782.0, 170.0, 286.0);
    });

    let mut interval = parts::param(
        &a("interval"),
        "Interval",
        Range {
            steps: Some(25),
            ..Range::linear(-12.0, 12.0, Unit::Semitones)
        },
        7.0,
    )
    .bipolar();
    interval.featured = true;
    let params = vec![
        parts::time_ms(&a("time"), "Time", T_MIN, T_MAX, 375.0).featured(),
        parts::choice(&a("sync"), "Time Mode", parts::TIME_MODES, 0),
        parts::choice(&a("div"), "Note", parts::DIVISIONS, parts::DEFAULT_DIV),
        parts::toggle(&a("tap"), "Tap Tempo", false),
        parts::feedback(&a("feedback"), "Feedback", 45.0).featured(),
        interval,
        parts::param(
            &a("spread"),
            "Spread",
            Range {
                steps: Some(51),
                ..Range::linear(0.0, 50.0, Unit::Cents)
            },
            12.0,
        ),
        parts::percent(&a("tone"), "Tone", 50.0),
        parts::percent(&a("drive"), "Drive", 0.0),
        parts::percent(&a("duck"), "Duck", 0.0),
        parts::percent(&a("mix"), "Mix", 35.0).featured(),
        parts::choice(&a("mode"), "Voicing", MODES, 0),
    ];
    let streams = vec![parts::level_stream(&a("wet"), "Wet Level", 2)];
    Face {
        file: "04-pitch",
        plugin: "delay",
        scene,
        params,
        streams,
    }
}
