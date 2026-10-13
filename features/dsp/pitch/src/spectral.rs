//! Phase-locked phase-vocoder pitch shifter (Laroche–Dolson peak shifting).
//!
//! Every analysis frame is split into *regions of influence* around the
//! spectral peaks. Each region is moved, as a block, to where its peak
//! belongs after the shift, and all its bins are rotated by one common
//! phase `θ` that accumulates `(α − 1)·ω·hop` per frame (α = pitch ratio,
//! ω = the peak's instantaneous frequency). Because the bins of a region
//! keep their input phase relationships (identity phase locking), there is
//! no phasiness, no resampling, no grain crossfade and so no grain-rate
//! amplitude modulation — the warble that time-domain shifters put into a
//! feedback loop. See J. Laroche & M. Dolson, *New phase-vocoder techniques
//! for pitch-shifting, harmonizing and other exotic effects*, WASPAA 1999.
//!
//! Refinements over the paper:
//! - **Centre-referenced phases** (zero-phase framing). The integer bin shift
//!   leaves each region up to ½ bin away from its exact target; with phases
//!   referenced to the frame centre that error is zero where the window
//!   weight is largest instead of growing across the frame.
//! - **Per-region phase reset at onsets** (after Röbel / Duxbury): a region
//!   whose energy jumps within one hop restarts `θ` at zero, so a pick
//!   attack keeps its vertical phase coherence (a sharp transient) while
//!   partials that are still ringing elsewhere keep their continuity.
//! - Peak tracking for `θ`: a peak inherits the rotation of the region that
//!   held its bin in the previous frame.
//!
//! Real-time: every buffer is allocated in `new()` for the largest frame;
//! `tick()` never allocates, locks or branches on anything but the hop
//! counter. The FFT work happens once per hop (`fft_size / overlap`).
//!
//! Latency: exactly `fft_size` samples (`latency()`).
//! Loop gain: the shifted spectrum keeps each region's magnitudes, so the
//! output level equals the input level on tonal material (≤ unity on
//! broadband material) — safe inside feedback loops.

use std::f64::consts::PI;
use std::sync::Arc;

use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

/// Largest supported frame (samples).
pub const MAX_FFT: usize = 8192;
/// Smallest supported frame (samples).
pub const MIN_FFT: usize = 256;

struct Plan {
    n: usize,
    fwd: Arc<dyn RealToComplex<f64>>,
    inv: Arc<dyn ComplexToReal<f64>>,
}

/// Every frame size's FFT plans, and the scratch the largest needs — built
/// once and shared by every shifter. A plan is immutable (each call brings
/// its own scratch), and a rig holds a hundred shifters: each building its
/// own twiddle tables was ~0.4 MB apiece.
fn shared_plans() -> &'static (Vec<Plan>, usize) {
    static PLANS: std::sync::OnceLock<(Vec<Plan>, usize)> = std::sync::OnceLock::new();
    PLANS.get_or_init(|| {
        let mut planner = RealFftPlanner::<f64>::new();
        let mut plans = Vec::new();
        let mut n = MIN_FFT;
        let mut max_scratch = 0;
        while n <= MAX_FFT {
            let fwd = planner.plan_fft_forward(n);
            let inv = planner.plan_fft_inverse(n);
            max_scratch = max_scratch
                .max(fwd.get_scratch_len())
                .max(inv.get_scratch_len());
            plans.push(Plan { n, fwd, inv });
            n *= 2;
        }
        (plans, max_scratch)
    })
}

/// Phase-locked phase-vocoder pitch shifter.
pub struct SpectralShifter {
    /// Pitch ratio: 0.5 = octave down, 2.0 = octave up.
    pub speed: f64,
    /// Mix: 0.0 = dry only, 1.0 = wet only.
    pub mix: f64,
    /// Frame size in samples (power of two, `MIN_FFT..=MAX_FFT`), scaled from
    /// 48 kHz by `update()`. Applied on `update()`.
    pub fft_size: usize,
    /// Frames per window (4 or 8). Applied on `update()`.
    pub overlap: usize,
    /// Restart the phase rotation of regions that jump in energy (keeps pick
    /// attacks sharp).
    pub transient_reset: bool,
    /// Line broadening (Hz, FWHM; 0 = off). Each region's phase takes a
    /// random walk, turning every shifted partial into a narrow Lorentzian
    /// band this wide — no pitch drift, no periodic modulation. Inside a
    /// reverb loop it keeps a pure line from parking on one sharp tank mode
    /// (whose gain can sit far above the tank's average) and lets the loop
    /// see the average gain its stability bound assumes. A few Hz reads as
    /// a faint ensemble; the grain shifters it replaces smeared ±20–40 Hz.
    pub line_width_hz: f64,

    plans: &'static [Plan],
    plan_idx: usize,
    n: usize,
    hop: usize,
    bins: usize,

    window: Vec<f64>,
    /// Synthesis gain: 1 / (N · Σ window² over the overlapping frames).
    out_gain: f64,

    in_ring: Vec<f64>,
    in_pos: usize,
    hop_count: usize,

    frame: Vec<f64>,
    spec: Vec<Complex<f64>>,
    out_spec: Vec<Complex<f64>>,
    scratch_fwd: Vec<Complex<f64>>,
    scratch_inv: Vec<Complex<f64>>,

    /// |X|² of the current / previous frame.
    pow: Vec<f64>,
    prev_pow: Vec<f64>,
    /// Previous frame's (centre-referenced) spectrum, for phase advance.
    prev_spec: Vec<Complex<f64>>,
    /// Rotation carried by each input bin's region in the previous frame.
    theta_prev: Vec<f64>,
    theta_cur: Vec<f64>,
    peaks: Vec<usize>,
    rng: u64,

    ola: Vec<f64>,
    out_block: Vec<f64>,
    out_idx: usize,

    sample_rate: f64,
    /// The largest frame the buffers hold (a power of two, `MIN_FFT..=MAX_FFT`);
    /// a frame asked for beyond it is clamped to it.
    cap: usize,
}

impl SpectralShifter {
    /// Buffers for every frame up to [`MAX_FFT`].
    pub fn new() -> Self {
        Self::with_max_frame(MAX_FFT)
    }

    /// The frame `fft_size` (a 48 kHz length) becomes at `sample_rate`.
    #[must_use]
    pub fn frame_for(fft_size: usize, sample_rate: f64) -> usize {
        let want = (fft_size as f64 * sample_rate / 48000.0).max(1.0);
        let mut n = MIN_FFT;
        while n < MAX_FFT && (n as f64) * 1.414 < want {
            n *= 2;
        }
        n
    }

    /// The largest frame this shifter's buffers hold.
    #[must_use]
    pub const fn max_frame(&self) -> usize {
        self.cap
    }

    /// Buffers for frames up to `max` (rounded up to a power of two): a
    /// shifter that only ever runs 2048-sample frames holds a quarter of what
    /// `new` does — a guitar rig carries a hundred.
    pub fn with_max_frame(max: usize) -> Self {
        let cap = max.clamp(MIN_FFT, MAX_FFT).next_power_of_two().min(MAX_FFT);
        let (plans, _) = shared_plans();
        let plans = plans.as_slice();
        let max_scratch = plans
            .iter()
            .filter(|p| p.n <= cap)
            .map(|p| p.fwd.get_scratch_len().max(p.inv.get_scratch_len()))
            .max()
            .unwrap_or(0);
        let bins = cap / 2 + 1;
        let zero = Complex::new(0.0, 0.0);
        let mut s = Self {
            speed: 2.0,
            mix: 1.0,
            fft_size: 4096,
            overlap: 8,
            transient_reset: true,
            line_width_hz: 0.0,
            plans,
            plan_idx: 0,
            n: 4096,
            hop: 512,
            bins: 4096 / 2 + 1,
            window: vec![0.0; cap],
            out_gain: 0.0,
            in_ring: vec![0.0; cap],
            in_pos: 0,
            hop_count: 0,
            frame: vec![0.0; cap],
            spec: vec![zero; bins],
            out_spec: vec![zero; bins],
            scratch_fwd: vec![zero; max_scratch],
            scratch_inv: vec![zero; max_scratch],
            pow: vec![0.0; bins],
            prev_pow: vec![0.0; bins],
            prev_spec: vec![zero; bins],
            theta_prev: vec![0.0; bins],
            theta_cur: vec![0.0; bins],
            peaks: Vec::with_capacity(bins),
            rng: 0x9E37_79B9_7F4A_7C15,
            ola: vec![0.0; cap],
            out_block: vec![0.0; cap],
            out_idx: 0,
            sample_rate: 48000.0,
            cap,
        };
        s.configure();
        s
    }

    /// Apply `fft_size` / `overlap` for `sample_rate` and clear state.
    /// Never allocates (the buffers hold [`max_frame`](Self::max_frame), and a
    /// frame is never larger).
    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.configure();
    }

    fn configure(&mut self) {
        // Scale the 48 kHz frame to the running rate, snapped to a power of two.
        let n = Self::frame_for(self.fft_size, self.sample_rate).min(self.cap);
        self.n = n;
        self.plan_idx = self.plans.iter().position(|p| p.n == n).unwrap_or(0);
        let overlap = if self.overlap >= 8 { 8 } else { 4 };
        self.hop = n / overlap;
        self.bins = n / 2 + 1;
        // Periodic Hann, used for analysis and synthesis. Σ w² over frames
        // spaced n/overlap apart = 3·overlap/8.
        for i in 0..n {
            let w = 0.5 - 0.5 * (2.0 * PI * i as f64 / n as f64).cos();
            self.window[i] = w;
        }
        self.out_gain = 1.0 / (n as f64 * 3.0 * overlap as f64 / 8.0);
        self.clear();
    }

    fn clear(&mut self) {
        self.in_ring.fill(0.0);
        self.ola.fill(0.0);
        self.out_block.fill(0.0);
        self.prev_pow.fill(0.0);
        self.prev_spec.fill(Complex::new(0.0, 0.0));
        self.theta_prev.fill(0.0);
        self.rng = 0x9E37_79B9_7F4A_7C15;
        self.in_pos = 0;
        self.hop_count = 0;
        self.out_idx = 0;
    }

    pub fn reset(&mut self) {
        self.clear();
    }

    /// Latency in samples (the frame length).
    pub fn latency(&self) -> usize {
        self.n
    }

    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        let n = self.n;
        self.in_ring[self.in_pos] = input;
        self.in_pos = (self.in_pos + 1) & (n - 1);
        let wet = self.out_block[self.out_idx];
        self.out_idx += 1;
        self.hop_count += 1;
        if self.hop_count >= self.hop {
            self.hop_count = 0;
            self.out_idx = 0;
            self.process_frame();
        }
        input * (1.0 - self.mix) + wet * self.mix
    }

    fn process_frame(&mut self) {
        let n = self.n;
        let hop = self.hop;
        let bins = self.bins;
        let alpha = self.speed.clamp(0.25, 4.0);
        // Phase random-walk step for the requested line width: a walk with
        // per-hop variance σ² gives a Lorentzian of FWHM σ²/(2π·T_hop).
        let hop_s = hop as f64 / self.sample_rate;
        let jitter = (self.line_width_hz.max(0.0) * 2.0 * PI * hop_s).sqrt();
        // Uniform on ±√3·σ has variance σ².
        let jitter_span = jitter * 3f64.sqrt();

        // ── analysis ──
        for i in 0..n {
            self.frame[i] = self.in_ring[(self.in_pos + i) & (n - 1)] * self.window[i];
        }
        let plan = &self.plans[self.plan_idx];
        let _ = plan.fwd.process_with_scratch(
            &mut self.frame[..n],
            &mut self.spec[..bins],
            &mut self.scratch_fwd,
        );
        let mut max_pow = 0.0f64;
        for k in 0..bins {
            // (−1)^k: phases referenced to the frame centre.
            if k & 1 == 1 {
                self.spec[k] = -self.spec[k];
            }
            let p = self.spec[k].norm_sqr();
            self.pow[k] = p;
            max_pow = max_pow.max(p);
        }
        for z in &mut self.out_spec[..bins] {
            *z = Complex::new(0.0, 0.0);
        }

        if max_pow > 1e-18 {
            // Unity: pass the frame through untouched (exact reconstruction).
            let unity = (alpha - 1.0).abs() < 1e-9 && self.line_width_hz <= 0.0;

            // ── peaks: larger than the four nearest neighbours ──
            self.peaks.clear();
            let floor = max_pow * 1e-10; // −100 dB re the frame maximum
            let pw = &self.pow;
            for k in 2..bins.saturating_sub(2) {
                let m = pw[k];
                if m > floor && m > pw[k - 1] && m >= pw[k + 1] && m > pw[k - 2] && m >= pw[k + 2] {
                    self.peaks.push(k);
                }
            }

            let expected_adv = 2.0 * PI * hop as f64 / n as f64; // per bin
            let np = self.peaks.len();
            for pi in 0..np {
                let k = self.peaks[pi];
                // Region of influence: from the lowest bin between this peak
                // and the previous one, to the lowest bin before the next.
                let lo = if pi == 0 {
                    0
                } else {
                    let a = self.peaks[pi - 1];
                    let mut lo = a + 1;
                    for j in a + 1..k {
                        if self.pow[j] < self.pow[lo] {
                            lo = j;
                        }
                    }
                    lo
                };
                let hi = if pi + 1 == np {
                    bins - 1
                } else {
                    let b = self.peaks[pi + 1];
                    let mut hi = k + 1;
                    for j in k + 1..b {
                        if self.pow[j] <= self.pow[hi] {
                            hi = j;
                        }
                    }
                    hi.saturating_sub(1).max(k)
                };

                // Instantaneous frequency (bins) from the phase advance,
                // falling back to parabolic interpolation when the phase
                // history is meaningless (onset / first frame).
                let adv = (self.spec[k] * self.prev_spec[k].conj()).arg();
                let dphi = princarg(adv - expected_adv * k as f64);
                let mut dev = dphi / expected_adv;
                if dev.abs() > 1.0 {
                    // (ln |X|² — the same vertex as ln |X|)
                    let (a, b, c) = (
                        (self.pow[k - 1] + 1e-60).ln(),
                        (self.pow[k] + 1e-60).ln(),
                        (self.pow[k + 1] + 1e-60).ln(),
                    );
                    let den = a - 2.0 * b + c;
                    dev = if den.abs() > 1e-12 {
                        (0.5 * (a - c) / den).clamp(-0.5, 0.5)
                    } else {
                        0.0
                    };
                }
                let f_bins = k as f64 + dev;

                // Onset in this region? (energy up > 6 dB within one hop)
                let mut e_now = 0.0;
                let mut e_prev = 0.0;
                for j in lo..=hi {
                    e_now += self.pow[j];
                    e_prev += self.prev_pow[j];
                }
                let onset = self.transient_reset && e_now > 4.0 * e_prev + 1e-18;

                let mut theta = if unity {
                    0.0
                } else if onset {
                    0.0
                } else {
                    let omega = f_bins * 2.0 * PI / n as f64; // rad / sample
                    princarg(self.theta_prev[k] + (alpha - 1.0) * omega * hop as f64)
                };
                if jitter_span > 0.0 {
                    self.rng ^= self.rng << 13;
                    self.rng ^= self.rng >> 7;
                    self.rng ^= self.rng << 17;
                    let u = (self.rng >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0;
                    theta = princarg(theta + u * jitter_span);
                }
                for j in lo..=hi {
                    self.theta_cur[j] = theta;
                }

                // Integer shift that lands the peak nearest its target.
                let shift = if unity {
                    0
                } else {
                    (f_bins * alpha - f_bins).round() as isize
                };
                let rot = Complex::new(theta.cos(), theta.sin());
                for j in lo..=hi {
                    let t = j as isize + shift;
                    if t < 0 || t >= bins as isize {
                        continue;
                    }
                    self.out_spec[t as usize] += self.spec[j] * rot;
                }
            }
        } else {
            for j in 0..bins {
                self.theta_cur[j] = 0.0;
            }
        }

        // Remember this frame's phases / magnitudes / rotations.
        self.prev_spec[..bins].copy_from_slice(&self.spec[..bins]);
        self.prev_pow[..bins].copy_from_slice(&self.pow[..bins]);
        self.theta_prev[..bins].copy_from_slice(&self.theta_cur[..bins]);

        // ── synthesis ──
        for k in 0..bins {
            if k & 1 == 1 {
                self.out_spec[k] = -self.out_spec[k];
            }
        }
        self.out_spec[0].im = 0.0;
        self.out_spec[bins - 1].im = 0.0;
        let plan = &self.plans[self.plan_idx];
        let _ = plan.inv.process_with_scratch(
            &mut self.out_spec[..bins],
            &mut self.frame[..n],
            &mut self.scratch_inv,
        );
        let g = self.out_gain;
        for i in 0..n {
            self.ola[i] += self.frame[i] * self.window[i] * g;
        }
        self.out_block[..hop].copy_from_slice(&self.ola[..hop]);
        self.ola.copy_within(hop..n, 0);
        self.ola[n - hop..n].fill(0.0);
    }
}

impl Default for SpectralShifter {
    fn default() -> Self {
        Self::new()
    }
}

#[inline]
fn princarg(x: f64) -> f64 {
    x - 2.0 * PI * ((x + PI) / (2.0 * PI)).floor()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    fn make(speed: f64, n: usize) -> SpectralShifter {
        let mut s = SpectralShifter::new();
        s.speed = speed;
        s.fft_size = n;
        s.update(SR);
        s
    }

    fn goertzel(x: &[f64], f: f64) -> f64 {
        let w = 2.0 * PI * f / SR;
        let c = 2.0 * w.cos();
        let (mut s1, mut s2) = (0.0, 0.0);
        for &v in x {
            let s0 = v + c * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        (s1 * s1 + s2 * s2 - c * s1 * s2) / x.len() as f64
    }

    #[test]
    fn unity_reconstructs_exactly_after_latency() {
        let mut s = make(1.0, 2048);
        let lat = s.latency();
        let x: Vec<f64> = (0..20000)
            .map(|i| ((i * 7919) % 101) as f64 / 101.0 - 0.5)
            .collect();
        let y: Vec<f64> = x.iter().map(|&v| s.tick(v)).collect();
        for i in 2 * lat..x.len() {
            assert!(
                (y[i] - x[i - lat]).abs() < 1e-9,
                "sample {i}: {} vs {}",
                y[i],
                x[i - lat]
            );
        }
    }

    #[test]
    fn silence_in_silence_out() {
        let mut s = make(2.0, 4096);
        for _ in 0..20000 {
            assert!(s.tick(0.0).abs() < 1e-12);
        }
    }

    #[test]
    fn octave_up_and_down_land_on_target_at_unity_gain() {
        for (speed, f_in) in [(2.0, 220.0), (0.5, 440.0), (1.4983, 330.0)] {
            let mut s = make(speed, 4096);
            let x: Vec<f64> = (0..96000)
                .map(|i| (2.0 * PI * f_in * i as f64 / SR).sin() * 0.5)
                .collect();
            let y: Vec<f64> = x.iter().map(|&v| s.tick(v)).collect();
            let tail = &y[48000..];
            let target = goertzel(tail, f_in * speed);
            let orig = goertzel(tail, f_in);
            assert!(
                target > 1000.0 * orig,
                "speed {speed}: target {target:e} orig {orig:e}"
            );
            let rms_in = (x[48000..].iter().map(|v| v * v).sum::<f64>() / 48000.0).sqrt();
            let rms_out = (tail.iter().map(|v| v * v).sum::<f64>() / 48000.0).sqrt();
            let db = 20.0 * (rms_out / rms_in).log10();
            assert!(db.abs() < 0.5, "speed {speed}: gain {db:.2} dB");
        }
    }

    #[test]
    fn line_broadening_keeps_level_and_centre() {
        let run = |width: f64| -> (f64, f64) {
            let mut s = make(2.0, 2048);
            s.line_width_hz = width;
            let x: Vec<f64> = (0..144000)
                .map(|i| (2.0 * PI * 440.0 * i as f64 / SR).sin() * 0.5)
                .collect();
            let y: Vec<f64> = x.iter().map(|&v| s.tick(v)).collect();
            let tail = &y[48000..];
            let rms = (tail.iter().map(|v| v * v).sum::<f64>() / tail.len() as f64).sqrt();
            // share of the power within ±1 Hz of the exact octave
            let on = goertzel(tail, 880.0);
            (20.0 * (rms / (0.5 / 2f64.sqrt())).log10(), on / (rms * rms))
        };
        let (g0, c0) = run(0.0);
        let (g6, c6) = run(6.0);
        assert!(
            g0.abs() < 0.5 && g6.abs() < 1.5,
            "levels {g0:.2} / {g6:.2} dB"
        );
        assert!(
            c6 < 0.5 * c0,
            "a 6 Hz line should spread off the exact bin: {c6:.3} vs {c0:.3}"
        );
    }

    #[test]
    fn bounded_on_noise_never_above_unity() {
        let mut s = make(2.0, 2048);
        let mut st = 0x1234_5678_9abc_def1u64;
        let (mut ein, mut eout) = (0.0, 0.0);
        for i in 0..96000 {
            st ^= st << 13;
            st ^= st >> 7;
            st ^= st << 17;
            let x = (st >> 11) as f64 / (1u64 << 53) as f64 - 0.5;
            let y = s.tick(x);
            assert!(y.is_finite());
            if i > 4096 {
                ein += x * x;
                eout += y * y;
            }
        }
        assert!(
            eout <= ein * 1.05,
            "noise gain {:.2} dB",
            10.0 * (eout / ein).log10()
        );
    }

    #[test]
    fn reconfigure_does_not_allocate_larger_than_max() {
        let mut s = make(2.0, 1 << 20);
        assert_eq!(s.latency(), MAX_FFT);
        s.fft_size = 1;
        s.update(SR);
        assert_eq!(s.latency(), MIN_FFT);
    }
}
