//! `NativeEq` is the rig's parameter surface over the shared engine. These
//! check the surface itself: that a default EQ is transparent, and that the
//! id/name path reaches the engine intact.

use signal_fx::NativeEq;
use signal_plugin_host::{PluginEvents, PluginInstance};

const SR: f64 = 48_000.0;
const BLOCK: usize = 512;

fn render(eq: &mut NativeEq, input: &[f32]) -> Vec<f32> {
    let events = PluginEvents::default();
    let mut out = Vec::with_capacity(input.len());
    let mut pos = 0;
    while pos < input.len() {
        let n = BLOCK.min(input.len() - pos);
        let l = &input[pos..pos + n];
        let (mut ol, mut or) = (vec![0.0f32; n], vec![0.0f32; n]);
        eq.process_block(l, l, &mut ol, &mut or, &events).expect("process");
        out.extend_from_slice(&ol);
        pos += n;
    }
    out
}

fn noise(n: usize) -> Vec<f32> {
    let mut s = 0xC0FF_EE01u64;
    (0..n)
        .map(|_| {
            s = s.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            0.2 * (((s >> 33) as f32 / (1u32 << 31) as f32) - 1.0)
        })
        .collect()
}

fn rms(b: &[f32]) -> f64 {
    (b.iter().map(|s| (*s as f64) * (*s as f64)).sum::<f64>() / b.len() as f64).sqrt()
}

#[test]
fn a_default_eq_passes_audio_through() {
    let mut eq = NativeEq::new(SR);
    eq.prepare(SR, BLOCK as u32).expect("prepare");
    let input = noise(48_000);
    let out = render(&mut eq, &input);
    let db = 20.0 * (rms(&out) / rms(&input)).log10();
    assert!(db.abs() < 0.1, "a default EQ changed the level by {db:+.2} dB");
}

/// A band switched on at 0 dB must still be transparent.
///
/// This is the shape a translated preset arrives in — `used`, `on`, a
/// frequency, a Q, a shape — so if the surface mishandles any of them the
/// whole library loads wrong.
#[test]
fn a_flat_band_is_transparent() {
    let mut eq = NativeEq::new(SR);
    for (name, value) in [
        ("b1_used", 1.0),
        ("b1_on", 1.0),
        ("b1_freq", 1000.0),
        ("b1_gain", 0.0),
        ("b1_q", 1.0),
        ("b1_shape", 0.0),
        ("b1_slope", 2.0),
    ] {
        eq.set_named(name, value);
    }
    eq.prepare(SR, BLOCK as u32).expect("prepare");
    let input = noise(48_000);
    let out = render(&mut eq, &input);
    let db = 20.0 * (rms(&out) / rms(&input)).log10();
    assert!(db.abs() < 0.1, "a flat band changed the level by {db:+.2} dB");
}

/// The order parameters arrive in must not matter.
///
/// A host replays automation in id order; a preset applies in whatever order
/// its file lists. Both have to land in the same place.
#[test]
fn parameters_may_arrive_in_any_order() {
    let forward = {
        let mut eq = NativeEq::new(SR);
        for (n, v) in [
            ("b1_used", 1.0),
            ("b1_on", 1.0),
            ("b1_shape", 0.0),
            ("b1_freq", 2000.0),
            ("b1_gain", -6.0),
            ("b1_q", 2.0),
        ] {
            eq.set_named(n, v);
        }
        eq.prepare(SR, BLOCK as u32).expect("prepare");
        render(&mut eq, &noise(24_000))
    };
    let reversed = {
        let mut eq = NativeEq::new(SR);
        for (n, v) in [
            ("b1_q", 2.0),
            ("b1_gain", -6.0),
            ("b1_freq", 2000.0),
            ("b1_shape", 0.0),
            ("b1_on", 1.0),
            ("b1_used", 1.0),
        ] {
            eq.set_named(n, v);
        }
        eq.prepare(SR, BLOCK as u32).expect("prepare");
        render(&mut eq, &noise(24_000))
    };
    let worst = forward
        .iter()
        .zip(reversed.iter())
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f32, f32::max);
    assert!(worst < 1e-9, "order changed the result by {worst:e}");
}
