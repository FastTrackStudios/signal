//! BigSky MX pass-C in-algorithm params: Shimmer dual shift +
//! feedback modes, Magneto ping-pong, NonLinear chop / gate speed /
//! late stage. All defaults must be bit-transparent against a chain
//! that never touched the new param structs.

use reverb_dsp::algorithm::{ShimmerFeedbackMode, ShimmerParams};
use reverb_dsp::chain::ReverbChain;
use reverb_dsp::AlgorithmType;

use audiocore_dsp::{AudioConfig, Processor};

const SR: f64 = 48000.0;

fn config() -> AudioConfig {
    AudioConfig {
        sample_rate: SR,
        max_buffer_size: 512,
    }
}

fn make_chain(algo: AlgorithmType) -> ReverbChain {
    let mut c = ReverbChain::new();
    c.set_algorithm(algo);
    c.mix = 1.0;
    c.update(config());
    c
}

fn energy(buf: &[f64]) -> f64 {
    buf.iter().map(|s| s * s).sum()
}

/// Goertzel power of `freq` over `buf`.
fn goertzel(buf: &[f64], freq: f64) -> f64 {
    let w = std::f64::consts::TAU * freq / SR;
    let coeff = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0f64, 0.0f64);
    for &x in buf {
        let s0 = x + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    (s1 * s1 + s2 * s2 - coeff * s1 * s2) / (buf.len() as f64).powi(2)
}

/// Render `secs` of a 440 Hz sine burst (first 0.5 s) through the chain.
fn render_sine(chain: &mut ReverbChain, secs: f64) -> (Vec<f64>, Vec<f64>) {
    let n = (SR * secs) as usize;
    let burst = (SR * 0.5) as usize;
    let mut l: Vec<f64> = (0..n)
        .map(|i| {
            if i < burst {
                (std::f64::consts::TAU * 440.0 * i as f64 / SR).sin() * 0.5
            } else {
                0.0
            }
        })
        .collect();
    let mut r = l.clone();
    chain.process(&mut l, &mut r);
    (l, r)
}

fn render_impulse(chain: &mut ReverbChain, n: usize) -> (Vec<f64>, Vec<f64>) {
    let mut l = vec![0.0; n];
    let mut r = vec![0.0; n];
    l[0] = 1.0;
    r[0] = 1.0;
    chain.process(&mut l, &mut r);
    (l, r)
}

// ── defaults transparent ────────────────────────────────────────────

#[test]
fn defaults_are_transparent() {
    for algo in [
        AlgorithmType::Shimmer,
        AlgorithmType::Magneto,
        AlgorithmType::NonLinear,
    ] {
        let mut plain = make_chain(algo);
        let mut touched = make_chain(algo);
        // Explicitly re-push the default structs through the setters.
        touched.shimmer = Default::default();
        touched.magneto = Default::default();
        touched.nonlinear = Default::default();
        touched.update_params();

        let (pl, _) = render_sine(&mut plain, 2.0);
        let (tl, _) = render_sine(&mut touched, 2.0);
        for (i, (a, b)) in pl.iter().zip(tl.iter()).enumerate() {
            assert!(
                (a - b).abs() < 1e-12,
                "{algo:?} defaults not transparent at {i}: {a} vs {b}"
            );
        }
    }
}

// ── Shimmer ─────────────────────────────────────────────────────────

#[test]
fn shimmer_dual_shift_two_goertzel_peaks() {
    // Input mode (no laddering), voice 1 = +12 st (880 Hz), voice 2 =
    // -12 st (220 Hz): the tail must carry BOTH shifted partials well
    // above the single-voice baseline's level at the missing interval.
    let tail = |shift2_on: bool| {
        let mut c = make_chain(AlgorithmType::Shimmer);
        c.shimmer = ShimmerParams {
            shift1_semitones: Some(12.0),
            shift2_semitones: Some(-12.0),
            voice2: shift2_on,
            amount: Some(0.8),
            feedback_mode: ShimmerFeedbackMode::Input,
        };
        c.update_params();
        let (l, _) = render_sine(&mut c, 3.0);
        // Analyze the sustained tail (input still ringing the tank).
        l[(SR * 0.6) as usize..(SR * 2.5) as usize].to_vec()
    };

    let single = tail(false);
    let dual = tail(true);

    let up_single = goertzel(&single, 880.0);
    let up_dual = goertzel(&dual, 880.0);
    let down_single = goertzel(&single, 220.0);
    let down_dual = goertzel(&dual, 220.0);

    // Voice 1 (octave up) present in both.
    assert!(up_single > 1e-12 && up_dual > 1e-12, "octave-up voice missing");
    // Voice 2 (octave down) only meaningful in dual mode.
    assert!(
        down_dual > down_single * 4.0,
        "dual shift should add the octave-down partial: dual={down_dual:e} single={down_single:e}"
    );
}

#[test]
fn shimmer_regen_ladders_input_does_not() {
    // Regenerative + octave up keeps shifting the loop: energy appears
    // TWO octaves up (1760 Hz). Input mode shifts only once — its
    // 1760 Hz content must be far weaker relative to the first octave.
    let tail = |mode: ShimmerFeedbackMode| {
        let mut c = make_chain(AlgorithmType::Shimmer);
        c.params.decay = 0.9;
        c.shimmer = ShimmerParams {
            shift1_semitones: Some(12.0),
            shift2_semitones: None,
            voice2: false,
            amount: Some(0.9),
            feedback_mode: mode,
        };
        c.update_params();
        let (l, _) = render_sine(&mut c, 4.0);
        l[(SR * 1.0) as usize..(SR * 3.8) as usize].to_vec()
    };

    let regen = tail(ShimmerFeedbackMode::Regenerative);
    let input = tail(ShimmerFeedbackMode::Input);

    let ladder_regen = goertzel(&regen, 1760.0) / goertzel(&regen, 880.0).max(1e-30);
    let ladder_input = goertzel(&input, 1760.0) / goertzel(&input, 880.0).max(1e-30);
    assert!(
        ladder_regen > ladder_input * 3.0,
        "regen must ladder (2-oct partial): regen={ladder_regen:e} input={ladder_input:e}"
    );
    for v in regen.iter().chain(input.iter()) {
        assert!(v.is_finite());
    }
}

// ── Magneto ─────────────────────────────────────────────────────────

#[test]
fn magneto_ping_pong_alternates_heads() {
    let make = |pp: bool| {
        let mut c = make_chain(AlgorithmType::Magneto);
        // Sharp taps: no diffusion smear, no extra modulation.
        c.params.diffusion = 0.0;
        c.params.modulation = 0.0;
        c.params.size = 0.5; // head spacing base = 0.25 s
        c.params.decay = 0.0;
        c.magneto.ping_pong = pp;
        c.update_params();
        c
    };

    // Head period: base = (0.05 + size*0.35) * SR.
    let head = ((0.05 + 0.5 * 0.35) * SR) as usize;
    let n = head * 5;

    let (l, r) = render_impulse(&mut make(true), n);
    // Window around each tap.
    let win = |buf: &[f64], center: usize| {
        let a = center.saturating_sub(400);
        let b = (center + 2400).min(buf.len());
        energy(&buf[a..b])
    };
    // Head 0 (even) → left, head 1 (odd) → right.
    let h0_l = win(&l, head);
    let h0_r = win(&r, head);
    let h1_l = win(&l, head * 2);
    let h1_r = win(&r, head * 2);
    assert!(
        h0_l > h0_r * 20.0,
        "head 1 must be hard left: L={h0_l:e} R={h0_r:e}"
    );
    assert!(
        h1_r > h1_l * 20.0,
        "head 2 must be hard right: L={h1_l:e} R={h1_r:e}"
    );

    // Off: taps arrive on both sides.
    let (l2, r2) = render_impulse(&mut make(false), n);
    let b0_l = win(&l2, head);
    let b0_r = win(&r2, head);
    assert!(
        b0_l < b0_r * 4.0 && b0_r < b0_l * 4.0,
        "ping-pong off must stay roughly centered: L={b0_l:e} R={b0_r:e}"
    );
}

// ── NonLinear ───────────────────────────────────────────────────────

#[test]
fn nonlinear_chop_modulates_decay() {
    // Chop is a pure multiplicative tremolo on the algorithm output,
    // so at the algorithm level (no chain post-filters) the identity
    // chopped[i] == flat[i] * (0.5 + 0.5·cos(2π·rate·i/SR)) is exact.
    use reverb_dsp::algorithm::{NonLinearParams, ReverbAlgorithm};
    use reverb_dsp::algorithms::nonlinear::NonLinear;

    let rate = 8.0;
    let render = |depth: f64| {
        let mut nl = NonLinear::new(SR);
        nl.set_nonlinear_params(&NonLinearParams {
            chop_rate_hz: rate,
            chop_depth: depth,
            ..Default::default()
        });
        let n = (SR * 1.5) as usize;
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let x = if i < 64 { 1.0 } else { 0.0 };
            let (l, _r) = nl.tick(x, x);
            assert!(l.is_finite());
            out.push(l);
        }
        out
    };

    let flat = render(0.0);
    let chopped = render(1.0);
    let mut checked = 0usize;
    for i in 0..flat.len() {
        let trem = 0.5 + 0.5 * (std::f64::consts::TAU * rate * i as f64 / SR).cos();
        let expect = flat[i] * trem;
        if flat[i].abs() > 1e-9 {
            assert!(
                (chopped[i] - expect).abs() < 1e-9 + expect.abs() * 1e-6,
                "chop AM mismatch at {i}: got {} want {expect}",
                chopped[i]
            );
            checked += 1;
        }
    }
    assert!(checked > 1000, "too little signal to verify chop ({checked})");
    // And the troughs actually silence the decay.
    let trough = (SR / 16.0) as usize; // half period at 8 Hz
    assert!(chopped[trough].abs() < flat[trough].abs() * 1e-3 + 1e-12);
}

#[test]
fn nonlinear_gate_speed_shortens_hold() {
    let render = |speed: f64| {
        let mut c = make_chain(AlgorithmType::NonLinear);
        c.params.extra_a = 0.3; // Gate shape
        c.params.size = 0.5; // env window ≈ 1.05 s
        c.nonlinear.gate_speed = speed;
        c.update_params();
        let (l, _) = render_sine(&mut c, 2.0);
        l
    };

    let env_len = (0.1 + 0.5 * 1.9) * SR; // matches size mapping
    // Window between the fast hold point (0.5) and the slow one (0.9):
    // slow (speed 1) is still at full level there, fast (speed 0) has
    // released.
    let w0 = (env_len * 0.62) as usize;
    let w1 = (env_len * 0.85) as usize;

    let fast = energy(&render(0.0)[w0..w1]);
    let slow = energy(&render(1.0)[w0..w1]);
    assert!(
        slow > fast * 2.0,
        "faster gate must release earlier: fast={fast:e} slow={slow:e}"
    );
}

#[test]
fn nonlinear_late_stage_adds_tail() {
    let render = |level: f64| {
        let mut c = make_chain(AlgorithmType::NonLinear);
        c.params.size = 0.2; // short burst window ≈ 0.48 s
        c.nonlinear.late_level = level;
        c.nonlinear.late_decay = 0.9;
        c.nonlinear.late_speed = 0.8;
        c.update_params();
        let (l, _) = render_sine(&mut c, 3.0);
        l
    };

    let off = render(0.0);
    let on = render(1.0);
    for v in on.iter() {
        assert!(v.is_finite(), "late stage produced non-finite output");
    }
    // Well after the nonlinear burst window, the late tail dominates.
    let late = (SR * 1.8) as usize;
    let e_off = energy(&off[late..]);
    let e_on = energy(&on[late..]);
    assert!(
        e_on > e_off * 5.0 && e_on > 1e-12,
        "late stage must ring past the burst: on={e_on:e} off={e_off:e}"
    );
}
