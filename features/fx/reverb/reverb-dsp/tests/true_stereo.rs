//! True-stereo (4-leg) convolution behavior.

use reverb_dsp::algorithm::ReverbAlgorithm;
use reverb_dsp::algorithms::convolution::Convolution;

const SR: f64 = 48000.0;

/// A one-tap IR: unit spike at `at` samples.
fn spike(at: usize, len: usize) -> Vec<f64> {
    let mut v = vec![0.0; len];
    v[at] = 1.0;
    v
}

#[test]
fn cross_legs_route_left_input_to_right_output() {
    let mut c = Convolution::new(SR);
    // LL = spike@10, LR = spike@20, RL = spike@30, RR = spike@40.
    assert!(c.try_load_ir_true_stereo(
        &spike(10, 256),
        &spike(20, 256),
        &spike(30, 256),
        &spike(40, 256),
    ));

    // Impulse on LEFT only.
    let mut peaks: Vec<(usize, f64, f64)> = Vec::new();
    for n in 0..2048 {
        let x = if n == 0 { 1.0 } else { 0.0 };
        let (l, r) = c.tick(x, 0.0);
        if l.abs() > 0.5 || r.abs() > 0.5 {
            peaks.push((n, l, r));
        }
    }
    // Expect L at latency+10 (LL) and R at latency+20 (LR); nothing
    // from RL/RR because the right input is silent.
    assert_eq!(peaks.len(), 2, "expected exactly two peaks: {peaks:?}");
    let (n_l, l0, _) = peaks
        .iter()
        .find(|(_, l, _)| l.abs() > 0.5)
        .copied()
        .unwrap();
    let (n_r, _, r0) = peaks
        .iter()
        .find(|(_, _, r)| r.abs() > 0.5)
        .copied()
        .unwrap();
    assert!(
        l0 > 0.9 && r0 > 0.9,
        "unit taps should come through: {peaks:?}"
    );
    assert_eq!(
        n_r - n_l,
        10,
        "LR tap must land 10 samples after LL: {peaks:?}"
    );
}

#[test]
fn plain_stereo_load_disengages_cross() {
    let mut c = Convolution::new(SR);
    assert!(c.try_load_ir_true_stereo(
        &spike(10, 256),
        &spike(20, 256),
        &spike(30, 256),
        &spike(40, 256),
    ));
    // Overwrite with a plain stereo IR: cross legs must fall silent.
    c.load_ir_stereo(&spike(10, 256), &spike(40, 256));

    let mut r_energy = 0.0;
    for n in 0..2048 {
        let x = if n == 0 { 1.0 } else { 0.0 };
        let (_, r) = c.tick(x, 0.0);
        r_energy += r * r;
    }
    assert!(
        r_energy < 1e-9,
        "left-only input must stay out of the right channel on a stereo IR: {r_energy:e}"
    );
}

#[test]
fn reprepare_keeps_cross_in_sync() {
    let mut c = Convolution::new(SR);
    // Decaying-noise legs so the reverse shape audibly changes them.
    let leg = |seed: u64| -> Vec<f64> {
        let mut s = seed;
        (0..4800)
            .map(|i| {
                s = s
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let r = ((s >> 33) as f64 / (1u64 << 31) as f64) - 1.0;
                r * (-3.0 * i as f64 / 4800.0).exp()
            })
            .collect()
    };
    assert!(c.try_load_ir_true_stereo(&leg(1), &leg(2), &leg(3), &leg(4)));

    let mut p = c.impulse_params();
    p.direction = reverb_dsp::algorithm::ImpulseDirection::Reverse;
    c.set_impulse(&p, true);
    c.reprepare_now();

    // A reversed IR swells: for a left-only impulse the RIGHT channel
    // (LR leg) must peak in its late half, proving the cross legs took
    // the same reshape as the direct ones.
    let mut early = 0.0;
    let mut late = 0.0;
    for n in 0..9600 {
        let x = if n == 0 { 1.0 } else { 0.0 };
        let (_, r) = c.tick(x, 0.0);
        // 1024-sample convolver latency; IR is 4800 long.
        if (1024..3024).contains(&n) {
            early += r * r;
        } else if (3624..5824).contains(&n) {
            late += r * r;
        }
    }
    assert!(
        late > early * 3.0,
        "reversed cross leg should swell late: early={early:.4} late={late:.4}"
    );
}
