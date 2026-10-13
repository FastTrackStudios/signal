//! Vibrato: a cream pedal in the manner of an amp's vibrato channel.
//!
//! SPEED and INTENSITY on black chicken-heads, the voicing trims under
//! them, and no MIX: vibrato is wet only — pitch modulation with a dry path
//! is a chorus. The lid's artwork is a tone whose pitch swings; the circuit
//! (the valve-style one or the poly-synth's) is a bat beside the rate lamp.

use frame::surface::kit::{Ink, Place};

use crate::face::{Face, canvas};
use crate::reverb::ui::{self, Dial, bind};

use super::pedal::{self, LW};

const F: &str = "vibrato";
const INK: &str = "#2f2416";
const DIM: &str = "#6d5c43";

/// A carrier whose frequency swings: the vibrato, drawn.
fn swing(
    b: &mut frame::surface::widgets::Builder<'_>,
    lid: frame::document::proto::NodeId,
    cy: f64,
) {
    let (x0, x1) = (40.0, LW - 40.0);
    let mut phase = 0.0;
    let mut pts = Vec::new();
    for i in 0..=320 {
        let u = f64::from(i) / 320.0;
        let f = 9.0 + 5.0 * (u * std::f64::consts::TAU * 1.5).sin();
        phase += f / 320.0;
        pts.push((
            x0 + (x1 - x0) * u,
            cy - 16.0 * (phase * std::f64::consts::TAU).sin(),
        ));
    }
    ui::line(b, lid, "swing", &pts, 1.3, &ui::alpha("#8a5a24", 0.28));
}

pub fn face() -> Face {
    let scene = canvas("Vibrato", |b, root| {
        let lid = pedal::body(b, root, ["#f3e9cf", "#e4d4ad", "#cdb98c"]);

        let big = Dial::new("knob.chicken_head", "black", 60.0, INK, DIM)
            .scale(&["", "", "", "", "", ""])
            .legend(10.0)
            .legend_at(128.0);
        pedal::knob_row(
            b,
            lid,
            &[("vibrato/rate", "Speed"), ("vibrato/depth", "Intensity")],
            72.0,
            &big,
        );
        let small = Dial::new("knob.bakelite", "brown", 32.0, INK, DIM)
            .legend(7.5)
            .legend_at(214.0);
        pedal::knob_row(
            b,
            lid,
            &[
                ("vibrato/tone", "Tone"),
                ("vibrato/width", "Width"),
                ("vibrato/voices", "Voices"),
                ("vibrato/output", "Output"),
            ],
            170.0,
            &small,
        );

        pedal::rule(b, lid, 236.0, &ui::alpha(INK, 0.3));
        swing(b, lid, 290.0);
        pedal::title(
            b,
            lid,
            "Vibrato",
            "True pitch vibrato · wet only",
            290.0,
            INK,
            DIM,
        );
        pedal::rule(b, lid, 340.0, &ui::alpha(INK, 0.3));

        let bat = Place::new("Circuit")
            .size(38.0)
            .variant("black")
            .legend("")
            .bind(bind("vibrato/model"))
            .options(&["Poly", "Valve"])
            .ink(Ink::Color(INK.into()));
        let t = ui::put(b, lid, "switch.toggle_bat", &bat, 96.0, 378.0);
        ui::strip_legend(b, t);
        ui::silk(b, lid, "Circuit", 96.0, 412.0, 7.5, INK, 800.0);
        pedal::rate_lamp(b, lid, F, LW - 90.0, 378.0, "#ffae2e", INK);

        pedal::stomp(b, lid, F, "#ff3b2f");
    });

    let params = vec![
        pedal::rate(F, 5.0),
        pedal::depth(F, 40.0),
        pedal::amount(F, "tone", "Tone", 50.0),
        pedal::amount(F, "width", "Width", 60.0),
        pedal::amount(F, "voices", "Voices", 0.0),
        pedal::output(F),
        ui::choice("vibrato/model", "Circuit", &["Valve", "Poly Synth"], 0).featured(),
        pedal::engaged(F),
    ];
    Face {
        file: "03-vibrato",
        plugin: "modulation",
        scene,
        params,
        streams: pedal::lfo(F),
    }
}
