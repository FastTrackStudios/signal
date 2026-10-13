//! Special: black, lamps, and no pretence of being a room — the engines
//! that are effects rather than spaces (shimmer, chorale, magneto,
//! non-linear, reflections, the classic comb-allpass).
//!
//! The engine is picked on a chicken-head selector, because an effect has a
//! state, not a shape; the centrepiece is a ladder of tall lamps lighting
//! with the tail's level. Shimmer's INTERVAL — the whole effect, and a
//! different instrument at +12 than at +7 — gets its own stepped dial
//! printed in semitones. The shared controls run along the bottom.

use frame::surface::proto::{Range, Unit};

use super::ui::{self, Dial};
use crate::face::{Face, canvas};

const W: f64 = 960.0;
const H: f64 = 320.0;
const INK: &str = "#f0e9df";
const DIM: &str = "#8d8880";
const ACCENT: &str = "#ff6a3d";

const ENGINES: &[&str] = &[
    "Shimmer", "Chorale", "Magneto", "Non-Lin", "Reflect", "Classic",
];

pub fn face() -> Face {
    let scene = canvas("Special Reverb", |b, root| {
        ui::fit_root(b, root, W, H);
        let panel = ui::panel(b, root, "panel.brushed", "black", W, H, "ears");

        ui::plain(b, panel, "Special", 86.0, 34.0, 17.0, INK, 800.0);
        ui::silk_left(
            b,
            panel,
            "Effect reverberator",
            132.0,
            30.0,
            7.5,
            INK,
            800.0,
        );
        ui::silk_left(
            b,
            panel,
            "Pitch · tape · gate · comb",
            132.0,
            42.0,
            6.0,
            DIM,
            700.0,
        );

        // ── Engine selector ────────────────────────────────────────────
        let sel = Dial::new("switch.rotary", "red", 50.0, INK, DIM)
            .options(ENGINES)
            .legend(10.0)
            .legend_at(186.0);
        ui::dial(b, panel, "special/engine", "Engine", 150.0, 118.0, &sel);

        // ── The lamp ladder ────────────────────────────────────────────
        let (lx, ly, n, pitch) = (286.0, 70.0, 14, 30.0);
        let lamps = |i: usize| {
            if i >= 12 {
                "#ff3b2f".to_string()
            } else {
                ui::blend("#ffb02e", ACCENT, i as f64 / 11.0)
            }
        };
        ui::section(
            b,
            panel,
            lx - 30.0,
            ly - 16.0,
            pitch * n as f64 + 60.0,
            104.0,
            "#ffffff22",
            1.0,
        );
        let scale = ["-48", "-36", "-24", "-12", "0 dB"];
        ui::led_bar(
            b,
            panel,
            "Tail",
            "special/out",
            (lx - 2.0, ly - 2.0, pitch * n as f64 + 4.0, 56.0),
            n,
            (-48.0, 0.0),
            &scale,
            DIM,
            &lamps,
        );
        ui::silk(
            b,
            panel,
            "Tail",
            lx + pitch * n as f64 / 2.0,
            ly - 26.0,
            7.0,
            DIM,
            800.0,
        );

        // ── Interval ───────────────────────────────────────────────────
        let iv = Dial::new("knob.silver_top", "silver", 58.0, INK, DIM)
            .scale(&["-12", "-6", "0", "+6", "+12"])
            .legend(10.0)
            .legend_at(186.0);
        ui::dial(b, panel, "special/interval", "Interval", 832.0, 110.0, &iv);

        // ── The row ────────────────────────────────────────────────────
        ui::rule(b, panel, 56.0, 206.0, 848.0, 1.0, "#00000099");
        ui::rule(b, panel, 56.0, 207.0, 848.0, 1.0, "#ffffff12");
        let small = Dial::new("knob.silver_top", "silver", 40.0, INK, DIM).legend_at(298.0);
        let row: &[(&str, &str)] = &[
            ("special/decay", "Decay"),
            ("special/size", "Size"),
            ("special/damping", "Damping"),
            ("special/tone", "Tone"),
            ("special/amount", "Amount"),
            ("special/shape", "Shape"),
            ("special/motion", "Motion"),
            ("special/mix", "Mix"),
        ];
        for (i, (addr, legend)) in row.iter().enumerate() {
            ui::dial(
                b,
                panel,
                addr,
                legend,
                110.0 + 106.0 * i as f64,
                246.0,
                &small,
            );
        }
    });

    let mut interval = ui::param(
        "special/interval",
        "Interval",
        Range::linear(-12.0, 12.0, Unit::Semitones),
        12.0,
    );
    interval.range.steps = Some(25);
    let params = vec![
        ui::choice(
            "special/engine",
            "Engine",
            &[
                "Shimmer",
                "Chorale",
                "Magneto",
                "Non-Linear",
                "Reflections",
                "Classic Comb",
            ],
            0,
        )
        .featured(),
        interval.bipolar().featured(),
        super::decay("special/decay", "Decay", 0.3, 20.0, 5.0),
        super::pct("special/size", "Size", 70.0),
        super::pct("special/damping", "Damping", 30.0),
        super::pct("special/tone", "Tone", 55.0),
        super::pct("special/amount", "Amount", 50.0).featured(),
        super::pct("special/shape", "Shape", 50.0),
        super::pct("special/motion", "Motion", 20.0),
        super::mix("special/mix", 35.0),
    ];
    Face {
        file: "08-special",
        plugin: "reverb",
        scene,
        params,
        streams: super::levels("special"),
    }
}
