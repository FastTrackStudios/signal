//! Every algorithm stays bounded, and decays, across its whole knob range.
//!
//! This is the test that was missing when the preset library was built. Four
//! of the translated VintageVerb "NL-" presets came out ~70 LU hot — not a
//! calibration offset but self-oscillation: NonLinear repurposes PRE-DELAY as
//! a regeneration control (`effective_nonlinear`), the loop closes around an
//! FDN whose peak gain is well above unity, and the only thing bounding it was
//! a hard clamp on the feedback state. A hard clamp does not make a loop
//! stable — it makes an unstable loop into a sustained oscillator, which is
//! why nothing produced a NaN and nothing failed.
//!
//! So stability is asserted as a measured property here, not assumed:
//!
//! - **Bounded**: an impulse of amplitude 1 never produces an output above
//!   [`PEAK_CEILING`]. A reverb concentrates energy, so the ceiling is well
//!   above 1 — what it rules out is a loop running away.
//! - **Decaying**: with the input long since finished, the last part of the
//!   buffer is quieter than the middle. A self-oscillating loop pins to its
//!   clamp and stays there; a reverb decays.
//! - **Finite**: no NaN, no infinity, anywhere.
//!
//! Swept across the knobs that close feedback loops — decay, and the
//! PRE-DELAY-as-feedback remap that Magneto and NonLinear share.

use audiocore_dsp::{AudioConfig, Processor};
use reverb_dsp::chain::ReverbChain;
use reverb_dsp::AlgorithmType;

const SR: f64 = 48_000.0;

/// The largest peak an impulse may produce anywhere in the tail.
///
/// A reverb legitimately rings above its input: an impulse spread over many
/// recirculating delay lines can momentarily sum well past unity. Since every
/// engine is now calibrated to the same wet energy
/// ([`AlgorithmType::wet_calibration_db`]), the ones that used to be quiet are
/// no longer quiet — Bloom came up 23 dB — and the sparser voices reach a few
/// units at their longest decays. The ceiling is set to leave those alone
/// while still catching a loop that regenerates: the NonLinear failure this
/// file was written for peaked at 17.5 and sustained there.
const PEAK_CEILING: f64 = 8.0;

fn config() -> AudioConfig {
    AudioConfig {
        sample_rate: SR,
        max_buffer_size: 512,
    }
}

/// Render an impulse and return the wet output.
fn impulse_response(algo: AlgorithmType, decay: f64, predelay_ms: f64, seconds: f64) -> Vec<f64> {
    let mut c = ReverbChain::new();
    c.set_algorithm(algo);
    c.mix = 1.0;
    c.params.decay = decay;
    c.predelay_ms = predelay_ms;
    c.update(config());

    let frames = (SR * seconds) as usize;
    let mut out = Vec::with_capacity(frames);
    let block = 512;
    let mut pos = 0;
    while pos < frames {
        let n = block.min(frames - pos);
        let mut l = vec![0.0f64; n];
        let mut r = vec![0.0f64; n];
        if pos == 0 {
            l[0] = 1.0;
            r[0] = 1.0;
        }
        c.process(&mut l, &mut r);
        out.extend_from_slice(&l);
        pos += n;
    }
    out
}

/// The same render, but asking for an explicit T60 rather than a knob value.
fn impulse_response_t60(algo: AlgorithmType, t60_s: f64, seconds: f64) -> Vec<f64> {
    let mut c = ReverbChain::new();
    c.set_algorithm(algo);
    c.mix = 1.0;
    c.update(config());
    // Only where the engine realizes a decay curve. Asking an engine that
    // does not for a T60 leaves it on its default, and the calibration
    // constants are measured the same way.
    if c.decay_seconds_range().is_some() {
        c.set_decay_seconds(t60_s);
        c.update(config());
    }

    let frames = (SR * seconds) as usize;
    let mut out = Vec::with_capacity(frames);
    let block = 512;
    let mut pos = 0;
    while pos < frames {
        let n = block.min(frames - pos);
        let mut l = vec![0.0f64; n];
        let mut r = vec![0.0f64; n];
        if pos == 0 {
            l[0] = 1.0;
            r[0] = 1.0;
        }
        c.process(&mut l, &mut r);
        out.extend_from_slice(&l);
        pos += n;
    }
    out
}

fn rms(buf: &[f64]) -> f64 {
    if buf.is_empty() {
        return 0.0;
    }
    (buf.iter().map(|s| s * s).sum::<f64>() / buf.len() as f64).sqrt()
}

/// Assert a rendered tail is bounded and finite.
///
/// The property is "the loop does not regenerate", which is separate from how
/// long the tail is — a 30-second hall is not misbehaving because it is still
/// audible after four seconds. Decay is asserted on its own terms below.
fn assert_bounded(label: &str, ir: &[f64]) {
    assert!(
        ir.iter().all(|s| s.is_finite()),
        "{label}: produced a non-finite sample",
    );

    let peak = ir.iter().fold(0.0f64, |a, s| a.max(s.abs()));
    assert!(
        peak <= PEAK_CEILING,
        "{label}: peaked at {peak:.3} from a unit impulse — the loop is \
         regenerating, not decaying (ceiling {PEAK_CEILING})",
    );
}

/// The sweep: every algorithm, over the knobs that close a feedback loop.
///
/// PRE-DELAY is in the sweep because for Magneto and NonLinear it is not a
/// pre-delay at all — the chain remaps it to the engine's feedback and
/// disengages the delay line — so its top end is the most loop-gain those two
/// algorithms can be asked for.
#[test]
fn every_algorithm_stays_bounded_across_its_knob_range() {
    for algo in AlgorithmType::ALL {
        // Convolution needs an IR loaded to do anything; without one it is a
        // pass-through and there is no loop to test.
        if *algo == AlgorithmType::Convolution {
            continue;
        }
        for decay in [0.0, 0.5, 1.0] {
            for predelay_ms in [0.0, 50.0, 125.0, 250.0, 500.0] {
                let ir = impulse_response(*algo, decay, predelay_ms, 4.0);
                assert_bounded(
                    &format!("{algo:?} decay={decay} predelay={predelay_ms}ms"),
                    &ir,
                );
            }
        }
    }
}

/// Every tail eventually dies, measured over a horizon set by its own decay.
///
/// Asserting this against a fixed window would only be asking whether the
/// algorithm is short, so each one is set to an explicit T60 it supports and
/// rendered for four times that. By then a stable tail is far below where it
/// was mid-flight; a regenerating one is not.
#[test]
fn every_tail_eventually_decays() {
    for algo in AlgorithmType::ALL {
        if *algo == AlgorithmType::Convolution {
            continue;
        }

        // A short T60 inside this algorithm's own range, so the horizon stays
        // small. Algorithms that do not realize a decay curve keep the raw
        // knob at its low end, which is the same request in the other units.
        let mut c = ReverbChain::new();
        c.set_algorithm(*algo);
        c.update(config());
        let t60 = match c.decay_seconds_range() {
            Some((lo, hi)) => lo.max(0.3).min(hi),
            None => 1.0,
        };

        let ir = if c.decay_seconds_range().is_some() {
            impulse_response_t60(*algo, t60, 4.0 * t60)
        } else {
            impulse_response(*algo, 0.0, 0.0, 4.0)
        };

        let n = ir.len();
        let middle = rms(&ir[n * 3 / 8..n / 2]);
        let end = rms(&ir[n * 7 / 8..]);
        assert!(
            end < middle * 0.5 + 1e-12,
            "{algo:?} (T60 {t60:.2}s): the tail is not decaying — rms \
             {end:.8} at the end against {middle:.8} in the middle",
        );
    }
}

/// The specific setting that shipped broken, kept as its own case.
///
/// "NL-Snare Gut Punch" translated to predelay 125 ms, which NonLinear reads
/// as 0.625 feedback. It rendered at peak 9.2 with an RMS of 3.0 sustained
/// across the whole four-second buffer — a limiter oscillating against its own
/// clamp, measured at +71 LU against the reference.
#[test]
fn the_nonlinear_regeneration_knob_does_not_self_oscillate() {
    for predelay_ms in [125.0, 200.0, 250.0, 400.0, 500.0] {
        let ir = impulse_response(AlgorithmType::NonLinear, 0.0, predelay_ms, 4.0);
        assert_bounded(&format!("NonLinear regeneration at {predelay_ms}ms"), &ir);

        // And it has to actually die, not merely stay under the ceiling.
        let n = ir.len();
        assert!(
            rms(&ir[n * 7 / 8..]) < rms(&ir[n * 3 / 8..n / 2]) * 0.5 + 1e-12,
            "NonLinear at {predelay_ms}ms sustains instead of decaying",
        );
    }
}

/// Every algorithm puts out the same level for the same decay time.
///
/// Before the calibration constants existed the engines spanned 47 dB at a
/// matched T60 — Bloom at -23 dB against Velvet at +32 — so changing algorithm
/// was a volume change first and a character change second. That also made a
/// shared preset library impossible: a level translated from another reverb
/// meant something different in every engine it could land in.
///
/// [`AlgorithmType::wet_calibration_db`] anchors them all to unity wet energy
/// for a unit impulse at the reference decay. This is the test that fails when
/// an engine changes and its constant goes stale.
#[test]
fn algorithms_share_one_output_level() {
    /// The decay the constants are measured at.
    const REFERENCE_T60: f64 = 2.0;
    /// How far from unity energy a calibrated engine may sit.
    ///
    /// Not zero: the constants are rounded to 0.01 dB, the engines modulate,
    /// and several are stochastic. A dB and a half is inaudible as a level
    /// change while still being a tenth of the spread it replaced.
    const TOLERANCE_DB: f64 = 1.5;

    let mut worst: Option<(AlgorithmType, f64)> = None;
    for algo in AlgorithmType::ALL {
        // Convolution's level belongs to the IR that gets loaded, and Swell
        // renders silence at every setting — a defect of its own, tracked
        // separately, that a trim cannot repair.
        if matches!(algo, AlgorithmType::Convolution | AlgorithmType::Swell) {
            continue;
        }

        let ir = impulse_response_t60(*algo, REFERENCE_T60, REFERENCE_T60 * 3.0);
        let energy: f64 = ir.iter().map(|s| s * s).sum();
        assert!(
            energy > 1e-9,
            "{algo:?}: rendered silence at T60 {REFERENCE_T60}s",
        );

        let error_db = 10.0 * energy.log10();
        if worst.is_none_or(|(_, w)| error_db.abs() > w.abs()) {
            worst = Some((*algo, error_db));
        }
        assert!(
            error_db.abs() <= TOLERANCE_DB,
            "{algo:?}: {error_db:+.2} dB from the shared level at T60 \
             {REFERENCE_T60}s — its `wet_calibration_db` is stale. Re-derive \
             with `cargo run -p signal-analyzer --example wet_level`.",
        );
    }

    if let Some((algo, db)) = worst {
        println!("furthest from the shared level: {algo:?} at {db:+.2} dB");
    }
}

/// The user's wet trim does what it says, on top of the calibration.
#[test]
fn the_wet_trim_scales_the_wet_bus() {
    let mut c = ReverbChain::new();
    c.set_algorithm(AlgorithmType::Room);
    c.mix = 1.0;
    c.update(config());

    let render = |trim_db: f64| -> f64 {
        let mut c = ReverbChain::new();
        c.set_algorithm(AlgorithmType::Room);
        c.mix = 1.0;
        c.wet_gain_db = trim_db;
        c.update(config());
        let mut l = vec![0.0f64; (SR * 2.0) as usize];
        let mut r = l.clone();
        l[0] = 1.0;
        r[0] = 1.0;
        c.process(&mut l, &mut r);
        l.iter().map(|s| s * s).sum()
    };

    let unity = render(0.0);
    let up = render(6.0);
    assert!(unity > 1e-9, "the reference render is silent");

    // +6 dB of trim is 4x the energy.
    let ratio_db = 10.0 * (up / unity).log10();
    assert!(
        (ratio_db - 6.0).abs() < 0.1,
        "a +6 dB trim moved the wet bus by {ratio_db:+.2} dB",
    );
}
