//! Holters–Parker combined BBD model.
//!
//! "A Combined Model for a Bucket Brigade Device and its Input and
//! Output Filters" (Holters & Parker, DAFx-18). Structure and the
//! Juno-60 5th-order filter pole/residue tables follow
//! `jatinchowdhury18/BBDDelay` (BSD-3-Clause, © 2020 jatinchowdhury18);
//! the tick/weight bookkeeping here is re-derived from the paper in
//! per-audio-sample units (the standalone repo mixes seconds and
//! samples in its exponents — flagged `@TODO` in its own header).
//!
//! The BBD is a fixed-length queue of `stages` charge samples clocked
//! at `2·stages / delay` (alternating input/output half-ticks). The
//! continuous-time anti-aliasing and reconstruction filters are
//! partial-fraction section banks whose states advance at AUDIO rate
//! but are **evaluated at the exact clock instants**:
//!
//! - input tick at fraction `t` into the sample: bucket value
//!   `= Re Σ_m (Ts·r_m·p̄_m^t)·x_m`
//! - output tick: the ZOH step `Δ = y − y_prev` is injected into the
//!   output sections weighted by `(r_m/p_m)·p̄_m^{1−t}`, plus the
//!   direct term `H0·y` with `H0 = −Σ Re(r_m/p_m)`.
//!
//! No interpolation anywhere: the real BBD's aliasing and imaging
//! emerge from the clocked sampling itself. Slow clocks (long delays)
//! are CHEAPER, and the filter cutoffs scale with the clock so long
//! delays darken exactly like the hardware families the filters came
//! from.

/// Minimal complex value — enough for the section bookkeeping, no deps.
#[derive(Debug, Clone, Copy, Default)]
struct C {
    re: f64,
    im: f64,
}

impl C {
    const ZERO: C = C { re: 0.0, im: 0.0 };

    #[inline]
    fn mul(self, o: C) -> C {
        C {
            re: self.re * o.re - self.im * o.im,
            im: self.re * o.im + self.im * o.re,
        }
    }

    #[inline]
    fn add(self, o: C) -> C {
        C {
            re: self.re + o.re,
            im: self.im + o.im,
        }
    }

    #[inline]
    fn scale(self, k: f64) -> C {
        C {
            re: self.re * k,
            im: self.im * k,
        }
    }

    /// e^self.
    #[inline]
    fn exp(self) -> C {
        let e = self.re.exp();
        C {
            re: e * self.im.cos(),
            im: e * self.im.sin(),
        }
    }

    #[inline]
    fn div(self, o: C) -> C {
        let d = o.re * o.re + o.im * o.im;
        C {
            re: (self.re * o.re + self.im * o.im) / d,
            im: (self.im * o.re - self.re * o.im) / d,
        }
    }
}

const N_FILT: usize = 5;

/// Juno-60 chorus input (anti-aliasing) filter, s-plane partial
/// fractions (rad/s). Nominal cutoff ≈ 9.4 kHz at unity scale.
const IN_ROOTS: [C; N_FILT] = [
    C { re: 251_589.0, im: 0.0 },
    C { re: -130_428.0, im: -4_165.0 },
    C { re: -130_428.0, im: 4_165.0 },
    C { re: 4_634.0, im: -22_873.0 },
    C { re: 4_634.0, im: 22_873.0 },
];
const IN_POLES: [C; N_FILT] = [
    C { re: -46_580.0, im: 0.0 },
    C { re: -55_482.0, im: -25_082.0 },
    C { re: -55_482.0, im: 25_082.0 },
    C { re: -26_292.0, im: -59_437.0 },
    C { re: -26_292.0, im: 59_437.0 },
];

/// Juno-60 chorus output (reconstruction) filter. Nominal cutoff
/// ≈ 11 kHz at unity scale.
const OUT_ROOTS: [C; N_FILT] = [
    C { re: 5_092.0, im: 0.0 },
    C { re: -11_256.0, im: -99_566.0 },
    C { re: -11_256.0, im: 99_566.0 },
    C { re: -13_802.0, im: -24_606.0 },
    C { re: -13_802.0, im: 24_606.0 },
];
const OUT_POLES: [C; N_FILT] = [
    C { re: -176_261.0, im: 0.0 },
    C { re: -51_468.0, im: -21_437.0 },
    C { re: -51_468.0, im: 21_437.0 },
    C { re: -26_276.0, im: -59_699.0 },
    C { re: -26_276.0, im: 59_699.0 },
];

/// Per-bucket write hook: charge-transfer degradation, noise, etc.
pub trait StageShaper {
    fn shape(&mut self, v: f64) -> f64;
}

/// Transparent shaper.
pub struct NoShaper;

impl StageShaper for NoShaper {
    #[inline]
    fn shape(&mut self, v: f64) -> f64 {
        v
    }
}

pub const MAX_STAGES: usize = 8192;

pub struct BbdCore {
    sample_rate: f64,
    stages: usize,
    /// Filter cutoff scale (1.0 = the Juno's ≈100 kHz-clock voicing).
    cutoff_scale: f64,

    buffer: Box<[f64]>,
    ptr: usize,
    even_on: bool,
    /// Time of the next BBD half-tick, in samples past the current
    /// audio sample's start (carries the fractional remainder).
    tn: f64,
    /// Half-tick period in samples (= sample_rate / clock_hz).
    ts_bbd: f64,

    // Input sections: state x, per-sample pole p̄, its inverse, base
    // weight Ts·r, per-input-tick advance p̄^{2·ts_bbd}, running p̄^{tn}.
    in_x: [C; N_FILT],
    in_pbar: [C; N_FILT],
    in_pbar_inv: [C; N_FILT],
    in_g0: [C; N_FILT],
    in_aplus: [C; N_FILT],
    in_arec: [C; N_FILT],
    /// p̂ = p·k·Ts, kept for recomputing the tick advances.
    in_phat: [C; N_FILT],

    // Output sections: weight base (r/p)·p̄, per-output-tick advance
    // p̄^{−2·ts_bbd}, running p̄^{−tn}.
    out_x: [C; N_FILT],
    out_pbar: [C; N_FILT],
    out_gp_pbar: [C; N_FILT],
    out_aplus: [C; N_FILT],
    out_arec: [C; N_FILT],
    out_phat: [C; N_FILT],

    h0: f64,
    /// Unity-insertion makeup: 1 / (H_in(0)·H_out(0)). The raw Juno
    /// filter chain carries several dB of insertion gain; inside a
    /// compander loop that inflates loop gain quadratically, so the
    /// core is normalized to unity at DC.
    makeup: f64,
    y_bbd_old: f64,
    /// Countdown to the periodic exact re-anchor of the running
    /// exponentials (kills multiplicative drift).
    renorm: u32,
}

impl BbdCore {
    pub fn new() -> Self {
        let mut core = Self {
            sample_rate: 48_000.0,
            stages: MAX_STAGES,
            cutoff_scale: 1.0,
            buffer: vec![0.0; MAX_STAGES].into_boxed_slice(),
            ptr: 0,
            even_on: true,
            tn: 0.0,
            ts_bbd: 1.0,
            in_x: [C::ZERO; N_FILT],
            in_pbar: [C::ZERO; N_FILT],
            in_pbar_inv: [C::ZERO; N_FILT],
            in_g0: [C::ZERO; N_FILT],
            in_aplus: [C::ZERO; N_FILT],
            in_arec: [C::ZERO; N_FILT],
            in_phat: [C::ZERO; N_FILT],
            out_x: [C::ZERO; N_FILT],
            out_pbar: [C::ZERO; N_FILT],
            out_gp_pbar: [C::ZERO; N_FILT],
            out_aplus: [C::ZERO; N_FILT],
            out_arec: [C::ZERO; N_FILT],
            out_phat: [C::ZERO; N_FILT],
            h0: 0.0,
            makeup: 1.0,
            y_bbd_old: 0.0,
            renorm: 0,
        };
        core.configure(48_000.0, MAX_STAGES, 1.0);
        core
    }

    /// Control-rate setup: audio rate, stage count (voice) and the
    /// filter cutoff scale. Resets the running exponentials to exact
    /// values; bucket charge is preserved.
    pub fn configure(&mut self, sample_rate: f64, stages: usize, cutoff_scale: f64) {
        self.sample_rate = sample_rate;
        self.stages = stages.clamp(64, MAX_STAGES);
        self.cutoff_scale = cutoff_scale.clamp(0.05, 2.0);
        let ts = 1.0 / sample_rate;
        let k = self.cutoff_scale;

        self.h0 = 0.0;
        let mut hin0 = C::ZERO;
        let mut hout0 = C::ZERO;
        #[allow(clippy::needless_range_loop)] // i spans arrays + self state
        for i in 0..N_FILT {
            hin0 = hin0.add(IN_ROOTS[i].div(IN_POLES[i]).scale(-1.0));
            hout0 = hout0.add(OUT_ROOTS[i].div(OUT_POLES[i]).scale(-1.0));
        }
        self.makeup = 1.0 / (hin0.re * hout0.re).abs().max(1e-6);
        #[allow(clippy::needless_range_loop)] // i spans arrays + self state
        for i in 0..N_FILT {
            // Input side: scale poles AND roots by k (the C++ reference
            // scales both, keeping the response shape).
            let p_hat = IN_POLES[i].scale(k * ts);
            self.in_phat[i] = p_hat;
            self.in_pbar[i] = p_hat.exp();
            self.in_pbar_inv[i] = p_hat.scale(-1.0).exp();
            self.in_g0[i] = IN_ROOTS[i].scale(k * ts);
            self.in_arec[i] = p_hat.scale(self.tn).exp();

            let po_hat = OUT_POLES[i].scale(k * ts);
            self.out_phat[i] = po_hat;
            self.out_pbar[i] = po_hat.exp();
            // (r/p) is scale-invariant (both scale by k).
            let gp = OUT_ROOTS[i].div(OUT_POLES[i]);
            self.out_gp_pbar[i] = gp.mul(self.out_pbar[i]);
            self.out_arec[i] = po_hat.scale(-self.tn).exp();
            self.h0 -= gp.re;
        }
        self.set_clock_samples(self.ts_bbd * 2.0 * self.stages as f64);
    }

    /// Per-sample clock update from the (possibly modulated) delay in
    /// samples. Cheap enough for audio-rate modulation: 10 complex exps.
    pub fn set_clock_samples(&mut self, delay_samples: f64) {
        let delay = delay_samples.max(16.0);
        self.ts_bbd = delay / (2.0 * self.stages as f64);
        let dt = 2.0 * self.ts_bbd;
        #[allow(clippy::needless_range_loop)] // i spans arrays + self state
        for i in 0..N_FILT {
            self.in_aplus[i] = self.in_phat[i].scale(dt).exp();
            self.out_aplus[i] = self.out_phat[i].scale(-dt).exp();
        }
    }

    /// Current clock rate in Hz.
    pub fn clock_hz(&self) -> f64 {
        self.sample_rate / self.ts_bbd
    }

    pub fn stages(&self) -> usize {
        self.stages
    }

    /// One audio sample through the model. `u` is the (already
    /// compressed/driven) loop input; `shaper` colors each bucket write.
    pub fn process(&mut self, u: f64, shaper: &mut impl StageShaper) -> f64 {
        let mut out_accum = [C::ZERO; N_FILT];

        while self.tn < 1.0 {
            if self.even_on {
                // Input tick: evaluate the AA filter bank at this exact
                // clock instant and charge the bucket.
                let mut v = 0.0;
                #[allow(clippy::needless_range_loop)] // i spans two arrays + self state
                #[allow(clippy::needless_range_loop)] // i spans arrays + self state
                for i in 0..N_FILT {
                    self.in_arec[i] = self.in_arec[i].mul(self.in_aplus[i]);
                    let g = self.in_g0[i].mul(self.in_arec[i]);
                    v += self.in_x[i].mul(g).re;
                }
                self.buffer[self.ptr] = shaper.shape(v);
                self.ptr += 1;
                if self.ptr >= self.stages {
                    self.ptr = 0;
                }
            } else {
                // Output tick: ZOH step into the reconstruction bank.
                let y = self.buffer[self.ptr];
                let delta = y - self.y_bbd_old;
                self.y_bbd_old = y;
                #[allow(clippy::needless_range_loop)] // i spans arrays + self state
                for i in 0..N_FILT {
                    self.out_arec[i] = self.out_arec[i].mul(self.out_aplus[i]);
                    out_accum[i] = out_accum[i]
                        .add(self.out_gp_pbar[i].mul(self.out_arec[i]).scale(delta));
                }
            }
            self.even_on = !self.even_on;
            self.tn += self.ts_bbd;
        }
        self.tn -= 1.0;

        // Per-sample: rewind the running exponentials by one sample and
        // advance the section states at audio rate.
        let mut out = self.h0 * self.y_bbd_old;
        #[allow(clippy::needless_range_loop)] // i spans arrays + self state
        for i in 0..N_FILT {
            self.in_arec[i] = self.in_arec[i].mul(self.in_pbar_inv[i]);
            self.out_arec[i] = self.out_arec[i].mul(self.out_pbar[i]);
            self.in_x[i] = self.in_x[i].mul(self.in_pbar[i]).add(C { re: u, im: 0.0 });
            self.out_x[i] = self.out_x[i].mul(self.out_pbar[i]).add(out_accum[i]);
            out += self.out_x[i].re;
        }

        // Re-anchor the running exponentials periodically so hours of
        // multiplies can't drift them.
        self.renorm += 1;
        if self.renorm >= 256 {
            self.renorm = 0;
            #[allow(clippy::needless_range_loop)] // i spans arrays + self state
            for i in 0..N_FILT {
                self.in_arec[i] = self.in_phat[i].scale(self.tn).exp();
                self.out_arec[i] = self.out_phat[i].scale(-self.tn).exp();
            }
        }

        out * self.makeup
    }

    pub fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.ptr = 0;
        self.even_on = true;
        self.tn = 0.0;
        self.y_bbd_old = 0.0;
        self.renorm = 0;
        self.in_x = [C::ZERO; N_FILT];
        self.out_x = [C::ZERO; N_FILT];
        #[allow(clippy::needless_range_loop)] // i spans two arrays + self state
        #[allow(clippy::needless_range_loop)] // i spans arrays + self state
        for i in 0..N_FILT {
            self.in_arec[i] = C { re: 1.0, im: 0.0 };
            self.out_arec[i] = C { re: 1.0, im: 0.0 };
        }
    }
}

impl Default for BbdCore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    #[test]
    fn unity_ish_passband_at_high_clock(){
        // Short delay (fast clock) at unity cutoff scale: a mid-band
        // sine should pass at reasonable level after the delay.
        let mut core = BbdCore::new();
        core.configure(SR, 4096, 1.0);
        let delay_samples = 0.02 * SR; // 20 ms
        core.set_clock_samples(delay_samples);
        let mut peak = 0.0f64;
        for i in 0..24000 {
            let x = (core::f64::consts::TAU * 440.0 * i as f64 / SR).sin() * 0.5;
            let y = core.process(x, &mut NoShaper);
            if i > (delay_samples as usize) + 2000 {
                peak = peak.max(y.abs());
            }
        }
        assert!(
            (0.2..1.2).contains(&peak),
            "passband level off: {peak}"
        );
    }

    #[test]
    fn delays_by_the_stage_count() {
        let mut core = BbdCore::new();
        core.configure(SR, 2048, 1.0);
        let delay_samples = 0.05 * SR; // 50 ms
        core.set_clock_samples(delay_samples);
        // Short burst, find its return.
        let mut first_out = None;
        for i in 0..24000 {
            let x = if i < 96 { 0.8 } else { 0.0 };
            let y = core.process(x, &mut NoShaper);
            if first_out.is_none() && i > 200 && y.abs() > 0.05 {
                first_out = Some(i);
            }
        }
        let arrived = first_out.expect("burst never arrived") as f64;
        assert!(
            (arrived - delay_samples).abs() < delay_samples * 0.1 + 200.0,
            "arrived at {arrived}, expected ≈{delay_samples}"
        );
    }

    #[test]
    fn slow_clock_darkens() {
        // A 6 kHz probe through a fast clock at unity cutoff passes;
        // through a slow clock with the cutoff scaled down it must be
        // strongly attenuated. (First-difference metrics don't work
        // here — the model's intentional ZOH staircase images dominate
        // them at slow clocks.)
        let probe_level = |delay_s: f64, scale: f64| -> f64 {
            let mut core = BbdCore::new();
            core.configure(SR, 4096, scale);
            core.set_clock_samples(delay_s * SR);
            let f = 6000.0;
            let mut sin_acc = 0.0;
            let mut cos_acc = 0.0;
            let n = ((delay_s + 0.5) * SR) as usize;
            let start = ((delay_s + 0.1) * SR) as usize;
            for i in 0..n {
                let ph = core::f64::consts::TAU * f * i as f64 / SR;
                let y = core.process(ph.sin() * 0.5, &mut NoShaper);
                if i > start {
                    sin_acc += y * ph.sin();
                    cos_acc += y * ph.cos();
                }
            }
            let m = (n - start) as f64;
            ((sin_acc / m).powi(2) + (cos_acc / m).powi(2)).sqrt()
        };
        let bright = probe_level(0.03, 1.0);
        let dark = probe_level(0.5, 0.25);
        assert!(bright > 0.02, "probe should pass the fast clock: {bright}");
        assert!(
            dark < bright * 0.35,
            "slow clock + scaled cutoff should darken 6 kHz: {dark} vs {bright}"
        );
    }

    #[test]
    fn stable_and_finite_under_clock_sweeps() {
        let mut core = BbdCore::new();
        core.configure(SR, 8192, 1.0);
        for i in 0..96000 {
            // Sweep the clock hard while feeding a tone.
            let sweep = 0.08 + 0.35 * (0.5 + 0.5 * (i as f64 * 0.0001).sin());
            core.set_clock_samples(sweep * SR);
            let x = (core::f64::consts::TAU * 330.0 * i as f64 / SR).sin() * 0.5;
            let y = core.process(x, &mut NoShaper);
            assert!(y.is_finite(), "NaN at {i}");
            assert!(y.abs() < 100.0, "runaway at {i}: {y}");
        }
    }
}
