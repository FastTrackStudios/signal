//! No setting of a delay or reverb may run away.
//!
//! Every reverb algorithm at full decay, the shimmer at its most extreme
//! (full amount, two coherent octave voices), every delay style at the top
//! of its feedback: a burst in, then silence, and the output must stay
//! finite and never grow past what the burst put in. A regenerating block
//! that rings long is fine; one that climbs is a speaker-destroying bug —
//! the shimmer's did, to infinity in six seconds.

use daw::plugin::{PluginEvents, PluginInstance};
use fx_blocks::{NativeDelay, NativeReverb};

const SR: f64 = 48_000.0;
const BLOCK: usize = 512;

/// Peak of the first second (the burst and its onset) and the largest peak
/// of any later second, over `secs`.
fn peaks(fx: &mut dyn PluginInstance, secs: usize) -> (f32, f32) {
    fx.prepare(SR, BLOCK as u32).unwrap();
    let per_sec = SR as usize / BLOCK;
    let mut seed = 5u32;
    let (mut first, mut later) = (0.0f32, 0.0f32);
    for sec in 0..secs {
        for b in 0..per_sec {
            let x: Vec<f32> = (0..BLOCK)
                .map(|_| {
                    if sec == 0 && b < 10 {
                        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                        ((seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.5
                    } else {
                        0.0
                    }
                })
                .collect();
            let (mut l, mut r) = (vec![0.0f32; BLOCK], vec![0.0f32; BLOCK]);
            fx.process_block(&x, &x, &mut l, &mut r, &PluginEvents::EMPTY).unwrap();
            let p = l.iter().chain(&r).fold(0.0f32, |m, s| {
                if s.is_finite() { m.max(s.abs()) } else { f32::INFINITY }
            });
            if sec == 0 {
                first = first.max(p);
            } else {
                later = later.max(p);
            }
        }
    }
    (first, later)
}

fn assert_bounded(what: &str, fx: &mut dyn PluginInstance) {
    let (first, later) = peaks(fx, 12);
    assert!(first.is_finite() && later.is_finite(), "{what}: not finite");
    // Room to bloom (a swell, a shimmer building), none to run away.
    assert!(later <= first.max(0.05) * 4.0, "{what}: grew from {first} to {later}");
}

#[test]
fn every_reverb_at_full_decay_stays_bounded() {
    for algorithm in 0..=13 {
        for variant in 0..=2 {
            let mut r = NativeReverb::new(SR);
            for (n, v) in [
                ("algorithm", f64::from(algorithm)),
                ("variant", f64::from(variant)),
                ("decay", 0.99),
                ("size", 1.0),
                ("mix", 1.0),
                ("dry", 0.0),
                ("modulation", 1.0),
            ] {
                r.set_named(n, v);
            }
            assert_bounded(&format!("reverb {algorithm}/{variant}"), &mut r);
        }
    }
}

#[test]
fn the_shimmer_at_its_most_extreme_stays_bounded() {
    for (amount, voice2, shift2) in [(1.0, 1.0, 12.0), (1.0, 0.0, 12.0), (0.5, 1.0, 12.0), (1.0, 1.0, 7.0)] {
        let mut r = NativeReverb::new(SR);
        for (n, v) in [
            ("algorithm", 6.0),
            ("decay", 1.0),
            ("size", 1.0),
            ("mix", 1.0),
            ("dry", 0.0),
            ("shim_amount", amount),
            ("shim_shift1", 12.0),
            ("shim_shift2", shift2),
            ("shim_voice2", voice2),
        ] {
            r.set_named(n, v);
        }
        assert_bounded(&format!("shimmer amount {amount} voice2 {voice2} +{shift2}"), &mut r);
    }
}

#[test]
fn every_delay_style_at_top_feedback_stays_bounded() {
    for style in 0..=12 {
        let mut d = NativeDelay::new(SR);
        for (n, v) in [
            ("style", f64::from(style)),
            ("time", 250.0),
            ("feedback", 0.95),
            ("mix", 1.0),
            ("dry", 0.0),
            ("interval", 27.0),
            ("blend", 1.0),
        ] {
            d.set_named(n, v);
        }
        assert_bounded(&format!("delay style {style}"), &mut d);
    }
}
