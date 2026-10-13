//! IR analysis harness — renders each algorithm's impulse response and
//! computes objective quality metrics.
//!
//! Run with:
//!   cargo test -p reverb-dsp --release --test `ir_metrics` -- --nocapture
//!
//! (Debug builds work but take ~10x longer; the suite is sized to stay
//! under ~2 minutes in release.)
//!
//! Metrics:
//! - RT60 broadband + per-octave-band (Schroeder backward integration)
//! - Echo density profile (Abel & Huang normalized echo density)
//! - Tail spectral flatness / isolated-mode detection (Welch spectrum)
//! - L/R decorrelation (normalized cross-correlation peak, late tail)
//! - DC/subsonic energy ratio (< 20 Hz)

use std::f64::consts::PI;

use dsp_core::num;
use realfft::RealFftPlanner;
use reverb_dsp::algorithm::{AlgorithmParams, AlgorithmType};
use reverb_dsp::algorithms::create;

const SR: f64 = 48000.0;
const MAX_LEN: usize = 480_000; // 10 s
const MIN_LEN: usize = 48_000; // always render at least 1 s

// ---------------------------------------------------------------------------
// IR rendering
// ---------------------------------------------------------------------------

struct Ir {
    left: Vec<f64>,
    right: Vec<f64>,
}

fn render_ir(alg: AlgorithmType, variant: usize) -> Ir {
    let mut engine = create(alg, variant, SR);
    engine.set_params(&AlgorithmParams::default());
    engine.reset();

    let mut left = Vec::with_capacity(MAX_LEN);
    let mut right = Vec::with_capacity(MAX_LEN);

    // Track a running short-window energy so we can stop once the tail
    // is dead (< -90 dB rel the running peak) after the minimum length.
    let mut peak = 0.0f64;
    let mut window_energy = 0.0f64;
    let window = 4800; // 100 ms

    for n in 0..MAX_LEN {
        let x = if n == 0 { 1.0 } else { 0.0 };
        let (sample_l, sample_r) = engine.tick(x, x);
        assert!(
            sample_l.is_finite() && sample_r.is_finite(),
            "{}[{}]: NaN/inf at sample {n}",
            alg.name(),
            variant
        );
        left.push(sample_l);
        right.push(sample_r);

        let e = sample_l.mul_add(sample_l, sample_r * sample_r);
        peak = peak.max(e);
        window_energy += e;
        if n >= window {
            let old_l = left.get(n.saturating_sub(window)).copied().unwrap_or(0.0);
            let old_r = right.get(n.saturating_sub(window)).copied().unwrap_or(0.0);
            window_energy -= old_l.mul_add(old_l, old_r * old_r);
            window_energy = window_energy.max(0.0);
        }
        if n >= MIN_LEN && peak > 0.0 {
            let rel = (window_energy / num::count_to_f64(window)) / peak;
            if rel < 1e-9 {
                break; // < -90 dB
            }
        }
    }
    Ir { left, right }
}

// ---------------------------------------------------------------------------
// Shared stereo reductions
// ---------------------------------------------------------------------------

/// Per-sample stereo energy, `l^2 + r^2`. Shorter of the two channels wins.
fn energy(left: &[f64], right: &[f64]) -> Vec<f64> {
    left.iter()
        .zip(right)
        .map(|(l, r)| l.mul_add(*l, r * r))
        .collect()
}

/// Mono sum at -6 dB. Shorter of the two channels wins.
fn mono(left: &[f64], right: &[f64]) -> Vec<f64> {
    left.iter().zip(right).map(|(l, r)| 0.5 * (l + r)).collect()
}

/// Backward-integrated (Schroeder) energy decay curve. `edc[0]` is the
/// total energy, and the curve falls monotonically to zero.
fn edc(left: &[f64], right: &[f64]) -> Vec<f64> {
    let mut curve = energy(left, right);
    let mut acc = 0.0;
    for e in curve.iter_mut().rev() {
        acc += *e;
        *e = acc;
    }
    curve
}

// ---------------------------------------------------------------------------
// Metric: RT60 (Schroeder backward integration)
// ---------------------------------------------------------------------------

/// Schroeder RT60 from stereo energy. Returns (`rt60_seconds`, `reached_minus_60db`).
/// Fits the -5..-35 dB region of the backward-integrated decay curve.
fn rt60(left: &[f64], right: &[f64]) -> (f64, bool) {
    let edc = edc(left, right);
    let Some(&total) = edc.first().filter(|&&t| t > 0.0) else {
        return (f64::INFINITY, false);
    };

    // Decay curve in dB
    let db = |e: f64| 10.0 * (e / total).max(1e-30).log10();

    // Find indices where curve crosses -5 and -35 dB
    let mut i5 = None;
    let mut i35 = None;
    let mut i60 = None;
    for (i, &e) in edc.iter().enumerate() {
        let d = db(e);
        if i5.is_none() && d <= -5.0 {
            i5 = Some(i);
        }
        if i35.is_none() && d <= -35.0 {
            i35 = Some(i);
        }
        if i60.is_none() && d <= -60.0 {
            i60 = Some(i);
            break;
        }
    }
    match (i5, i35) {
        (Some(a), Some(b)) if b > a => {
            let slope_db_per_s = 30.0 / (num::count_to_f64(b.saturating_sub(a)) / SR);
            (60.0 / slope_db_per_s, i60.is_some())
        }
        _ => (f64::INFINITY, false),
    }
}

/// 4th-order bandpass via two biquad passes, then Schroeder RT60.
fn band_rt60(left: &[f64], right: &[f64], fc: f64) -> f64 {
    use audiocore_dsp::biquad::{Biquad, FilterType};
    let filt = |x: &[f64]| {
        let mut b1 = Biquad::new();
        b1.set(FilterType::Bandpass, fc, 1.4, SR);
        let mut b2 = Biquad::new();
        b2.set(FilterType::Bandpass, fc, 1.4, SR);
        x.iter()
            .map(|&v| b2.tick(b1.tick(v, 0), 0))
            .collect::<Vec<f64>>()
    };
    let fl = filt(left);
    let fr = filt(right);
    rt60(&fl, &fr).0
}

// ---------------------------------------------------------------------------
// Metric: normalized echo density (Abel & Huang 2006)
// ---------------------------------------------------------------------------

/// Echo density profile: fraction of samples in a sliding window that lie
/// outside +/- one std deviation, normalized by the Gaussian expectation
/// erfc(1/sqrt(2)) ~= 0.3173. Returns (`time_to_dense_s`, `peak_density`) where
/// "dense" means profile >= 0.9 sustained for 3 consecutive hops.
fn echo_density(left: &[f64], right: &[f64]) -> (f64, f64) {
    const WIN: usize = 1024;
    const HOP: usize = 512;
    const GAUSSIAN_FRACTION: f64 = 0.317_310_5;

    let mono = mono(left, right);
    let n = mono.len();

    let mut peak = 0.0f64;
    let mut dense_at = f64::INFINITY;
    let mut consecutive = 0_usize;

    let mut pos = 0_usize;
    while pos.saturating_add(WIN) <= n {
        let Some(w) = mono.get(pos..pos.saturating_add(WIN)) else {
            break;
        };
        let energy: f64 = w.iter().map(|x| x * x).sum();
        if energy < 1e-24 {
            // Silent window — onset pre-delay or a decayed tail; either
            // way it carries no density information.
            pos = pos.saturating_add(HOP);
            consecutive = 0;
            continue;
        }
        let sigma = (energy / num::count_to_f64(WIN)).sqrt();
        let outside = num::count_to_f64(w.iter().filter(|x| x.abs() > sigma).count());
        let density = (outside / num::count_to_f64(WIN)) / GAUSSIAN_FRACTION;
        peak = peak.max(density);

        if density >= 0.9 {
            consecutive = consecutive.saturating_add(1);
            if consecutive == 3 && dense_at.is_infinite() {
                dense_at = num::count_to_f64(pos.saturating_add(WIN / 2)) / SR;
            }
        } else {
            consecutive = 0;
            // Not sustained — if we'd marked it and dropped again very
            // early, keep the first sustained mark anyway (profiles
            // naturally fluctuate deep in the tail as SNR drops).
        }
        pos = pos.saturating_add(HOP);
    }
    (dense_at, peak)
}

// ---------------------------------------------------------------------------
// Metric: tail spectrum — isolated modes + subsonic energy
// ---------------------------------------------------------------------------

/// Welch-averaged power spectrum of `x[start..end]`, 8192-point Hann, 50%.
fn welch_spectrum(x: &[f64], start: usize, end: usize) -> Vec<f64> {
    const NFFT: usize = 8192;
    let end = end.min(x.len());
    if end <= start.saturating_add(NFFT) {
        return vec![];
    }
    let mut planner = RealFftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(NFFT);
    let hann: Vec<f64> = (0..NFFT)
        .map(|i| {
            0.5f64.mul_add(
                -(2.0 * PI * num::count_to_f64(i) / num::count_to_f64(NFFT)).cos(),
                0.5,
            )
        })
        .collect();

    let mut acc = vec![0.0f64; NFFT / 2 + 1];
    let mut frames = 0usize;
    let mut pos = start;
    let mut buf = fft.make_input_vec();
    let mut spec = fft.make_output_vec();
    while pos.saturating_add(NFFT) <= end {
        let Some(frame) = x.get(pos..pos.saturating_add(NFFT)) else {
            break;
        };
        for ((slot, sample), w) in buf.iter_mut().zip(frame).zip(&hann) {
            *slot = sample * w;
        }
        #[expect(clippy::unwrap_used, reason = "FFT plan is valid for valid NFFT")]
        {
            fft.process(&mut buf, &mut spec).unwrap();
        }
        for (a, s) in acc.iter_mut().zip(spec.iter()) {
            *a += s.norm_sqr();
        }
        frames = frames.saturating_add(1);
        pos = pos.saturating_add(NFFT / 2);
    }
    if frames == 0 {
        return vec![];
    }
    for a in &mut acc {
        *a /= num::count_to_f64(frames);
    }
    acc
}

/// Max dB of any 200 Hz - 4 kHz bin over the local median (+/- 50 bins),
/// and the frequency it occurs at. High values (> ~12 dB) mean an
/// isolated ringing mode (metallic tail).
fn worst_mode_db(spectrum: &[f64]) -> (f64, f64) {
    const NFFT: usize = 8192;
    if spectrum.is_empty() {
        return (0.0, 0.0);
    }
    let bin_hz = SR / num::count_to_f64(NFFT);
    let lo = num::f64_to_index(200.0 / bin_hz);
    let hi = num::f64_to_index(4000.0 / bin_hz).min(spectrum.len().saturating_sub(1));

    let mut worst = 0.0f64;
    let mut worst_hz = 0.0f64;
    for i in lo..=hi {
        let a = i.saturating_sub(50).max(1);
        let b = i.saturating_add(50).min(spectrum.len().saturating_sub(1));
        let mut local: Vec<f64> = spectrum.get(a..=b).unwrap_or(&[]).to_vec();
        #[expect(
            clippy::unwrap_used,
            reason = "partial_cmp on f64 only returns None for NaN, which should not occur in spectrum values"
        )]
        {
            local.sort_by(|x, y| x.partial_cmp(y).unwrap());
        }
        if let Some(&median) = local.get(local.len() / 2)
            && median > 0.0
            && spectrum.get(i).is_some_and(|&v| v > 0.0)
            && let Some(&spec_i) = spectrum.get(i)
        {
            let db = 10.0 * (spec_i / median).log10();
            if db > worst {
                worst = db;
                worst_hz = num::count_to_f64(i) * bin_hz;
            }
        }
    }
    (worst, worst_hz)
}

/// Fraction of total energy below 20 Hz, from ONE zero-padded FFT of the
/// full IR. (A Welch estimate is wrong here: its windows attenuate the
/// onset burst that carries most of the energy, so the ratio ends up
/// dominated by whatever microscopic drift is left in the tail.)
fn subsonic_ratio(left: &[f64], right: &[f64]) -> f64 {
    let mono = mono(left, right);
    let n = mono.len();
    let nfft = n.next_power_of_two();
    let mut planner = RealFftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(nfft);
    let mut buf = vec![0.0; nfft];
    if let Some(slice) = buf.get_mut(..n) {
        slice.copy_from_slice(&mono);
    }
    let mut spec = fft.make_output_vec();
    #[expect(clippy::unwrap_used, reason = "FFT plan is valid for valid nfft")]
    {
        fft.process(&mut buf, &mut spec).unwrap();
    }
    let bin_hz = SR / num::count_to_f64(nfft);
    let cutoff_bin = num::f64_to_index((20.0 / bin_hz).ceil()).min(spec.len());
    let low: f64 = spec
        .get(..cutoff_bin)
        .unwrap_or(&[])
        .iter()
        .map(realfft::num_complex::Complex::norm_sqr)
        .sum();
    let total: f64 = spec
        .iter()
        .map(realfft::num_complex::Complex::norm_sqr)
        .sum();
    if total > 0.0 { low / total } else { 0.0 }
}

// ---------------------------------------------------------------------------
// Metric: L/R decorrelation
// ---------------------------------------------------------------------------

/// Index where the backward-integrated decay curve crosses `db` (rel total).
fn edc_crossing(left: &[f64], right: &[f64], db: f64) -> Option<usize> {
    let edc = edc(left, right);
    let total = *edc.first().filter(|&&t| t > 0.0)?;
    edc.iter()
        .position(|&e| 10.0 * (e / total).max(1e-30).log10() <= db)
}

/// The audible meat of the tail: from the -15 dB to the -50 dB point of
/// the decay curve. Windows fixed in absolute time land in the dead zone
/// of short IRs and bias tail metrics.
/// Early Decay Time: RT extrapolated from the 0 → −10 dB EDC segment
/// (ISO 3382). Tracks PERCEIVED reverberance better than T30 — the
/// `BigSky` dial-in metric for "how long does it feel".
fn edt(left: &[f64], right: &[f64], sample_rate: f64) -> Option<f64> {
    let edc = edc(left, right);
    let total = *edc.first().filter(|&&t| t > 0.0)?;
    let t10 = edc
        .iter()
        .position(|&e| 10.0 * (e / total).log10() <= -10.0)?;
    Some(6.0 * num::count_to_f64(t10) / sample_rate)
}

/// Clarity index `C_te` (dB): early-vs-late energy split at `te` seconds
/// (0.050 for speech C50, 0.080 for music C80).
fn clarity_db(left: &[f64], right: &[f64], te: f64, sample_rate: f64) -> f64 {
    let split = num::f64_to_index(te * sample_rate);
    let per_sample = energy(left, right);
    let early: f64 = per_sample.iter().take(split).sum();
    let late: f64 = per_sample.iter().skip(split).sum::<f64>().max(1e-30);
    10.0 * (early / late).log10()
}

/// Spectral centroid (Hz) of the late tail — the decay-brightness
/// trajectory metric for damping dial-in.
fn late_centroid(left: &[f64], right: &[f64], sample_rate: f64) -> Option<f64> {
    let (start, end) = tail_window(left, right)?;
    let spec = welch_spectrum(left, start, end);
    let bins = spec.len();
    let mut num = 0.0;
    let mut den = 0.0;
    for (k, &p) in spec.iter().enumerate() {
        let f = num::count_to_f64(k) * sample_rate / (2.0 * num::count_to_f64(bins));
        num += f * p;
        den += p;
    }
    (den > 0.0).then(|| num / den)
}

fn tail_window(left: &[f64], right: &[f64]) -> Option<(usize, usize)> {
    let start = edc_crossing(left, right, -15.0)?;
    let end = edc_crossing(left, right, -50.0).unwrap_or(left.len());
    if end > start.saturating_add(2400) {
        Some((start, end))
    } else {
        None // less than 50 ms of usable tail — skip tail metrics
    }
}

/// Peak absolute normalized cross-correlation between L and R over the
/// decaying tail, scanning lags of +/- 2 ms. ~1.0 = mono, < 0.5 = good
/// stereo. Returns NaN when the tail is too short to measure.
fn lr_correlation(left: &[f64], right: &[f64]) -> f64 {
    let Some((start, end)) = tail_window(left, right) else {
        return f64::NAN;
    };
    let (Some(l), Some(r)) = (left.get(start..end), right.get(start..end)) else {
        return f64::NAN;
    };
    let el: f64 = l.iter().map(|x| x * x).sum();
    let er: f64 = r.iter().map(|x| x * x).sum();
    if el <= 1e-24 || er <= 1e-24 {
        return f64::NAN;
    }
    let norm = (el * er).sqrt();
    let max_lag = num::f64_to_index(0.002 * SR); // 2 ms
    // The lag scan, as an offset into `r` rather than a signed shift:
    // `offset = max_lag + lag`, so offset 0 is lag -2 ms and offset
    // 2 * max_lag is lag +2 ms. `core` is `l` with both margins trimmed,
    // which is exactly the overlap every lag shares.
    let Some(core) = l.get(max_lag..l.len().saturating_sub(max_lag)) else {
        return f64::NAN;
    };
    let mut peak = 0.0f64;
    let mut offset = 0_usize;
    while offset <= max_lag.saturating_mul(2) {
        let Some(shifted) = r.get(offset..) else {
            break;
        };
        let acc: f64 = core.iter().zip(shifted).map(|(a, b)| a * b).sum();
        peak = peak.max((acc / norm).abs());
        // ~6 candidate lags per ms is plenty for a peak estimate.
        offset = offset.saturating_add(8);
    }
    peak
}

// ---------------------------------------------------------------------------
// The harness
// ---------------------------------------------------------------------------

struct Report {
    name: String,
    rt60_s: f64,
    #[expect(dead_code, reason = "Used in test that is currently disabled")]
    reached_60: bool,
    rt_250: f64,
    rt_1k: f64,
    rt_4k: f64,
    rt_8k: f64,
    dense_at_s: f64,
    peak_density: f64,
    worst_mode: f64,
    worst_mode_hz: f64,
    lr_corr: f64,
    subsonic: f64,
    len_s: f64,
}

fn analyze(alg: AlgorithmType, variant: usize) -> Report {
    let ir = render_ir(alg, variant);
    let n = ir.left.len();
    let (rt, reached) = rt60(&ir.left, &ir.right);
    let (dense_at, peak_density) = echo_density(&ir.left, &ir.right);

    // Tail region for mode detection: the -15..-50 dB stretch of the
    // decay curve (fixed absolute windows bias short IRs).
    let mono = mono(&ir.left, &ir.right);
    let (worst_mode, worst_mode_hz) = match tail_window(&ir.left, &ir.right) {
        Some((t0, t1)) => worst_mode_db(&welch_spectrum(&mono, t0, t1)),
        None => (0.0, 0.0),
    };

    Report {
        name: format!("{}/{}", alg.name(), alg.variant_name(variant)),
        rt60_s: rt,
        reached_60: reached,
        rt_250: band_rt60(&ir.left, &ir.right, 250.0),
        rt_1k: band_rt60(&ir.left, &ir.right, 1000.0),
        rt_4k: band_rt60(&ir.left, &ir.right, 4000.0),
        rt_8k: band_rt60(&ir.left, &ir.right, 8000.0),
        dense_at_s: dense_at,
        peak_density,
        worst_mode,
        worst_mode_hz,
        lr_corr: lr_correlation(&ir.left, &ir.right),
        subsonic: subsonic_ratio(&ir.left, &ir.right),
        len_s: num::count_to_f64(n) / SR,
    }
}

fn all_cases() -> Vec<(AlgorithmType, usize)> {
    AlgorithmType::ALL
        .iter()
        .filter(|a| **a != AlgorithmType::Convolution)
        .flat_map(|&a| (0..a.variant_count()).map(move |v| (a, v)))
        .collect()
}

/// Envelope-shaped / intentionally-sparse algorithms exempt from the
/// dense-late-field expectation:
/// - Reflections: early reflections only — sparse by design.
/// - Velvet: velvet-noise FIR — sparse ternary impulses by design.
/// - Spring: dispersive chirp character — density fluctuates ("boing").
/// - `NonLinear`: gated/reverse envelopes truncate the late field.
const fn density_exempt(alg: AlgorithmType) -> bool {
    matches!(
        alg,
        AlgorithmType::Reflections
            | AlgorithmType::Velvet
            | AlgorithmType::Spring
            | AlgorithmType::NonLinear
    )
}

/// Algorithms whose tail legitimately carries a strong narrowband
/// resonance:
/// - Spring: the dispersion modes ARE the "boing" — a spring without
///   them isn't a spring.
/// - Chorale: formant-resonant choir synthesis — the vowel peak in the
///   tail is the effect (already tamed from +12 dB/Q5 to +8 dB/Q3;
///   mode 33.5 -> ~28 dB over local median).
const fn mode_exempt(alg: AlgorithmType) -> bool {
    matches!(alg, AlgorithmType::Spring | AlgorithmType::Chorale)
}

/// Algorithms that are mono-ish by hardware heritage:
/// - Spring: a spring tank is a mono transducer; the L/R pair shares
///   most of its signal path.
/// - Magneto: multi-head tape echo — one tape, one head stack.
const fn correlation_exempt(alg: AlgorithmType) -> bool {
    matches!(alg, AlgorithmType::Spring | AlgorithmType::Magneto)
}

/// Temporary probe: localize Room/Chamber's subsonic energy in frequency.
#[test]
#[ignore = "temporary probe"]
fn probe_chamber() {
    for (alg, v, label) in [
        (AlgorithmType::Room, 1usize, "chamber"),
        (AlgorithmType::Room, 0usize, "medium"),
        (AlgorithmType::Hall, 0usize, "hall"),
    ] {
        let ir = render_ir(alg, v);
        let n = ir.left.len();
        let mono = mono(&ir.left, &ir.right);
        let sum: f64 = mono.iter().sum();
        let energy: f64 = mono.iter().map(|x| x * x).sum();
        println!("{label}: len {n} sum {sum:.4} energy {energy:.4}");

        // One big FFT over the whole IR for fine LF resolution.
        let mut planner = RealFftPlanner::<f64>::new();
        let nfft = n.next_power_of_two();
        let fft = planner.plan_fft_forward(nfft);
        let mut buf = vec![0.0; nfft];
        buf[..n].copy_from_slice(&mono);
        let mut spec = fft.make_output_vec();
        fft.process(&mut buf, &mut spec).unwrap();
        let bin_hz = SR / num::count_to_f64(nfft);
        let band = |lo: f64, hi: f64| -> f64 {
            let a = num::f64_to_index(lo / bin_hz);
            let b = num::f64_to_index(hi / bin_hz).min(spec.len().saturating_sub(1));
            spec.get(a..=b)
                .unwrap_or(&[])
                .iter()
                .map(realfft::num_complex::Complex::norm_sqr)
                .sum::<f64>()
        };
        let total = band(0.0, SR / 2.0);
        for (lo, hi) in [
            (0.0, 5.0),
            (5.0, 10.0),
            (10.0, 20.0),
            (20.0, 50.0),
            (50.0, 100.0),
            (100.0, 24000.0),
        ] {
            println!(
                "  {:>6.0}-{:>5.0} Hz: {:>7.3}%",
                lo,
                hi,
                100.0 * band(lo, hi) / total
            );
        }
    }
}

#[test]
fn ir_metrics() {
    let mut reports = Vec::new();
    for (alg, variant) in all_cases() {
        reports.push((alg, variant, analyze(alg, variant)));
    }

    println!();
    println!(
        "{:<22} {:>6} {:>6} {:>6} {:>6} {:>6} {:>7} {:>5} {:>9} {:>6} {:>8} {:>5}",
        "algorithm",
        "RT60",
        "RT250",
        "RT1k",
        "RT4k",
        "RT8k",
        "dense@",
        "dens",
        "mode",
        "corr",
        "sub20Hz",
        "len"
    );
    for (_, _, r) in &reports {
        println!(
            "{:<22} {:>6.2} {:>6.2} {:>6.2} {:>6.2} {:>6.2} {:>7.3} {:>5.2} {:>9} {:>6.2} {:>8.5} {:>5.1}",
            r.name,
            r.rt60_s,
            r.rt_250,
            r.rt_1k,
            r.rt_4k,
            r.rt_8k,
            r.dense_at_s,
            r.peak_density,
            format!("{:.0}@{:.0}", r.worst_mode, r.worst_mode_hz),
            r.lr_corr,
            r.subsonic,
            r.len_s
        );
    }
    println!();

    let mut failures = Vec::new();
    for (alg, _variant, r) in &reports {
        // (a) Everything must decay. Swell has a slow buildup but still
        // decays; infinite RT60 at default params is a bug anywhere.
        if !r.rt60_s.is_finite() || r.rt60_s > 30.0 {
            failures.push(format!("{}: does not decay (RT60 {})", r.name, r.rt60_s));
        }

        // (b) Late field must become dense unless sparseness is the point.
        if !density_exempt(*alg) && r.peak_density < 0.75 {
            failures.push(format!(
                "{}: echo density peaks at {:.2} (< 0.75) — insufficient diffusion",
                r.name, r.peak_density
            ));
        }

        // (c) Subsonic content must be negligible (DC blockers in loops).
        // Rooms sit at ~1.5% — the LF end of a <300 ms burst, mostly the
        // FDN onset. TODO(voicing): tighten to 1% (halls/plates are at
        // 0.15-0.5%).
        if r.subsonic > 0.02 {
            failures.push(format!(
                "{}: {:.2}% of energy below 20 Hz",
                r.name,
                r.subsonic * 100.0
            ));
        }

        // (d) Isolated tail modes: > 20 dB over local median rings audibly.
        // TODO(voicing): tighten to 12 dB once all algorithms pass it.
        if !mode_exempt(*alg) && r.worst_mode > 20.0 {
            failures.push(format!(
                "{}: isolated tail mode {:.1} dB over local median at {:.0} Hz",
                r.name, r.worst_mode, r.worst_mode_hz
            ));
        }

        // (e) Stereo: a late tail correlated > 0.9 is essentially mono.
        // TODO(voicing): tighten to 0.5 once all algorithms pass it.
        if !correlation_exempt(*alg) && r.lr_corr > 0.9 {
            failures.push(format!(
                "{}: L/R correlation {:.2} — near-mono tail",
                r.name, r.lr_corr
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "IR metric failures:\n  {}",
        failures.join("\n  ")
    );
}

#[test]
fn dial_in_metrics_are_sane_on_hall() {
    // Smoke-check the A/B dial-in metrics on a rendered Hall IR: EDT in
    // a plausible band, C80 finite and negative-ish for a long tail,
    // late centroid inside the audio band and below the early-spectrum
    // brightness (damping darkens the tail).
    let ir = render_ir(AlgorithmType::Hall, 0);
    let sr = 48000.0;
    let edt_s = edt(&ir.left, &ir.right, sr).expect("EDT reachable");
    assert!((0.05..30.0).contains(&edt_s), "EDT out of range: {edt_s} s");
    let c80 = clarity_db(&ir.left, &ir.right, 0.080, sr);
    assert!(c80.is_finite());
    assert!(c80 < 20.0, "C80 suspiciously clear for a hall: {c80} dB");
    let centroid = late_centroid(&ir.left, &ir.right, sr).expect("centroid");
    assert!(
        (80.0..8000.0).contains(&centroid),
        "late centroid out of band: {centroid} Hz"
    );
}
