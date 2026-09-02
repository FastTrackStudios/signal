//! The Farina exponential swept sine: linear response and every harmonic
//! order, from one pass.
//!
//! [`crate::harmonics`] answers "how much distortion, at this frequency, at
//! this level" — one point at a time. To characterise a saturating device
//! across the band that way costs a render per frequency, and still says
//! nothing about the *phase* of what it added.
//!
//! An exponential sine sweep answers the whole question at once. Because its
//! instantaneous frequency rises exponentially, a harmonic generated at time
//! `t` corresponds to a fundamental the device will not reach until later —
//! so when the response is deconvolved against the sweep's inverse filter,
//! the harmonic orders separate **in time**, arriving as a train of impulse
//! responses ahead of the linear one. Farina's method (AES 108, 2000).
//!
//! What that buys, concretely: the linear impulse response is the device's
//! frequency response with the distortion removed, and the *n*-th harmonic
//! impulse response is the amplitude and phase of the *n*-th order
//! nonlinearity as a function of frequency. Those are exactly the
//! coefficients a parallel Hammerstein model needs — a static polynomial per
//! branch followed by a linear filter — which is a structure our own DSP can
//! evaluate directly. So this is a measurement that can be *played back*,
//! not merely plotted.
//!
//! # The catch, for compressors specifically
//!
//! The method assumes the device is nonlinear but **time-invariant**. A
//! compressor is neither: its gain moves while the sweep passes, and a slow
//! release smears that movement across octaves. Worse, at low frequencies a
//! fast detector tracks *within* each cycle, producing harmonic content that
//! looks exactly like saturation and is not — it is envelope ripple.
//!
//! The two are separable, and [`crate::harmonics`] plus this module together
//! are how: true saturation is instantaneous and does not care about the
//! attack and release settings, while detector ripple moves with them. Sweep
//! the same device at several release times and whatever stays put is the
//! static nonlinearity. Measuring a compressor's saturation with its detector
//! active and calling the result a saturation curve is the single easiest way
//! to fit a model to the wrong thing.

use realfft::RealFftPlanner;
use serde::{Deserialize, Serialize};

/// A sweep's parameters. `duration_s` dominates the separation between
/// harmonic orders — too short and the harmonic impulse responses overlap the
/// linear one and cannot be windowed apart.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SweepSpec {
    pub start_hz: f64,
    pub end_hz: f64,
    pub duration_s: f64,
    /// Peak amplitude in dBFS. The nonlinearity being measured is
    /// level-dependent, so this is a measurement condition, not a detail —
    /// sweep at several levels to trace how the distortion grows.
    pub level_db: f64,
}

impl Default for SweepSpec {
    fn default() -> Self {
        Self { start_hz: 20.0, end_hz: 20_000.0, duration_s: 8.0, level_db: -12.0 }
    }
}

impl SweepSpec {
    pub fn samples(&self, sample_rate: f64) -> usize {
        (self.duration_s * sample_rate).max(1.0) as usize
    }

    /// `L`, the sweep rate constant: seconds per neper of frequency.
    ///
    /// The *n*-th harmonic arrives `L·ln(n)` seconds *before* the linear
    /// impulse, which is the whole basis of the separation.
    pub fn rate(&self) -> f64 {
        self.duration_s / (self.end_hz / self.start_hz).ln()
    }

    /// How far ahead of the linear impulse the `n`-th harmonic lands, in
    /// seconds. `n = 1` is the linear response itself, at zero.
    pub fn harmonic_lead_s(&self, n: usize) -> f64 {
        self.rate() * (n as f64).ln()
    }
}

/// Generate the sweep.
pub fn sweep(spec: &SweepSpec, sample_rate: f64) -> Vec<f32> {
    let n = spec.samples(sample_rate);
    let amp = 10.0f64.powf(spec.level_db / 20.0);
    let l = spec.rate();
    (0..n)
        .map(|i| {
            let t = i as f64 / sample_rate;
            let phase = std::f64::consts::TAU * spec.start_hz * l * ((t / l).exp() - 1.0);
            (amp * phase.sin()) as f32
        })
        .collect()
}

/// The inverse filter: the sweep reversed, with a `-6 dB/octave` amplitude
/// envelope that undoes the sweep's own pink spectrum.
///
/// Convolving the sweep with this yields a delta function; convolving the
/// *response* with it yields the impulse response train.
pub fn inverse_filter(spec: &SweepSpec, sample_rate: f64) -> Vec<f32> {
    let s = sweep(spec, sample_rate);
    let n = s.len();
    let l = spec.rate();
    // Amplitude correction: the exponential sweep spends equal time per
    // octave, so it delivers 3 dB less energy per octave as frequency rises.
    // e^{-t/L} restores a flat deconvolution.
    (0..n)
        .map(|i| {
            let t = i as f64 / sample_rate;
            let gain = (-t / l).exp();
            (s[n - 1 - i] as f64 * gain) as f32
        })
        .collect()
}

/// The deconvolved impulse-response train: harmonics first, linear last.
///
/// Index `linear_index` is the linear impulse; the `n`-th harmonic sits
/// `harmonic_lead_s(n)` seconds earlier.
#[derive(Debug, Clone)]
pub struct Deconvolved {
    pub samples: Vec<f32>,
    pub linear_index: usize,
    pub sample_rate: f64,
}

/// Deconvolve a rendered sweep response against the sweep's inverse filter.
///
/// Uses FFT convolution — the buffers are hundreds of thousands of samples
/// and direct convolution is quadratic.
pub fn deconvolve(response: &[f32], spec: &SweepSpec, sample_rate: f64) -> Deconvolved {
    let inv = inverse_filter(spec, sample_rate);
    let n = (response.len() + inv.len()).next_power_of_two();

    let mut planner = RealFftPlanner::<f64>::new();
    let fwd = planner.plan_fft_forward(n);
    let inv_fft = planner.plan_fft_inverse(n);

    let mut a = vec![0.0f64; n];
    let mut b = vec![0.0f64; n];
    for (i, v) in response.iter().enumerate() {
        a[i] = *v as f64;
    }
    for (i, v) in inv.iter().enumerate() {
        b[i] = *v as f64;
    }

    let mut fa = fwd.make_output_vec();
    let mut fb = fwd.make_output_vec();
    fwd.process(&mut a, &mut fa).expect("fft");
    fwd.process(&mut b, &mut fb).expect("fft");
    for (x, y) in fa.iter_mut().zip(fb.iter()) {
        *x *= *y;
    }
    let mut out = vec![0.0f64; n];
    inv_fft.process(&mut fa, &mut out).expect("ifft");

    let scale = 1.0 / n as f64;
    let samples: Vec<f32> = out.iter().map(|v| (v * scale) as f32).collect();

    // The linear impulse lands where the sweep and its reverse align — at the
    // end of the inverse filter.
    Deconvolved { samples, linear_index: inv.len().saturating_sub(1), sample_rate }
}

impl Deconvolved {
    /// Cut out the `n`-th order impulse response. `n = 1` is the linear one.
    ///
    /// The window runs from halfway to the neighbouring order on each side, so
    /// orders cannot bleed into one another. Higher orders crowd together —
    /// the spacing goes as `ln(n)` — which is why a longer sweep is needed to
    /// reach far up the series.
    pub fn order(&self, n: usize, spec: &SweepSpec) -> &[f32] {
        let sr = self.sample_rate;
        let at = |k: usize| -> f64 {
            self.linear_index as f64 - spec.harmonic_lead_s(k) * sr
        };
        let centre = at(n.max(1));
        // Halfway to the next order up (earlier) and the previous (later).
        let earlier = at(n + 1);
        let lo = ((centre + earlier) / 2.0).max(0.0) as usize;
        let hi = if n <= 1 {
            self.samples.len()
        } else {
            let later = at(n - 1);
            (((centre + later) / 2.0) as usize).min(self.samples.len())
        };
        if lo >= hi {
            return &[];
        }
        &self.samples[lo..hi]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48_000.0;

    fn spec() -> SweepSpec {
        SweepSpec { start_hz: 50.0, end_hz: 12_000.0, duration_s: 4.0, level_db: 0.0 }
    }

    fn peak_index(x: &[f32]) -> usize {
        x.iter()
            .enumerate()
            .max_by(|a, b| a.1.abs().partial_cmp(&b.1.abs()).unwrap())
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    #[test]
    fn the_sweep_rises_from_start_to_end() {
        let s = spec();
        let x = sweep(&s, SR);
        assert_eq!(x.len(), 4 * 48_000);
        let crossings = |b: &[f32]| b.windows(2).filter(|w| w[0].signum() != w[1].signum()).count();
        let n = x.len();
        let head = crossings(&x[..n / 20]);
        let tail = crossings(&x[n - n / 20..]);
        assert!(tail > head * 20, "frequency should rise: {head} -> {tail}");
    }

    #[test]
    fn harmonic_lead_follows_the_log_law() {
        let s = spec();
        assert_eq!(s.harmonic_lead_s(1), 0.0);
        // The 2nd harmonic leads by L·ln 2, the 4th by exactly twice that.
        let l = s.rate();
        assert!((s.harmonic_lead_s(2) - l * std::f64::consts::LN_2).abs() < 1e-12);
        assert!((s.harmonic_lead_s(4) - 2.0 * s.harmonic_lead_s(2)).abs() < 1e-9);
        assert!(s.harmonic_lead_s(3) > s.harmonic_lead_s(2));
    }

    #[test]
    fn a_linear_system_deconvolves_to_a_single_impulse() {
        let s = spec();
        let x = sweep(&s, SR);
        let d = deconvolve(&x, &s, SR);
        // The peak must be the linear impulse, at the expected index.
        let p = peak_index(&d.samples);
        assert!(
            (p as isize - d.linear_index as isize).abs() < 64,
            "peak at {p}, expected near {}",
            d.linear_index
        );
    }

    #[test]
    fn a_delayed_linear_system_moves_the_impulse_by_that_delay() {
        let s = spec();
        let x = sweep(&s, SR);
        let delay = 480; // 10 ms
        let mut delayed = vec![0.0f32; delay];
        delayed.extend_from_slice(&x);
        let d = deconvolve(&delayed, &s, SR);
        let p = peak_index(&d.samples);
        assert!(
            (p as isize - (d.linear_index + delay) as isize).abs() < 64,
            "peak at {p}, expected near {}",
            d.linear_index + delay
        );
    }

    #[test]
    fn a_squaring_nonlinearity_puts_energy_at_the_second_order_position() {
        let s = spec();
        let x = sweep(&s, SR);
        // y = x + 0.3x² — a strong, purely second-order nonlinearity.
        let y: Vec<f32> = x.iter().map(|v| v + 0.3 * v * v).collect();
        let d = deconvolve(&y, &s, SR);

        let energy = |b: &[f32]| b.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>();
        let second = energy(d.order(2, &s));
        let third = energy(d.order(3, &s));
        assert!(
            second > third * 10.0,
            "second-order energy {second:.3e} should dominate third {third:.3e}"
        );
    }

    #[test]
    fn a_cubic_nonlinearity_puts_energy_at_the_third_order_position() {
        let s = spec();
        let x = sweep(&s, SR);
        let y: Vec<f32> = x.iter().map(|v| v + 0.3 * v * v * v).collect();
        let d = deconvolve(&y, &s, SR);
        let energy = |b: &[f32]| b.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>();
        assert!(
            energy(d.order(3, &s)) > energy(d.order(2, &s)) * 10.0,
            "a cubic must land on the third order, not the second"
        );
    }

    #[test]
    fn order_windows_do_not_overlap_and_are_ordered_in_time() {
        let s = spec();
        let x = sweep(&s, SR);
        let d = deconvolve(&x, &s, SR);
        // Higher orders sit earlier in the buffer, and windows are disjoint:
        // order n must end no later than order n-1 begins.
        let span = |n: usize| {
            let w = d.order(n, &s);
            assert!(!w.is_empty(), "order {n} window is empty");
            let start =
                (w.as_ptr() as usize - d.samples.as_ptr() as usize) / std::mem::size_of::<f32>();
            (start, start + w.len())
        };
        for n in 3..=6 {
            let (start, end) = span(n);
            let (lower_start, _) = span(n - 1);
            assert!(
                start < lower_start,
                "order {n} must begin before order {}: {start} vs {lower_start}",
                n - 1
            );
            assert!(
                end <= lower_start,
                "order {n} must not overlap order {}: ends {end}, which begins {lower_start}",
                n - 1
            );
        }
    }

    #[test]
    fn degenerate_arguments_do_not_panic() {
        let s = SweepSpec { duration_s: 0.01, ..spec() };
        let x = sweep(&s, SR);
        assert!(!x.is_empty());
        let d = deconvolve(&x, &s, SR);
        let _ = d.order(1, &s);
        let _ = d.order(20, &s);
    }
}
