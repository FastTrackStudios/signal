//! Where the dub is, before asking how it is bent.
//!
//! A warping matcher answers "which moment of the dub is which moment of
//! the reference" only over the range it is allowed to search, and that
//! range has to be narrow: the whole reason a band exists is that a wide
//! search finds better *scores* and worse alignments. Which leaves a
//! singer who came in a second late unalignable — the true path is
//! outside the band, and inside the band there is always some
//! cheap-looking wrong answer.
//!
//! So the offset is found first, as one number, and the warp searches
//! around it. This is not an optimisation; it is what makes the warp's
//! narrow band affordable in the first place.
//!
//! ## Two passes
//!
//! 1. **Envelope.** Correlate the two level envelopes at frame rate.
//!    Cheap, and immune to the thing that defeats waveform correlation:
//!    two takes of the same line are not phase-coherent, so their
//!    waveforms correlate poorly even when perfectly aligned, while their
//!    envelopes correlate strongly.
//! 2. **Waveform.** Correlate the raw samples, but only within a small
//!    radius of the envelope's answer. Where the two takes *are* phase
//!    coherent — two mics on one source, a re-amp, a bounced copy — this
//!    finds the offset to the sample. Where they are not, it finds
//!    nothing useful, which is why its result is only accepted when it
//!    scores well.
//!
//! Both passes are normalized cross-correlation, computed through an FFT
//! and normalized by the energy actually overlapping at each lag. The
//! normalization is the part that matters: raw correlation is largest
//! wherever the most signal overlaps, which is always zero lag, so an
//! unnormalized search reports "no offset" for everything.

use realfft::RealFftPlanner;
use realfft::num_complex::Complex;

/// A measured offset between two takes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Offset {
    /// Seconds to **add to dub time** to land on reference time. A dub
    /// that came in late has a negative offset — it must move earlier.
    pub seconds: f64,
    /// Peak correlation, `0.0..=1.0`, discounted by how much of the two
    /// takes overlapped at that lag. Below about 0.3 the answer is not
    /// evidence of anything.
    pub score: f64,
}

impl Offset {
    pub const NONE: Self = Self {
        seconds: 0.0,
        score: 0.0,
    };
}

/// How the offset search is bounded.
#[derive(Clone, Copy, Debug)]
pub struct OffsetConfig {
    /// Widest offset considered, in seconds.
    ///
    /// Generous by default: this is the stage whose whole job is to cope
    /// with a take that is badly placed, and unlike the warp band a wide
    /// search here is cheap and safe — a single global shift cannot
    /// mangle a performance, it can only be wrong.
    pub max_shift_secs: f64,
    /// Radius around the envelope's answer that the waveform pass
    /// searches.
    pub refine_radius_secs: f64,
    /// Correlation the waveform pass must reach before its answer is
    /// preferred to the envelope's.
    ///
    /// High, because a low-scoring waveform peak on two non-coherent
    /// takes is noise, and taking it would throw away a good envelope
    /// answer.
    pub refine_min_score: f64,
    /// Fraction of the shorter signal that must overlap for a lag to be
    /// considered. Without it, the extreme lags — where a handful of
    /// samples overlap — win on freak correlations.
    pub min_overlap: f64,
}

impl Default for OffsetConfig {
    fn default() -> Self {
        Self {
            max_shift_secs: 3.0,
            refine_radius_secs: 0.030,
            refine_min_score: 0.5,
            min_overlap: 0.25,
        }
    }
}

/// Find the global offset between two takes.
///
/// `reference_env` and `dub_env` are level envelopes at `frame_rate`;
/// `reference` and `dub` are the raw mono buffers at `sample_rate`. Pass
/// empty sample slices to skip the waveform pass — the envelope answer is
/// perfectly usable on its own, and a caller that has not loaded the
/// audio should not be forced to.
pub fn macro_offset(
    reference_env: &[f64],
    dub_env: &[f64],
    frame_rate: f64,
    reference: &[f64],
    dub: &[f64],
    sample_rate: f64,
    cfg: OffsetConfig,
) -> Offset {
    let coarse = correlate(
        reference_env,
        dub_env,
        frame_rate,
        -cfg.max_shift_secs,
        cfg.max_shift_secs,
        cfg.min_overlap,
    );
    if reference.is_empty() || dub.is_empty() || sample_rate <= 0.0 {
        return coarse;
    }

    // Search around the coarse answer, and around zero when there is no
    // coarse answer to search around.
    let centre = if coarse.score > 0.0 {
        coarse.seconds
    } else {
        0.0
    };
    let fine = correlate(
        reference,
        dub,
        sample_rate,
        centre - cfg.refine_radius_secs,
        centre + cfg.refine_radius_secs,
        cfg.min_overlap,
    );
    if fine.score >= cfg.refine_min_score {
        fine
    } else {
        coarse
    }
}

/// Normalized cross-correlation of `dub` against `reference`, searched
/// between two lags in seconds.
///
/// Returns the lag whose correlation is highest, refined to a fraction of
/// a sample by fitting a parabola to the peak and its neighbours — the
/// standard trick, and the difference between an offset good to one frame
/// and one good to a fraction of one.
pub fn correlate(
    reference: &[f64],
    dub: &[f64],
    rate: f64,
    min_shift_secs: f64,
    max_shift_secs: f64,
    min_overlap: f64,
) -> Offset {
    let (n, m) = (reference.len(), dub.len());
    if n < 4 || m < 4 || rate <= 0.0 {
        return Offset::NONE;
    }

    // Both signals are mean-removed. For a waveform this changes almost
    // nothing; for an envelope it is essential, because an envelope is
    // non-negative and its correlation would otherwise be dominated by
    // the constant part, which is the same at every lag.
    let a = centred(reference);
    let b = centred(dub);

    let raw = cross_correlate(&a, &b);
    let energy_a = prefix_energy(&a);
    let energy_b = prefix_energy(&b);

    let lag_lo = (min_shift_secs * rate).floor() as isize;
    let lag_hi = (max_shift_secs * rate).ceil() as isize;
    let lag_lo = lag_lo.max(-(m as isize - 1));
    let lag_hi = lag_hi.min(n as isize - 1);
    if lag_lo > lag_hi {
        return Offset::NONE;
    }
    let floor = (min_overlap.clamp(0.0, 1.0) * n.min(m) as f64) as usize;

    let mut best_lag = 0isize;
    let mut best = f64::NEG_INFINITY;
    let mut scores: Vec<f64> = vec![f64::NEG_INFINITY; (lag_hi - lag_lo + 1) as usize];
    for lag in lag_lo..=lag_hi {
        // Overlap in dub indices: `k` and `k + lag` must both be in range.
        let lo = 0.max(-lag) as usize;
        let hi = (m as isize).min(n as isize - lag).max(0) as usize;
        if hi <= lo || hi - lo < floor.max(4) {
            continue;
        }
        let ea = energy(
            &energy_a,
            (lo as isize + lag) as usize,
            (hi as isize + lag) as usize,
        );
        let eb = energy(&energy_b, lo, hi);
        let norm = (ea * eb).sqrt();
        if norm <= 1e-12 {
            continue;
        }
        // Discounted by how much of the shorter take actually overlaps.
        //
        // Without this the search reliably prefers a lag where only a
        // third of the material lines up, because a normalized
        // correlation over two well-matched seconds beats one over a
        // whole take that agrees slightly less — and any repetitive
        // performance, which is to say any performance, offers such a
        // lag. The discount is free where it should be: when one take is
        // simply late, the shorter of the two still overlaps completely
        // at the true lag, so the correct answer is never penalised.
        let overlap = (hi - lo) as f64 / n.min(m) as f64;
        let score = raw_at(&raw, lag) / norm * overlap;
        scores[(lag - lag_lo) as usize] = score;
        if score > best {
            best = score;
            best_lag = lag;
        }
    }
    if !best.is_finite() {
        return Offset::NONE;
    }

    let i = (best_lag - lag_lo) as usize;
    let at = |k: usize| -> f64 {
        scores
            .get(k)
            .copied()
            .filter(|v| v.is_finite())
            .unwrap_or(best)
    };
    let fraction = if i > 0 && i + 1 < scores.len() {
        parabolic_peak(at(i - 1), best, at(i + 1))
    } else {
        0.0
    };

    Offset {
        seconds: (best_lag as f64 + fraction) / rate,
        score: best.clamp(0.0, 1.0),
    }
}

fn centred(x: &[f64]) -> Vec<f64> {
    let mean = x.iter().sum::<f64>() / x.len() as f64;
    x.iter().map(|v| v - mean).collect()
}

/// `out[lag] = sum_k a[k + lag] * b[k]`, with negative lags stored from
/// the end of the buffer in the usual circular layout.
fn cross_correlate(a: &[f64], b: &[f64]) -> Vec<f64> {
    // Linear, not circular: padding to at least `n + m` keeps the wrap of
    // one lag from landing on top of another.
    let len = (a.len() + b.len()).next_power_of_two();
    let mut planner = RealFftPlanner::<f64>::new();
    let forward = planner.plan_fft_forward(len);
    let inverse = planner.plan_fft_inverse(len);

    let mut buf_a = vec![0.0; len];
    let mut buf_b = vec![0.0; len];
    buf_a[..a.len()].copy_from_slice(a);
    buf_b[..b.len()].copy_from_slice(b);

    let mut spec_a: Vec<Complex<f64>> = forward.make_output_vec();
    let mut spec_b: Vec<Complex<f64>> = forward.make_output_vec();
    // `process` is infallible for correctly sized buffers, which these
    // are by construction.
    let _ = forward.process(&mut buf_a, &mut spec_a);
    let _ = forward.process(&mut buf_b, &mut spec_b);

    for (x, y) in spec_a.iter_mut().zip(spec_b.iter()) {
        *x *= y.conj();
    }

    let mut out = vec![0.0; len];
    let _ = inverse.process(&mut spec_a, &mut out);
    let scale = 1.0 / len as f64;
    for v in &mut out {
        *v *= scale;
    }
    out
}

fn raw_at(correlation: &[f64], lag: isize) -> f64 {
    let len = correlation.len() as isize;
    let index = if lag >= 0 { lag } else { len + lag };
    if index < 0 || index >= len {
        0.0
    } else {
        correlation[index as usize]
    }
}

/// Prefix sums of squares, so the energy of any span is one subtraction.
fn prefix_energy(x: &[f64]) -> Vec<f64> {
    let mut out = Vec::with_capacity(x.len() + 1);
    out.push(0.0);
    let mut acc = 0.0;
    for &v in x {
        acc += v * v;
        out.push(acc);
    }
    out
}

fn energy(prefix: &[f64], from: usize, to: usize) -> f64 {
    let to = to.min(prefix.len() - 1);
    let from = from.min(to);
    prefix[to] - prefix[from]
}

/// Sub-sample peak position from three samples around a maximum.
fn parabolic_peak(left: f64, centre: f64, right: f64) -> f64 {
    let denominator = left - 2.0 * centre + right;
    if denominator.abs() < 1e-12 {
        return 0.0;
    }
    (0.5 * (left - right) / denominator).clamp(-0.5, 0.5)
}
