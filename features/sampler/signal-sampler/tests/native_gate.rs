//! (Here, where the rig builds the gate; fx-blocks itself is a processor
//! crate.)
//! The noise gate's modes: how far it closes (range), the expander's 2:1
//! under the threshold, and the hold keeping it open after the level
//! drops.

use daw::plugin::{PluginEvents, PluginInstance};
use fx_blocks::NativeGate;

const SR: f64 = 48_000.0;

/// The gain the gate settles to on a steady signal at `db` dBFS.
fn settled(fx: &mut NativeGate, db: f64) -> f64 {
    let a = 10f64.powf(db / 20.0) as f32;
    let x = vec![a; 4800];
    let (mut l, mut r) = (vec![0.0; 4800], vec![0.0; 4800]);
    for _ in 0..20 {
        fx.process_block(&x, &x, &mut l, &mut r, &PluginEvents::EMPTY).unwrap();
    }
    f64::from(l[4799] / a)
}

fn gate(params: &[(&str, f64)]) -> NativeGate {
    let mut fx = NativeGate::new(SR);
    fx.prepare(SR, 4800).unwrap();
    for (n, v) in params {
        fx.set_named(n, *v);
    }
    fx
}

#[test]
fn above_the_threshold_it_is_open() {
    let mut fx = gate(&[("threshold", -50.0)]);
    assert!((settled(&mut fx, -20.0) - 1.0).abs() < 1e-3);
}

#[test]
fn closed_it_falls_to_the_range() {
    let mut fx = gate(&[("threshold", -30.0), ("range", 20.0)]);
    let g = settled(&mut fx, -40.0);
    assert!((20.0 * g.log10() + 20.0).abs() < 0.5, "closed at {} dB", 20.0 * g.log10());
}

#[test]
fn the_expander_falls_two_to_one() {
    // 10 dB under the threshold: the level falls another 10 dB.
    let mut fx = gate(&[("threshold", -30.0), ("mode", 1.0)]);
    let g = settled(&mut fx, -40.0);
    assert!((20.0 * g.log10() + 10.0).abs() < 1.0, "expanded to {} dB", 20.0 * g.log10());
}

#[test]
fn the_hysteresis_keeps_it_open_just_under_the_threshold() {
    let mut fx = gate(&[("threshold", -30.0), ("hysteresis", 6.0)]);
    settled(&mut fx, -20.0);
    assert!((settled(&mut fx, -33.0) - 1.0).abs() < 1e-3);
    assert!(settled(&mut fx, -40.0) < 1e-3);
}

#[test]
fn the_hold_keeps_it_open_after_the_level_drops() {
    let mut fx = gate(&[("threshold", -30.0), ("hold", 200.0), ("release", 5.0)]);
    settled(&mut fx, -20.0);
    // 50 ms of quiet: still held open.
    let x = vec![0.0f32; 2400];
    let (mut l, mut r) = (vec![0.0; 2400], vec![0.0; 2400]);
    fx.process_block(&x, &x, &mut l, &mut r, &PluginEvents::EMPTY).unwrap();
    let y = vec![0.001f32; 1];
    let (mut l1, mut r1) = (vec![0.0; 1], vec![0.0; 1]);
    fx.process_block(&y, &y, &mut l1, &mut r1, &PluginEvents::EMPTY).unwrap();
    assert!(l1[0] / 0.001 > 0.9, "held gain {}", l1[0] / 0.001);
}
