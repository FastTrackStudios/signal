//! Pitch-shifter quality regression for the shimmer feedback path.
//!
//! History (440 Hz sine, ratio 2.0, full shimmer mix, no feedback;
//! inharmonicity = mean Goertzel energy at 8 non-harmonic probe bins /
//! energy at 880 Hz over the last second):
//!   - local GrainReader (pre-pitch-dsp):        ratio 0.0051
//!   - pitch-dsp GranularShifter (dual grains):  ratio 0.0261 (rejected)
//!   - pitch-dsp WsolaShifter (current):         ratio 0.0001
//!
//! The bound below holds the WSOLA-level quality; a regression back to
//! grain-level artifacts fails loudly.

use delay_dsp::shimmer_delay::ShimmerDelay;
use std::f64::consts::PI;

const SR: f64 = 48000.0;

fn goertzel(signal: &[f64], freq: f64) -> f64 {
    let omega = 2.0 * PI * freq / SR;
    let coeff = 2.0 * omega.cos();
    let (mut s1, mut s2) = (0.0f64, 0.0f64);
    for &x in signal {
        let s0 = x + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    (s1 * s1 + s2 * s2 - coeff * s1 * s2) / signal.len() as f64
}

#[test]
fn shimmer_octave_is_clean() {
    let mut d = ShimmerDelay::new();
    d.time_ms = 200.0;
    d.feedback = 0.0;
    d.pitch_ratio = 2.0;
    d.shimmer_mix = 1.0;
    d.hicut_freq = 0.0;
    d.decay_tilt = 0.0;
    d.update(SR);

    let n = (SR * 2.0) as usize;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let x = (2.0 * PI * 440.0 * i as f64 / SR).sin() * 0.5;
        out.push(d.tick(x, 0));
    }
    let tail = &out[n / 2..];

    let target = goertzel(tail, 880.0);
    let probes = [610.0, 700.0, 760.0, 990.0, 1100.0, 1180.0, 1500.0, 2100.0];
    let inharm: f64 = probes.iter().map(|&f| goertzel(tail, f)).sum::<f64>() / probes.len() as f64;
    let ratio = inharm / target;

    assert!(
        target > 100.0,
        "880 Hz content should dominate strongly (near-pure tone): {target:.3e}"
    );
    assert!(
        ratio < 0.002,
        "inharmonic artifact ratio regressed: {ratio:.5} (WSOLA baseline 0.0001, old grain 0.0051)"
    );
}
