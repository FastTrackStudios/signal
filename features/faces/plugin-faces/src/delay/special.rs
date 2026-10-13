//! The special delay: black, for the ones that don't give the signal
//! back — reverse, spectral, diffuse.
//!
//! A black box with a phosphor scope that draws the repeat as it has become:
//! the envelope running backwards, swelling to the end of each window and
//! smeared as it goes. Three lit keys choose the machine; WINDOW and
//! OVERLAP (how long each reversed grain is, how much they cross) sit next
//! to time and feedback; tone, drive, duck and mix after.

use frame::document::proto::{NodeId, TextAlign};
use frame::surface::kit::{Place, draw};
use frame::surface::widgets::Builder;

use super::art;
use super::parts::{self, Ink, Screen, bind};
use crate::face::{Face, canvas};

const W: f64 = 960.0;
const H: f64 = 300.0;
const INK: &str = "#f1ece4";
const DIM: &str = "#8b867e";
const PINK: &str = "#ff5d98";
const PINK_DIM: &str = "#7a2d4a";
const MODES: &[&str] = &["Reverse", "Spectral", "Diffuse"];
const T_MIN: f64 = 20.0;
const T_MAX: f64 = 4000.0;

fn a(s: &str) -> String {
    format!("special/{s}")
}

/// The smear: a reversed envelope, loudest at the end of each window, three
/// windows across, blurring as it repeats.
fn smear(b: &mut Builder<'_>, win: NodeId, (x, y, w, h): (f64, f64, f64, f64)) {
    art::graticule(b, win, (x, y, w, h), (10, 4), "#ff5d9814");
    let mid = y + h / 2.0;
    parts::rule(b, win, x, x + w, mid, 0.8, PINK_DIM);
    let bars = 84;
    let windows = 3.0;
    for i in 0..bars {
        let t = f64::from(i) / f64::from(bars);
        let local = (t * windows).fract();
        let pass = (t * windows).floor();
        // Backwards: quiet first, loudest at the end of the window.
        let env = local.powf(1.6) * 0.82_f64.powf(pass * 2.0);
        let jitter = ((f64::from(i) * 7.3319).sin() * 21_337.0).fract().abs();
        let amp = env * (0.55 + 0.45 * jitter) * (h / 2.0 - 4.0);
        let bx = x + w * t;
        let bw = w / f64::from(bars) * 0.62;
        art::bar(
            b,
            win,
            (bx, mid - amp, bw, 2.0 * amp.max(0.5)),
            PINK,
            0.35 + 0.6 * env,
        );
    }
}

pub fn face() -> Face {
    let scene = canvas("Special Delay", |b, root| {
        parts::fit_root(b, root, W, H);
        let panel = parts::panel(b, root, "panel.painted", "black", W, H, "ears");
        parts::repaint(
            b,
            panel,
            &[(0.0, "#1d1d20"), (0.5, "#141416"), (1.0, "#0b0b0d")],
        );

        // ── Model line, with the accent stripe ────────────────────────
        b.add(
            panel,
            draw::rect(
                "stripe",
                50.0,
                22.0,
                4.0,
                40.0,
                draw::linear(180.0, &[(0.0, PINK), (1.0, "#a3305e")]),
            ),
        );
        parts::silk_left(
            b,
            panel,
            "Special",
            62.0,
            32.0,
            Ink::new(15.0, INK).weight(800.0).track(0.1),
        );
        parts::silk_left(
            b,
            panel,
            "Delay · reverse / spectral / diffuse",
            62.0,
            52.0,
            Ink::new(6.5, DIM).weight(600.0).track(0.16),
        );
        let modes = Place::new("Machine")
            .size(50.0)
            .variant("red")
            .bind(bind(&a("mode")))
            .options(MODES);
        parts::put(b, panel, "button.illuminated_bank", &modes, 142.0, 100.0);
        parts::silk(
            b,
            panel,
            "Machine",
            142.0,
            136.0,
            Ink::new(6.5, DIM).weight(700.0).track(0.2),
        );

        // ── Scope ──────────────────────────────────────────────────────
        let win = parts::screen(
            b,
            panel,
            "Scope",
            (268.0, 24.0, 460.0, 112.0),
            Screen {
                glass: ("#12060b", "#1a0810"),
                ink: PINK,
                mesh: false,
            },
        );
        parts::shader(b, win, "crt", &[("strength", 0.45), ("scale", 3.0)]);
        smear(b, win, (12.0, 20.0, 312.0, 82.0));
        art::tag(b, win, "Repeat, reversed", (14.0, 11.0), 6.5, PINK_DIM);
        parts::readout(
            b,
            win,
            "Time",
            bind(&a("time")),
            (336.0, 30.0, 112.0),
            22.0,
            PINK,
            TextAlign::Right,
        );
        parts::note_picker(
            b,
            win,
            "Note",
            &a("div"),
            (382.0, 58.0, 66.0),
            11.0,
            PINK,
            PINK_DIM,
        );
        art::tag(b, win, "Window", (340.0, 84.0), 6.5, PINK_DIM);
        parts::readout(
            b,
            win,
            "Window readout",
            bind(&a("window")),
            (380.0, 84.0, 68.0),
            9.0,
            PINK,
            TextAlign::Right,
        );
        art::tag(b, win, "Overlap", (340.0, 98.0), 6.5, PINK_DIM);
        parts::readout(
            b,
            win,
            "Overlap readout",
            bind(&a("overlap")),
            (380.0, 98.0, 68.0),
            9.0,
            PINK,
            TextAlign::Right,
        );

        let sync = Place::new("Time mode")
            .size(38.0)
            .bind(bind(&a("sync")))
            .options(parts::TIME_MODES);
        parts::put(b, panel, "button.square_bank", &sync, 790.0, 58.0);
        let tap = Place::new("Tap")
            .size(38.0)
            .variant("red")
            .legend("Tap")
            .options(&["momentary"])
            .bind(bind(&a("tap")));
        parts::put(b, panel, "button.illuminated", &tap, 790.0, 108.0);
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
        let knobs: [(&str, &str, f64, f64, &[(f64, &str)]); 8] = [
            ("time", "Time", 96.0, 54.0, &tm),
            ("feedback", "Feedback", 200.0, 54.0, parts::FB_MARKS),
            ("window", "Window", 316.0, 54.0, parts::PCT_MARKS),
            ("overlap", "Overlap", 442.0, 44.0, parts::PCT_MARKS),
            ("tone", "Tone", 536.0, 44.0, parts::PCT_MARKS),
            ("drive", "Drive", 630.0, 44.0, parts::PCT_MARKS),
            ("duck", "Duck", 724.0, 44.0, parts::PCT_MARKS),
            ("mix", "Mix", 852.0, 50.0, parts::PCT_MARKS),
        ];
        for (addr, name, x, d, marks) in knobs {
            let k = parts::knob(
                b,
                panel,
                "knob.metal_fluted",
                "",
                &a(addr),
                name,
                (x, row),
                d,
            );
            let sweep = parts::sweep_of(b, k);
            parts::marks(b, panel, (x, row), d / 2.0 + 5.0, sweep, marks, 11, scale);
            parts::legend_at(b, k, name, 266.0, legend);
            if !matches!(addr, "time" | "window" | "overlap") {
                parts::value_at(b, k, 280.0, value);
            }
        }
        parts::vgroove(b, panel, 384.0, 170.0, 286.0);
        parts::vgroove(b, panel, 780.0, 170.0, 286.0);
    });

    let params = vec![
        parts::time_ms(&a("time"), "Time", T_MIN, T_MAX, 500.0).featured(),
        parts::choice(&a("sync"), "Time Mode", parts::TIME_MODES, 0),
        parts::choice(&a("div"), "Note", parts::DIVISIONS, 6),
        parts::toggle(&a("tap"), "Tap Tempo", false),
        parts::feedback(&a("feedback"), "Feedback", 35.0).featured(),
        parts::percent(&a("window"), "Window", 50.0).featured(),
        parts::percent(&a("overlap"), "Overlap", 50.0),
        parts::percent(&a("tone"), "Tone", 50.0),
        parts::percent(&a("drive"), "Drive", 0.0),
        parts::percent(&a("duck"), "Duck", 0.0),
        parts::percent(&a("mix"), "Mix", 40.0).featured(),
        parts::choice(&a("mode"), "Machine", MODES, 0).featured(),
    ];
    let streams = vec![parts::level_stream(&a("wet"), "Wet Level", 2)];
    Face {
        file: "06-special",
        plugin: "delay",
        scene,
        params,
        streams,
    }
}
