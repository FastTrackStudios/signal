//! Ambient: no walls. A deep indigo panel with the wash lit from inside.
//!
//! The centrepiece is the halo — concentric light over a faint horizon,
//! brightest at its core where the wash is densest — and the WASH ladder
//! beside it lights with the tail's level. DECAY and SPREAD, the two a wash
//! is set by, are the big wing knobs; the engine (Cloud, Bloom, Swell,
//! Velvet) is on backlit keys; the shaping controls run along the bottom.

use frame::document::proto::NodeId;
use frame::surface::kit::draw;
use frame::surface::widgets::Builder;

use super::ui::{self, Dial};
use crate::face::{Face, canvas};

const W: f64 = 960.0;
const H: f64 = 340.0;
const INK: &str = "#e8e4f7";
const DIM: &str = "#a098c8";
const GLOW: &str = "#a584f4";

fn halo(b: &mut Builder<'_>, panel: NodeId, cx: f64, cy: f64) {
    // The bloom behind everything.
    b.add(
        panel,
        draw::circle(
            "bloom",
            cx,
            cy,
            120.0,
            draw::radial(&[
                (0.0, "#a584f466"),
                (0.35, "#7d5fd62a"),
                (0.7, "#5a40b010"),
                (1.0, "#3a2a8000"),
            ]),
        ),
    );
    // Horizon.
    ui::line(
        b,
        panel,
        "horizon",
        &[(cx - 165.0, cy), (cx + 165.0, cy)],
        0.8,
        &ui::alpha(INK, 0.28),
    );
    for i in 0..7 {
        let fi = f64::from(i);
        let r = 14.0 + fi * 14.5;
        let o = (0.95 * (1.0 - fi / 7.5)).max(0.08);
        let w = 2.4 - fi * 0.22;
        let pts = ui::ellipse_pts(cx, cy, r, r * 0.86, 96);
        b.add(
            panel,
            ui::glowing(
                ui::line_node("ring", &pts, w, &ui::alpha(GLOW, o)),
                GLOW,
                o,
                6.0,
            ),
        );
    }
    b.add(
        panel,
        ui::glowing(
            draw::circle(
                "core",
                cx,
                cy,
                7.0,
                draw::radial(&[(0.0, "#ffffff"), (0.5, "#d6c8ff"), (1.0, GLOW)]),
            ),
            GLOW,
            1.0,
            14.0,
        ),
    );
}

pub fn face() -> Face {
    let scene = canvas("Ambient Reverb", |b, root| {
        ui::fit_root(b, root, W, H);
        let panel = ui::panel(b, root, "panel.painted", "black", W, H, "ears");
        ui::repaint(
            b,
            panel,
            &[(0.0, "#2a2447"), (0.5, "#1d1834"), (1.0, "#120f22")],
        );

        ui::plain(b, panel, "Ambient", 94.0, 34.0, 17.0, INK, 800.0);
        ui::silk_left(b, panel, "Wash generator", 146.0, 30.0, 7.5, INK, 800.0);
        ui::silk_left(
            b,
            panel,
            "No walls · infinite diffusion",
            146.0,
            42.0,
            6.0,
            DIM,
            700.0,
        );

        halo(b, panel, 480.0, 132.0);

        // DECAY and SPREAD.
        let big = Dial::new("knob.marconi", "blue", 64.0, INK, DIM)
            .scale(&["", "", "", "", "", ""])
            .legend(10.0)
            .legend_at(196.0);
        ui::dial(b, panel, "ambient/decay", "Decay", 128.0, 124.0, &big);
        ui::dial(b, panel, "ambient/spread", "Spread", 248.0, 124.0, &big);

        // The engine, and the wash ladder.
        let engine = Dial::new("knob.marconi", "grey", 56.0, INK, DIM)
            .options(&["Cloud", "Bloom", "Swell", "Velvet"])
            .sweep(150.0)
            .legend(9.0)
            .legend_at(128.0);
        ui::dial(b, panel, "ambient/engine", "Engine", 790.0, 82.0, &engine);
        let lamps = |i: usize| {
            if i >= 10 {
                "#ff7ad9".to_string()
            } else {
                ui::blend("#6f7cff", GLOW, i as f64 / 9.0)
            }
        };
        ui::led_bar(
            b,
            panel,
            "Wash",
            "ambient/out",
            (698.0, 154.0, 184.0, 12.0),
            12,
            (-48.0, 0.0),
            &[],
            INK,
            &lamps,
        );
        ui::silk(b, panel, "Wash", 790.0, 180.0, 7.5, INK, 800.0);

        // The row.
        let small = Dial::new("knob.marconi", "grey", 40.0, INK, DIM).legend_at(314.0);
        let row: &[(&str, &str)] = &[
            ("ambient/onset", "Onset"),
            ("ambient/air", "Air"),
            ("ambient/shape", "Shape"),
            ("ambient/grain", "Grain"),
            ("ambient/width", "Width"),
            ("ambient/mix", "Mix"),
        ];
        for (i, (addr, legend)) in row.iter().enumerate() {
            ui::dial(
                b,
                panel,
                addr,
                legend,
                180.0 + 120.0 * i as f64,
                262.0,
                &small,
            );
        }
    });

    let params = vec![
        super::decay("ambient/decay", "Decay", 1.0, 30.0, 8.0),
        super::pct("ambient/spread", "Spread", 70.0).featured(),
        ui::choice(
            "ambient/engine",
            "Engine",
            &["Cloud", "Bloom", "Swell", "Velvet"],
            0,
        )
        .featured(),
        super::predelay("ambient/onset", "Onset", 40.0),
        super::pct("ambient/air", "Air", 40.0),
        super::pct("ambient/shape", "Shape", 50.0),
        super::pct("ambient/grain", "Grain", 30.0),
        super::width("ambient/width"),
        super::mix("ambient/mix", 40.0),
    ];
    Face {
        file: "06-ambient",
        plugin: "reverb",
        scene,
        params,
        streams: super::levels("ambient"),
    }
}
