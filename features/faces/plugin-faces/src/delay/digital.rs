//! The digital delay line: a black rack unit that doesn't pretend to be
//! old.
//!
//! The VFD is the unit. It reads both sides at once — each time in
//! milliseconds and, beside it, the note value it is locked to — with the
//! FREE/SYNC and LINK state lit over them, and draws the echo train the
//! feedback makes (exact repeats on a ruled line: a digital delay is
//! arithmetic). Beside the glass, the keys: TAP (momentary), FREE/SYNC,
//! LINK and the voicing (clean or filtered). The two time encoders,
//! feedback and mix sit where they sit on every delay face; tone, the
//! filtered voicing's own pair and duck sit between. Wet level on two LED
//! columns at the right.

use document_proto::{NodeId, TextAlign};
use surface_kit::{Place, draw};
use surface_widgets::Builder;

use super::art;
use super::parts::{self, Ink, Screen, bind};
use crate::face::{Face, canvas};

const W: f64 = 960.0;
const H: f64 = 300.0;
const INK: &str = "#e6ecf3";
const DIM: &str = "#8d99a7";
const VFD: &str = "#5ff2d2";
const VFD_DIM: &str = "#2f9c89";

const MODES: &[&str] = &["Clean", "Filtered"];
const T_MIN: f64 = 1.0;
const T_MAX: f64 = 4000.0;

fn a(s: &str) -> String {
    format!("digital/{s}")
}

/// One side of the display: L or R in a lit block, its time, its note.
fn side(b: &mut Builder<'_>, win: NodeId, which: &str, cy: f64) {
    let blk = draw::rounded(
        draw::rect("channel", 12.0, cy - 9.0, 16.0, 16.0, draw::solid(VFD_DIM)),
        2.0,
    );
    b.add(win, parts::emit(blk, VFD, 0.4, 6.0));
    parts::silk(
        b,
        win,
        which,
        20.0,
        cy - 1.0,
        Ink::new(10.0, "#04110f").weight(800.0).track(0.0),
    );
    let lower = which.to_lowercase();
    parts::readout(
        b,
        win,
        &format!("Time {which}"),
        bind(&a(&format!("time_{lower}"))),
        (36.0, cy, 150.0),
        27.0,
        VFD,
        TextAlign::Right,
    );
    parts::note_picker(
        b,
        win,
        &format!("Note {which}"),
        &a(&format!("div_{lower}")),
        (194.0, cy, 72.0),
        15.0,
        VFD,
        VFD_DIM,
    );
}

pub fn face() -> Face {
    let scene = canvas("Digital Delay", |b, root| {
        parts::fit_root(b, root, W, H);
        let panel = parts::panel(b, root, "panel.brushed", "black", W, H, "ears");

        // ── Model line ─────────────────────────────────────────────────
        parts::silk_left(
            b,
            panel,
            "Digital Delay Line",
            50.0,
            34.0,
            Ink::new(13.0, INK).weight(800.0).track(0.06),
        );
        parts::rule(b, panel, 50.0, 222.0, 46.0, 1.0, "#e6ecf340");
        parts::silk_left(
            b,
            panel,
            "DDL-4 · 4000 ms · 24-bit",
            50.0,
            58.0,
            Ink::new(7.0, DIM).weight(600.0).track(0.16),
        );

        // Voicing: a clean line, or repeats through a resonant filter.
        let voice = Place::new("Voicing")
            .size(46.0)
            .bind(bind(&a("mode")))
            .options(MODES);
        parts::put(b, panel, "button.square_bank", &voice, 116.0, 100.0);
        parts::silk(
            b,
            panel,
            "Voicing",
            116.0,
            132.0,
            Ink::new(7.0, DIM).weight(700.0).track(0.2),
        );

        // ── The display ────────────────────────────────────────────────
        let (dx, dy, dw, dh) = (250.0, 26.0, 396.0, 108.0);
        let win = parts::screen(
            b,
            panel,
            "Display",
            (dx, dy, dw, dh),
            Screen {
                glass: ("#03100e", "#071a16"),
                ink: VFD,
                mesh: true,
            },
        );
        // Annunciators: the time mode and whether R follows L.
        parts::readout(
            b,
            win,
            "Time mode",
            bind(&a("sync")),
            (12.0, 15.0, 44.0),
            9.0,
            VFD,
            TextAlign::Left,
        );
        art::tag(b, win, "Link", (84.0, 15.0), 8.0, VFD_DIM);
        parts::readout(
            b,
            win,
            "Link state",
            bind(&a("link")),
            (112.0, 15.0, 30.0),
            9.0,
            VFD,
            TextAlign::Left,
        );
        side(b, win, "L", 48.0);
        side(b, win, "R", 86.0);
        // The echo train: exact repeats, each lower by the feedback.
        let (tx, tw) = (280.0, 104.0);
        let base = 92.0;
        art::graticule(b, win, (tx, 22.0, tw, 70.0), (4, 2), "#5ff2d21a");
        let fb = 0.62_f64;
        for i in 0..8 {
            let level = fb.powi(i);
            let h = 66.0 * level;
            art::bar(
                b,
                win,
                (tx + 4.0 + f64::from(i) * 12.8, base - h, 5.0, h),
                VFD,
                0.35 + 0.65 * level,
            );
        }
        art::tag(b, win, "Repeats", (tx, 101.0), 6.5, VFD_DIM);

        // ── Keys ───────────────────────────────────────────────────────
        let tap = Place::new("Tap")
            .size(50.0)
            .variant("red")
            .legend("Tap")
            .options(&["momentary"])
            .bind(bind(&a("tap")));
        parts::put(b, panel, "button.illuminated", &tap, 700.0, 80.0);
        parts::silk(
            b,
            panel,
            "Tempo",
            700.0,
            118.0,
            Ink::new(6.5, DIM).weight(700.0).track(0.2),
        );
        let sync = Place::new("Time mode")
            .size(36.0)
            .bind(bind(&a("sync")))
            .options(parts::TIME_MODES);
        parts::put(b, panel, "button.square_bank", &sync, 786.0, 58.0);
        let link = Place::new("Link")
            .size(36.0)
            .legend("Link")
            .bind(bind(&a("link")));
        parts::put(b, panel, "button.square", &link, 768.0, 106.0);
        parts::silk_left(
            b,
            panel,
            "R = L",
            792.0,
            106.0,
            Ink::new(6.5, DIM).weight(700.0).track(0.14),
        );

        // ── Wet level ──────────────────────────────────────────────────
        for (i, x) in [852.0, 868.0].into_iter().enumerate() {
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
        for (db, y) in [("0", 36.0), ("-12", 67.0), ("-36", 124.0)] {
            parts::silk_left(
                b,
                panel,
                db,
                885.0,
                y,
                Ink::new(6.0, DIM).weight(600.0).track(0.0),
            );
        }
        parts::silk(
            b,
            panel,
            "Wet",
            866.0,
            140.0,
            Ink::new(6.5, DIM).weight(700.0).track(0.2),
        );

        // ── The knob row ───────────────────────────────────────────────
        parts::groove(b, panel, 40.0, W - 40.0, 156.0);
        let row = 212.0;
        let legend_y = 262.0;
        let value_y = 277.0;
        let big = 56.0;
        let small = 44.0;
        let tm = parts::time_marks(T_MIN, T_MAX);
        let knobs: [(&str, &str, f64, f64, &[(f64, &str)], bool); 8] = [
            ("time_l", "Time L", 96.0, big, &tm, false),
            ("time_r", "Time R", 204.0, big, &tm, false),
            ("feedback", "Feedback", 312.0, big, parts::FB_MARKS, true),
            ("tone", "Tone", 430.0, small, parts::PCT_MARKS, true),
            ("filter", "Filter", 518.0, small, parts::PCT_MARKS, true),
            (
                "resonance",
                "Resonance",
                606.0,
                small,
                parts::PCT_MARKS,
                true,
            ),
            ("duck", "Duck", 694.0, small, parts::PCT_MARKS, true),
            ("mix", "Mix", 836.0, 50.0, parts::PCT_MARKS, true),
        ];
        let scale = Ink::new(6.5, DIM).weight(600.0).track(0.0).plain();
        for (addr, legend, x, d, marks, value) in knobs {
            let k = parts::knob(b, panel, "knob.metal", "", &a(addr), legend, (x, row), d);
            let sweep = parts::sweep_of(b, k);
            parts::marks(b, panel, (x, row), d / 2.0 + 5.0, sweep, marks, 11, scale);
            parts::legend_at(
                b,
                k,
                legend,
                legend_y,
                Ink::new(8.0, INK).weight(700.0).track(0.14),
            );
            if value {
                parts::value_at(
                    b,
                    k,
                    value_y,
                    Ink::new(7.5, DIM).weight(600.0).track(0.02).plain(),
                );
            }
        }
        parts::vgroove(b, panel, 372.0, 170.0, 284.0);
        parts::vgroove(b, panel, 758.0, 170.0, 284.0);
    });

    let params = vec![
        parts::time_ms(&a("time_l"), "Time L", T_MIN, T_MAX, 375.0).featured(),
        parts::time_ms(&a("time_r"), "Time R", T_MIN, T_MAX, 375.0),
        parts::choice(&a("sync"), "Time Mode", parts::TIME_MODES, 0).featured(),
        parts::choice(
            &a("div_l"),
            "Time L Note",
            parts::DIVISIONS,
            parts::DEFAULT_DIV,
        ),
        parts::choice(
            &a("div_r"),
            "Time R Note",
            parts::DIVISIONS,
            parts::DEFAULT_DIV,
        ),
        parts::toggle(&a("link"), "Link", true),
        parts::toggle(&a("tap"), "Tap Tempo", false),
        parts::feedback(&a("feedback"), "Feedback", 35.0).featured(),
        parts::percent(&a("tone"), "Tone", 50.0),
        parts::percent(&a("filter"), "Filter", 50.0),
        parts::percent(&a("resonance"), "Resonance", 50.0),
        parts::percent(&a("duck"), "Duck", 0.0),
        parts::percent(&a("mix"), "Mix", 35.0).featured(),
        parts::choice(&a("mode"), "Voicing", MODES, 0),
    ];
    let streams = vec![parts::level_stream(&a("wet"), "Wet Level", 2)];
    Face {
        file: "01-digital",
        plugin: "delay",
        scene,
        params,
        streams,
    }
}
