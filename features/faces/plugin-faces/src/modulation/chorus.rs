//! Chorus: a sky-blue stereo chorus pedal.
//!
//! RATE, DEPTH and MIX up top, the voicing trims (voices, tone, feedback,
//! width, output) under them; the name with three detuned voices printed
//! behind it; the circuit on a small selector (poly-synth, bucket-brigade,
//! tape, orbit, clean) beside the rate lamp; the stomp.

use crate::face::{Face, canvas};
use crate::reverb::ui::{self, Dial};

use super::pedal::{self, LW};

const F: &str = "chorus";
const INK: &str = "#10263a";
const DIM: &str = "#2d4a63";

const MODELS: &[&str] = &["Poly", "BBD", "Tape", "Orbit", "Clean"];

pub fn face() -> Face {
    let scene = canvas("Stereo Chorus", |b, root| {
        let lid = pedal::body(b, root, ["#b9dcf2", "#8fc3e6", "#6ea7d0"]);

        let big = Dial::new("knob.soft_pointer", "black", 58.0, INK, DIM).scale(&["", "", "", "", "", ""]).legend(10.0).legend_at(128.0);
        pedal::knob_row(b, lid, &[("chorus/rate", "Rate"), ("chorus/depth", "Depth"), ("chorus/mix", "Mix")], 70.0, &big);
        let small = Dial::new("knob.soft_pointer", "black", 32.0, INK, DIM).legend(7.5).legend_at(214.0);
        pedal::knob_row(
            b,
            lid,
            &[("chorus/voices", "Voices"), ("chorus/tone", "Tone"), ("chorus/feedback", "Fdbk"), ("chorus/width", "Width"), ("chorus/output", "Output")],
            170.0,
            &small,
        );

        // The name, with three detuned voices behind it.
        pedal::rule(b, lid, 236.0, &ui::alpha(INK, 0.35));
        for (ph, o) in [(0.0, 0.5), (0.08, 0.32), (0.16, 0.2)] {
            pedal::wave(b, lid, 40.0, LW - 40.0, 290.0, 20.0, 2.5, ph, 2.0, &ui::alpha("#ffffff", o), &f64::sin);
        }
        pedal::title(b, lid, "Stereo Chorus", "Two bucket-brigade lines · stereo", 290.0, INK, DIM);
        pedal::rule(b, lid, 340.0, &ui::alpha(INK, 0.35));

        // Circuit selector and the rate lamp.
        let sel = Dial::new("switch.rotary", "black", 38.0, INK, DIM).options(MODELS).sweep(200.0).legend(7.5).legend_at(420.0);
        ui::dial(b, lid, "chorus/model", "Circuit", 96.0, 388.0, &sel);
        pedal::rate_lamp(b, lid, F, LW - 90.0, 378.0, "#ff3b2f", INK);

        pedal::stomp(b, lid, F, "#ff3b2f");
    });

    let params = vec![
        pedal::rate(F, 0.6),
        pedal::depth(F, 55.0),
        ui::percent("chorus/mix", "Mix", 50.0).featured(),
        pedal::amount(F, "voices", "Voices", 50.0),
        pedal::amount(F, "tone", "Tone", 50.0),
        pedal::amount(F, "feedback", "Feedback", 0.0),
        pedal::amount(F, "width", "Width", 80.0),
        pedal::output(F),
        ui::choice("chorus/model", "Circuit", &["Poly Synth", "Bucket Brigade", "Tape", "Orbit", "Clean"], 0).featured(),
        pedal::engaged(F),
    ];
    Face { file: "01-chorus", plugin: "modulation", scene, params, streams: pedal::lfo(F) }
}
