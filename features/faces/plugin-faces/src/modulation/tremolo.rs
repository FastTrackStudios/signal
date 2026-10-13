//! Tremolo: a brownface tremolo pedal.
//!
//! SPEED and DEPTH on brown pointer knobs; groove, feel, mix and output
//! under them. The lid shows what made a tremolo — a lamp throbbing at an
//! LDR, drawn with the light between them — and the rate lamp here *is*
//! that lamp, pulsing with the LFO. The shape (opto, stereo, harmonic) is
//! a three-way selector.

use frame::document::proto::NodeId;
use frame::surface::kit::draw;
use frame::surface::widgets::Builder;

use crate::face::{Face, canvas};
use crate::reverb::ui::{self, Dial};

use super::pedal::{self, LW};

const F: &str = "tremolo";
const INK: &str = "#f4e7cf";
const DIM: &str = "#c6ad89";

/// The lamp, its light, and the photocell.
fn opto(b: &mut Builder<'_>, lid: NodeId, cx: f64, cy: f64) {
    // Bulb.
    let bulb = ui::ellipse_pts(cx - 44.0, cy, 11.0, 14.0, 48);
    ui::line(b, lid, "bulb", &bulb, 1.4, &ui::alpha(INK, 0.6));
    ui::line(
        b,
        lid,
        "filament",
        &[
            (cx - 50.0, cy + 4.0),
            (cx - 47.0, cy - 4.0),
            (cx - 44.0, cy + 4.0),
            (cx - 41.0, cy - 4.0),
            (cx - 38.0, cy + 4.0),
        ],
        1.2,
        &ui::alpha("#ffcf7a", 0.8),
    );
    b.add(
        lid,
        draw::rect(
            "base",
            cx - 51.0,
            cy + 13.0,
            14.0,
            6.0,
            draw::solid(&ui::alpha(INK, 0.45)),
        ),
    );
    // Light.
    for dy in [-8.0, 0.0, 8.0] {
        ui::line(
            b,
            lid,
            "ray",
            &[(cx - 26.0, cy + dy * 0.6), (cx + 18.0, cy + dy)],
            1.0,
            &ui::alpha("#ffcf7a", 0.45),
        );
    }
    // Photocell: a disc with its serpentine track.
    let cell = ui::ellipse_pts(cx + 38.0, cy, 14.0, 14.0, 48);
    ui::line(b, lid, "cell", &cell, 1.4, &ui::alpha(INK, 0.6));
    let track: Vec<(f64, f64)> = (0..=40)
        .map(|i| {
            let u = f64::from(i) / 40.0;
            (
                cx + 29.0 + 18.0 * u,
                cy + 6.0 * (u * std::f64::consts::TAU * 3.0).sin(),
            )
        })
        .collect();
    ui::line(b, lid, "track", &track, 1.0, &ui::alpha("#e08a3a", 0.7));
}

pub fn face() -> Face {
    let scene = canvas("Tremolo", |b, root| {
        let lid = pedal::body(b, root, ["#80643f", "#5f4629", "#43301b"]);

        let big = Dial::new("knob.pointer", "brown", 60.0, INK, DIM)
            .scale(&["", "", "", "", "", ""])
            .legend(10.0)
            .legend_at(128.0);
        pedal::knob_row(
            b,
            lid,
            &[("tremolo/rate", "Speed"), ("tremolo/depth", "Depth")],
            72.0,
            &big,
        );
        let small = Dial::new("knob.pointer", "black", 32.0, INK, DIM)
            .legend(7.5)
            .legend_at(214.0);
        pedal::knob_row(
            b,
            lid,
            &[
                ("tremolo/groove", "Groove"),
                ("tremolo/feel", "Feel"),
                ("tremolo/mix", "Mix"),
                ("tremolo/output", "Output"),
            ],
            170.0,
            &small,
        );

        pedal::rule(b, lid, 236.0, &ui::alpha(INK, 0.3));
        pedal::title(
            b,
            lid,
            "Tremolo",
            "Optical · lamp and photocell",
            262.0,
            INK,
            DIM,
        );
        opto(b, lid, LW / 2.0, 318.0);
        pedal::rule(b, lid, 346.0, &ui::alpha(INK, 0.3));

        let shape = Dial::new("switch.rotary", "cream", 38.0, INK, DIM)
            .options(&["Opto", "Stereo", "Harm"])
            .sweep(120.0)
            .legend(7.5)
            .legend_at(414.0);
        ui::dial(b, lid, "tremolo/shape", "Shape", 96.0, 384.0, &shape);
        pedal::rate_lamp(b, lid, F, LW - 90.0, 382.0, "#ffae2e", INK);

        pedal::stomp(b, lid, F, "#ff3b2f");
    });

    let params = vec![
        pedal::rate(F, 4.5),
        pedal::depth(F, 60.0),
        pedal::amount(F, "groove", "Groove", 0.0),
        pedal::amount(F, "feel", "Feel", 50.0),
        ui::percent("tremolo/mix", "Mix", 100.0),
        pedal::output(F),
        ui::choice("tremolo/shape", "Shape", &["Opto", "Stereo", "Harmonic"], 0).featured(),
        pedal::engaged(F),
    ];
    Face {
        file: "04-tremolo",
        plugin: "modulation",
        scene,
        params,
        streams: pedal::lfo(F),
    }
}
