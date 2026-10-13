//! Two engines must not draw the same picture.
//!
//! The flanger and the phaser both rendered as vertical banding and were, to
//! the eye, one panel with two labels — which nothing caught, because every
//! frame was still a legal picture. The delay and reverb families have had
//! this test since they were written; the engines did not, and that is
//! exactly the pair it would have flagged.
//!
//! Rendered on a real device through `fts_audio_ui::shader::probe`, because
//! the branches are in WGSL and a Rust-side assertion tests a different
//! program.

use fts_audio_ui::shader::Uniforms;
use modulation_ui::viz::{Engine, SHADER, engine_index};

const W: u32 = 480;
const H: u32 = 150;

fn frame(
    probe: &fts_audio_ui::shader::probe::Probe,
    engine: Engine,
    t: f32,
) -> fts_audio_ui::shader::probe::Frame {
    with_params(probe, engine, t, [0.9, 0.75, 0.5, 1.0])
}

fn with_params(
    probe: &fts_audio_ui::shader::probe::Probe,
    engine: Engine,
    t: f32,
    params: [f32; 4],
) -> fts_audio_ui::shader::probe::Frame {
    let u = Uniforms {
        frame: [W as f32, H as f32, t, engine_index(engine)],
        params,
        color: [34.0 / 255.0, 211.0 / 255.0, 238.0 / 255.0, 1.0],
    };
    probe.render(SHADER, bytemuck::bytes_of(&u), W, H)
}

#[test]
fn every_engine_draws_something_and_no_two_draw_the_same_thing() {
    let Some(probe) = fts_audio_ui::shader::probe::Probe::open() else {
        eprintln!("no GPU here — skipping");
        return;
    };

    let frames: Vec<_> = Engine::ALL
        .iter()
        .map(|&e| (e, frame(&probe, e, 1.7)))
        .collect();

    for (engine, f) in &frames {
        assert!(
            f.coverage() > 0.05,
            "{engine:?} drew nothing — a branch that renders an empty panel is \
             indistinguishable from a shader that failed"
        );
    }

    for (i, (a, fa)) in frames.iter().enumerate() {
        for (b, fb) in &frames[i + 1..] {
            let d = fa.difference(fb);
            assert!(
                d > 0.02,
                "{a:?} and {b:?} draw the same picture (difference {d:.4}) — \
                 an engine that looks like another one tells the player nothing"
            );
        }
    }
}

/// Every one of these is a moving picture, and an engine that is identical
/// between two moments is one whose animation is not running.
#[test]
fn every_engine_actually_moves() {
    let Some(probe) = fts_audio_ui::shader::probe::Probe::open() else {
        eprintln!("no GPU here — skipping");
        return;
    };
    for engine in Engine::ALL {
        let a = frame(&probe, engine, 0.4);
        let b = frame(&probe, engine, 1.9);
        let d = a.difference(&b);
        assert!(
            d > 0.002,
            "{engine:?} is the same at two different times (difference {d:.5}) — \
             a still modulation panel says the effect is not running"
        );
    }
}

/// Every engine answers its own knobs.
///
/// A panel that animates but ignores the controls is the subtlest failure
/// here: it looks alive, so nobody checks, and the player learns to ignore
/// it because turning a knob does nothing. Each parameter is moved on its
/// own, at a fixed moment, so only that parameter can account for the
/// difference.
#[test]
fn every_engine_answers_rate_depth_and_mix() {
    let Some(probe) = fts_audio_ui::shader::probe::Probe::open() else {
        eprintln!("no GPU here — skipping");
        return;
    };
    const T: f32 = 1.1;
    // rate, depth, mix — each swung from low to high with the others held.
    let knobs: [(&str, [f32; 4], [f32; 4]); 3] = [
        ("rate", [0.25, 0.7, 0.6, 1.0], [3.5, 0.7, 0.6, 1.0]),
        ("depth", [1.0, 0.10, 0.6, 1.0], [1.0, 0.95, 0.6, 1.0]),
        ("mix", [1.0, 0.7, 0.05, 1.0], [1.0, 0.7, 1.00, 1.0]),
    ];
    for engine in Engine::ALL {
        for (name, low, high) in knobs {
            let a = with_params(&probe, engine, T, low);
            let b = with_params(&probe, engine, T, high);
            let d = a.difference(&b);
            assert!(
                d > 0.002,
                "{engine:?} ignores {name} (difference {d:.5}) — a panel that \
                 animates but does not answer its knobs looks alive and teaches \
                 the player to stop looking at it"
            );
        }
    }
}

/// Bypassed is dimmer than engaged, and still drawn. "Off" and "not
/// configured" must not be the same picture.
#[test]
fn bypassed_is_dimmer_but_not_blank() {
    let Some(probe) = fts_audio_ui::shader::probe::Probe::open() else {
        eprintln!("no GPU here — skipping");
        return;
    };
    for engine in Engine::ALL {
        let on = with_params(&probe, engine, 1.1, [1.0, 0.7, 0.6, 1.0]);
        let off = with_params(&probe, engine, 1.1, [1.0, 0.7, 0.6, 0.0]);
        assert!(
            off.coverage() > 0.02,
            "{engine:?} bypassed is blank — it should keep its shape"
        );
        assert!(
            on.difference(&off) > 0.01,
            "{engine:?} looks the same bypassed as engaged"
        );
    }
}
