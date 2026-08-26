//! Identify what a reference render is playing, sample-exactly.
//!
//! A Kontakt bounce of a library we hold is a mix of the very samples in the
//! pack, and the round-robin set is finite — so "what is CSS doing here" is an
//! identification problem, not an estimation one. Solve it and the answers are
//! ground truth: the wall-clock onset of every note, how far into its sample
//! Kontakt started (the `$1fvjk` skip), which round-robin and dynamic layer it
//! chose, and the gain it applied to each.
//!
//! # Why single-sample correlation fails
//!
//! The obvious estimator — correlate one candidate at a time, take the best —
//! does not work here, and the way it fails is instructive. At any moment the
//! reference is a MIXTURE: two crossfaded dynamic layers, a transition voice
//! over a body, the vibrato pair. A single sample can only explain its own
//! share of that energy, so its correlation tops out well below 1 (measured:
//! 0.64 even at CC1=127, where one layer dominates) — low enough that a
//! coincidentally-similar sample at the wrong offset outscores the true one.
//! Sustained strings make it worse: the signal is near-stationary, so a
//! mid-note window matches almost anywhere.
//!
//! # The estimator that works
//!
//! Fit the whole group at once. Members of a group (the dynamic layers of one
//! zone family) sound TOGETHER from a shared onset, so for a given offset they
//! form a linear model of the reference window: `ref ≈ Σ gᵢ·cᵢ`. Solving the
//! normal equations for every gain simultaneously explains the mixture instead
//! of competing with it, and the score becomes the fraction of the window's
//! energy the group accounts for — which a wrong group cannot fake, because
//! its residual stays large no matter how the gains are chosen.
//!
//! Scanning is two-stage: an FFT cross-correlation sweeps the whole search
//! range (seconds of audio) in `O(N log N)` to propose offsets, then each
//! proposal is fitted at full rate. The coarse stage must not decimate — a
//! decimated peak on a sustained note is broad and frequently in the wrong
//! place, which is exactly how the first attempt at this went wrong.
//!
//! # Overlapping voices
//!
//! A legato transition is not one sample: the outgoing body, the transition
//! recording and the incoming body sound together, each from its OWN onset. A
//! single shared-offset fit cannot express that — given two onsets it locks
//! onto one and leaves the other in the residual. [`decompose`] pulls that
//! thread: fit the best group, subtract exactly what it contributed, fit
//! again. Each pass names another voice and the moment it entered.
//!
//! # What it took to be trustworthy
//!
//! Four things, each found by a ground-truth test on REAL samples (see the
//! `--self-test` mode of `fts signal pack match-ref`) rather than reasoned
//! about — the noise-based unit tests below pass happily without any of them,
//! because noise has neither periodic structure nor an envelope:
//!
//! - **Gains per block.** A sampler's output is the sample times a MOVING
//!   gain, so one constant gain cannot span a window that rises 20 dB.
//! - **A ridge term**, because the dynamic layers of one note are
//!   near-collinear and plain least squares answered with large cancelling
//!   gains.
//! - **Whitening with a CENTRED MOVING rms** in the coarse sweep. Per-block
//!   whitening does not commute with shifting, which distorts every lag
//!   differently and destroys the peak: an exact mixture of real samples fit
//!   at 33%, 216 ms off.
//! - **Proposals on a uniform grid**, not only the coarse ranking. The window
//!   is a mixture of members that resemble each other, so the correlation peak
//!   is not dependable — the true offset, which the fit explains at 100%, was
//!   not in the coarse top 48 at all while the search settled for 60%
//!   elsewhere. The fit cannot be fooled; the ranking can, so the ranking only
//!   proposes.
//!
//! Measured on an exact mixture of three real dynamic layers: offset recovered
//! sample-exactly, gains 0.800/0.397/0.243 against a truth of 0.8/0.4/0.267,
//! and both onsets of an overlapping pair found to the sample.

use realfft::num_complex::Complex32;
use realfft::RealFftPlanner;

/// A fitted explanation of one reference window by one candidate group.
#[derive(Debug, Clone)]
pub struct GroupFit {
    /// Frame inside the candidates that aligns with the window's start.
    pub offset: usize,
    /// Fraction of the window's energy the group explains, 0..=1.
    pub explained: f32,
    /// Least-squares gain per member, in the order given.
    pub gains: Vec<f32>,
    /// The fitted signal itself, window-length — what this group contributes.
    /// Subtracting it is how overlapping voices are peeled apart.
    pub model: Vec<f32>,
}

/// One voice found in a window: which group, and how it was fitted.
#[derive(Debug, Clone)]
pub struct VoiceFit {
    /// Index into the `groups` passed to [`decompose`].
    pub group: usize,
    pub fit: GroupFit,
    /// Fraction of the ORIGINAL window energy this voice accounts for.
    pub share: f32,
}

/// Sliding dot products of `window` against `hay`, via FFT.
///
/// `out[k] = Σᵢ hay[k+i]·window[i]`, for every k the haystack allows.
fn xcorr(hay: &[f32], window: &[f32], planner: &mut RealFftPlanner<f32>) -> Vec<f32> {
    let n = (hay.len() + window.len()).next_power_of_two();
    let fwd = planner.plan_fft_forward(n);
    let inv = planner.plan_fft_inverse(n);

    let w = window.len();
    let mut a = vec![0.0f32; n];
    a[..hay.len()].copy_from_slice(hay);
    let mut b = vec![0.0f32; n];
    // Correlation is convolution with the time-reversed window, laid at the
    // START of the buffer — which puts lag k at index k + w − 1.
    for (i, &v) in window.iter().enumerate() {
        b[w - 1 - i] = v;
    }

    let mut fa = fwd.make_output_vec();
    let mut fb = fwd.make_output_vec();
    fwd.process(&mut a, &mut fa).expect("fft len");
    fwd.process(&mut b, &mut fb).expect("fft len");
    let prod: Vec<Complex32> = fa.iter().zip(&fb).map(|(x, y)| x * y).collect();

    let mut out = vec![0.0f32; n];
    let mut prod = prod;
    inv.process(&mut prod, &mut out).expect("ifft len");
    let scale = 1.0 / n as f32;
    let valid = hay.len().saturating_sub(w);
    (0..=valid).map(|k| out[k + w - 1] * scale).collect()
}

/// Flatten `x`'s level to unity in [`BLOCK`]-sized steps.
///
/// The coarse sweep correlates raw waveforms, which silently assumes a
/// constant gain — the very assumption a sampler breaks. A note's attack is
/// tens of dB below its body, so an unwhitened sweep scores the loud middle of
/// a sample far above its quiet head and never proposes the true onset: the
/// fit ranked the correct offset at 74.9% explained while the search was
/// offering it offsets that fit 57.5%. Whitening makes a quiet region as
/// proposable as a loud one; the fit, which models the envelope properly,
/// still decides.
/// The normaliser is a CENTRED MOVING rms, not a per-block one, because
/// whitening has to commute with shifting. Fixed blocks do not: shift the
/// signal half a block and different samples share a normaliser, so every lag
/// is distorted differently and the correlation peak is destroyed. That bug
/// made an exact mixture of real samples — no envelope, no engine, a window
/// that IS the model — fit at 33% and 216 ms off.
fn whiten(x: &[f32]) -> Vec<f32> {
    let n = x.len();
    if n == 0 {
        return Vec::new();
    }
    let e = energy_prefix(x);
    let global = (e[n] / n as f64).sqrt();
    let floor = (global * 1e-3).max(1e-12);
    let half = BLOCK / 2;
    (0..n)
        .map(|i| {
            let lo = i.saturating_sub(half);
            let hi = (i + half).min(n);
            let rms = ((e[hi] - e[lo]) / (hi - lo).max(1) as f64).max(0.0).sqrt();
            (x[i] as f64 / rms.max(floor)) as f32
        })
        .collect()
}

/// Running `Σ x²` prefix sums, for window energies in O(1).
fn energy_prefix(x: &[f32]) -> Vec<f64> {
    let mut acc = Vec::with_capacity(x.len() + 1);
    acc.push(0.0);
    let mut s = 0.0f64;
    for &v in x {
        s += (v as f64) * (v as f64);
        acc.push(s);
    }
    acc
}

/// Solve `G g = b` for a small symmetric positive-definite `G` (Gaussian
/// elimination with partial pivoting). `None` when the system is singular —
/// which happens when two members are the same audio.
fn solve(mut g: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for col in 0..n {
        let (piv, mag) = (col..n).fold((col, 0.0), |acc, r| {
            let m = g[r][col].abs();
            if m > acc.1 {
                (r, m)
            } else {
                acc
            }
        });
        if mag < 1e-12 {
            return None;
        }
        g.swap(col, piv);
        b.swap(col, piv);
        for r in (col + 1)..n {
            let f = g[r][col] / g[col][col];
            if f == 0.0 {
                continue;
            }
            for c in col..n {
                g[r][c] -= f * g[col][c];
            }
            b[r] -= f * b[col];
        }
    }
    let mut x = vec![0.0; n];
    for r in (0..n).rev() {
        let mut s = b[r];
        for c in (r + 1)..n {
            s -= g[r][c] * x[c];
        }
        x[r] = s / g[r][r];
    }
    Some(x)
}

/// Sub-block over which a player's gain is treated as constant, in frames at
/// 48 kHz (10 ms). A sampler's output is the sample times a MOVING gain — amp
/// envelope, CC1 crossfade, swell — so a single gain per member cannot
/// represent a 250 ms window that rises 20 dB, and forcing one makes the fit
/// meaningless. Solving per block models the envelope instead of fighting it,
/// and the recovered gain curve is itself the envelope Kontakt applied.
const BLOCK: usize = 480;

/// Ridge term, relative to the mean diagonal of the Gram matrix.
///
/// The dynamic layers of one note are near-collinear — same pitch, same room,
/// different bow force — so the normal equations are ill-conditioned and plain
/// least squares answers with large cancelling gains that fit noise. A small
/// ridge keeps the solution honest at negligible cost to a genuine fit.
const RIDGE: f64 = 1e-3;

/// Least-squares fit of `members` (all aligned at `offset`) to `window`, with
/// a gain per member per [`BLOCK`].
///
/// Returns the mean gain of each member (energy-weighted across blocks) and
/// the fraction of the window's energy explained.
fn fit_at(window: &[f32], members: &[&[f32]], offset: usize) -> Option<(Vec<f32>, f32, Vec<f32>)> {
    let w = window.len();
    if members.iter().any(|m| offset + w > m.len()) {
        return None;
    }
    let k = members.len();
    let mut resid_total = 0.0f64;
    let mut energy_total = 0.0f64;
    let mut gain_acc = vec![0.0f64; k];
    let mut weight_acc = 0.0f64;
    let mut model = vec![0.0f32; w];

    for b0 in (0..w).step_by(BLOCK) {
        let b1 = (b0 + BLOCK).min(w);
        let n = b1 - b0;
        if n < 16 {
            break;
        }
        let mut gram = vec![vec![0.0f64; k]; k];
        let mut rhs = vec![0.0f64; k];
        for i in 0..k {
            let ci = &members[i][offset + b0..offset + b1];
            for j in i..k {
                let cj = &members[j][offset + b0..offset + b1];
                let dot: f64 = ci
                    .iter()
                    .zip(cj)
                    .map(|(a, b)| (*a as f64) * (*b as f64))
                    .sum();
                gram[i][j] = dot;
                gram[j][i] = dot;
            }
            rhs[i] = ci
                .iter()
                .zip(&window[b0..b1])
                .map(|(a, b)| (*a as f64) * (*b as f64))
                .sum();
        }
        let ridge = RIDGE * (0..k).map(|i| gram[i][i]).sum::<f64>() / k as f64;
        for (i, row) in gram.iter_mut().enumerate() {
            row[i] += ridge;
        }
        let energy: f64 = window[b0..b1]
            .iter()
            .map(|v| (*v as f64) * (*v as f64))
            .sum();
        energy_total += energy;
        let Some(gains) = solve(gram.clone(), rhs.clone()) else {
            resid_total += energy;
            continue;
        };
        for i in 0..k {
            let ci = &members[i][offset + b0..offset + b1];
            let g = gains[i] as f32;
            for (m, c) in model[b0..b1].iter_mut().zip(ci) {
                *m += g * c;
            }
        }
        let mut resid = energy;
        for i in 0..k {
            resid -= 2.0 * gains[i] * rhs[i];
            for j in 0..k {
                resid += gains[i] * gains[j] * gram[i][j];
            }
        }
        resid_total += resid.max(0.0);
        // Weight the reported gain by block energy: the loud part of a note
        // says more about the fader than its tail does.
        for i in 0..k {
            gain_acc[i] += gains[i] * energy;
        }
        weight_acc += energy;
    }

    if energy_total <= 0.0 || weight_acc <= 0.0 {
        return None;
    }
    let explained = (1.0 - (resid_total / energy_total)).clamp(0.0, 1.0) as f32;
    let gains = gain_acc
        .into_iter()
        .map(|g| (g / weight_acc) as f32)
        .collect();
    Some((gains, explained, model))
}

/// Best alignment of a candidate group under `window`.
///
/// `members` are the group's samples, already resampled to the reference's
/// rate; each is searched over its first `scan` frames. `peaks` proposals from
/// the FFT sweep are fitted at full rate, each refined over ±`refine` frames.
pub fn best_fit(
    window: &[f32],
    members: &[&[f32]],
    scan: usize,
    peaks: usize,
    refine: usize,
) -> Option<GroupFit> {
    if members.is_empty() || window.is_empty() {
        return None;
    }
    let w = window.len();
    let limit = members
        .iter()
        .map(|m| m.len().saturating_sub(w + 1))
        .min()
        .unwrap_or(0)
        .min(scan);
    if limit == 0 {
        return None;
    }

    // Coarse sweep: normalised correlation of each member over the whole
    // range, summed. Full bandwidth on purpose — decimating here low-passes a
    // sustained note into ambiguity.
    let mut planner = RealFftPlanner::<f32>::new();
    let wwin = whiten(window);
    let ref_norm = (wwin.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>()).sqrt();
    let mut coarse = vec![0.0f32; limit + 1];
    for m in members {
        let hay = whiten(&m[..(limit + w + 1).min(m.len())]);
        let dots = xcorr(&hay, &wwin, &mut planner);
        let eng = energy_prefix(&hay);
        for k in 0..=limit.min(dots.len().saturating_sub(1)) {
            let e = (eng[k + w] - eng[k]).max(1e-20).sqrt();
            coarse[k] += (dots[k] as f64 / (e * ref_norm.max(1e-20))) as f32;
        }
    }

    let mut order: Vec<usize> = (0..=limit).collect();
    order.sort_by(|&a, &b| coarse[b].total_cmp(&coarse[a]));

    // Coverage, not just ranking. The coarse score correlates each member
    // against the window, but the window is a MIXTURE of members that are
    // themselves alike (one note at several dynamics), so its peak is not
    // dependable: measured on an exact mixture of real samples, the true
    // offset — which the fit explains at 100% — was not in the coarse top 48
    // at all, and the search settled for 60% elsewhere. So propose a uniform
    // grid as well and let the fit, which cannot be fooled, decide between
    // them. The grid step is the refine radius, so every offset is within
    // reach of some proposal.
    let step = refine.max(1);
    order.truncate(peaks);
    order.extend((0..=limit).step_by(step));

    // Fitting stage. Every surviving proposal is refined SAMPLE BY SAMPLE,
    // because a fit is only meaningful within a few samples of the truth: at a
    // grid step of half a millisecond the waveform is already a third of a
    // period out of phase, so scoring proposals at their grid point alone
    // ranks the true region below its neighbours and the answer lands 5 ms
    // off. Speed has to come from having fewer CANDIDATES — which is what
    // knowing the MIDI buys — not from fitting each one less carefully.
    let mut best: Option<GroupFit> = None;
    let mut seen: Vec<usize> = Vec::new();
    for &c in order.iter() {
        if seen.iter().any(|&s| c.abs_diff(s) <= refine) {
            continue;
        }
        seen.push(c);
        let lo = c.saturating_sub(refine);
        let hi = (c + refine).min(limit);
        for off in lo..=hi {
            if let Some((gains, explained, model)) = fit_at(window, members, off) {
                if best.as_ref().is_none_or(|b| explained > b.explained) {
                    best = Some(GroupFit {
                        offset: off,
                        explained,
                        gains,
                        model,
                    });
                }
            }
        }
    }
    best
}

/// Peel overlapping voices apart, one at a time.
///
/// A legato transition is not one sample: the outgoing body, the transition
/// recording and the incoming body sound TOGETHER, each from its own onset. A
/// single shared-offset fit cannot express that — offered two onsets it locks
/// onto one and leaves the other in the residual, which is exactly the handle
/// to pull. So fit the best group anywhere in the scan, subtract precisely
/// what it contributed, and fit again: each pass names another voice and the
/// timing it entered at, which is what "which samples were added together, and
/// when" means concretely.
///
/// Stops at `max_voices`, or when a pass explains less than `min_share` of the
/// original window — an honest floor, since with enough candidates something
/// will always shave a little off a residual.
pub fn decompose(
    window: &[f32],
    groups: &[Vec<&[f32]>],
    scan: usize,
    peaks: usize,
    refine: usize,
    max_voices: usize,
    min_share: f32,
) -> Vec<VoiceFit> {
    let total: f64 = window.iter().map(|v| (*v as f64) * (*v as f64)).sum();
    if total <= 0.0 {
        return Vec::new();
    }
    let mut residual = window.to_vec();
    let mut found = Vec::new();
    for _ in 0..max_voices {
        let mut best: Option<(usize, GroupFit)> = None;
        for (gi, members) in groups.iter().enumerate() {
            let Some(fit) = best_fit(&residual, members, scan, peaks, refine) else {
                continue;
            };
            if best
                .as_ref()
                .is_none_or(|(_, b)| fit.explained > b.explained)
            {
                best = Some((gi, fit));
            }
        }
        let Some((gi, fit)) = best else { break };
        // Share of the ORIGINAL window, not of the shrinking residual, so the
        // numbers across voices stay comparable and the floor means something.
        let energy: f64 = fit.model.iter().map(|v| (*v as f64) * (*v as f64)).sum();
        let share = (energy / total) as f32;
        if share < min_share {
            break;
        }
        for (r, m) in residual.iter_mut().zip(&fit.model) {
            *r -= m;
        }
        found.push(VoiceFit {
            group: gi,
            fit,
            share,
        });
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noise(n: usize, seed: u32) -> Vec<f32> {
        let mut s = seed;
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(1664525).wrapping_add(1013904223);
                ((s >> 8) as f32 / 8388608.0) - 1.0
            })
            .collect()
    }

    /// The whole point: a mixture of two members at known gains and a known
    /// offset is recovered exactly — the case single-sample correlation could
    /// not resolve.
    #[test]
    fn recovers_offset_and_gains_from_a_mixture() {
        let a = noise(48000, 1);
        let b = noise(48000, 7);
        let (offset, ga, gb) = (12345usize, 0.6f32, 0.25f32);
        let w = 4096;
        let window: Vec<f32> = (0..w)
            .map(|i| ga * a[offset + i] + gb * b[offset + i])
            .collect();

        let fit = best_fit(&window, &[a.as_slice(), b.as_slice()], 30000, 32, 24).expect("a fit");
        assert_eq!(fit.offset, offset, "offset must be sample-exact");
        assert!((fit.gains[0] - ga).abs() < 1e-3, "gain a {:?}", fit.gains);
        assert!((fit.gains[1] - gb).abs() < 1e-3, "gain b {:?}", fit.gains);
        assert!(fit.explained > 0.995, "explained {}", fit.explained);
    }

    /// A group that is not what is playing must not be able to fake a fit:
    /// its residual stays large whatever the gains.
    #[test]
    fn a_wrong_group_explains_little() {
        let real = noise(48000, 3);
        let other = noise(48000, 99);
        let window: Vec<f32> = real[5000..5000 + 4096].to_vec();

        let right = best_fit(&window, &[real.as_slice()], 30000, 32, 24).expect("fit");
        let wrong = best_fit(&window, &[other.as_slice()], 30000, 32, 24).expect("fit");
        assert!(right.explained > 0.995, "right {}", right.explained);
        assert!(
            wrong.explained < 0.2,
            "wrong group explained {} — the score must separate them",
            wrong.explained
        );
    }
}
