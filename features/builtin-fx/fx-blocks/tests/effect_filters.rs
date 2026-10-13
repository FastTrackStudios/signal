//! The delay's and reverb's band controls shape the effect — and only the
//! effect: the dry passes untouched. Checked on the wet alone (`dry` 0),
//! set at build time and again live, as a knob turned while playing.

use daw::plugin::{PluginEvents, PluginInstance};
use fx_blocks::{NativeDelay, NativeReverb};

const SR: f64 = 48_000.0;
const BLOCK: usize = 256;

/// A short noise burst, then silence; the output's (low, high) band energy
/// after the burst — i.e. the effect alone.
fn bands(fx: &mut dyn PluginInstance, live: &[(u32, f64)]) -> (f64, f64) {
    fx.prepare(SR, BLOCK as u32).unwrap();
    let mut seed = 1u32;
    let mut out = Vec::new();
    for b in 0..120 {
        let input: Vec<f32> = (0..BLOCK)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                if b < 4 { (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5 } else { 0.0 }
            })
            .collect();
        let ev = PluginEvents { params: if b == 0 { live } else { &[] }, ..PluginEvents::default() };
        let (mut ol, mut or) = (vec![0.0f32; BLOCK], vec![0.0f32; BLOCK]);
        fx.process_block(&input, &input, &mut ol, &mut or, &ev).unwrap();
        if b >= 4 {
            out.extend_from_slice(&ol);
        }
    }
    // Low: a ~300 Hz one-pole low-pass; high: first difference (rises ~6 dB/oct).
    let k = 1.0 - (-std::f64::consts::TAU * 300.0 / SR).exp();
    let (mut lp, mut prev, mut lo, mut hi) = (0.0f64, 0.0f64, 0.0, 0.0);
    for &s in &out {
        let s = f64::from(s);
        lp += (s - lp) * k;
        lo += lp * lp;
        hi += (s - prev) * (s - prev);
        prev = s;
    }
    (lo, hi)
}

fn reverb(set: &[(&str, f64)]) -> NativeReverb {
    let mut r = NativeReverb::new(SR);
    r.set_named("dry", 0.0);
    r.set_named("decay", 0.3);
    for (n, v) in set {
        r.set_named(n, *v);
    }
    r
}

fn delay(set: &[(&str, f64)]) -> NativeDelay {
    let mut d = NativeDelay::new(SR);
    d.set_named("dry", 0.0);
    d.set_named("time", 80.0);
    d.set_named("feedback", 0.6);
    d.set_named("style", 1.0);
    for (n, v) in set {
        d.set_named(n, *v);
    }
    d
}

#[test]
fn reverb_high_cut_takes_the_top_off_the_tail() {
    let (_, open) = bands(&mut reverb(&[("mix", 1.0)]), &[]);
    let (_, cut) = bands(&mut reverb(&[("mix", 1.0), ("high_cut", 2000.0)]), &[]);
    assert!(cut < open * 0.3, "high band {cut} vs open {open}");
}

#[test]
fn reverb_low_cut_takes_the_bottom_off_the_tail() {
    let (open, _) = bands(&mut reverb(&[("mix", 1.0)]), &[]);
    let (cut, _) = bands(&mut reverb(&[("mix", 1.0), ("low_cut", 800.0)]), &[]);
    assert!(cut < open * 0.3, "low band {cut} vs open {open}");
}

#[test]
fn reverb_cuts_apply_live() {
    let (_, open) = bands(&mut reverb(&[("mix", 1.0)]), &[]);
    let (_, cut) = bands(&mut reverb(&[("mix", 1.0)]), &[(96, 2000.0)]);
    assert!(cut < open * 0.3, "a live high cut: {cut} vs open {open}");
}

#[test]
fn delay_high_cut_darkens_the_repeats() {
    let (_, open) = bands(&mut delay(&[("high_cut", 20_000.0)]), &[]);
    let (_, cut) = bands(&mut delay(&[("high_cut", 2000.0)]), &[]);
    assert!(cut < open * 0.3, "high band {cut} vs open {open}");
    let (_, live) = bands(&mut delay(&[("high_cut", 20_000.0)]), &[(63, 2000.0)]);
    assert!(live < open * 0.3, "live: {live} vs open {open}");
}
