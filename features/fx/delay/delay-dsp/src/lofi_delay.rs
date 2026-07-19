//! LoFiDelay — bit-crushed, sample-rate-reduced delay for degraded sound.
//!
//! TimeLine MX "Lo Fi" machine parity (spec/timeline-mx-reference.md):
//!
//! - Degradation order is grit saturation → sample-rate hold → bit
//!   quantize, so grit's added harmonics ALIAS at low sample rates
//!   (the spec calls this interaction out as the point).
//! - `lofi_mix` blends the degraded path against the full-resolution
//!   read ON the delay line, pre-feedback, so the chosen blend
//!   recirculates.
//! - `vinyl` (dVinyl): first half of the range = DYNAMIC noise gated
//!   by repeat activity, second half = STATIC always-on noise.
//! - `filter_shape`: a bank of device voicings (telephone, victrola,
//!   megaphone, ...) applied to the mixed signal AND the vinyl noise,
//!   per the spec.

use audiocore_dsp::biquad::{Biquad, FilterType};
use audiocore_dsp::delay_line::DelayLine;
use audiocore_dsp::envelope::EnvelopeFollower;
use audiocore_dsp::prng::XorShift32;
use audiocore_dsp::smoothing::ParamSmoother;

/// Device voicings for the Lo-Fi output filter (TimeLine MX "Filter
/// Shape"). Center frequencies / resonances are chosen per the device
/// character descriptions; exact hardware curves are unknown.
// interpretation: voicings designed to the MX manual's device list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoFiFilterShape {
    #[default]
    Off,
    /// Small-speaker vintage amp: mid-forward, soft edges.
    VintageAmp,
    /// Victrola phonograph: horn resonance, steep top loss.
    Victrola,
    /// 70s clock radio: boxy single-speaker mid bump.
    ClockRadio,
    /// Bullhorn megaphone: hard mid honk.
    Bullhorn,
    /// Cheerleader's plastic megaphone: brighter, plasticky honk.
    Megaphone,
    /// Antique telephone earpiece: 300–3400 Hz band, presence bump.
    AntiqueTelephone,
    /// Cell phone speaker: no lows, spiky 3 kHz presence.
    CellPhone,
    /// Apartment intercom: thin, papery mids.
    Intercom,
}

impl LoFiFilterShape {
    pub fn from_index(i: usize) -> Self {
        match i {
            1 => Self::VintageAmp,
            2 => Self::Victrola,
            3 => Self::ClockRadio,
            4 => Self::Bullhorn,
            5 => Self::Megaphone,
            6 => Self::AntiqueTelephone,
            7 => Self::CellPhone,
            8 => Self::Intercom,
            _ => Self::Off,
        }
    }

    /// (hp_hz, lp_hz, peak_hz, peak_q, peak_db) for the 3-section bank.
    fn design(self) -> Option<(f64, f64, f64, f64, f64)> {
        // interpretation: 2–3 biquad approximations of each device.
        match self {
            Self::Off => None,
            Self::VintageAmp => Some((120.0, 4500.0, 2200.0, 1.5, 4.0)),
            Self::Victrola => Some((250.0, 3000.0, 800.0, 3.0, 6.0)),
            Self::ClockRadio => Some((150.0, 5000.0, 1200.0, 1.0, 3.0)),
            Self::Bullhorn => Some((400.0, 4000.0, 1800.0, 4.0, 8.0)),
            Self::Megaphone => Some((500.0, 5000.0, 2500.0, 5.0, 8.0)),
            Self::AntiqueTelephone => Some((300.0, 3400.0, 2000.0, 2.0, 5.0)),
            Self::CellPhone => Some((400.0, 5000.0, 3000.0, 3.0, 6.0)),
            Self::Intercom => Some((250.0, 6000.0, 1500.0, 2.5, 5.0)),
        }
    }
}

/// dVinyl-style noise generator: sparse crackle through a bandpass,
/// a filtered dust bed, and occasional low-frequency thumps.
// interpretation: matches the described behavior (dynamic vs static
// halves, record-noise character), not measured hardware.
struct VinylNoise {
    rng: XorShift32,
    crackle_bp: Biquad,
    dust_lp: Biquad,
    /// Samples until the next crackle impulse.
    crackle_countdown: f64,
    /// Remaining samples of the current crackle burst.
    crackle_hold: f64,
    crackle_gain: f64,
    /// Samples until the next thump.
    thump_countdown: f64,
    /// Thump oscillator state (decaying LF sine).
    thump_phase: f64,
    thump_env: f64,
    sample_rate: f64,
}

impl VinylNoise {
    fn new() -> Self {
        Self {
            rng: XorShift32::new(0x71AF_33D1),
            crackle_bp: Biquad::new(),
            dust_lp: Biquad::new(),
            crackle_countdown: 480.0,
            crackle_hold: 0.0,
            crackle_gain: 0.0,
            thump_countdown: 96000.0,
            thump_phase: 0.0,
            thump_env: 0.0,
            sample_rate: 48000.0,
        }
    }

    fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.crackle_bp
            .set(FilterType::Bandpass, 3500.0, 0.8, sample_rate);
        self.dust_lp
            .set(FilterType::Lowpass, 8000.0, 0.707, sample_rate);
    }

    /// One sample of vinyl noise at `amount` (0..1) intensity.
    #[inline]
    fn tick(&mut self, amount: f64) -> f64 {
        if amount <= 0.0 {
            return 0.0;
        }
        let rand01 = (self.rng.next_bipolar() + 1.0) * 0.5;

        // Crackle: Poisson-ish sparse impulses, denser as amount rises
        // (~4/s subtle → ~30/s heavy), 0.3–1.5 ms bursts.
        self.crackle_countdown -= 1.0;
        if self.crackle_countdown <= 0.0 {
            let rate_hz = 4.0 + amount * 26.0;
            self.crackle_countdown = self.sample_rate / rate_hz * (0.3 + rand01 * 1.4);
            self.crackle_hold = self.sample_rate * (0.0003 + rand01 * 0.0012);
            self.crackle_gain = 0.4 + rand01 * 0.6;
        }
        let crackle_src = if self.crackle_hold > 0.0 {
            self.crackle_hold -= 1.0;
            self.rng.next_bipolar() * self.crackle_gain
        } else {
            0.0
        };
        let crackle = self.crackle_bp.tick(crackle_src, 0);

        // Dust bed: continuous filtered hiss, well under the crackle.
        let dust = self.dust_lp.tick(self.rng.next_bipolar() * 0.02, 0);

        // Thump: rare (every ~1.5–4 s) decaying ~60 Hz pulse.
        self.thump_countdown -= 1.0;
        if self.thump_countdown <= 0.0 {
            self.thump_countdown = self.sample_rate * (1.5 + rand01 * 2.5);
            self.thump_env = 0.5 + rand01 * 0.5;
            self.thump_phase = 0.0;
        }
        let thump = if self.thump_env > 1e-4 {
            self.thump_phase += std::f64::consts::TAU * 60.0 / self.sample_rate;
            self.thump_env *= 1.0 - 30.0 / self.sample_rate;
            self.thump_phase.sin() * self.thump_env * 0.3
        } else {
            0.0
        };

        // Calibration: subtle at 0.25, obvious at 1.0.
        (crackle * 1.4 + dust + thump) * amount * 0.22
    }

    fn reset(&mut self) {
        self.crackle_bp.reset();
        self.dust_lp.reset();
        self.crackle_countdown = 480.0;
        self.crackle_hold = 0.0;
        self.thump_countdown = 96000.0;
        self.thump_env = 0.0;
        self.thump_phase = 0.0;
    }
}

/// Lo-Fi delay with bit crushing, sample rate reduction, dVinyl noise
/// and device filter voicings.
pub struct LoFiDelay {
    /// Delay time in milliseconds.
    pub time_ms: f64,
    /// Feedback amount (0.0–1.0).
    pub feedback: f64,
    /// Bit depth for quantization (4–32).
    pub bit_depth: f64,
    /// Sample rate divisor (1–64). 1 = no reduction.
    pub sample_rate_div: f64,
    /// Grit (0.0–1.0): saturation BEFORE the sample-rate hold, so the
    /// harmonics it adds alias at low sample rates (spec interaction).
    pub grit: f64,
    /// Degraded↔clean blend on the delay line (1.0 = fully degraded,
    /// the previous fixed behavior). Recirculates.
    pub lofi_mix: f64,
    /// dVinyl amount: 0–0.5 = dynamic (gated by repeat activity),
    /// 0.5–1.0 = static (always on).
    pub vinyl: f64,
    /// Output device voicing (applies to signal + vinyl noise).
    pub filter_shape: LoFiFilterShape,
    /// Noise floor injection (0.0–1.0). Adds hiss/noise to feedback path.
    pub noise: f64,
    /// High-cut filter frequency in Hz (0 = disabled).
    pub hicut_freq: f64,
    /// Low-cut filter frequency in Hz (0 = disabled).
    pub locut_freq: f64,
    /// Filter Q.
    pub filter_q: f64,
    /// Decay EQ tilt (-1.0 = darken repeats, 0 = neutral, +1.0 = brighten).
    pub decay_tilt: f64,

    decay_eq: Biquad,
    delay: DelayLine,
    hicut: Biquad,
    locut: Biquad,
    // Filter-shape bank: HP -> peak -> LP.
    shape_hp: Biquad,
    shape_peak: Biquad,
    shape_lp: Biquad,
    vinyl_gen: VinylNoise,
    /// Envelope on the wet signal — gates DYNAMIC vinyl noise.
    wet_env: EnvelopeFollower,
    feedback_sample: f64,
    sample_rate: f64,
    smoother: ParamSmoother,
    // Sample-rate reduction state
    sr_counter: f64,
    sr_hold: f64,
    // Noise PRNG
    rng: XorShift32,
}

impl Default for LoFiDelay {
    fn default() -> Self {
        Self::new()
    }
}

impl LoFiDelay {
    const MAX_DELAY_S: f64 = 5.0;

    pub fn new() -> Self {
        let mut wet_env = EnvelopeFollower::new(0.0);
        wet_env.set_times_ms(5.0, 300.0, 48000.0);
        Self {
            time_ms: 250.0,
            feedback: 0.4,
            bit_depth: 12.0,
            sample_rate_div: 4.0,
            grit: 0.0,
            lofi_mix: 1.0,
            vinyl: 0.0,
            filter_shape: LoFiFilterShape::Off,
            noise: 0.0,
            hicut_freq: 0.0,
            locut_freq: 0.0,
            filter_q: 0.707,
            decay_tilt: 0.0,
            decay_eq: Biquad::new(),
            delay: DelayLine::new(48000 * 5 + 1024),
            hicut: Biquad::new(),
            locut: Biquad::new(),
            shape_hp: Biquad::new(),
            shape_peak: Biquad::new(),
            shape_lp: Biquad::new(),
            vinyl_gen: VinylNoise::new(),
            wet_env,
            feedback_sample: 0.0,
            sample_rate: 48000.0,
            smoother: ParamSmoother::new(0.0),
            sr_counter: 0.0,
            sr_hold: 0.0,
            rng: XorShift32::new(0xCAFE_BABE),
        }
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let max_len = (sample_rate * Self::MAX_DELAY_S) as usize + 1024;
        if self.delay.len() < max_len {
            self.delay = DelayLine::new(max_len);
        }

        if self.hicut_freq > 0.0 {
            self.hicut.set(
                FilterType::Lowpass,
                self.hicut_freq,
                self.filter_q,
                sample_rate,
            );
        }
        if self.locut_freq > 0.0 {
            self.locut.set(
                FilterType::Highpass,
                self.locut_freq,
                self.filter_q,
                sample_rate,
            );
        }

        if let Some((hp, lp, peak, q, db)) = self.filter_shape.design() {
            self.shape_hp
                .set(FilterType::Highpass, hp, 0.707, sample_rate);
            self.shape_peak
                .set(FilterType::Peak { gain_db: db }, peak, q, sample_rate);
            self.shape_lp.set(FilterType::Lowpass, lp, 0.9, sample_rate);
        }
        self.vinyl_gen.update(sample_rate);
        self.wet_env.set_times_ms(5.0, 300.0, sample_rate);

        // Decay EQ: tilt filter in feedback path
        if self.decay_tilt.abs() > 0.01 {
            if self.decay_tilt < 0.0 {
                let freq = 20000.0 * (1.0 + self.decay_tilt).max(0.05);
                self.decay_eq
                    .set(FilterType::Lowpass, freq, 0.707, sample_rate);
            } else {
                let freq = 20.0 + self.decay_tilt * 2000.0;
                self.decay_eq
                    .set(FilterType::Highpass, freq, 0.707, sample_rate);
            }
        }

        self.smoother.set_time(0.15, sample_rate);
        let target = self.time_ms * 0.001 * sample_rate;
        if self.smoother.value() == 0.0 {
            self.smoother.set_immediate(target);
        }
    }

    /// Quantize to simulated bit depth.
    #[inline]
    fn quantize(x: f64, bits: f64) -> f64 {
        let steps = (2.0f64).powf(bits);
        (x * steps).round() / steps
    }

    pub fn tick(&mut self, input: f64, ch: usize) -> f64 {
        let target_delay = self.time_ms * 0.001 * self.sample_rate;
        self.smoother.set_target(target_delay);
        let smooth_delay = self.smoother.tick();

        let max_read = self.delay.len() as f64 - 4.0;
        let read_pos = smooth_delay.clamp(1.0, max_read);
        let clean = self.delay.read_cubic(read_pos);

        // Degraded path. Order matters (spec): grit saturation FIRST so
        // its harmonics alias through the sample-rate hold.
        let mut degraded = clean;
        if self.grit > 0.0 {
            let drive = 1.0 + self.grit * 6.0;
            degraded = (degraded * drive).tanh() / (1.0 + self.grit * 1.5);
        }
        self.sr_counter += 1.0;
        if self.sr_counter >= self.sample_rate_div {
            self.sr_counter = 0.0;
            self.sr_hold = degraded;
        }
        degraded = self.sr_hold;
        degraded = Self::quantize(degraded, self.bit_depth);

        // LoFi Mix: degraded vs full-resolution blend, pre-feedback —
        // the blend is what recirculates.
        let mix = self.lofi_mix.clamp(0.0, 1.0);
        let mut output = degraded * mix + clean * (1.0 - mix);

        // Feedback path
        let mut fb = output * self.feedback;

        // Noise injection (lo-fi hiss/noise floor)
        if self.noise > 0.0 {
            fb += self.rng.next_bipolar() * self.noise * 0.05;
        }

        if self.hicut_freq > 0.0 {
            fb = self.hicut.tick(fb, ch);
        }
        if self.locut_freq > 0.0 {
            fb = self.locut.tick(fb, ch);
        }

        if self.decay_tilt.abs() > 0.01 {
            fb = self.decay_eq.tick(fb, ch);
        }

        fb = fb.clamp(-1.5, 1.5);

        self.delay.write(input + fb);
        self.feedback_sample = fb;

        // dVinyl: dynamic half rides the wet envelope, static half is
        // always on. Noise joins the signal BEFORE the shape filter.
        if self.vinyl > 0.0 {
            let env = self.wet_env.tick(output.abs());
            let gate = (env / 0.02).clamp(0.0, 1.0);
            let dynamic = (self.vinyl.min(0.5) * 2.0) * gate;
            let static_amt = ((self.vinyl - 0.5).max(0.0)) * 2.0;
            output += self.vinyl_gen.tick((dynamic + static_amt).min(1.0));
        }

        // Device voicing applies to signal + vinyl noise (spec).
        if self.filter_shape != LoFiFilterShape::Off {
            output = self.shape_hp.tick(output, ch);
            output = self.shape_peak.tick(output, ch);
            output = self.shape_lp.tick(output, ch);
        }

        output
    }

    pub fn last_feedback(&self) -> f64 {
        self.feedback_sample
    }

    pub fn reset(&mut self) {
        self.delay.clear();
        self.hicut.reset();
        self.locut.reset();
        self.decay_eq.reset();
        self.shape_hp.reset();
        self.shape_peak.reset();
        self.shape_lp.reset();
        self.vinyl_gen.reset();
        self.wet_env.reset(0.0);
        self.feedback_sample = 0.0;
        self.smoother.reset(0.0);
        self.sr_counter = 0.0;
        self.sr_hold = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    /// Goertzel energy at `freq` over `signal`.
    fn goertzel(signal: &[f64], freq: f64) -> f64 {
        let w = std::f64::consts::TAU * freq / SR;
        let coeff = 2.0 * w.cos();
        let (mut s0, mut s1, mut s2) = (0.0, 0.0, 0.0);
        for &x in signal {
            s0 = x + coeff * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        (s1 * s1 + s2 * s2 - coeff * s1 * s2) / (signal.len() as f64).powi(2)
    }

    #[test]
    fn quantize_reduces_precision() {
        // 8-bit quantization should snap to 256 levels
        let q = LoFiDelay::quantize(0.123456, 8.0);
        let step = 1.0 / 256.0;
        let remainder = (q / step).fract();
        assert!(
            remainder.abs() < 1e-10 || (1.0 - remainder).abs() < 1e-10,
            "Should quantize to step: q={q}"
        );
    }

    #[test]
    fn impulse_delayed() {
        let mut d = LoFiDelay::new();
        d.time_ms = 100.0;
        d.feedback = 0.0;
        d.bit_depth = 24.0; // High bit depth for clean test
        d.sample_rate_div = 1.0; // No SR reduction
        d.update(SR);

        let mut peak_pos = 0;
        let mut peak_val = 0.0f64;

        for i in 0..10000 {
            let input = if i == 0 { 1.0 } else { 0.0 };
            let out = d.tick(input, 0);
            if out.abs() > peak_val {
                peak_val = out.abs();
                peak_pos = i;
            }
        }

        assert!(
            (peak_pos as i64 - 4800).unsigned_abs() < 10,
            "Peak at {peak_pos}, expected near 4800"
        );
    }

    #[test]
    fn no_nan() {
        let mut d = LoFiDelay::new();
        d.time_ms = 200.0;
        d.feedback = 0.7;
        d.bit_depth = 8.0;
        d.sample_rate_div = 8.0;
        d.hicut_freq = 4000.0;
        d.grit = 0.8;
        d.vinyl = 1.0;
        d.filter_shape = LoFiFilterShape::AntiqueTelephone;
        d.update(SR);

        for i in 0..96000 {
            let input = (std::f64::consts::PI * 2.0 * 440.0 * i as f64 / SR).sin() * 0.5;
            let out = d.tick(input, 0);
            assert!(out.is_finite(), "NaN at sample {i}");
        }
    }

    #[test]
    fn sr_reduction_changes_output() {
        let mut d_clean = LoFiDelay::new();
        d_clean.time_ms = 50.0;
        d_clean.feedback = 0.0;
        d_clean.bit_depth = 24.0;
        d_clean.sample_rate_div = 1.0;
        d_clean.update(SR);

        let mut d_lofi = LoFiDelay::new();
        d_lofi.time_ms = 50.0;
        d_lofi.feedback = 0.0;
        d_lofi.bit_depth = 24.0;
        d_lofi.sample_rate_div = 16.0;
        d_lofi.update(SR);

        let mut diff = 0.0;
        for i in 0..9600 {
            let s = (std::f64::consts::PI * 2.0 * 440.0 * i as f64 / SR).sin() * 0.5;
            let a = d_clean.tick(s, 0);
            let b = d_lofi.tick(s, 0);
            diff += (a - b).abs();
        }

        assert!(diff > 0.1, "SR reduction should change output: diff={diff}");
    }

    #[test]
    fn lofi_mix_zero_is_clean() {
        // With lofi_mix = 0 the degraded path must be inaudible even
        // with heavy degradation dialed in.
        let render = |mix: f64| -> Vec<f64> {
            let mut d = LoFiDelay::new();
            d.time_ms = 80.0;
            d.feedback = 0.3;
            d.bit_depth = 4.0;
            d.sample_rate_div = 32.0;
            d.grit = 1.0;
            d.lofi_mix = mix;
            d.update(SR);
            (0..24000)
                .map(|i| {
                    let s = (std::f64::consts::TAU * 330.0 * i as f64 / SR).sin() * 0.5;
                    d.tick(s, 0)
                })
                .collect()
        };
        let clean = {
            let mut d = LoFiDelay::new();
            d.time_ms = 80.0;
            d.feedback = 0.3;
            d.bit_depth = 32.0;
            d.sample_rate_div = 1.0;
            d.lofi_mix = 1.0;
            d.update(SR);
            (0..24000)
                .map(|i| {
                    let s = (std::f64::consts::TAU * 330.0 * i as f64 / SR).sin() * 0.5;
                    d.tick(s, 0)
                })
                .collect::<Vec<_>>()
        };
        let mixed_off = render(0.0);
        let mixed_on = render(1.0);

        let err_off: f64 = mixed_off
            .iter()
            .zip(&clean)
            .map(|(a, b)| (a - b).abs())
            .sum();
        let err_on: f64 = mixed_on
            .iter()
            .zip(&clean)
            .map(|(a, b)| (a - b).abs())
            .sum();
        assert!(
            err_off < err_on * 0.05,
            "mix=0 should be near-clean: off={err_off} on={err_on}"
        );
    }

    #[test]
    fn grit_increases_aliasing_at_low_sample_rate() {
        // 440 Hz tone at ~5 kHz effective SR: grit's added harmonics
        // above Nyquist fold back to inharmonic frequencies. The 7th
        // harmonic (3080 Hz) aliases to 5000-3080 = 1920 Hz, which is
        // not a 440 multiple.
        let render = |grit: f64| -> Vec<f64> {
            let mut d = LoFiDelay::new();
            d.time_ms = 60.0;
            d.feedback = 0.0;
            d.bit_depth = 32.0;
            d.sample_rate_div = (SR / 5000.0).round(); // ≈ 5 kHz hold rate
            d.grit = grit;
            d.lofi_mix = 1.0;
            d.update(SR);
            let mut out = Vec::with_capacity(48000);
            for i in 0..48000 {
                let s = (std::f64::consts::TAU * 440.0 * i as f64 / SR).sin() * 0.5;
                let v = d.tick(s, 0);
                if i > 12000 {
                    out.push(v);
                }
            }
            out
        };
        let clean = render(0.0);
        let gritty = render(0.9);
        // Effective hold rate: SR / round(SR/5000) ≈ 4800 Hz here; the
        // 7th harmonic folds to hold_rate - 3080.
        let hold_rate = SR / (SR / 5000.0f64).round();
        let alias_freq = hold_rate - 3080.0;
        let e_clean = goertzel(&clean, alias_freq);
        let e_gritty = goertzel(&gritty, alias_freq);
        assert!(
            e_gritty > e_clean * 2.0,
            "grit should raise aliased inharmonic energy: clean={e_clean:.3e} gritty={e_gritty:.3e}"
        );
    }

    #[test]
    fn vinyl_noise_dynamic_vs_static() {
        // Static vinyl (param > 0.5) produces noise with no input;
        // dynamic vinyl (param <= 0.5) stays quiet without repeats.
        let render_silence = |vinyl: f64| -> f64 {
            let mut d = LoFiDelay::new();
            d.time_ms = 100.0;
            d.feedback = 0.0;
            d.vinyl = vinyl;
            d.update(SR);
            let mut energy = 0.0;
            for _ in 0..96000 {
                let v = d.tick(0.0, 0);
                energy += v * v;
            }
            energy
        };
        let dynamic = render_silence(0.4);
        let statik = render_silence(1.0);
        assert!(
            statik > 1e-4,
            "static vinyl should be audible in silence: {statik:.3e}"
        );
        assert!(
            dynamic < statik * 0.05,
            "dynamic vinyl should gate closed in silence: dyn={dynamic:.3e} static={statik:.3e}"
        );
    }

    #[test]
    fn filter_shapes_are_distinct_and_stable() {
        let render = |shape: LoFiFilterShape| -> Vec<f64> {
            let mut d = LoFiDelay::new();
            d.time_ms = 60.0;
            d.feedback = 0.4;
            d.filter_shape = shape;
            d.update(SR);
            (0..24000)
                .map(|i| {
                    // Broadband-ish test signal.
                    let t = i as f64 / SR;
                    let s = (std::f64::consts::TAU * 220.0 * t).sin() * 0.3
                        + (std::f64::consts::TAU * 1700.0 * t).sin() * 0.2
                        + (std::f64::consts::TAU * 5200.0 * t).sin() * 0.2;
                    let v = d.tick(s, 0);
                    assert!(v.is_finite());
                    v
                })
                .collect()
        };
        let shapes = [
            LoFiFilterShape::VintageAmp,
            LoFiFilterShape::Victrola,
            LoFiFilterShape::ClockRadio,
            LoFiFilterShape::Bullhorn,
            LoFiFilterShape::Megaphone,
            LoFiFilterShape::AntiqueTelephone,
            LoFiFilterShape::CellPhone,
            LoFiFilterShape::Intercom,
        ];
        let outputs: Vec<Vec<f64>> = shapes.iter().map(|&s| render(s)).collect();
        let off = render(LoFiFilterShape::Off);
        for (i, out) in outputs.iter().enumerate() {
            let diff: f64 = out.iter().zip(&off).map(|(a, b)| (a - b).abs()).sum();
            assert!(diff > 1.0, "shape {i} should differ from Off: {diff}");
        }
        // Pairwise distinct (voicings are not duplicates).
        for i in 0..outputs.len() {
            for j in (i + 1)..outputs.len() {
                let diff: f64 = outputs[i]
                    .iter()
                    .zip(&outputs[j])
                    .map(|(a, b)| (a - b).abs())
                    .sum();
                assert!(diff > 0.5, "shapes {i} and {j} should differ: {diff}");
            }
        }
    }

    #[test]
    fn short_time_chorus_flange_viable() {
        // 10 ms + repeats = modulated-comb territory; 3 ms at max
        // repeats must ring near oscillation without blowing up.
        for (time_ms, fb) in [(10.0, 0.85), (3.0, 0.95)] {
            let mut d = LoFiDelay::new();
            d.time_ms = time_ms;
            d.feedback = fb;
            d.bit_depth = 32.0;
            d.sample_rate_div = 1.0;
            d.update(SR);
            let mut peak = 0.0f64;
            for i in 0..96000 {
                // Slow sinusoidal time wobble like the chain's mod.
                d.time_ms = time_ms + (i as f64 / SR * std::f64::consts::TAU * 0.8).sin() * 0.8;
                let s = (std::f64::consts::TAU * 440.0 * i as f64 / SR).sin() * 0.3;
                let v = d.tick(s, 0);
                assert!(v.is_finite(), "NaN at {i} (time={time_ms})");
                peak = peak.max(v.abs());
            }
            assert!(peak < 6.0, "bounded near oscillation: peak={peak}");
            assert!(peak > 0.1, "should ring: peak={peak}");
        }
    }
}
