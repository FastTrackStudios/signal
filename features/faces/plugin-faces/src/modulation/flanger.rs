//! Flanger: a purple metal-flake flanger.
//!
//! RATE, DEPTH and REGEN — the jet — up top on brushed caps; tone, width,
//! voices, mix and output under them. The lid carries the comb the circuit
//! makes, notches swept across the spectrum, behind the name; the circuit
//! (tape-style through-zero or bucket-brigade) is a bat switch beside the
//! rate lamp.

use surface_kit::{Ink, Place};

use crate::face::{Face, canvas};
use crate::reverb::ui::{self, Dial, bind};

use super::pedal::{self, LW};

const F: &str = "flanger";
const INK: &str = "#f1ebfb";
const DIM: &str = "#c4b5e0";

/// The comb: notches across the lid, drawn as a swept response.
fn comb(b: &mut surface_widgets::Builder<'_>, lid: document_proto::NodeId, cy: f64) {
    let (x0, x1) = (36.0, LW - 36.0);
    let pts: Vec<(f64, f64)> = (0..=240)
        .map(|i| {
            let u = f64::from(i) / 240.0;
            // Notch spacing widens along the lid, as the sweep does.
            let ph = (u * 7.0 + u * u * 5.0) * std::f64::consts::TAU;
            let mag = (0.5 + 0.5 * ph.cos()).powf(0.6);
            (x0 + (x1 - x0) * u, cy + 22.0 - 44.0 * mag)
        })
        .collect();
    ui::line(b, lid, "comb haze", &pts, 5.0, &ui::alpha("#d9b8ff", 0.05));
    ui::line(b, lid, "comb", &pts, 1.3, &ui::alpha("#e8d6ff", 0.2));
}

pub fn face() -> Face {
    let scene = canvas("Flanger", |b, root| {
        let lid = pedal::body(b, root, ["#7a5aa8", "#553a82", "#3a2660"]);

        let big = Dial::new("knob.metal", "silver", 58.0, INK, DIM).scale(&["", "", "", "", "", ""]).legend(10.0).legend_at(128.0);
        pedal::knob_row(b, lid, &[("flanger/rate", "Rate"), ("flanger/depth", "Depth"), ("flanger/regen", "Regen")], 70.0, &big);
        let small = Dial::new("knob.metal", "black", 32.0, INK, DIM).legend(7.5).legend_at(214.0);
        pedal::knob_row(
            b,
            lid,
            &[("flanger/tone", "Tone"), ("flanger/width", "Width"), ("flanger/voices", "Voices"), ("flanger/mix", "Mix"), ("flanger/output", "Output")],
            170.0,
            &small,
        );

        pedal::rule(b, lid, 236.0, &ui::alpha(INK, 0.3));
        comb(b, lid, 290.0);
        pedal::title(b, lid, "Flanger", "Through-zero jet · regeneration", 290.0, INK, DIM);
        pedal::rule(b, lid, 340.0, &ui::alpha(INK, 0.3));

        let bat = Place::new("Circuit").size(38.0).variant("chrome").legend("").bind(bind("flanger/model")).options(&["BBD", "Tape"]).ink(Ink::Color(INK.into()));
        let t = ui::put(b, lid, "switch.toggle_bat", &bat, 96.0, 378.0);
        ui::strip_legend(b, t);
        ui::silk(b, lid, "Circuit", 96.0, 412.0, 7.5, INK, 800.0);
        pedal::rate_lamp(b, lid, F, LW - 90.0, 378.0, "#c38bff", INK);

        pedal::stomp(b, lid, F, "#b77dff");
    });

    let params = vec![
        pedal::rate(F, 0.3),
        pedal::depth(F, 60.0),
        pedal::amount(F, "regen", "Regen", 55.0).featured(),
        pedal::amount(F, "tone", "Tone", 50.0),
        pedal::amount(F, "width", "Width", 70.0),
        pedal::amount(F, "voices", "Voices", 0.0),
        ui::percent("flanger/mix", "Mix", 50.0).featured(),
        pedal::output(F),
        ui::choice("flanger/model", "Circuit", &["Tape", "Bucket Brigade"], 0).featured(),
        pedal::engaged(F),
    ];
    Face { file: "02-flanger", plugin: "modulation", scene, params, streams: pedal::lfo(F) }
}
