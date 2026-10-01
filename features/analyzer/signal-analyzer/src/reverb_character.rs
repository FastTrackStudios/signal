//! What a reverb *sounds like*, as numbers two reverbs can be compared on.
//!
//! [`crate::decay`] answers "does it ring for as long, per octave?". That is
//! necessary and nowhere near sufficient: two reverbs with identical RT60s
//! can be a grainy multitap wash and a smooth plate. Matching a character
//! reverb (Strymon's Cloud, a Valhalla mode) needs the rest of what the ear
//! hears, each measured on its own because each is fixed by a different
//! part of the algorithm:
//!
//! | measurement | heard as | set by |
//! |---|---|---|
//! | onset, peak time, 10 ms envelope | pre-delay; how long the tail *swells* before it falls | pre-delay, input diffusion, tank build-up |
//! | decay (via [`crate::decay`]), broadband + octave | length; how the tail darkens | loop gain, in-loop damping |
//! | echo density (Abel & Huang) | discrete "grain" (≈0) → smooth "fog" (≈1) | diffusers, multitap count |
//! | third-octave spectrum, early and late | colour of the attack vs the tail | input/output EQ, damping |
//! | spectral centroid over time | how fast the tail darkens, audibly | in-loop damping |
//! | L/R correlation over time | width, and how fast it opens | cross-seeding, output taps |
//! | sideband energy on a held sine | chorus depth × rate; "shimmer" of the tail | delay-line / diffuser modulation |
//!
//! Same rules as the rest of the crate: offline, allocation-tolerant, and
//! "not measurable" is `None`, never an invented number.

use std::f64::consts::PI;
use std::fmt::Write as _;

use realfft::RealFftPlanner;

use crate::decay::{DecayFit, reverb_time_best_effort};
use crate::filters::{OCTAVE_CENTRES_HZ, octave_bands};
use crate::reverb_stimuli::SINE_AMPLITUDE;

/// Envelope resolution, ms.
pub const ENV_MS: f64 = 10.0;
/// Echo-density hop, ms (first 500 ms after onset).
pub const NED_HOP_MS: f64 = 5.0;
const NED_HOPS: usize = 100;
/// Spectral-centroid frame, ms (first 3 s after onset).
pub const CENTROID_MS: f64 = 50.0;
const CENTROID_FRAMES: usize = 60;
/// The early field, for the "early" spectrum.
pub const EARLY_MS: f64 = 150.0;

/// Third-octave centres for the spectra, 63 Hz … 16 kHz.
#[must_use]
pub fn third_octaves() -> Vec<f64> {
    (-12..=12).map(|k| 1000.0 * 2f64.powf(f64::from(k) / 3.0)).collect()
}

/// Decay times of one signal, seconds.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Decay {
    /// Early decay time: 0 → −10 dB, ×6. What the ear hears as "length"
    /// while the music is still playing.
    pub edt: Option<f64>,
    pub t20: Option<f64>,
    pub t30: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Band {
    pub centre_hz: f64,
    pub decay: Decay,
    /// Band energy relative to the broadband energy, dB.
    pub level_db: f64,
}

/// Modulation, measured on a held sine.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Modulation {
    pub f0: f64,
    /// Wet level while the tone is held, dB re the input tone.
    pub sustain_gain_db: f64,
    /// Energy within ±60 Hz but outside ±3 Hz of f0, dB re the ±3 Hz core,
    /// while the tone is held. More modulation → higher.
    pub sustain_side_db: f64,
    /// RMS deviation from f0 over ±60 Hz while held, Hz.
    pub sustain_spread_hz: f64,
    /// The same two, in the tail after the tone stops.
    pub tail_side_db: f64,
    pub tail_spread_hz: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Character {
    pub sample_rate: f64,
    /// Total output energy, dB (mean of the two channels' energy).
    pub energy_db: f64,
    /// First sample within 40 dB of the peak, ms after the stimulus start.
    pub onset_ms: f64,
    /// Start of the loudest 10 ms window, ms after the stimulus start.
    pub peak_ms: f64,
    /// Where the decay is measured from (the excitation's end), ms.
    pub decay_from_ms: f64,
    /// 10 ms RMS envelope from the stimulus start, dB re its loudest window.
    pub envelope_db: Vec<f64>,
    pub decay: Decay,
    pub bands: Vec<Band>,
    /// Normalized echo density every [`NED_HOP_MS`] from the onset.
    pub echo_density: Vec<f64>,
    /// First time (ms after onset) the echo density reaches 0.9.
    pub mixing_ms: Option<f64>,
    /// Third-octave levels, dB re their mean: first [`EARLY_MS`] after the
    /// onset, and the tail after that down to −40 dB (at most 4 s).
    pub spectrum_early: Vec<f64>,
    pub spectrum_late: Vec<f64>,
    /// Spectral centroid every [`CENTROID_MS`] from the onset, Hz.
    pub centroid_hz: Vec<f64>,
    /// L/R correlation over 0–50, 50–200, 200–1000 ms and 1 s+ after onset.
    pub correlation: [f64; 4],
    pub modulation: Option<Modulation>,
}

fn db(power: f64) -> f64 {
    10.0 * power.max(1e-30).log10()
}

/// Hann-windowed power spectrum of `x`, zero-padded to `n`; `n / 2 + 1` bins.
fn power_spectrum(planner: &mut RealFftPlanner<f64>, x: &[f64], n: usize) -> Vec<f64> {
    let fft = planner.plan_fft_forward(n);
    let mut input = fft.make_input_vec();
    let m = x.len().min(n).max(1);
    for (i, (slot, v)) in input.iter_mut().zip(x).enumerate() {
        *slot = v * (0.5 - 0.5 * (2.0 * PI * i as f64 / m as f64).cos());
    }
    let mut spec = fft.make_output_vec();
    if fft.process(&mut input, &mut spec).is_err() {
        return vec![0.0; n / 2 + 1];
    }
    spec.iter().map(|c| c.norm_sqr()).collect()
}

/// Welch-averaged power spectrum (Hann, 50 % overlap) and its bin width.
fn welch(planner: &mut RealFftPlanner<f64>, x: &[f64], frame: usize, sr: f64) -> (Vec<f64>, f64) {
    let mut acc = vec![0.0; frame / 2 + 1];
    let mut count = 0usize;
    let mut a = 0;
    while a < x.len() {
        let seg = x.get(a..(a + frame).min(x.len())).unwrap_or(&[]);
        for (s, v) in acc.iter_mut().zip(power_spectrum(planner, seg, frame)) {
            *s += v;
        }
        count += 1;
        if a + frame >= x.len() {
            break;
        }
        a += frame / 2;
    }
    let count = count.max(1) as f64;
    (acc.into_iter().map(|v| v / count).collect(), sr / frame as f64)
}

/// Band levels (dB re their mean) at `centres`, `width_oct` wide.
fn band_levels(power: &[f64], df: f64, centres: &[f64], width_oct: f64) -> Vec<f64> {
    let raw: Vec<f64> = centres
        .iter()
        .map(|fc| {
            let lo = (fc * 2f64.powf(-width_oct / 2.0) / df).ceil() as usize;
            let hi = (fc * 2f64.powf(width_oct / 2.0) / df).floor() as usize;
            power.get(lo..=hi.min(power.len().saturating_sub(1))).map_or(0.0, |s| s.iter().sum())
        })
        .collect();
    let mean = db(raw.iter().sum::<f64>() / raw.len().max(1) as f64);
    raw.iter().map(|v| db(*v) - mean).collect()
}

/// Cut `energy` where it meets its noise floor, so a hardware-style noise
/// floor (a modelled pedal's) is not read as an endless tail.
fn truncate_at_floor(energy: &[f64], sr: f64) -> &[f64] {
    let win = ((0.05 * sr) as usize).max(1);
    let floor = energy
        .get(energy.len() * 4 / 5..)
        .unwrap_or(&[])
        .chunks(win)
        .map(|c| c.iter().sum::<f64>() / c.len() as f64)
        .fold(f64::INFINITY, f64::min);
    let cut = energy
        .chunks(win)
        .position(|c| c.iter().sum::<f64>() / (c.len() as f64) <= floor * 3.0 + 1e-30)
        .map_or(energy.len(), |w| (w * win).max(win));
    energy.get(..cut.min(energy.len())).unwrap_or(energy)
}

/// Decay times of a per-sample energy signal starting at the decay origin.
fn decay_of(energy: &[f64], sr: f64) -> Decay {
    // `crate::decay` squares its input: hand it the RMS amplitude.
    let amp: Vec<f32> = truncate_at_floor(energy, sr).iter().map(|e| e.sqrt() as f32).collect();
    Decay {
        edt: edt(&amp, sr),
        t20: reverb_time_best_effort(&amp, sr, DecayFit::T20),
        t30: crate::decay::reverb_time(&amp, sr, DecayFit::T30),
    }
}

/// Early decay time: slope of the EDC from 0 to −10 dB, ×6.
///
/// Not a straight-line fit with an r² guard like [`crate::decay`]'s — a
/// swelling reverb's first 10 dB is curved by design, and EDT is meant to
/// report exactly that part.
fn edt(amp: &[f32], sr: f64) -> Option<f64> {
    let edc = crate::decay::energy_decay_curve(amp);
    let end = edc.iter().position(|v| *v <= -10.0)?;
    (end > 0).then(|| end as f64 / sr * 6.0)
}

/// Abel & Huang normalized echo density of `h`: the fraction of samples
/// outside one standard deviation in a 20 ms Hann window, over the
/// fraction a Gaussian would have there (erfc(1/√2)).
fn echo_density(h: &[f64], sr: f64, from: usize) -> Vec<f64> {
    let half = ((0.01 * sr) as usize).max(1);
    let hop = ((NED_HOP_MS * 1e-3 * sr) as usize).max(1);
    let w: Vec<f64> = (0..2 * half).map(|i| 0.5 - 0.5 * (PI * i as f64 / half as f64).cos()).collect();
    let wsum: f64 = w.iter().sum();
    const GAUSSIAN_OUTSIDE: f64 = 0.317_310_507_862_914_1;
    (0..NED_HOPS)
        .map(|k| {
            let c = from + k * hop;
            let Some(seg) = c.checked_sub(half).and_then(|a| h.get(a..c + half)) else {
                return 0.0;
            };
            let sigma = (seg.iter().zip(&w).map(|(x, w)| w * x * x).sum::<f64>() / wsum).sqrt();
            if sigma <= 0.0 {
                return 0.0;
            }
            let outside: f64 = seg.iter().zip(&w).filter(|(x, _)| x.abs() > sigma).map(|(_, w)| w).sum();
            outside / wsum / GAUSSIAN_OUTSIDE
        })
        .collect()
}

fn correlation(l: &[f64], r: &[f64]) -> f64 {
    let (mut lr, mut ll, mut rr) = (0.0, 0.0, 0.0);
    for (a, b) in l.iter().zip(r) {
        lr += a * b;
        ll += a * a;
        rr += b * b;
    }
    if ll * rr <= 0.0 { 0.0 } else { lr / (ll * rr).sqrt() }
}

/// Sideband energy (dB re the core) and RMS spread (Hz) around `f0`,
/// from 32k-sample windows (0.68 s at 48 kHz, 4× zero-padded) every 8k
/// samples across `[start, end)`, their power spectra averaged. A
/// modulated reverb's spectrum around a tone is random from window to
/// window; averaging is what makes the number repeatable.
fn tone_spread(planner: &mut RealFftPlanner<f64>, x: &[f64], start: usize, end: usize, f0: f64, sr: f64) -> Option<(f64, f64)> {
    const N: usize = 32_768;
    const HOP: usize = 8_192;
    const PAD: usize = 131_072;
    let mut acc = vec![0.0; PAD / 2 + 1];
    let mut a = start;
    let mut windows = 0usize;
    while a + N <= end.min(x.len()) {
        for (s, v) in acc.iter_mut().zip(power_spectrum(planner, x.get(a..a + N)?, PAD)) {
            *s += v;
        }
        windows += 1;
        a += HOP;
    }
    if windows == 0 {
        return None;
    }
    let df = sr / PAD as f64;
    let (mut total, mut core, mut moment) = (0.0, 0.0, 0.0);
    for (k, p) in acc.iter().enumerate() {
        let d = k as f64 * df - f0;
        if d.abs() <= 60.0 {
            total += p;
            moment += d * d * p;
            if d.abs() <= 3.0 {
                core += p;
            }
        }
    }
    (core > 0.0).then(|| (db((total - core) / core), (moment / total).sqrt()))
}

/// Measure a stereo response. `excitation_end_s` is where the stimulus
/// stops driving the reverb (0 for an impulse); `tone_hz` enables the
/// modulation measurement for a held sine of that frequency at
/// [`SINE_AMPLITUDE`].
#[must_use]
pub fn measure(left: &[f32], right: &[f32], sample_rate: f64, excitation_end_s: f64, tone_hz: Option<f64>) -> Character {
    let sr = sample_rate;
    let n = left.len().min(right.len());
    let l: Vec<f64> = left.iter().take(n).map(|v| f64::from(*v)).collect();
    let r: Vec<f64> = right.iter().take(n).map(|v| f64::from(*v)).collect();
    let energy: Vec<f64> = l.iter().zip(&r).map(|(a, b)| 0.5 * (a * a + b * b)).collect();
    let at = |ms: f64| ((ms * 1e-3 * sr) as usize).min(n);
    let mut planner = RealFftPlanner::<f64>::new();

    let peak = l.iter().chain(&r).fold(0.0f64, |m, v| m.max(v.abs()));
    let onset = l.iter().zip(&r).position(|(a, b)| a.abs().max(b.abs()) > peak * 0.01).unwrap_or(0);

    let env_win = at(ENV_MS).max(1);
    let env: Vec<f64> = energy.chunks(env_win).map(|c| c.iter().sum::<f64>() / c.len() as f64).collect();
    let env_peak = env.iter().copied().fold(0.0, f64::max).max(1e-30);
    let peak_win = env.iter().position(|v| *v >= env_peak).unwrap_or(0);
    let envelope_db: Vec<f64> = env.iter().map(|v| db(v / env_peak).max(-120.0)).collect();

    let from = at(excitation_end_s * 1000.0).max(onset);
    let tail = |e: &[f64]| decay_of(e.get(from..).unwrap_or(&[]), sr);
    let total = energy.iter().sum::<f64>().max(1e-30);
    let (bl, br) = (octave_bands(left, sr), octave_bands(right, sr));
    let bands = OCTAVE_CENTRES_HZ
        .iter()
        .zip(bl.iter().zip(&br))
        .map(|(&centre_hz, (a, b))| {
            let e: Vec<f64> = a.iter().zip(b).map(|(x, y)| 0.5 * (f64::from(*x).powi(2) + f64::from(*y).powi(2))).collect();
            Band { centre_hz, level_db: db(e.iter().sum::<f64>() / total), decay: tail(&e) }
        })
        .collect();

    let echo = echo_density(&l, sr, onset);
    let mixing_ms = echo.iter().position(|v| *v >= 0.9).map(|k| k as f64 * NED_HOP_MS);

    let mono: Vec<f64> = l.iter().zip(&r).map(|(a, b)| 0.5 * (a + b)).collect();
    let early_end = (onset + at(EARLY_MS)).min(n);
    let late_start = early_end.max(from);
    let late_end = envelope_db
        .iter()
        .enumerate()
        .skip(peak_win)
        .find(|(_, v)| **v < -40.0)
        .map_or(n, |(k, _)| k * env_win)
        .clamp(late_start, (late_start + at(4000.0)).min(n).max(late_start));
    let thirds = third_octaves();
    let mut spectrum = |a: usize, b: usize| {
        let (p, df) = welch(&mut planner, mono.get(a..b).unwrap_or(&[]), 4096, sr);
        band_levels(&p, df, &thirds, 1.0 / 3.0)
    };
    let spectrum_early = spectrum(onset, early_end);
    let spectrum_late = spectrum(late_start, late_end);

    let frame = at(CENTROID_MS).max(1);
    let centroid_hz = (0..CENTROID_FRAMES)
        .map_while(|k| {
            let a = onset + k * frame;
            let p = power_spectrum(&mut planner, mono.get(a..a + 2048)?, 2048);
            let df = sr / 2048.0;
            let (num, den) = p.iter().enumerate().fold((0.0, 0.0), |(nu, de), (i, v)| (nu + i as f64 * df * v, de + v));
            Some(if den > 0.0 { num / den } else { 0.0 })
        })
        .collect();

    let corr = |a_ms: f64, b_ms: f64| {
        let (a, b) = ((onset + at(a_ms)).min(n), (onset + at(b_ms)).min(n));
        correlation(l.get(a..b).unwrap_or(&[]), r.get(a..b).unwrap_or(&[]))
    };
    let correlation = [corr(0.0, 50.0), corr(50.0, 200.0), corr(200.0, 1000.0), corr(1000.0, 1e7)];

    let modulation = tone_hz.and_then(|f0| {
        let end_ms = excitation_end_s * 1000.0;
        let (sustain_side_db, sustain_spread_hz) = tone_spread(&mut planner, &l, at(600.0), at(end_ms - 50.0), f0, sr)?;
        let (tail_side_db, tail_spread_hz) =
            tone_spread(&mut planner, &l, at(end_ms + 50.0), at(end_ms + 2050.0), f0, sr)?;
        let held = energy.get(at(600.0)..at(end_ms - 50.0))?;
        let rms = (held.iter().sum::<f64>() / held.len().max(1) as f64).sqrt();
        Some(Modulation {
            f0,
            sustain_gain_db: 20.0 * (rms / (SINE_AMPLITUDE / 2f64.sqrt())).max(1e-15).log10(),
            sustain_side_db,
            sustain_spread_hz,
            tail_side_db,
            tail_spread_hz,
        })
    });

    Character {
        sample_rate: sr,
        energy_db: db(total),
        onset_ms: onset as f64 / sr * 1000.0,
        peak_ms: peak_win as f64 * ENV_MS,
        decay_from_ms: from as f64 / sr * 1000.0,
        envelope_db,
        decay: tail(&energy),
        bands,
        echo_density: echo,
        mixing_ms,
        spectrum_early,
        spectrum_late,
        centroid_hz,
        correlation,
        modulation,
    }
}

/// How far a candidate is from a reference, one number per dimension —
/// kept apart for the same reason [`crate::compare`] keeps its metrics
/// apart: each is fixed by a different part of the algorithm.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Distance {
    /// Energy difference, candidate − reference, dB.
    pub level_db: f64,
    /// Mean |log2(candidate / reference)| of T20 over the fitted octave
    /// bands; 0.1 ≈ 7 % off on average.
    pub decay_log2: Option<f64>,
    /// RMS difference of the 10 ms envelopes (dB, clamped at −60) over the
    /// reference's first 60 dB.
    pub envelope_db: f64,
    /// Mean |Δ| echo density over the first 200 ms.
    pub echo_density: f64,
    /// RMS third-octave difference, early and late, dB — over the bands
    /// within 30 dB of the reference's loudest.
    pub spectrum_early_db: f64,
    pub spectrum_late_db: f64,
    /// Mean |Δ| L/R correlation over the four windows.
    pub correlation: f64,
    /// |Δ| sideband level while held and in the tail, dB.
    pub modulation_db: Option<f64>,
    /// Mean |log2(candidate / reference)| of the spread, held and tail.
    pub modulation_spread_log2: Option<f64>,
}

fn rms_diff(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len().min(b.len()).max(1) as f64;
    (a.iter().zip(b).map(|(x, y)| (x - y).powi(2)).sum::<f64>() / n).sqrt()
}

/// How far below the reference's loudest band a band still counts in the
/// spectral distance. A held sine leaves most third-octaves at the noise
/// floor, and differencing two floors measures nothing about the reverb.
const SPECTRUM_RANGE_DB: f64 = 30.0;

/// RMS band difference over the bands the reference actually has energy in.
fn spectrum_diff(reference: &[f64], candidate: &[f64]) -> f64 {
    let top = reference.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let pairs: Vec<(f64, f64)> = reference
        .iter()
        .zip(candidate)
        .filter(|(r, _)| **r >= top - SPECTRUM_RANGE_DB)
        .map(|(r, c)| (*r, *c))
        .collect();
    let n = pairs.len().max(1) as f64;
    (pairs.iter().map(|(r, c)| (r - c).powi(2)).sum::<f64>() / n).sqrt()
}

#[must_use]
pub fn distance(reference: &Character, candidate: &Character) -> Distance {
    let ratios: Vec<f64> = reference
        .bands
        .iter()
        .zip(&candidate.bands)
        .filter_map(|(r, c)| Some((c.decay.t20? / r.decay.t20?).log2().abs()))
        .collect();
    let horizon = reference.envelope_db.iter().rposition(|v| *v > -60.0).unwrap_or(0) + 1;
    let clamp = |v: &[f64]| v.iter().take(horizon).map(|x| x.max(-60.0)).collect::<Vec<_>>();
    let tonal = reference.modulation.is_some();
    let ned = 200.0 / NED_HOP_MS;
    let ned_n = ned as usize;
    Distance {
        level_db: candidate.energy_db - reference.energy_db,
        decay_log2: (!ratios.is_empty()).then(|| ratios.iter().sum::<f64>() / ratios.len() as f64),
        envelope_db: rms_diff(&clamp(&reference.envelope_db), &clamp(&candidate.envelope_db)),
        echo_density: reference
            .echo_density
            .iter()
            .zip(&candidate.echo_density)
            .take(ned_n)
            .map(|(a, b)| (a - b).abs())
            .sum::<f64>()
            / ned,
        // A held tone has one band worth comparing; its spectrum says
        // nothing about the reverb's colour.
        spectrum_early_db: if tonal { f64::NAN } else { spectrum_diff(&reference.spectrum_early, &candidate.spectrum_early) },
        spectrum_late_db: if tonal { f64::NAN } else { spectrum_diff(&reference.spectrum_late, &candidate.spectrum_late) },
        correlation: reference.correlation.iter().zip(&candidate.correlation).map(|(a, b)| (a - b).abs()).sum::<f64>() / 4.0,
        modulation_db: match (reference.modulation, candidate.modulation) {
            (Some(r), Some(c)) => {
                Some(0.5 * ((c.sustain_side_db - r.sustain_side_db).abs() + (c.tail_side_db - r.tail_side_db).abs()))
            }
            _ => None,
        },
        modulation_spread_log2: match (reference.modulation, candidate.modulation) {
            (Some(r), Some(c)) => Some(
                0.5 * ((c.sustain_spread_hz / r.sustain_spread_hz).log2().abs()
                    + (c.tail_spread_hz / r.tail_spread_hz).log2().abs()),
            ),
            _ => None,
        },
    }
}

fn opt(v: Option<f64>, scale: f64, unit: &str) -> String {
    v.map_or_else(|| "—".into(), |x| format!("{:.2}{unit}", x * scale))
}

/// A Markdown side-by-side of two measurements of the same stimulus.
#[must_use]
pub fn report(title: &str, reference: &Character, candidate: &Character, ref_name: &str, cand_name: &str) -> String {
    let d = distance(reference, candidate);
    let mut s = String::new();
    let _ = writeln!(s, "### {title}\n");
    let _ = writeln!(s, "| | {ref_name} | {cand_name} |\n|---|---:|---:|");
    let row = |s: &mut String, k: &str, a: String, b: String| {
        let _ = writeln!(s, "| {k} | {a} | {b} |");
    };
    row(&mut s, "energy (dB)", format!("{:.1}", reference.energy_db), format!("{:.1}", candidate.energy_db));
    row(&mut s, "onset (ms)", format!("{:.1}", reference.onset_ms), format!("{:.1}", candidate.onset_ms));
    row(&mut s, "peak (ms)", format!("{:.0}", reference.peak_ms), format!("{:.0}", candidate.peak_ms));
    row(&mut s, "EDT (s)", opt(reference.decay.edt, 1.0, ""), opt(candidate.decay.edt, 1.0, ""));
    row(&mut s, "T20 (s)", opt(reference.decay.t20, 1.0, ""), opt(candidate.decay.t20, 1.0, ""));
    row(&mut s, "T30 (s)", opt(reference.decay.t30, 1.0, ""), opt(candidate.decay.t30, 1.0, ""));
    row(&mut s, "mixing time, NED ≥ 0.9 (ms)", opt(reference.mixing_ms, 1.0, ""), opt(candidate.mixing_ms, 1.0, ""));
    for (k, i) in [("L/R corr 0–50 ms", 0), ("L/R corr 50–200 ms", 1), ("L/R corr 0.2–1 s", 2), ("L/R corr 1 s+", 3)] {
        row(&mut s, k, format!("{:.2}", reference.correlation[i]), format!("{:.2}", candidate.correlation[i]));
    }
    if let (Some(a), Some(b)) = (reference.modulation, candidate.modulation) {
        row(&mut s, "held wet gain (dB)", format!("{:.1}", a.sustain_gain_db), format!("{:.1}", b.sustain_gain_db));
        row(&mut s, "held sidebands (dB)", format!("{:.1}", a.sustain_side_db), format!("{:.1}", b.sustain_side_db));
        row(&mut s, "held spread (Hz)", format!("{:.2}", a.sustain_spread_hz), format!("{:.2}", b.sustain_spread_hz));
        row(&mut s, "tail sidebands (dB)", format!("{:.1}", a.tail_side_db), format!("{:.1}", b.tail_side_db));
        row(&mut s, "tail spread (Hz)", format!("{:.2}", a.tail_spread_hz), format!("{:.2}", b.tail_spread_hz));
    }

    let _ = writeln!(s, "\n| octave | T20 {ref_name} | T20 {cand_name} | ratio | level {ref_name} | level {cand_name} |\n|---:|---:|---:|---:|---:|---:|");
    for (a, b) in reference.bands.iter().zip(&candidate.bands) {
        let ratio = a.decay.t20.zip(b.decay.t20).map(|(x, y)| y / x);
        let _ = writeln!(
            s,
            "| {:.0} | {} | {} | {} | {:.1} | {:.1} |",
            a.centre_hz,
            opt(a.decay.t20, 1.0, ""),
            opt(b.decay.t20, 1.0, ""),
            opt(ratio, 1.0, "×"),
            a.level_db,
            b.level_db
        );
    }

    let _ = writeln!(s, "\n| t (ms) | envelope dB {ref_name} | {cand_name} | echo density {ref_name} | {cand_name} |\n|---:|---:|---:|---:|---:|");
    for t in [10.0, 20.0, 50.0, 100.0, 150.0, 200.0, 300.0, 500.0, 1000.0, 2000.0, 4000.0] {
        let e = |c: &Character| c.envelope_db.get((t / ENV_MS) as usize).map_or("—".into(), |v| format!("{v:.1}"));
        let n = |c: &Character| {
            let k = ((t - (c.onset_ms - reference.onset_ms).max(0.0)) / NED_HOP_MS) as usize;
            c.echo_density.get(k).filter(|_| t <= 500.0).map_or("".into(), |v| format!("{v:.2}"))
        };
        let _ = writeln!(s, "| {t:.0} | {} | {} | {} | {} |", e(reference), e(candidate), n(reference), n(candidate));
    }

    let _ = writeln!(s, "\n| ⅓-oct (Hz) | early {ref_name} | {cand_name} | late {ref_name} | {cand_name} |\n|---:|---:|---:|---:|---:|");
    for (i, fc) in third_octaves().iter().enumerate().step_by(3) {
        let g = |v: &[f64]| v.get(i).map_or("—".into(), |x| format!("{x:+.1}"));
        let _ = writeln!(
            s,
            "| {fc:.0} | {} | {} | {} | {} |",
            g(&reference.spectrum_early),
            g(&candidate.spectrum_early),
            g(&reference.spectrum_late),
            g(&candidate.spectrum_late)
        );
    }

    let _ = writeln!(
        s,
        "\n**distance** — level {:+.1} dB · decay {} · envelope {:.1} dB · echo density {:.2} · spectrum early {} / late {} dB · correlation {:.2} · modulation {}\n",
        d.level_db,
        d.decay_log2.map_or_else(|| "—".into(), |v| format!("{:.0}%", (2f64.powf(v) - 1.0) * 100.0)),
        d.envelope_db,
        d.echo_density,
        if d.spectrum_early_db.is_finite() { format!("{:.1}", d.spectrum_early_db) } else { "—".into() },
        if d.spectrum_late_db.is_finite() { format!("{:.1}", d.spectrum_late_db) } else { "—".into() },
        d.correlation,
        d.modulation_spread_log2.map_or_else(|| "—".into(), |v| format!("spread {:.0}% off", (2f64.powf(v) - 1.0) * 100.0)),
    );
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic exponential noise tail with a known RT60 measures back
    /// to it, and against itself has zero distance.
    #[test]
    fn exponential_noise_tail_measures_its_rt60() {
        let sr = 48_000.0;
        let rt60 = 1.5;
        let n = (sr * 4.0) as usize;
        let mut seed = 1u32;
        let mut noise = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            f64::from(seed >> 8) / f64::from(1u32 << 24) - 0.5
        };
        let (l, r): (Vec<f32>, Vec<f32>) = (0..n)
            .map(|i| {
                let g = 10f64.powf(-3.0 * i as f64 / sr / rt60);
                ((noise() * g) as f32, (noise() * g) as f32)
            })
            .unzip();
        let c = measure(&l, &r, sr, 0.0, None);
        let t20 = c.decay.t20.unwrap_or(0.0);
        assert!((t20 - rt60).abs() < 0.1, "T20 {t20}");
        assert!(c.correlation[3].abs() < 0.1, "independent channels decorrelate");
        let late_density = c.echo_density.get(60).copied().unwrap_or(0.0);
        assert!(late_density > 0.8, "noise is fog: {late_density}");
        let d = distance(&c, &c);
        assert!(d.envelope_db.abs() < 1e-9 && d.decay_log2 == Some(0.0));
    }
}
