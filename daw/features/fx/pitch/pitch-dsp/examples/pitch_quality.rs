//! Objective pitch-shifter quality harness.
//!
//! Renders guitar-like material through every shifter in the library at
//! +12 / −12 / +7 / +5 semitones and scores each render:
//!
//! - `cents`   median |pitch error| (YIN on output vs YIN(input)·ratio,
//!             aligned by the measured latency) over stable voiced frames
//!             of plucked notes + the real DI
//! - `gross%`  share of those frames more than 50 cents off (octave errors,
//!             unvoiced/garbled output)
//! - `xam dB`  grain/hop-rate amplitude modulation on a steady tone: std of
//!             the de-trended log envelope (10 ms frames), minus that of the
//!             ideal shifted tone. 0 = perfect
//! - `nonh dB` energy NOT on the expected shifted partials of a steady
//!             harmonic tone, relative to total (sidebands, warble, aliasing,
//!             noise). Lower is better
//! - `xflux`   mean normalised frame-to-frame spectral change on the steady
//!             tone (×100), minus that of the ideal shifted tone. 0 = perfect;
//!             warble/phasiness/grain switching raise it
//! - `hf dB`   energy above the band the shifted tone can occupy (aliasing /
//!             HF garbage), relative to total
//! - `rise ms` median 10→90 % attack time of plucked onsets in the output
//! - `pre dB`  energy in the 30 ms before each (latency-aligned) onset
//!             relative to the 30 ms after — pre-echo/smear
//! - `gain dB` output/input RMS on a strummed chord (unity shifting ≈ 0)
//! - `lat ms`  measured latency (envelope cross-correlation) / reported
//! - `ns/smp`  CPU cost per sample (release build, this machine)
//!
//! Run: cargo run --release -p pitch-dsp --example pitch_quality -- [wav_out_dir] [di.wav]

use std::f64::consts::PI;
use std::time::Instant;

use audiocore_dsp::grain_pitch::GrainPitchShifter;
use pitch_dsp::allpass_shift::AllpassShifter;
use pitch_dsp::granular::GranularShifter;
use pitch_dsp::pog::{OctaveShift, PolyOctave};
use pitch_dsp::psola::PsolaShifter;
use pitch_dsp::wsola::WsolaShifter;
use rustfft::{num_complex::Complex, FftPlanner};

const SR: f64 = 48000.0;

// ───────────────────────────── shifter adapters ─────────────────────────────

trait Shifter {
    fn tick(&mut self, x: f64) -> f64;
    fn latency(&self) -> usize;
}

struct Adapter<T> {
    inner: T,
    tick: fn(&mut T, f64) -> f64,
    lat: fn(&T) -> usize,
}
impl<T> Shifter for Adapter<T> {
    fn tick(&mut self, x: f64) -> f64 {
        (self.tick)(&mut self.inner, x)
    }
    fn latency(&self) -> usize {
        (self.lat)(&self.inner)
    }
}
fn boxed<T: 'static>(
    inner: T,
    tick: fn(&mut T, f64) -> f64,
    lat: fn(&T) -> usize,
) -> Box<dyn Shifter> {
    Box::new(Adapter { inner, tick, lat })
}

struct Candidate {
    name: &'static str,
    /// Build for a pitch ratio; `None` when the ratio is unsupported.
    make: fn(f64) -> Option<Box<dyn Shifter>>,
}

fn candidates() -> Vec<Candidate> {
    let mut v = vec![
        Candidate {
            name: "dry(ref)",
            make: |_| Some(boxed((), |_, x| x, |_| 0)),
        },
        Candidate {
            name: "granular1024",
            make: |r| {
                let mut g = GranularShifter::new();
                g.speed = r;
                g.mix = 1.0;
                g.grain_size = 1024;
                g.update(SR);
                Some(boxed(g, GranularShifter::tick, GranularShifter::latency))
            },
        },
        Candidate {
            // The Ice delay's shifter as the Flute patch configures it
            // (Long slice at 340 ms → 306 ms grains).
            name: "granular-ice306ms",
            make: |r| {
                let mut g = GranularShifter::new();
                g.speed = r;
                g.mix = 1.0;
                g.grain_size = 14688;
                g.update(SR);
                Some(boxed(g, GranularShifter::tick, |g| g.grain_size))
            },
        },
        Candidate {
            // The shimmer reverb's voice (audiocore GrainPitchShifter, 50 ms).
            name: "grainpitch50ms",
            make: |r| {
                let mut g = GrainPitchShifter::new(2400);
                g.set_grain_ms(50.0, SR);
                g.set_speed(r);
                Some(boxed(g, GrainPitchShifter::tick, |_| 0))
            },
        },
        Candidate {
            name: "wsola1024",
            make: |r| {
                let mut w = WsolaShifter::new();
                w.speed = r;
                w.mix = 1.0;
                w.update(SR);
                Some(boxed(w, WsolaShifter::tick, WsolaShifter::latency))
            },
        },
        Candidate {
            name: "wsola256(live)",
            make: |r| {
                let mut w = WsolaShifter::new();
                w.speed = r;
                w.mix = 1.0;
                w.base_grain_size = 256;
                w.update(SR);
                Some(boxed(w, WsolaShifter::tick, WsolaShifter::latency))
            },
        },
        Candidate {
            name: "psola2048",
            make: |r| {
                let mut p = PsolaShifter::new();
                p.speed = r;
                p.mix = 1.0;
                p.update(SR);
                Some(boxed(p, PsolaShifter::tick, PsolaShifter::latency))
            },
        },
        Candidate {
            name: "psola512(live)",
            make: |r| {
                let mut p = PsolaShifter::new();
                p.speed = r;
                p.mix = 1.0;
                p.base_window_size = 512;
                p.update(SR);
                Some(boxed(p, PsolaShifter::tick, PsolaShifter::latency))
            },
        },
        Candidate {
            name: "allpass",
            make: |r| {
                let mut a = AllpassShifter::new();
                a.speed = r;
                a.mix = 1.0;
                a.update(SR);
                Some(boxed(a, AllpassShifter::tick, AllpassShifter::latency))
            },
        },
        Candidate {
            name: "pog(oct only)",
            make: |r| {
                let st = 12.0 * r.log2();
                if (st.abs() - 12.0).abs() > 0.01 {
                    return None;
                }
                let mut p = PolyOctave::new();
                p.shift = OctaveShift::from_semitones(st);
                p.mix = 1.0;
                p.update(SR);
                Some(boxed(p, PolyOctave::tick, PolyOctave::latency))
            },
        },
    ];
    v.extend(extra_candidates());
    v
}

/// New engines (added as they land in the library).
fn extra_candidates() -> Vec<Candidate> {
    extra::list()
}

mod extra {
    use super::*;
    use pitch_dsp::spectral::SpectralShifter;
    fn pv(r: f64, n: usize, ov: usize) -> Option<Box<dyn Shifter>> {
        let mut s = SpectralShifter::new();
        s.speed = r;
        s.mix = 1.0;
        s.fft_size = n;
        s.overlap = ov;
        s.update(SR);
        Some(boxed(s, SpectralShifter::tick, SpectralShifter::latency))
    }
    pub fn list() -> Vec<Candidate> {
        vec![
            Candidate {
                name: "pv1024x8",
                make: |r| pv(r, 1024, 8),
            },
            Candidate {
                name: "pv2048x8",
                make: |r| pv(r, 2048, 8),
            },
            Candidate {
                name: "pv4096x8",
                make: |r| pv(r, 4096, 8),
            },
            Candidate {
                name: "pv4096x4",
                make: |r| pv(r, 4096, 4),
            },
            Candidate {
                name: "pv8192x8",
                make: |r| pv(r, 8192, 8),
            },
            Candidate {
                name: "pvnoreset4096x8",
                make: |r| {
                    let mut s = SpectralShifter::new();
                    s.speed = r;
                    s.fft_size = 4096;
                    s.transient_reset = false;
                    s.update(SR);
                    Some(boxed(s, SpectralShifter::tick, SpectralShifter::latency))
                },
            },
        ]
    }
}

// ───────────────────────────── test material ─────────────────────────────

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
    }
}

/// Karplus–Strong plucked string (fractional-delay tuned, pick-position
/// comb on the excitation), added into `out` at `start`.
fn ks_pluck(out: &mut [f64], start: usize, f0: f64, dur_s: f64, amp: f64, decay: f64, seed: u64) {
    let period = SR / f0 - 0.5; // loss filter adds half a sample
    let n = period.floor() as usize;
    let frac = period - n as f64;
    let ap = (1.0 - frac) / (1.0 + frac); // first-order allpass tuner
    let mut line = vec![0.0f64; n];
    let mut rng = Rng(seed | 1);
    // excitation: noise, lowpassed, pick comb at 1/5 of the string
    let mut lp = 0.0;
    let exc: Vec<f64> = (0..n)
        .map(|_| {
            lp += 0.5 * (rng.next() - lp);
            lp
        })
        .collect();
    let pick = (n / 5).max(1);
    for i in 0..n {
        line[i] = exc[i] - if i >= pick { exc[i - pick] } else { 0.0 };
    }
    let m = line.iter().map(|x| x.abs()).fold(0.0, f64::max).max(1e-9);
    for x in &mut line {
        *x *= amp / m;
    }
    let (mut idx, mut prev, mut ap_x1, mut ap_y1) = (0usize, 0.0f64, 0.0f64, 0.0f64);
    let len = (dur_s * SR) as usize;
    for t in 0..len {
        if start + t >= out.len() {
            break;
        }
        let x = line[idx];
        let lossed = decay * 0.5 * (x + prev);
        prev = x;
        let y = ap * lossed + ap_x1 - ap * ap_y1;
        ap_x1 = lossed;
        ap_y1 = y;
        line[idx] = y;
        idx = (idx + 1) % n;
        // release fade in the last 20 ms
        let rem = len - t;
        let g = if rem < 960 { rem as f64 / 960.0 } else { 1.0 };
        out[start + t] += x * g;
    }
}

struct Material {
    plucks: Vec<f64>,
    pluck_onsets: Vec<usize>,
    chord: Vec<f64>,
    tone: Vec<f64>,
    tone_f0: f64,
    tone_top: f64,
    di: Option<Vec<f64>>,
}

fn material(di_path: Option<&str>) -> Material {
    // Single plucked notes across the neck.
    let notes = [82.41, 110.0, 146.83, 196.0, 246.94, 329.63, 440.0, 659.26];
    let note_len = (0.9 * SR) as usize;
    let mut plucks = vec![0.0; note_len * notes.len() + (0.5 * SR) as usize];
    let mut pluck_onsets = vec![];
    for (i, &f) in notes.iter().enumerate() {
        let s = i * note_len + 2400;
        ks_pluck(&mut plucks, s, f, 0.85, 0.5, 0.996, 7 + i as u64);
        pluck_onsets.push(s);
    }
    // Strummed open E major, twice.
    let chord_f = [82.41, 123.47, 164.81, 207.65, 246.94, 329.63];
    let mut chord = vec![0.0; (4.5 * SR) as usize];
    for rep in 0..2 {
        for (j, &f) in chord_f.iter().enumerate() {
            let s = rep * (2.2 * SR) as usize + 2400 + j * 720;
            ks_pluck(
                &mut chord,
                s,
                f,
                2.0,
                0.25,
                0.998,
                100 + (rep * 10 + j) as u64,
            );
        }
    }
    // Steady harmonic tone (G3), partials to 12·f0, slow decay: the
    // ground-truth signal for the spectral metrics.
    let tone_f0 = 196.0;
    let tone = harmonic_tone(tone_f0);
    let di = di_path.and_then(|p| {
        let mut r = hound::WavReader::open(p).ok()?;
        let spec = r.spec();
        let ch = spec.channels as usize;
        let v: Vec<f64> = match spec.sample_format {
            hound::SampleFormat::Float => r
                .samples::<f32>()
                .filter_map(Result::ok)
                .map(f64::from)
                .collect(),
            hound::SampleFormat::Int => {
                let s = (1u64 << (spec.bits_per_sample - 1)) as f64;
                r.samples::<i32>()
                    .filter_map(Result::ok)
                    .map(|x| x as f64 / s)
                    .collect()
            }
        };
        Some(v.chunks(ch).map(|c| c[0]).collect())
    });
    Material {
        plucks,
        pluck_onsets,
        chord,
        tone,
        tone_f0,
        tone_top: tone_f0 * 12.0,
        di,
    }
}

/// The steady test tone at `f0` (partials below 20 kHz only) — also the
/// ideal output of a perfect shifter, for relative flux / AM.
fn harmonic_tone(f0: f64) -> Vec<f64> {
    let n = (4.0 * SR) as usize;
    (0..n)
        .map(|i| {
            let t = i as f64 / SR;
            let env = (-t / 3.0).exp() * (1.0 - (-t / 0.004).exp());
            let mut s = 0.0;
            for k in 1..=12 {
                let kf = k as f64;
                if f0 * kf < 20000.0 {
                    s += (2.0 * PI * f0 * kf * t + 0.7 * kf * kf).sin() / kf.powf(1.1);
                }
            }
            0.25 * env * s
        })
        .collect()
}

// ───────────────────────────── analysis ─────────────────────────────

struct Fft {
    planner: FftPlanner<f64>,
}
impl Fft {
    fn spectrum(&mut self, x: &[f64], n: usize) -> Vec<Complex<f64>> {
        let fft = self.planner.plan_fft_forward(n);
        let mut buf: Vec<Complex<f64>> = (0..n)
            .map(|i| Complex::new(if i < x.len() { x[i] } else { 0.0 }, 0.0))
            .collect();
        fft.process(&mut buf);
        buf
    }
}

/// FFT-based YIN. Returns (f0, aperiodicity) per hop.
fn yin_track(x: &[f64], fft: &mut Fft, hop: usize) -> Vec<(f64, f64)> {
    let w = 1600usize; // integration window
    let max_lag = (SR / 32.0) as usize; // 1500
    let min_lag = (SR / 1500.0) as usize;
    let frame = w + max_lag;
    let nfft = (2 * frame).next_power_of_two();
    let mut out = vec![];
    let mut pos = 0;
    let inv = fft.planner.plan_fft_inverse(nfft);
    while pos + frame <= x.len() {
        let seg = &x[pos..pos + frame];
        let energy: f64 = seg[..w].iter().map(|v| v * v).sum();
        if energy < 1e-7 * w as f64 {
            out.push((0.0, 1.0));
            pos += hop;
            continue;
        }
        // r(τ) = Σ_{j<w} x[j] x[j+τ] via cross-correlation of seg[..w] and seg.
        let a = fft.spectrum(&seg[..w], nfft);
        let b = fft.spectrum(seg, nfft);
        let mut c: Vec<Complex<f64>> = a.iter().zip(&b).map(|(p, q)| p.conj() * q).collect();
        inv.process(&mut c);
        let r: Vec<f64> = c
            .iter()
            .take(max_lag + 1)
            .map(|z| z.re / nfft as f64)
            .collect();
        // energy terms
        let mut cum = vec![0.0; frame + 1];
        for j in 0..frame {
            cum[j + 1] = cum[j] + seg[j] * seg[j];
        }
        let e0 = cum[w];
        let mut d = vec![0.0; max_lag + 1];
        for tau in 1..=max_lag {
            let et = cum[tau + w] - cum[tau];
            d[tau] = (e0 + et - 2.0 * r[tau]).max(0.0);
        }
        let mut cmnd = vec![1.0; max_lag + 1];
        let mut run = 0.0;
        for tau in 1..=max_lag {
            run += d[tau];
            cmnd[tau] = if run > 0.0 {
                d[tau] * tau as f64 / run
            } else {
                1.0
            };
        }
        let mut found = None;
        let mut tau = min_lag;
        while tau < max_lag {
            if cmnd[tau] < 0.15 {
                while tau + 1 < max_lag && cmnd[tau + 1] < cmnd[tau] {
                    tau += 1;
                }
                found = Some(tau);
                break;
            }
            tau += 1;
        }
        let tau = found.unwrap_or_else(|| {
            (min_lag..max_lag)
                .min_by(|&p, &q| cmnd[p].partial_cmp(&cmnd[q]).unwrap())
                .unwrap()
        });
        let (y0, y1, y2) = (cmnd[tau - 1], cmnd[tau], cmnd[tau + 1]);
        let den = y0 - 2.0 * y1 + y2;
        let shift = if den.abs() > 1e-12 {
            0.5 * (y0 - y2) / den
        } else {
            0.0
        };
        out.push((SR / (tau as f64 + shift.clamp(-1.0, 1.0)), cmnd[tau]));
        pos += hop;
    }
    out
}

/// RMS envelope, `win`-sample frames.
fn envelope(x: &[f64], win: usize) -> Vec<f64> {
    x.chunks(win)
        .map(|c| (c.iter().map(|v| v * v).sum::<f64>() / c.len() as f64).sqrt())
        .collect()
}

/// Latency (samples) by cross-correlating 1 ms log envelopes, searched
/// over 0..`max_ms`.
fn measure_latency(input: &[f64], output: &[f64], max_ms: usize) -> usize {
    let win = 48;
    let f = |x: &[f64]| -> Vec<f64> {
        let e = envelope(x, win);
        // onset-emphasis: positive log-envelope differences
        let l: Vec<f64> = e.iter().map(|v| (v + 1e-5).ln()).collect();
        (0..l.len())
            .map(|i| {
                if i == 0 {
                    0.0
                } else {
                    (l[i] - l[i - 1]).max(0.0)
                }
            })
            .collect()
    };
    let a = f(input);
    let b = f(output);
    let mut best = (0usize, f64::MIN);
    for lag in 0..max_ms {
        let mut s = 0.0;
        for i in 0..a.len().saturating_sub(lag) {
            if i + lag < b.len() {
                s += a[i] * b[i + lag];
            }
        }
        if s > best.1 {
            best = (lag, s);
        }
    }
    best.0 * win
}

struct PitchScore {
    errs: Vec<f64>,
}
impl PitchScore {
    fn add(&mut self, input: &[f64], output: &[f64], ratio: f64, lat: usize, fft: &mut Fft) {
        let hop = 480;
        let ti = yin_track(input, fft, hop);
        if output.len() <= lat {
            return;
        }
        let to = yin_track(&output[lat..], fft, hop);
        for i in 3..ti.len().saturating_sub(3) {
            let (f, ap) = ti[i];
            if f <= 0.0 || ap > 0.1 {
                continue;
            }
            // stable input pitch: neighbours within 10 cents, all voiced
            let stable = (i - 3..=i + 3).all(|j| {
                let (g, a2) = ti[j];
                g > 0.0 && a2 < 0.2 && (1200.0 * (g / f).log2()).abs() < 10.0
            });
            if !stable || i >= to.len() {
                continue;
            }
            let (fo, _) = to[i];
            let err = if fo > 0.0 {
                1200.0 * (fo / (f * ratio)).log2()
            } else {
                1200.0
            };
            self.errs.push(err.abs());
        }
    }
    fn median(&self) -> f64 {
        let mut e = self.errs.clone();
        if e.is_empty() {
            return f64::NAN;
        }
        e.sort_by(|a, b| a.partial_cmp(b).unwrap());
        e[e.len() / 2]
    }
    fn gross(&self) -> f64 {
        if self.errs.is_empty() {
            return f64::NAN;
        }
        100.0 * self.errs.iter().filter(|&&e| e > 50.0).count() as f64 / self.errs.len() as f64
    }
}

fn blackman_harris(n: usize) -> Vec<f64> {
    (0..n)
        .map(|i| {
            let x = 2.0 * PI * i as f64 / (n - 1) as f64;
            0.35875 - 0.48829 * x.cos() + 0.14128 * (2.0 * x).cos() - 0.01168 * (3.0 * x).cos()
        })
        .collect()
}

/// (nonharmonic dB, hf dB) on a 1 s steady window.
fn spectral_scores(seg: &[f64], f0: f64, top: f64, fft: &mut Fft) -> (f64, f64) {
    let n = 65536;
    let w = blackman_harris(seg.len());
    let x: Vec<f64> = seg.iter().zip(&w).map(|(a, b)| a * b).collect();
    let s = fft.spectrum(&x, n);
    let bin_hz = SR / n as f64;
    let (mut total, mut harm, mut hf) = (0.0, 0.0, 0.0);
    for (k, z) in s.iter().enumerate().take(n / 2).skip(1) {
        let f = k as f64 * bin_hz;
        if f < 30.0 || f > 20000.0 {
            continue;
        }
        let p = z.norm_sqr();
        total += p;
        let h = (f / f0).round();
        if h >= 1.0 && (f - h * f0).abs() < 6.0 {
            harm += p;
        }
        if f > top {
            hf += p;
        }
    }
    let db = |v: f64| 10.0 * (v.max(1e-20) / total.max(1e-20)).log10();
    (db(total - harm), db(hf))
}

/// De-trended log-envelope std (dB) — amplitude modulation.
fn am_score(seg: &[f64]) -> f64 {
    let e = envelope(seg, 480);
    let y: Vec<f64> = e.iter().map(|v| 20.0 * (v + 1e-9).log10()).collect();
    let n = y.len() as f64;
    let xm = (n - 1.0) / 2.0;
    let ym = y.iter().sum::<f64>() / n;
    let (mut sxy, mut sxx) = (0.0, 0.0);
    for (i, v) in y.iter().enumerate() {
        sxy += (i as f64 - xm) * (v - ym);
        sxx += (i as f64 - xm) * (i as f64 - xm);
    }
    let b = sxy / sxx;
    let r: f64 = y
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let d = v - (ym + b * (i as f64 - xm));
            d * d
        })
        .sum::<f64>()
        / n;
    r.sqrt()
}

/// Mean normalised spectral flux (×100), 2048 window, 256 hop.
fn flux_score(seg: &[f64], fft: &mut Fft) -> f64 {
    let n = 2048;
    let w = blackman_harris(n);
    let mut prev: Option<Vec<f64>> = None;
    let (mut acc, mut cnt) = (0.0, 0);
    let mut pos = 0;
    while pos + n <= seg.len() {
        let x: Vec<f64> = seg[pos..pos + n]
            .iter()
            .zip(&w)
            .map(|(a, b)| a * b)
            .collect();
        let m: Vec<f64> = fft
            .spectrum(&x, n)
            .iter()
            .take(n / 2)
            .map(|z| z.norm())
            .collect();
        let norm: f64 = m.iter().map(|v| v * v).sum::<f64>().sqrt();
        if let Some(p) = &prev {
            let pn: f64 = p.iter().map(|v| v * v).sum::<f64>().sqrt();
            let d: f64 = m
                .iter()
                .zip(p)
                .map(|(a, b)| {
                    let q = a / norm.max(1e-12) - b / pn.max(1e-12);
                    q * q
                })
                .sum::<f64>()
                .sqrt();
            acc += d;
            cnt += 1;
        }
        prev = Some(m);
        pos += 256;
    }
    100.0 * acc / cnt.max(1) as f64
}

/// (median rise ms, median pre-echo dB) over pluck onsets.
fn transient_scores(out: &[f64], onsets: &[usize], lat: usize) -> (f64, f64) {
    let env = envelope(out, 24); // 0.5 ms
    let mut rises = vec![];
    let mut pres = vec![];
    for &o in onsets {
        let t0 = (o + lat) / 24;
        let a = t0.saturating_sub(20);
        let b = (t0 + 200).min(env.len()); // +100 ms
        if b <= a + 10 {
            continue;
        }
        let win = &env[a..b];
        let (pk_i, pk) =
            win.iter().enumerate().fold(
                (0, 0.0f64),
                |acc, (i, &v)| if v > acc.1 { (i, v) } else { acc },
            );
        if pk <= 1e-6 {
            continue;
        }
        // walk back from peak to 10 %
        let mut i90 = pk_i;
        while i90 > 0 && win[i90] > 0.9 * pk {
            i90 -= 1;
        }
        let mut i10 = i90;
        while i10 > 0 && win[i10] > 0.1 * pk {
            i10 -= 1;
        }
        rises.push((i90 - i10) as f64 * 0.5);
        let e = |s: usize, e: usize| -> f64 {
            out[s.min(out.len())..e.min(out.len())]
                .iter()
                .map(|v| v * v)
                .sum()
        };
        let on = o + lat;
        let pre = e(on.saturating_sub(1440), on.saturating_sub(96));
        let post = e(on, on + 1440);
        pres.push(10.0 * ((pre + 1e-12) / (post + 1e-12)).log10());
    }
    let med = |mut v: Vec<f64>| -> f64 {
        if v.is_empty() {
            return f64::NAN;
        }
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[v.len() / 2]
    };
    (med(rises), med(pres))
}

fn rms(x: &[f64]) -> f64 {
    (x.iter().map(|v| v * v).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

fn run(
    make: fn(f64) -> Option<Box<dyn Shifter>>,
    ratio: f64,
    x: &[f64],
) -> Option<(Vec<f64>, usize, f64)> {
    let mut s = make(ratio)?;
    let lat = s.latency();
    let t = Instant::now();
    let y: Vec<f64> = x.iter().map(|&v| s.tick(v)).collect();
    let ns = t.elapsed().as_nanos() as f64 / x.len() as f64;
    Some((y, lat, ns))
}

fn write_wav(path: &str, x: &[f64]) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SR as u32,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for &v in x {
        w.write_sample(v as f32).unwrap();
    }
    w.finalize().unwrap();
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out_dir = args.get(1).cloned().filter(|s| !s.is_empty());
    let di_path = args.get(2).cloned();
    let only: Option<String> = std::env::var("PQ_ONLY").ok();
    let m = material(di_path.as_deref());
    let mut fft = Fft {
        planner: FftPlanner::new(),
    };
    if let Some(d) = &out_dir {
        std::fs::create_dir_all(d).ok();
        write_wav(&format!("{d}/in_plucks.wav"), &m.plucks);
        write_wav(&format!("{d}/in_chord.wav"), &m.chord);
        write_wav(&format!("{d}/in_tone.wav"), &m.tone);
    }
    let intervals = [12.0, -12.0, 7.0, 5.0];
    println!(
        "| {:<18} | {:>4} | {:>6} | {:>6} | {:>6} | {:>7} | {:>6} | {:>7} | {:>7} | {:>6} | {:>7} | {:>13} | {:>6} |",
        "shifter", "st", "cents", "gross%", "xam dB", "nonh dB", "xflux", "hf dB", "rise ms", "pre dB", "gain dB", "lat ms meas/rep", "ns/smp"
    );
    println!("|{}|", "-".repeat(150));
    for c in candidates() {
        if let Some(o) = &only {
            if !o.split(',').any(|p| c.name.starts_with(p)) && c.name != "dry(ref)" {
                continue;
            }
        }
        for &st in &intervals {
            let ratio = (st / 12.0f64).exp2();
            let eff_ratio = if c.name == "dry(ref)" { 1.0 } else { ratio };
            let Some((yp, lat_rep, ns1)) = run(c.make, ratio, &m.plucks) else {
                continue;
            };
            // Search up to the reported latency + 30 ms (a shifter whose heads
            // wander has a mean delay below its reported bound); 150 ms when
            // the shifter reports none.
            let max_ms = if lat_rep > 0 { lat_rep / 48 + 30 } else { 150 };
            let lat = if c.name == "dry(ref)" {
                0
            } else {
                measure_latency(&m.plucks, &yp, max_ms)
            };
            let (yc, _, ns2) = run(c.make, ratio, &m.chord).unwrap();
            let (yt, _, ns3) = run(c.make, ratio, &m.tone).unwrap();
            let mut ps = PitchScore { errs: vec![] };
            ps.add(&m.plucks, &yp, eff_ratio, lat, &mut fft);
            let mut ns = ns1 + ns2 + ns3;
            let mut nsn = 3.0;
            let mut ydi = None;
            if let Some(di) = &m.di {
                let (yd, _, ns4) = run(c.make, ratio, di).unwrap();
                ps.add(di, &yd, eff_ratio, lat, &mut fft);
                ns += ns4;
                nsn += 1.0;
                ydi = Some(yd);
            }
            // steady window on the tone: 1.5 s .. 2.5 s after latency
            let s0 = (1.5 * SR) as usize + lat;
            let s1 = s0 + SR as usize;
            let seg = &yt[s0.min(yt.len())..s1.min(yt.len())];
            let (nonh, hf) = spectral_scores(
                seg,
                m.tone_f0 * eff_ratio,
                m.tone_top * eff_ratio * 1.1 + 150.0,
                &mut fft,
            );
            // AM and flux relative to the ideal shifted tone.
            let ideal = harmonic_tone(m.tone_f0 * eff_ratio);
            let iseg = &ideal[(1.5 * SR) as usize..(2.5 * SR) as usize];
            let am = am_score(seg) - am_score(iseg);
            let flux = flux_score(seg, &mut fft) - flux_score(iseg, &mut fft);
            let (rise, pre) = transient_scores(&yp, &m.pluck_onsets, lat);
            let gain = 20.0 * (rms(&yc[lat..]) / rms(&m.chord[..m.chord.len() - lat])).log10();
            println!(
                "| {:<18} | {:>+4} | {:>6.1} | {:>6.1} | {:>6.2} | {:>7.1} | {:>6.2} | {:>7.1} | {:>7.1} | {:>6.1} | {:>+7.2} | {:>6.1}/{:>6.1} | {:>6.0} |",
                c.name,
                st as i32,
                ps.median(),
                ps.gross(),
                am,
                nonh,
                flux,
                hf,
                rise,
                pre,
                gain,
                lat as f64 / 48.0,
                lat_rep as f64 / 48.0,
                ns / nsn
            );
            if let Some(d) = &out_dir {
                if c.name != "dry(ref)" {
                    let tag = format!("{}_{:+}", c.name.replace(['(', ')', ' '], ""), st as i32);
                    write_wav(&format!("{d}/{tag}_plucks.wav"), &yp);
                    write_wav(&format!("{d}/{tag}_chord.wav"), &yc);
                    if let Some(yd) = &ydi {
                        write_wav(&format!("{d}/{tag}_di.wav"), yd);
                    }
                }
            }
            if c.name == "dry(ref)" {
                break;
            }
        }
    }
}
