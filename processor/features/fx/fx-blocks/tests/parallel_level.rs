//! A delay's or reverb's `level` is a gain on the wet: with `mix` pinned at
//! 1, `level = 20·log10(m)` plays exactly what `mix = m` did.

use daw::plugin::{PluginEvents, PluginInstance};
use fx_blocks::{NativeDelay, NativeReverb};

const SR: f64 = 48_000.0;
const BLOCK: usize = 256;

fn render(fx: &mut dyn PluginInstance) -> Vec<f32> {
    fx.prepare(SR, BLOCK as u32).unwrap();
    let ev = PluginEvents::default();
    let mut out = Vec::new();
    for b in 0..80 {
        let mut input = vec![0.0f32; BLOCK];
        if b == 0 {
            input[0] = 1.0;
        }
        let (mut ol, mut or) = (vec![0.0f32; BLOCK], vec![0.0f32; BLOCK]);
        fx.process_block(&input, &input, &mut ol, &mut or, &ev).unwrap();
        out.extend_from_slice(&ol);
    }
    out
}

fn assert_same(a: &[f32], b: &[f32]) {
    let peak = a.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    let diff = a.iter().zip(b).fold(0.0f32, |m, (x, y)| m.max((x - y).abs()));
    assert!(peak > 0.01, "the effect made sound");
    assert!(diff < 1e-4 * peak.max(1.0), "max diff {diff}");
}

#[test]
fn delay_level_matches_the_mix_it_replaces() {
    let mut by_mix = NativeDelay::new(SR);
    by_mix.set_named("time", 120.0);
    by_mix.set_named("mix", 0.25);
    let mut by_level = NativeDelay::new(SR);
    by_level.set_named("time", 120.0);
    by_level.set_named("mix", 1.0);
    by_level.set_named("level", 20.0 * 0.25f64.log10());
    assert_same(&render(&mut by_mix), &render(&mut by_level));
}

#[test]
fn reverb_level_matches_the_mix_it_replaces() {
    let mut by_mix = NativeReverb::new(SR);
    by_mix.set_named("mix", 0.2);
    let mut by_level = NativeReverb::new(SR);
    by_level.set_named("mix", 1.0);
    by_level.set_named("level", 20.0 * 0.2f64.log10());
    assert_same(&render(&mut by_mix), &render(&mut by_level));
}

/// The gain block's pan: a −4.5 dB law normalised to the centre — the
/// centre untouched, hard right +4.5 dB on the right and silent on the left.
#[test]
fn gain_pan_is_a_4_5_db_law() {
    use fx_blocks::NativeGain;
    let run = |pan: f64| {
        let mut g = NativeGain::new(SR);
        g.set_named("pan", pan);
        g.prepare(SR, BLOCK as u32).unwrap();
        let x = vec![0.5f32; BLOCK];
        let (mut l, mut r) = (vec![0.0f32; BLOCK], vec![0.0f32; BLOCK]);
        g.process_block(&x, &x, &mut l, &mut r, &PluginEvents::EMPTY).unwrap();
        (l[BLOCK - 1], r[BLOCK - 1])
    };
    let (l, r) = run(0.0);
    assert!((l - 0.5).abs() < 1e-5 && (r - 0.5).abs() < 1e-5, "centre: unity");
    let (l, r) = run(1.0);
    let db = 20.0 * (r / 0.5).log10();
    assert!((db - 4.5).abs() < 0.05, "hard right: +4.5 dB (4.52, the law exactly), got {db:+.2}");
    assert!(l.abs() < 1e-6, "hard right: the left silent");
    let (l, r) = run(-1.0);
    assert!((20.0 * (l / 0.5).log10() - 4.5).abs() < 0.05 && r.abs() < 1e-6, "hard left mirrors it");
}
