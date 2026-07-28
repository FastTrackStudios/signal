//! BigSky MX Impulse live-parameter tests: decay/tail/attack/stretch/
//! direction re-shape the IR (synchronously here via `reprepare_now`;
//! the async worker path is covered by `chain_reshape_worker_applies`),
//! feedback recirculates at runtime.

use reverb_dsp::algorithm::{
    ImpulseDirection, ImpulseParams, ImpulseTail, ReverbAlgorithm,
};
use reverb_dsp::algorithms::convolution::Convolution;
use reverb_dsp::chain::ReverbChain;
use reverb_dsp::ir::ImpulseReshaper;
use reverb_dsp::AlgorithmType;

use audiocore_dsp::{AudioConfig, Processor};

const SR: f64 = 48000.0;
/// Convolver latency: one partition block.
const LATENCY: usize = 512;

/// Deterministic 0.5 s decaying-noise IR (identical L/R for simplicity).
fn test_ir() -> Vec<f64> {
    let n = (SR * 0.5) as usize;
    let mut state = 0x1234_5678_u32;
    let mut rng = || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        (state as i32) as f64 / i32::MAX as f64
    };
    (0..n)
        .map(|i| rng() * 10f64.powf(-3.0 * i as f64 / n as f64))
        .collect()
}

/// Render an impulse through a Convolution with the given params.
fn render(params: &ImpulseParams, seconds: f64) -> Vec<f64> {
    let mut conv = Convolution::new(SR);
    let ir = test_ir();
    conv.load_ir_stereo(&ir, &ir);
    conv.set_impulse(params, true);
    conv.reprepare_now();
    let n = (SR * seconds) as usize;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let x = if i == 0 { 1.0 } else { 0.0 };
        let (l, _r) = conv.tick(x, x);
        assert!(l.is_finite(), "non-finite at {i}");
        out.push(l);
    }
    out
}

fn energy(buf: &[f64]) -> f64 {
    buf.iter().map(|s| s * s).sum()
}

#[test]
fn defaults_are_transparent() {
    // A Convolution that never saw set_impulse vs one that got the
    // defaults + a re-preparation: outputs bit-identical (1e-12).
    let ir = test_ir();
    let mut a = Convolution::new(SR);
    a.load_ir_stereo(&ir, &ir);
    let mut b = Convolution::new(SR);
    b.load_ir_stereo(&ir, &ir);
    b.set_impulse(&ImpulseParams::default(), true);
    b.reprepare_now();

    for i in 0..(SR as usize) {
        let x = if i == 0 { 1.0 } else { 0.0 };
        let (la, _) = a.tick(x, x);
        let (lb, _) = b.tick(x, x);
        assert!(
            (la - lb).abs() < 1e-12,
            "diverged at {i}: {la} vs {lb}"
        );
    }
}

#[test]
fn decay_gate_truncates_hard() {
    let out = render(
        &ImpulseParams {
            decay: 0.5,
            tail: ImpulseTail::Gate,
            ..Default::default()
        },
        1.0,
    );
    let ir_len = (SR * 0.5) as usize;
    let cut = LATENCY + ir_len / 2;
    // Everything after the gate point (plus one FFT block of slop).
    let after = energy(&out[cut + 2 * LATENCY..]);
    let before = energy(&out[..cut]);
    assert!(
        after < before * 1e-8,
        "gate should silence the tail: after={after:e} before={before:e}"
    );
}

#[test]
fn envelope_ramps_gate_cuts() {
    let env = render(
        &ImpulseParams {
            decay: 0.5,
            tail: ImpulseTail::Envelope,
            ..Default::default()
        },
        1.0,
    );
    let gate = render(
        &ImpulseParams {
            decay: 0.5,
            tail: ImpulseTail::Gate,
            ..Default::default()
        },
        1.0,
    );
    let ir_len = (SR * 0.5) as usize;
    // Window just before the cut: the envelope has ramped down, the
    // gate is still at full level.
    let w0 = LATENCY + (ir_len as f64 * 0.45) as usize;
    let w1 = LATENCY + (ir_len as f64 * 0.49) as usize;
    let e_env = energy(&env[w0..w1]);
    let e_gate = energy(&gate[w0..w1]);
    assert!(
        e_env < e_gate * 0.25,
        "envelope should ramp into the cut: env={e_env:e} gate={e_gate:e}"
    );
}

#[test]
fn attack_softens_onset() {
    let hard = render(&ImpulseParams::default(), 0.8);
    let soft = render(
        &ImpulseParams {
            attack: 1.0,
            ..Default::default()
        },
        0.8,
    );
    let ir_len = (SR * 0.5) as usize;
    let head = LATENCY + ir_len / 10;
    let total_hard = energy(&hard);
    let total_soft = energy(&soft);
    let ratio_hard = energy(&hard[LATENCY..head]) / total_hard;
    let ratio_soft = energy(&soft[LATENCY..head]) / total_soft;
    assert!(
        ratio_soft < ratio_hard * 0.6,
        "attack should shift energy away from the onset: {ratio_soft} vs {ratio_hard}"
    );
}

#[test]
fn stretch_lengthens_and_darkens() {
    let normal = render(&ImpulseParams::default(), 1.6);
    let stretched = render(
        &ImpulseParams {
            stretch: 2.0,
            ..Default::default()
        },
        1.6,
    );
    let ir_len = (SR * 0.5) as usize;
    // Energy beyond the original IR length: near-zero unstretched,
    // substantial at 2x.
    let past = LATENCY + ir_len + 2 * LATENCY;
    let tail_normal = energy(&normal[past..]);
    let tail_stretched = energy(&stretched[past..]);
    assert!(
        tail_stretched > tail_normal * 100.0,
        "2x stretch should extend the decay: {tail_stretched:e} vs {tail_normal:e}"
    );
    // Zero-crossing rate down = darker (resampled to half rate).
    let zcr = |buf: &[f64]| {
        buf.windows(2)
            .filter(|w| (w[0] <= 0.0) != (w[1] <= 0.0))
            .count() as f64
    };
    let body = LATENCY..LATENCY + ir_len / 2;
    let z_norm = zcr(&normal[body.clone()]);
    let z_str = zcr(&stretched[body]);
    assert!(
        z_str < z_norm * 0.75,
        "2x stretch should darken: zcr {z_str} vs {z_norm}"
    );
}

#[test]
fn reverse_is_a_riser() {
    let out = render(
        &ImpulseParams {
            direction: ImpulseDirection::Reverse,
            ..Default::default()
        },
        1.0,
    );
    let ir_len = (SR * 0.5) as usize;
    let third = ir_len / 3;
    let first = energy(&out[LATENCY..LATENCY + third]);
    let last = energy(&out[LATENCY + 2 * third..LATENCY + ir_len]);
    assert!(
        last > first * 10.0,
        "reverse should rise toward the end: first={first:e} last={last:e}"
    );
    // And stop after the file plays out.
    let after = energy(&out[LATENCY + ir_len + 2 * LATENCY..]);
    assert!(after < last * 1e-6, "riser should stop: {after:e}");
}

#[test]
fn feedback_recirculates_and_stays_stable() {
    // Recirculation: with predelay + feedback, energy keeps arriving
    // long after the IR is over.
    let base = ImpulseParams {
        feedback: 0.0,
        ..Default::default()
    };
    let fb = ImpulseParams {
        feedback: 0.9,
        ..Default::default()
    };

    let render_fb = |p: &ImpulseParams, secs: f64| {
        let mut conv = Convolution::new(SR);
        let ir: Vec<f64> = test_ir()[..(SR * 0.2) as usize].to_vec();
        conv.load_ir_stereo(&ir, &ir);
        let mut mods = conv.mod_params();
        mods.predelay_ms = 80.0;
        conv.set_mod_params(&mods, true);
        conv.set_impulse(p, true);
        conv.reprepare_now();
        let n = (SR * secs) as usize;
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let x = if i == 0 { 1.0 } else { 0.0 };
            let (l, _r) = conv.tick(x, x);
            assert!(l.is_finite());
            out.push(l);
        }
        out
    };

    let dry = render_fb(&base, 2.0);
    let wet = render_fb(&fb, 2.0);
    let late = (SR * 1.2) as usize;
    let e_dry = energy(&dry[late..]);
    let e_wet = energy(&wet[late..]);
    assert!(
        e_wet > e_dry * 10.0 && e_wet > 1e-12,
        "feedback should keep the tail ringing: {e_wet:e} vs {e_dry:e}"
    );

    // Stability: fb = 1.0 for 10 s stays bounded.
    let extreme = ImpulseParams {
        feedback: 1.0,
        ..Default::default()
    };
    let out = render_fb(&extreme, 10.0);
    let peak = out.iter().fold(0.0f64, |m, s| m.max(s.abs()));
    assert!(peak.is_finite() && peak < 10.0, "fb=1.0 ran away: {peak}");
}

#[test]
fn new_ir_load_resets_params() {
    let ir = test_ir();
    let mut conv = Convolution::new(SR);
    conv.load_ir_stereo(&ir, &ir);
    conv.set_impulse(
        &ImpulseParams {
            decay: 0.3,
            tail: ImpulseTail::Gate,
            attack: 0.7,
            stretch: 2.0,
            direction: ImpulseDirection::Reverse,
            feedback: 0.5,
            ..Default::default()
        },
        true,
    );
    conv.reprepare_now();
    conv.load_ir_stereo(&ir, &ir);
    assert_eq!(
        conv.impulse_params(),
        ImpulseParams::default(),
        "loading a new IR must reset impulse params (MX manual)"
    );
}

#[test]
fn chain_reshape_worker_applies() {
    // The async path: chain marks dirty on update_params, the worker
    // re-bakes, the prepared receiver hot-swaps — no FFT or transform
    // work on the processing thread.
    let mut chain = ReverbChain::new();
    chain.set_algorithm(AlgorithmType::Convolution);
    chain.mix = 1.0;
    chain.update(AudioConfig {
        sample_rate: SR,
        max_buffer_size: 512,
    });
    let ir = test_ir();
    chain.load_convolution_ir(&ir, &ir);

    let (reshaper, rx) = ImpulseReshaper::new();
    chain.set_prepared_ir_receiver(rx);
    chain.set_reshape_sender(reshaper.sender());

    // Hard gate at 20% — unmistakable truncation once applied.
    chain.impulse.decay = 0.2;
    chain.impulse.tail = ImpulseTail::Gate;
    chain.update_params();

    let ir_len = (SR * 0.5) as usize;
    let render_tail = |chain: &mut ReverbChain| {
        chain.reset();
        let n = ir_len + 8 * LATENCY;
        let mut l = vec![0.0; n];
        let mut r = vec![0.0; n];
        l[0] = 1.0;
        r[0] = 1.0;
        chain.process(&mut l, &mut r);
        let cut = LATENCY + ir_len / 2;
        energy(&l[cut..])
    };

    // Poll until the reshaped IR lands (worker latency), max ~2 s.
    let mut applied = false;
    for _ in 0..100 {
        std::thread::sleep(std::time::Duration::from_millis(20));
        // process() drains the receiver.
        let tail = render_tail(&mut chain);
        if tail < 1e-10 {
            applied = true;
            break;
        }
    }
    assert!(applied, "reshaped IR never arrived / never truncated the tail");
}

#[test]
fn reshape_swap_does_not_duck_the_tail() {
    // Knob-driven reshapes hot-swap through an equal-power crossfade
    // where BOTH partition sets convolve the shared input history — the
    // ringing tail must survive the swap instead of collapsing to the
    // zeroed-history restart of a fresh load.
    let mut chain = ReverbChain::new();
    chain.set_algorithm(AlgorithmType::Convolution);
    chain.mix = 1.0;
    chain.update(AudioConfig {
        sample_rate: SR,
        max_buffer_size: 512,
    });
    let ir = test_ir();
    chain.load_convolution_ir(&ir, &ir);

    let (reshaper, rx) = ImpulseReshaper::new();
    chain.set_prepared_ir_receiver(rx);
    chain.set_reshape_sender(reshaper.sender());

    // Ring the tail with a burst, then go silent.
    let mut l = vec![0.0f64; 4096];
    let mut r = vec![0.0f64; 4096];
    for i in 0..256 {
        l[i] = (core::f64::consts::TAU * 500.0 * i as f64 / SR).sin() * 0.8;
        r[i] = l[i];
    }
    chain.process(&mut l, &mut r);

    // Trigger a mild reshape mid-tail and keep processing silence,
    // tracking per-block energy through the swap window.
    chain.impulse.stretch = 1.3;
    chain.update_params();

    let mut energies = Vec::new();
    for _ in 0..40 {
        std::thread::sleep(std::time::Duration::from_millis(10));
        let mut l = vec![0.0f64; 512];
        let mut r = vec![0.0f64; 512];
        chain.process(&mut l, &mut r);
        energies.push(energy(&l));
    }

    // The tail decays; assert no block collapses far below its
    // predecessor (a zeroed-history swap drops several orders of
    // magnitude within one block).
    let mut worst_drop = 1.0f64;
    for w in energies.windows(2) {
        if w[0] > 1e-9 {
            worst_drop = worst_drop.min(w[1] / w[0]);
        }
    }
    assert!(
        worst_drop > 0.05,
        "tail should survive the reshape swap: worst block-to-block drop {worst_drop:e}"
    );
}

#[test]
fn decay_eq_shortens_band_decay() {
    // decay_hi_db = −18: the high band must die off toward the tail
    // while the head keeps its brightness (frequency-dependent decay,
    // not a static EQ).
    let flat = render(&ImpulseParams::default(), 0.6);
    let dark = render(
        &ImpulseParams {
            decay_hi_db: -18.0,
            ..Default::default()
        },
        0.6,
    );
    let hf = |x: &[f64], a: usize, b: usize| -> f64 {
        x[a..b.min(x.len())]
            .windows(2)
            .map(|w| (w[1] - w[0]) * (w[1] - w[0]))
            .sum()
    };
    let n = flat.len().min(dark.len());
    let early_ratio = hf(&dark, 0, n / 6) / hf(&flat, 0, n / 6).max(1e-30);
    let late_ratio =
        hf(&dark, n * 2 / 3, n) / hf(&flat, n * 2 / 3, n).max(1e-30);
    assert!(
        late_ratio < early_ratio * 0.5,
        "high band should decay faster, not just be quieter: \
         early {early_ratio:.3} late {late_ratio:.3}"
    );
}
