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

use dsp_core::num;

/// Minimal complex value — enough for the section bookkeeping, no deps.
#[derive(Debug, Clone, Copy, Default)]
struct C {
    re: f64,
    im: f64,
}

impl C {
    const ZERO: Self = Self { re: 0.0, im: 0.0 };

    #[inline]
    fn mul(self, o: Self) -> Self {
        Self {
            re: self.re.mul_add(o.re, -(self.im * o.im)),
            im: self.re.mul_add(o.im, self.im * o.re),
        }
    }

    #[inline]
    fn add(self, o: Self) -> Self {
        Self {
            re: self.re + o.re,
            im: self.im + o.im,
        }
    }

    #[inline]
    fn scale(self, k: f64) -> Self {
        Self {
            re: self.re * k,
            im: self.im * k,
        }
    }

    /// e^self.
    #[inline]
    fn exp(self) -> Self {
        let e = self.re.exp();
        Self {
            re: e * self.im.cos(),
            im: e * self.im.sin(),
        }
    }

    #[inline]
    fn div(self, o: Self) -> Self {
        let d = o.re.mul_add(o.re, o.im * o.im);
        Self {
            re: self.re.mul_add(o.re, self.im * o.im) / d,
            im: self.im.mul_add(o.re, -(self.re * o.im)) / d,
        }
    }
}

const N_FILT: usize = 5;

/// Juno-60 chorus input (anti-aliasing) filter, s-plane partial
/// fractions (rad/s). Nominal cutoff ≈ 9.4 kHz at unity scale.
const IN_ROOTS: [C; N_FILT] = [
    C {
        re: 251_589.0,
        im: 0.0,
    },
    C {
        re: -130_428.0,
        im: -4_165.0,
    },
    C {
        re: -130_428.0,
        im: 4_165.0,
    },
    C {
        re: 4_634.0,
        im: -22_873.0,
    },
    C {
        re: 4_634.0,
        im: 22_873.0,
    },
];
const IN_POLES: [C; N_FILT] = [
    C {
        re: -46_580.0,
        im: 0.0,
    },
    C {
        re: -55_482.0,
        im: -25_082.0,
    },
    C {
        re: -55_482.0,
        im: 25_082.0,
    },
    C {
        re: -26_292.0,
        im: -59_437.0,
    },
    C {
        re: -26_292.0,
        im: 59_437.0,
    },
];

/// Juno-60 chorus output (reconstruction) filter. Nominal cutoff
/// ≈ 11 kHz at unity scale.
const OUT_ROOTS: [C; N_FILT] = [
    C {
        re: 5_092.0,
        im: 0.0,
    },
    C {
        re: -11_256.0,
        im: -99_566.0,
    },
    C {
        re: -11_256.0,
        im: 99_566.0,
    },
    C {
        re: -13_802.0,
        im: -24_606.0,
    },
    C {
        re: -13_802.0,
        im: 24_606.0,
    },
];
const OUT_POLES: [C; N_FILT] = [
    C {
        re: -176_261.0,
        im: 0.0,
    },
    C {
        re: -51_468.0,
        im: -21_437.0,
    },
    C {
        re: -51_468.0,
        im: 21_437.0,
    },
    C {
        re: -26_276.0,
        im: -59_699.0,
    },
    C {
        re: -26_276.0,
        im: 59_699.0,
    },
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

/// One pole of the input (anti-alias) filter bank.
///
/// This and [`OutSection`] replaced thirteen parallel `[C; N_FILT]` arrays
/// walked by a shared index. The arithmetic is identical; what changes is that
/// a section's coefficients and the state they drive can no longer drift apart,
/// and the loops below iterate rather than index — which also retires five
/// `#[expect(needless_range_loop)]` suppressions that were papering over the
/// indexing rather than removing it.
#[derive(Clone, Copy, Debug)]
struct InSection {
    /// Section state.
    x: C,
    /// Per-sample pole p̄, and its inverse.
    pbar: C,
    pbar_inv: C,
    /// Base weight Ts·r.
    g0: C,
    /// Per-input-tick advance p̄^{2·`ts_bbd`}.
    aplus: C,
    /// Running p̄^{tn}.
    arec: C,
    /// p̂ = p·k·Ts, kept for recomputing the tick advances.
    phat: C,
}

/// One pole of the output (reconstruction) filter bank.
#[derive(Clone, Copy, Debug)]
struct OutSection {
    /// Section state.
    x: C,
    /// Per-sample pole p̄.
    pbar: C,
    /// Weight base (r/p)·p̄.
    gp_pbar: C,
    /// Per-output-tick advance p̄^{−2·`ts_bbd`}.
    aplus: C,
    /// Running p̄^{−tn}.
    arec: C,
    /// p̂, kept for recomputing the tick advances.
    phat: C,
}

impl InSection {
    const ZERO: Self = Self {
        x: C::ZERO,
        pbar: C::ZERO,
        pbar_inv: C::ZERO,
        g0: C::ZERO,
        aplus: C::ZERO,
        arec: C::ZERO,
        phat: C::ZERO,
    };
}

impl OutSection {
    const ZERO: Self = Self {
        x: C::ZERO,
        pbar: C::ZERO,
        gp_pbar: C::ZERO,
        aplus: C::ZERO,
        arec: C::ZERO,
        phat: C::ZERO,
    };
}

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
    /// Half-tick period in samples (= `sample_rate` / `clock_hz`).
    ts_bbd: f64,

    in_sections: [InSection; N_FILT],
    out_sections: [OutSection; N_FILT],

    h0: f64,
    /// Unity-insertion makeup: 1 / (`H_in(0)·H_out(0)`). The raw Juno
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
    #[must_use]
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
            in_sections: [InSection::ZERO; N_FILT],
            out_sections: [OutSection::ZERO; N_FILT],
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
        for ((in_root, in_pole), (out_root, out_pole)) in IN_ROOTS
            .iter()
            .zip(&IN_POLES)
            .zip(OUT_ROOTS.iter().zip(&OUT_POLES))
        {
            hin0 = hin0.add(in_root.div(*in_pole).scale(-1.0));
            hout0 = hout0.add(out_root.div(*out_pole).scale(-1.0));
        }
        self.makeup = 1.0 / (hin0.re * hout0.re).abs().max(1e-6);
        let tn = self.tn;
        for ((section, pole), root) in self.in_sections.iter_mut().zip(&IN_POLES).zip(&IN_ROOTS) {
            // Input side: scale poles AND roots by k (the C++ reference
            // scales both, keeping the response shape).
            let p_hat = pole.scale(k * ts);
            section.phat = p_hat;
            section.pbar = p_hat.exp();
            section.pbar_inv = p_hat.scale(-1.0).exp();
            section.g0 = root.scale(k * ts);
            section.arec = p_hat.scale(tn).exp();
        }
        for ((section, pole), root) in self.out_sections.iter_mut().zip(&OUT_POLES).zip(&OUT_ROOTS)
        {
            let po_hat = pole.scale(k * ts);
            section.phat = po_hat;
            section.pbar = po_hat.exp();
            // (r/p) is scale-invariant (both scale by k).
            let gp = root.div(*pole);
            section.gp_pbar = gp.mul(section.pbar);
            section.arec = po_hat.scale(-tn).exp();
            self.h0 -= gp.re;
        }
        self.set_clock_samples(self.ts_bbd * 2.0 * num::count_to_f64(self.stages));
    }

    /// Per-sample clock update from the (possibly modulated) delay in
    /// samples. Cheap enough for audio-rate modulation: 10 complex exps.
    pub fn set_clock_samples(&mut self, delay_samples: f64) {
        let delay = delay_samples.max(16.0);
        self.ts_bbd = delay / (2.0 * num::count_to_f64(self.stages));
        let dt = 2.0 * self.ts_bbd;
        for (in_section, out_section) in self.in_sections.iter_mut().zip(&mut self.out_sections) {
            in_section.aplus = in_section.phat.scale(dt).exp();
            out_section.aplus = out_section.phat.scale(-dt).exp();
        }
    }

    /// Current clock rate in Hz.
    #[must_use]
    pub fn clock_hz(&self) -> f64 {
        self.sample_rate / self.ts_bbd
    }

    #[must_use]
    pub const fn stages(&self) -> usize {
        self.stages
    }

    /// One audio sample through the model. `u` is the (already
    /// compressed/driven) loop input; `shaper` colors each bucket write.
    pub fn process(&mut self, u: f64, shaper: &mut impl StageShaper) -> f64 {
        let mut out_accum = [C::ZERO; N_FILT];

        // Comparing the running tick position against a whole sample. This is
        // a scheduler, not an equality test: `tn` accumulates `ts_bbd` and the
        // loop drains whole samples out of it, so the float comparison is the
        // termination condition, and there is no epsilon that would make it
        // more correct.
        #[expect(
            clippy::while_float,
            reason = "draining a fractional clock accumulator; the comparison is the loop's purpose"
        )]
        while self.tn < 1.0 {
            if self.even_on {
                // Input tick: evaluate the AA filter bank at this exact
                // clock instant and charge the bucket.
                let mut v = 0.0;
                for section in &mut self.in_sections {
                    section.arec = section.arec.mul(section.aplus);
                    let g = section.g0.mul(section.arec);
                    v += section.x.mul(g).re;
                }
                if let Some(slot) = self.buffer.get_mut(self.ptr) {
                    *slot = shaper.shape(v);
                }
                self.ptr = self.ptr.saturating_add(1);
                if self.ptr >= self.stages {
                    self.ptr = 0;
                }
            } else {
                // Output tick: ZOH step into the reconstruction bank.
                // `ptr` is kept below `stages` by the wrap above, and `stages`
                // never exceeds the buffer, so the fallback is unreachable —
                // it exists so the read cannot panic on an audio callback.
                let y = self.buffer.get(self.ptr).copied().unwrap_or(0.0);
                let delta = y - self.y_bbd_old;
                self.y_bbd_old = y;
                for (section, accum) in self.out_sections.iter_mut().zip(&mut out_accum) {
                    section.arec = section.arec.mul(section.aplus);
                    *accum = accum.add(section.gp_pbar.mul(section.arec).scale(delta));
                }
            }
            self.even_on = !self.even_on;
            self.tn += self.ts_bbd;
        }
        self.tn -= 1.0;

        // Per-sample: rewind the running exponentials by one sample and
        // advance the section states at audio rate.
        let mut out = self.h0 * self.y_bbd_old;
        for ((in_section, out_section), accum) in self
            .in_sections
            .iter_mut()
            .zip(&mut self.out_sections)
            .zip(&out_accum)
        {
            in_section.arec = in_section.arec.mul(in_section.pbar_inv);
            out_section.arec = out_section.arec.mul(out_section.pbar);
            in_section.x = in_section.x.mul(in_section.pbar).add(C { re: u, im: 0.0 });
            out_section.x = out_section.x.mul(out_section.pbar).add(*accum);
            out += out_section.x.re;
        }

        // Re-anchor the running exponentials periodically so hours of
        // multiplies can't drift them.
        self.renorm = self.renorm.saturating_add(1);
        if self.renorm >= 256 {
            self.renorm = 0;
            let tn = self.tn;
            for (in_section, out_section) in self.in_sections.iter_mut().zip(&mut self.out_sections)
            {
                in_section.arec = in_section.phat.scale(tn).exp();
                out_section.arec = out_section.phat.scale(-tn).exp();
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
        for (in_section, out_section) in self.in_sections.iter_mut().zip(&mut self.out_sections) {
            in_section.x = C::ZERO;
            out_section.x = C::ZERO;
            in_section.arec = C { re: 1.0, im: 0.0 };
            out_section.arec = C { re: 1.0, im: 0.0 };
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
    fn unity_ish_passband_at_high_clock() {
        // Short delay (fast clock) at unity cutoff scale: a mid-band
        // sine should pass at reasonable level after the delay.
        let mut core = BbdCore::new();
        core.configure(SR, 4096, 1.0);
        let delay_samples = 0.02 * SR; // 20 ms
        core.set_clock_samples(delay_samples);
        let mut peak = 0.0f64;
        for i in 0..24000 {
            let x = (core::f64::consts::TAU * 440.0 * num::count_to_f64(i) / SR).sin() * 0.5;
            let y = core.process(x, &mut NoShaper);
            if i > num::f64_to_index(delay_samples) + 2000 {
                peak = peak.max(y.abs());
            }
        }
        assert!((0.2..1.2).contains(&peak), "passband level off: {peak}");
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
        let arrived = f64::from(first_out.expect("burst never arrived"));
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
            let n = num::f64_to_index((delay_s + 0.5) * SR);
            let start = num::f64_to_index((delay_s + 0.1) * SR);
            for i in 0..n {
                let ph = core::f64::consts::TAU * f * num::count_to_f64(i) / SR;
                let y = core.process(ph.sin() * 0.5, &mut NoShaper);
                if i > start {
                    sin_acc += y * ph.sin();
                    cos_acc += y * ph.cos();
                }
            }
            let m = num::count_to_f64(n - start);
            (sin_acc / m).hypot(cos_acc / m)
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
            let sweep = 0.35f64.mul_add(0.5f64.mul_add((f64::from(i) * 0.0001).sin(), 0.5), 0.08);
            core.set_clock_samples(sweep * SR);
            let x = (core::f64::consts::TAU * 330.0 * f64::from(i) / SR).sin() * 0.5;
            let y = core.process(x, &mut NoShaper);
            assert!(y.is_finite(), "NaN at {i}");
            assert!(y.abs() < 100.0, "runaway at {i}: {y}");
        }
    }
}
