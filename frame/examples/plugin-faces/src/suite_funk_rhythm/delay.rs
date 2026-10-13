//! `12-repeat-bigbox-404x286`: *Repeat-t-t*, a stereo delay in a rendered
//! big-box enclosure (404 × 286, the BigBox class) — bright green metallic
//! over the copper base, two mini toggles (Sync: free or DAW; Single /
//! Dual), a backlit dot-matrix LCD with the sync mode, the left and right
//! times and the tempo (live), MIX and FEEDBACK up top, the low-cut, TIME
//! L, TIME R and high-cut knobs (the cuts turned out as the reference has
//! them), an infinity sign, and two stomps: ENGAGE and a momentary TAP
//! TEMPO, each with its LED.

use document_proto::{NodeId, NodeKind, TextAlign};
use surface_kit::draw;
use surface_proto::{Range, Unit};
use surface_widgets::Builder;

use super::parts::{self, BIGBOX_W, PEDAL_H, Type};
use super::pedals::{self, K};
use crate::face::{Face, canvas};
use crate::reverb::ui;

const WHITE: &str = "#fbfaf6";
const DIVS: &[&str] = &["1/32", "1/16T", "1/16", "1/8T", "1/8", "1/8D", "1/4T", "1/4", "1/4D", "1/2", "1/1"];

/// Set every text node under `root` in the display's dot-matrix face.
fn dot_matrix(b: &mut Builder<'_>, root: NodeId, weight: f64) {
    let l = draw::Lettering::family(draw::family::DOTO).weight(weight);
    for id in b.scene.walk(root) {
        if let Some(n) = b.scene.node_mut(id)
            && matches!(n.kind, NodeKind::Text(_))
        {
            l.apply(n);
        }
    }
}

pub(super) fn face() -> Face {
    let scene = canvas("Repeat-t-t Delay", |b, root| {
        let lid = parts::stompbox_with(b, root, (BIGBOX_W, PEDAL_H), "#52bf4f", "powder", pedals::BASE, 14.0, pedals::LID);
        let t = Type::new(7.0, WHITE, 800.0);
        // The mini toggles and their legends.
        for (addr, legend, fx) in [("repeat/sync", "Sync", 0.31), ("repeat/dual", "Single/Dual", 0.69)] {
            parts::toggle(b, lid.id, addr, legend, 9.0, "chrome", lid.at(fx, 0.12));
            parts::centred(b, lid.id, legend, lid.at(fx, 0.04), t);
        }
        // Knobs: the cuts turned out (low cut left, high cut right).
        let ks = [
            K("repeat/mix", "Mix", (0.127, 0.25), true),
            K("repeat/feedback", "Feedback", (0.873, 0.25), true),
            K("repeat/low", "", (0.127, 0.51), false),
            K("repeat/time_l", "Time L", (0.373, 0.505), false),
            K("repeat/time_r", "Time R", (0.627, 0.505), false),
            K("repeat/high", "", (0.873, 0.51), false),
        ];
        pedals::knobs(b, &lid, &ks, 40.0);
        pedals::filter_marks(b, lid.id, lid.at(0.127, 0.645), lid.at(0.873, 0.645));
        // The LCD: sync mode and voices, then the times and the tempo, in
        // a dot-matrix face.
        let (x, y) = lid.at(0.27, 0.185);
        let (dw, dh) = (lid.w * 0.46, lid.h * 0.155);
        let face = ui::display(b, lid.id, "LCD", x, y, dw, dh, "#b8c8c0", "glass");
        let dim = "#a3b0a9";
        let lit = "#e6eee9";
        let top = ui::lit_text(b, face, "FREE / DAW / TAP", 7.0, 11.0, 90.0, 6.4, dim, TextAlign::Left, 500.0);
        let top2 = ui::lit_text(b, face, "SINGLE / DUAL", dw - 97.0, 11.0, 90.0, 6.4, dim, TextAlign::Right, 500.0);
        let l = ui::lit_readout(b, face, "Time L readout", ui::bind("repeat/time_l"), 6.0, 29.0, 44.0, 13.0, lit, TextAlign::Left);
        let tempo = ui::lit_readout(b, face, "Tempo readout", ui::bind("repeat/bpm"), 50.0, 29.0, dw - 100.0, 13.0, lit, TextAlign::Center);
        let r = ui::lit_readout(b, face, "Time R readout", ui::bind("repeat/time_r"), dw - 50.0, 29.0, 44.0, 13.0, lit, TextAlign::Right);
        for n in [top, top2] {
            dot_matrix(b, n, 600.0);
        }
        // The tempo to a tenth, as the unit's display prints it.
        if let Some(c) = b.controls.get_mut(&tempo) {
            c.format = Some(surface_proto::ValueFormat::decimals(1));
        }
        for n in [l, tempo, r] {
            dot_matrix(b, n, 700.0);
        }
        ui::glass(b, face);
        // LEDs over the stomps; the tap LED blinks the tempo.
        parts::status_led(b, lid.id, "repeat/on", 5.0, lid.at(0.127, 0.71), "#e8f6ff", "light", None);
        parts::status_led(b, lid.id, "repeat/tap", 5.0, lid.at(0.873, 0.71), "#e8f6ff", "light", Some("param(repeat/bpm)/60"));
        parts::footswitch(b, lid.id, "repeat/on", "Engage", 24.0, lid.at(0.127, 0.83), false);
        parts::footswitch(b, lid.id, "repeat/tap", "Tap Tempo", 24.0, lid.at(0.873, 0.83), true);
        parts::centred(b, lid.id, "Engage", lid.at(0.127, 0.95), Type::new(6.4, WHITE, 800.0));
        parts::centred(b, lid.id, "Tap Tempo", lid.at(0.873, 0.95), Type::new(6.4, WHITE, 800.0));
        // The infinity sign and the name.
        let (ix, iy) = lid.at(0.5, 0.8);
        let inf: Vec<(f64, f64)> = (0..=64)
            .map(|i| {
                let a = std::f64::consts::TAU * f64::from(i) / 64.0;
                let d = 1.0 + a.sin().powi(2);
                (ix + 30.0 * a.cos() / d, iy + 30.0 * a.sin() * a.cos() / d)
            })
            .collect();
        parts::art(b, lid.id, &[inf], 3.4, WHITE);
        parts::centred(b, lid.id, "Repeat-t-t", lid.at(0.5, 0.915), Type::new(12.5, WHITE, 800.0));
    });
    let params = vec![
        parts::pct("repeat/mix", "Mix", 50.0),
        parts::pct("repeat/feedback", "Feedback", 50.0),
        parts::hz("repeat/low", "Low Cut", 20.0, 1000.0, parts::log_at(20.0, 1000.0, 0.2)),
        parts::hz("repeat/high", "High Cut", 1000.0, 20000.0, parts::log_at(1000.0, 20000.0, 0.8)),
        parts::pick("repeat/time_l", "Time L", DIVS, 7),
        parts::pick("repeat/time_r", "Time R", DIVS, 5),
        parts::switch("repeat/sync", "Sync", true),
        parts::switch("repeat/dual", "Single/Dual", true),
        ui::param("repeat/bpm", "Tempo", Range::linear(40.0, 240.0, Unit::Bpm), 120.0).featured(),
        parts::switch("repeat/on", "Engage", true),
        parts::switch("repeat/tap", "Tap Tempo", false),
    ];
    ui::formatted(Face { file: "12-repeat-bigbox-404x286", plugin: pedals::PLUGIN, scene, params, streams: Vec::new() })
}
