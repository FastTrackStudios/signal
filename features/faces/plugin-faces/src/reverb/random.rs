//! Random: a wash that never settles. Cooler and less lit than Ambient,
//! because the point is motion rather than bloom.
//!
//! The centrepiece is a scope of the delay lines themselves: each tap's
//! time drawn as a trace that wanders across the window, the way the
//! engine's lines drift — seen at a glance as "these move". DRIFT (how far
//! they wander) and DENSITY (how many there are) are the unit's own pair
//! and get their own ringed section on the right; DECAY and SIZE on the
//! left, the rest along the bottom with the TAIL ladder.

use document_proto::{NodeId, TextAlign};
use surface_kit::draw;
use surface_widgets::Builder;

use super::ui::{self, Dial};
use crate::face::{Face, canvas};

const W: f64 = 960.0;
const H: f64 = 340.0;
const INK: &str = "#dfeaf2";
const DIM: &str = "#8ea3b3";
const TRACE: &str = "#4fd0de";

/// Smooth value noise: a few octaves of sines at unrelated rates.
fn wander(seed: f64, t: f64) -> f64 {
    (t * 1.7 + seed * 3.1).sin() * 0.5
        + (t * 3.9 + seed * 7.3).sin() * 0.3
        + (t * 8.3 + seed * 1.9).sin() * 0.14
        + (t * 17.0 + seed * 5.0).sin() * 0.06
}

fn scope(b: &mut Builder<'_>, win: NodeId, w: f64, h: f64) {
    // Graticule.
    for i in 1..6 {
        ui::rule(
            b,
            win,
            w * f64::from(i) / 6.0,
            0.0,
            0.6,
            h,
            &ui::alpha(TRACE, 0.08),
        );
    }
    for i in 1..4 {
        ui::rule(
            b,
            win,
            0.0,
            h * f64::from(i) / 4.0,
            w,
            0.6,
            &ui::alpha(TRACE, 0.08),
        );
    }
    // Eight lines, each wandering about its own mean delay.
    let n = 8;
    for k in 0..n {
        let fk = f64::from(k);
        let y0 = 16.0 + (h - 32.0) * (fk + 0.5) / f64::from(n);
        let pts: Vec<(f64, f64)> = (0..=120)
            .map(|i| {
                let t = f64::from(i) / 120.0;
                (4.0 + (w - 8.0) * t, y0 + wander(fk * 1.37, t * 4.0) * 7.5)
            })
            .collect();
        let hue = ui::blend(TRACE, "#8fb6ff", fk / f64::from(n - 1));
        let o = 0.55 + 0.45 * super::jitter(fk + 2.0);
        b.add(
            win,
            ui::glowing(
                ui::line_node("trace", &pts, 1.3, &ui::alpha(&hue, o)),
                &hue,
                o,
                4.0,
            ),
        );
        // The write head: where the line is now.
        let (x, y) = pts[pts.len() - 1];
        b.add(
            win,
            ui::glowing(
                draw::circle("head", x - 2.0, y, 2.4, draw::solid(&hue)),
                &hue,
                0.9,
                5.0,
            ),
        );
    }
    ui::lit_text(
        b,
        win,
        "Line delay  ·  drift",
        8.0,
        10.0,
        160.0,
        6.5,
        &ui::alpha(TRACE, 0.7),
        TextAlign::Left,
        700.0,
    );
}

pub fn face() -> Face {
    let scene = canvas("Random Reverb", |b, root| {
        ui::fit_root(b, root, W, H);
        let panel = ui::panel(b, root, "panel.painted", "black", W, H, "ears");
        ui::repaint(
            b,
            panel,
            &[(0.0, "#1e2b38"), (0.5, "#16212b"), (1.0, "#0e161d")],
        );

        ui::plain(b, panel, "Random", 92.0, 34.0, 17.0, INK, 800.0);
        ui::silk_left(b, panel, "Modulated space", 142.0, 30.0, 7.5, INK, 800.0);
        ui::silk_left(
            b,
            panel,
            "Randomised lines · never repeats",
            142.0,
            42.0,
            6.0,
            DIM,
            700.0,
        );

        // DECAY and SIZE.
        let big = Dial::new("knob.soft_pointer", "black", 60.0, INK, DIM)
            .scale(&["", "", "", "", "", ""])
            .legend(10.0)
            .legend_at(196.0);
        ui::dial(b, panel, "random/decay", "Decay", 116.0, 124.0, &big);
        ui::dial(b, panel, "random/size", "Size", 226.0, 124.0, &big);

        // The scope.
        let (sx, sy, sw, sh) = (304.0, 54.0, 352.0, 150.0);
        let win = ui::display(b, panel, "Lines", sx, sy, sw, sh, TRACE, "glass");
        scope(b, win, sw, sh);
        ui::glass(b, win);

        // DRIFT and DENSITY, ringed.
        ui::section(
            b,
            panel,
            690.0,
            48.0,
            226.0,
            160.0,
            &ui::alpha(TRACE, 0.45),
            1.2,
        );
        ui::silk(b, panel, "Randomiser", 803.0, 62.0, 7.0, TRACE, 800.0);
        let pair = Dial::new("knob.soft_pointer", "blue", 52.0, INK, DIM)
            .scale(&["0", "", "", "", "", "10"])
            .legend(9.5)
            .legend_at(186.0);
        ui::dial(b, panel, "random/drift", "Drift", 750.0, 118.0, &pair);
        ui::dial(b, panel, "random/density", "Density", 856.0, 118.0, &pair);

        // The row, and the tail ladder.
        let small = Dial::new("knob.soft_pointer", "black", 40.0, INK, DIM).legend_at(314.0);
        let row: &[(&str, &str)] = &[
            ("random/onset", "Onset"),
            ("random/air", "Air"),
            ("random/motion", "Motion"),
            ("random/width", "Width"),
            ("random/mix", "Mix"),
        ];
        for (i, (addr, legend)) in row.iter().enumerate() {
            ui::dial(
                b,
                panel,
                addr,
                legend,
                116.0 + 110.0 * i as f64,
                262.0,
                &small,
            );
        }
        let lamps = |i: usize| {
            if i >= 10 {
                "#ffb04a".to_string()
            } else {
                ui::blend("#2f9fd0", TRACE, i as f64 / 9.0)
            }
        };
        ui::led_bar(
            b,
            panel,
            "Tail",
            "random/out",
            (699.0, 256.0, 208.0, 13.0),
            12,
            (-48.0, 0.0),
            &[],
            INK,
            &lamps,
        );
        ui::silk(b, panel, "Tail", 803.0, 284.0, 7.5, INK, 800.0);
    });

    let params = vec![
        super::decay("random/decay", "Decay", 0.5, 20.0, 4.0),
        super::pct("random/size", "Size", 65.0).featured(),
        super::pct("random/drift", "Drift", 45.0).featured(),
        super::pct("random/density", "Density", 60.0).featured(),
        super::predelay("random/onset", "Onset", 20.0),
        super::pct("random/air", "Air", 40.0),
        super::pct("random/motion", "Motion", 30.0),
        super::width("random/width"),
        super::mix("random/mix", 35.0),
    ];
    Face {
        file: "07-random",
        plugin: "reverb",
        scene,
        params,
        streams: super::levels("random"),
    }
}
