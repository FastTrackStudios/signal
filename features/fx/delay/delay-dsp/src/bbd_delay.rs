//! BbdDelay — Bucket Brigade Device emulation with a true variable clock.
//!
//! The defining dBucket behavior (TimeLine MX spec): the delay line is a
//! fixed number of analog "stages"; delay time changes alter the read/write
//! CLOCK RATE, not the buffer contents. Audio already in the buckets is
//! preserved and re-pitched during time changes — sweep the time away and
//! back and the same audio re-emerges, unlike a digital fractional read
//! which smears or crossfades.
//!
//! Signal flow per channel:
//! Input → AA filter → `[write @ clock rate → N stages → read @ clock rate]`
//!       → reconstruction filter → output; feedback: output → tone LP →
//!       (analog-voiced, peaking near max filtering) → back into the write.
//!
//! Aliasing at long delay times is authentic and intentional: the virtual
//! clock IS low (e.g. 4096 stages / 800 ms ≈ 5.1 kHz), and the write path
//! is deliberately not oversampled.

use crate::bbd_core::{BbdCore, StageShaper};
use crate::modulation::CompanderEnv;
use crate::tilt::DecayTilt;
use audiocore_dsp::biquad::{Biquad, FilterType};
use audiocore_dsp::denormal::flush;
use audiocore_dsp::prng::XorShift32;
use audiocore_dsp::smoothing::ParamSmoother;

/// dBucket voice (TimeLine MX).
///
/// `Mx` (Brig lineage) runs 8192 virtual stages — double the clock rate at
/// any delay time, so wider bandwidth and less aliasing. `Classic`
/// (TimeLine v1) runs 4096 stages (MN3005-style): darker, grittier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BbdVoice {
    #[default]
    Mx,
    Classic,
}

impl BbdVoice {
    fn stages(self) -> f64 {
        match self {
            BbdVoice::Mx => 8192.0,
            BbdVoice::Classic => 4096.0,
        }
    }
}

/// Per-bucket charge degradation (the bucket-loss model), hooked into
/// the Holters–Parker core's write path as a [`StageShaper`].
struct BucketLoss {
    /// Severity for the current audio sample (set from the knob +
    /// clock ratio each tick).
    sev: f64,
    lp: f64,
    rng: XorShift32,
}

impl BucketLoss {
    fn new() -> Self {
        Self {
            sev: 0.0,
            lp: 0.0,
            rng: XorShift32::new(0xB0CC_E77E),
        }
    }

    fn reset(&mut self) {
        self.lp = 0.0;
    }
}

impl StageShaper for BucketLoss {
    #[inline]
    fn shape(&mut self, v: f64) -> f64 {
        if self.sev <= 0.001 {
            return v;
        }
        // HF droop: blend toward a one-pole of the written charge.
        self.lp = flush(self.lp + (v - self.lp) * (1.0 - self.sev * 0.6));
        let drooped = v + (self.lp - v) * self.sev * 0.7;
        // Charge-transfer nonlinearity + noise floor.
        let nl = drooped - self.sev * 0.35 * drooped * drooped * drooped;
        nl + self.rng.next_bipolar() * self.sev * 0.002
    }
}

/// BBD/Analog delay with variable-clock stage buffer.
pub struct BbdDelay {
    /// Delay time in milliseconds.
    pub time_ms: f64,
    /// Feedback amount (0.0–1.0).
    pub feedback: f64,
    /// LFO modulation depth (0.0–1.0). Modulates the CLOCK (true BBD
    /// chorus). Zero depth = dual-mono wet; the chain offsets the R
    /// channel's LFO phase for stereo spread when depth > 0.
    pub mod_depth: f64,
    /// LFO modulation rate in Hz.
    pub mod_rate: f64,
    /// Tone / low-pass cutoff frequency in Hz. Analog-voiced: as the
    /// cutoff drops toward maximum filtering the response morphs into a
    /// resonant peak before the rolloff (perceived-bandwidth trick).
    pub tone: f64,
    /// Clock jitter amount (0.0–1.0) — random clock-rate noise.
    pub clock_jitter: f64,
    /// Bucket loss (0.0–1.0): per-stage charge-transfer inaccuracy.
    /// HF droop + soft nonlinearity + noise floor, scaling with the knob
    /// AND with delay time (slower clock = more droop per stage).
    pub bucket_loss: f64,
    /// LFO phase offset (0.0–1.0) for stereo spread across two instances.
    pub lfo_phase_offset: f64,
    /// Voice: stage count (Mx = 8192, Classic = 4096).
    pub voice: BbdVoice,
    /// Decay EQ tilt (-1.0 = darken repeats, 0 = neutral, +1.0 = brighten).
    pub decay_tilt: f64,

    decay_tilt_eq: DecayTilt,
    /// Holters–Parker combined BBD core: clocked bucket queue with the
    /// AA/reconstruction filter banks evaluated at exact clock
    /// instants. Replaces the old interpolated stage buffer + biquads.
    core: BbdCore,
    loss: BucketLoss,
    /// Previous feedback value — the loop closes with one sample of
    /// latency because the core consumes input and produces output in
    /// one interleaved pass.
    fb_prev: f64,
    // Feedback tone filter
    tone_filter: Biquad,
    feedback_sample: f64,
    sample_rate: f64,
    smoother: ParamSmoother,
    lfo_phase: f64,
    rng: XorShift32,
    /// Smoothed jitter so clock noise wobbles instead of stepping.
    jitter_state: f64,
    /// NE570-style 2:1 compander (Raffel–Smith DAFx-10): compress into
    /// the buckets, expand at the reconstruction output. The averagers'
    /// mismatch against the DELAYED envelope is the authentic
    /// bucket-loss "breathing" on repeats, compounding per pass.
    comp_env: CompanderEnv,
    exp_env: CompanderEnv,
    /// Level-independent cubic THD constants (paper fit: a=1/8, b=1/18
    /// for 4096 stages; scaled by the voice's stage count).
    thd_a: f64,
    thd_b: f64,
    /// DC blocker after the x² term of the THD polynomial.
    thd_dc: audiocore_dsp::dc_blocker::DcBlocker,
}

impl Default for BbdDelay {
    fn default() -> Self {
        Self::new()
    }
}

impl BbdDelay {
    pub fn new() -> Self {
        Self {
            time_ms: 250.0,
            feedback: 0.4,
            mod_depth: 0.3,
            mod_rate: 1.0,
            tone: 4000.0,
            clock_jitter: 0.3,
            bucket_loss: 0.0,
            lfo_phase_offset: 0.0,
            voice: BbdVoice::Mx,
            decay_tilt: 0.0,
            decay_tilt_eq: DecayTilt::new(),
            core: BbdCore::new(),
            loss: BucketLoss::new(),
            fb_prev: 0.0,
            tone_filter: Biquad::new(),
            feedback_sample: 0.0,
            sample_rate: 48000.0,
            smoother: ParamSmoother::new(0.0),
            lfo_phase: 0.0,
            rng: XorShift32::new(0xDEAD_BEEF),
            jitter_state: 0.0,
            comp_env: CompanderEnv::new(),
            exp_env: CompanderEnv::new(),
            thd_a: 0.125,
            thd_b: 1.0 / 18.0,
            thd_dc: audiocore_dsp::dc_blocker::DcBlocker::new(),
        }
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.comp_env.configure(sample_rate);
        self.exp_env.configure(sample_rate);
        // THD scales with stage count (~1% per 1024 stages,
        // level-independent — the giveaway BBD flavor; NOT clipping).
        let stage_scale = self.voice.stages() / 4096.0;
        self.thd_a = 0.125 * stage_scale;
        self.thd_b = (1.0 / 18.0) * stage_scale;
        self.thd_dc.set_cutoff(10.0, sample_rate);
        self.sample_rate = sample_rate;

        // Holters–Parker core: stage count from the voice; the Juno
        // filter cutoffs scale with the nominal clock (2·stages/delay,
        // referenced to the ≈100 kHz clock the filters were designed
        // at), so long delays darken like the hardware and the Mx
        // voice's doubled clock keeps it brighter than Classic at the
        // same time setting.
        let n_stages = self.voice.stages();
        let delay_sec = (self.time_ms * 0.001).max(0.001);
        let clock_hz = 2.0 * n_stages / delay_sec;
        let cutoff_scale = (clock_hz / 100_000.0).clamp(0.22, 1.15);
        self.core
            .configure(sample_rate, n_stages as usize, cutoff_scale);

        // Analog-voiced tone: Q rises as the cutoff drops, so maximum
        // filtering has a resonant bump before the rolloff.
        let tone_freq = self.tone.clamp(200.0, sample_rate * 0.45);
        let norm = ((tone_freq - 200.0) / 11_800.0).clamp(0.0, 1.0);
        let q = 0.707 + (1.0 - norm).powi(2) * 1.8;
        self.tone_filter
            .set(FilterType::Lowpass, tone_freq, q, sample_rate);

        // Decay EQ: tilt filter in feedback path
        self.decay_tilt_eq.configure(self.decay_tilt, sample_rate);

        // Smooth delay-time (= clock-rate) changes; the sweep itself is
        // what produces the analog pitch-bend character.
        self.smoother.set_time_seeded(0.15, sample_rate, self.time_ms * 0.001 * sample_rate);
    }

    pub fn tick(&mut self, input: f64, ch: usize) -> f64 {
        let target_delay = self.time_ms * 0.001 * self.sample_rate;
        self.smoother.set_target(target_delay);
        let smooth_delay = self.smoother.tick().max(16.0);

        // LFO + jitter modulate the CLOCK RATE (true BBD behavior: pitch
        // wobble on everything in the buckets, not a moving read tap).
        let lfo_inc = self.mod_rate / self.sample_rate;
        self.lfo_phase += lfo_inc;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase -= 1.0;
        }
        let lfo =
            ((self.lfo_phase + self.lfo_phase_offset) * std::f64::consts::TAU).sin();
        // Smoothed random clock noise.
        self.jitter_state += 0.002 * (self.rng.next_bipolar() - self.jitter_state);
        let jitter = self.jitter_state * self.clock_jitter * 0.01;

        let n_stages = self.voice.stages();
        let clock_factor = (1.0 + lfo * self.mod_depth * 0.04 + jitter).clamp(0.25, 4.0);
        self.core.set_clock_samples(smooth_delay / clock_factor);

        // Bucket-loss severity: slower clock (fewer stage writes per
        // host sample) = more droop per stage.
        let ratio = (n_stages / smooth_delay).min(1.0);
        self.loss.sev = if self.bucket_loss <= 0.001 {
            0.0
        } else {
            self.bucket_loss * (0.35 + 0.65 * (1.0 - ratio))
        };

        // Loop front half: compander compress + the BBD's
        // level-independent cubic THD (f(x) = x − a·x² − b·x³,
        // DC-blocked) + charge noise (~−60 dB, inside the loop so
        // feedback > 1 self-oscillates from the floor). The Holters–
        // Parker core then handles AA filtering, the clocked buckets
        // (per-write bucket loss via the shaper), and reconstruction.
        let compressed = self.comp_env.compress(input + self.fb_prev);
        let x = compressed.clamp(-1.5, 1.5);
        let shaped = x - self.thd_a * x * x - self.thd_b * x * x * x;
        let write_in = self.thd_dc.tick(shaped) + self.rng.next_bipolar() * 1.0e-3;

        let raw = self.core.process(write_in, &mut self.loss);
        let output = self.exp_env.expand(raw);

        // Feedback path: tone filter (analog-voiced) → tilt → limit.
        // Closes with one sample of loop latency (the core consumes
        // input and produces output in one interleaved pass).
        let mut fb = output * self.feedback;
        fb = self.tone_filter.tick(fb, ch);
        fb = self.decay_tilt_eq.tick(fb, ch);
        fb = fb.clamp(-1.5, 1.5);
        self.fb_prev = fb;
        self.feedback_sample = fb;
        output
    }

    pub fn last_feedback(&self) -> f64 {
        self.feedback_sample
    }

    pub fn reset(&mut self) {
        self.core.reset();
        self.loss.reset();
        self.fb_prev = 0.0;
        self.jitter_state = 0.0;
        self.comp_env.reset();
        self.exp_env.reset();
        self.thd_dc.reset();
        self.tone_filter.reset();
        self.decay_tilt_eq.reset();
        self.feedback_sample = 0.0;
        self.smoother.reset(0.0);
        self.lfo_phase = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    fn make(time_ms: f64) -> BbdDelay {
        let mut d = BbdDelay::new();
        d.time_ms = time_ms;
        d.feedback = 0.0;
        d.mod_depth = 0.0;
        d.clock_jitter = 0.0;
        d.update(SR);
        d
    }

    #[test]
    fn silence_in_silence_out() {
        // The loop carries an analog noise floor (~−60 dB inside,
        // expander-gated at the output) — silence in means a residual
        // hiss floor out, far below audibility but not digital zero.
        let mut d = make(200.0);
        let mut peak = 0.0f64;
        for _ in 0..48000 {
            peak = peak.max(d.tick(0.0, 0).abs());
        }
        assert!(peak < 1.0e-3, "noise floor too hot: {peak}");
    }

    #[test]
    fn impulse_delayed() {
        // A short burst (physical transient — the compander's 1 ms
        // attack needs real signal duration; a 1-sample impulse is
        // legitimately crushed by an expander) must come back at the
        // delay time at significant level.
        let mut d = make(150.0);
        let burst = (0.003 * SR) as usize;
        let expected = (150.0 * SR / 1000.0) as i64;
        let mut peak = 0.0f64;
        let mut peak_idx = 0i64;
        for i in 0..24000 {
            let input = if i < burst {
                (core::f64::consts::TAU * 1000.0 * i as f64 / SR).sin() * 0.8
            } else {
                0.0
            };
            let out = d.tick(input, 0).abs();
            if i as i64 > burst as i64 + 100 && out > peak {
                peak = out;
                peak_idx = i as i64;
            }
        }
        assert!(
            (peak_idx - expected).unsigned_abs() < 500,
            "peak at {peak_idx}, expected near {expected}"
        );
        assert!(peak > 0.2, "Peak should be significant: {peak}");
    }

    #[test]
    fn modulation_changes_output() {
        let mut d_clean = make(100.0);
        let mut d_mod = make(100.0);
        d_mod.mod_depth = 0.8;
        d_mod.mod_rate = 2.0;
        d_mod.update(SR);

        let mut diff = 0.0;
        for i in 0..19200 {
            let s = (std::f64::consts::PI * 2.0 * 440.0 * i as f64 / SR).sin() * 0.5;
            let a = d_clean.tick(s, 0);
            let b = d_mod.tick(s, 0);
            diff += (a - b).abs();
        }
        assert!(diff > 0.1, "Modulation should change output: diff={diff}");
    }

    #[test]
    fn no_nan() {
        let mut d = BbdDelay::new();
        d.time_ms = 200.0;
        d.feedback = 0.7;
        d.mod_depth = 0.5;
        d.mod_rate = 3.0;
        d.clock_jitter = 0.5;
        d.bucket_loss = 0.8;
        d.update(SR);

        for i in 0..96000 {
            let input = (std::f64::consts::PI * 2.0 * 440.0 * i as f64 / SR).sin() * 0.5;
            let out = d.tick(input, 0);
            assert!(out.is_finite(), "NaN at sample {i}");
            assert!(out.abs() < 10.0, "Runaway at {i}: {out}");
        }
    }

    /// Count rising zero crossings in a window — cheap pitch estimate.
    fn zero_crossings(buf: &[f64]) -> usize {
        let mut n = 0;
        for w in buf.windows(2) {
            if w[0] <= 0.0 && w[1] > 0.0 {
                n += 1;
            }
        }
        n
    }

    /// The defining dBucket behavior: halving the delay time doubles the
    /// clock, so audio already in the buckets plays back at double pitch.
    /// A digital fractional read would play it at the original pitch.
    #[test]
    fn variable_clock_repitches_stored_audio() {
        let mut d = make(400.0);
        // Kill the time smoother's 150 ms lag influence by allowing
        // settle time after the switch: switch early, well before the
        // burst is due out at its new (earlier) arrival.
        let f_in = 500.0;

        // Write a 60 ms 500 Hz burst.
        let burst_len = (SR * 0.06) as usize;
        for i in 0..burst_len {
            let s = (std::f64::consts::TAU * f_in * i as f64 / SR).sin() * 0.8;
            d.tick(s, 0);
        }
        // Immediately halve the delay time: clock doubles.
        d.time_ms = 200.0;
        d.update(SR);

        // Collect output long enough to catch the burst. With the clock
        // at 2x, the remaining transit is compressed and pitch doubles.
        let mut out = Vec::with_capacity((SR * 0.5) as usize);
        for _ in 0..(SR * 0.5) as usize {
            out.push(d.tick(0.0, 0));
        }

        // Find the emitted burst: the contiguous high-energy region.
        let peak = out.iter().fold(0.0f64, |m, s| m.max(s.abs()));
        assert!(peak > 0.05, "burst should emerge: peak={peak}");
        let thresh = peak * 0.25;
        let start = out.iter().position(|s| s.abs() > thresh).unwrap();
        let end = out.len() - out.iter().rev().position(|s| s.abs() > thresh).unwrap();
        let window = &out[start..end];
        assert!(window.len() > 200, "burst window too small: {}", window.len());

        // Measured frequency of the emitted burst.
        let secs = window.len() as f64 / SR;
        let f_out = zero_crossings(window) as f64 / secs;
        let ratio = f_out / f_in;
        assert!(
            ratio > 1.5,
            "stored audio should re-pitch upward with a faster clock: \
             f_out={f_out:.0} Hz, ratio={ratio:.2} (digital read would be ~1.0)"
        );
    }

    /// Sweep the time away and back before stored audio plays out: the
    /// burst must re-emerge intact at ~its original pitch (the "come back
    /// to where you were" behavior), not smeared away.
    #[test]
    fn time_sweep_away_and_back_preserves_audio() {
        let mut d = make(600.0);
        let f_in = 500.0;
        let burst_len = (SR * 0.06) as usize;
        for i in 0..burst_len {
            let s = (std::f64::consts::TAU * f_in * i as f64 / SR).sin() * 0.8;
            d.tick(s, 0);
        }
        // Excursion: 600 -> 300 -> 600 ms, 50 ms in each leg, well before
        // the burst's ~600 ms transit completes.
        d.time_ms = 300.0;
        d.update(SR);
        for _ in 0..(SR * 0.05) as usize {
            d.tick(0.0, 0);
        }
        d.time_ms = 600.0;
        d.update(SR);

        let mut out = Vec::with_capacity((SR * 1.2) as usize);
        for _ in 0..(SR * 1.2) as usize {
            out.push(d.tick(0.0, 0));
        }
        let peak = out.iter().fold(0.0f64, |m, s| m.max(s.abs()));
        assert!(peak > 0.05, "burst should survive the sweep: peak={peak}");

        let thresh = peak * 0.25;
        let start = out.iter().position(|s| s.abs() > thresh).unwrap();
        let end = out.len() - out.iter().rev().position(|s| s.abs() > thresh).unwrap();
        let window = &out[start..end];
        let secs = window.len() as f64 / SR;
        let f_out = zero_crossings(window) as f64 / secs;
        let ratio = f_out / f_in;
        assert!(
            (0.8..1.25).contains(&ratio),
            "after returning, pitch should be ~original: ratio={ratio:.2}"
        );
    }

    /// Bucket loss degrades fidelity, and long delay times (slow clock)
    /// degrade more than short ones at the same knob setting.
    #[test]
    fn bucket_loss_degrades_more_at_long_times() {
        let distortion = |time_ms: f64, loss: f64| -> f64 {
            let mut clean = make(time_ms);
            let mut lossy = make(time_ms);
            lossy.bucket_loss = loss;
            lossy.update(SR);
            let mut diff = 0.0;
            let mut energy = 0.0;
            let n = (SR * (time_ms / 1000.0 + 0.4)) as usize;
            for i in 0..n {
                let s = (std::f64::consts::TAU * 880.0 * i as f64 / SR).sin() * 0.5;
                let a = clean.tick(s, 0);
                let b = lossy.tick(s, 0);
                diff += (a - b) * (a - b);
                energy += a * a;
            }
            diff / energy.max(1e-12)
        };

        let short = distortion(100.0, 0.7);
        let long = distortion(700.0, 0.7);
        assert!(short > 1e-6, "loss should degrade even short delays: {short}");
        assert!(
            long > short * 1.2,
            "slower clock should degrade more: short={short:.6}, long={long:.6}"
        );
    }

    /// Analog-voiced filter: near maximum filtering the response gains a
    /// resonant bump before the rolloff (bandpass-ish), so energy near the
    /// cutoff exceeds energy an octave below it.
    #[test]
    fn tone_filter_peaks_near_cutoff_at_max_filtering() {
        let band_energy = |freq: f64| -> f64 {
            let mut d = make(150.0);
            d.feedback = 0.85; // recirculate so the tone filter shapes hard
            d.tone = 800.0; // heavy filtering
            d.update(SR);
            let mut energy = 0.0;
            for i in 0..96000 {
                let s = (std::f64::consts::TAU * freq * i as f64 / SR).sin() * 0.3;
                let out = d.tick(s, 0);
                if i > 24000 {
                    energy += out * out;
                }
            }
            energy
        };
        // 700 Hz sits under the resonant bump; 200 Hz is an octave-plus
        // below and only sees the flat passband.
        let near = band_energy(700.0);
        let low = band_energy(200.0);
        assert!(
            near > low * 1.15,
            "peaking response near cutoff expected: near={near:.3}, low={low:.3}"
        );
    }

    /// Voices differ: at 500 ms the Classic clock (4096 stages ≈ 8.2 kHz)
    /// puts 6 kHz above its Nyquist, so the tone aliases away from 6 kHz.
    /// Mx (8192 stages ≈ 16.4 kHz clock) reproduces it on-frequency.
    /// Measure on-frequency content with a single-bin DFT, not raw energy
    /// (aliased energy would still show up in an RMS measure).
    #[test]
    fn voices_differ_in_fidelity() {
        let on_freq = |voice: BbdVoice| -> f64 {
            let mut d = make(500.0);
            d.voice = voice;
            d.update(SR);
            let f = 6000.0;
            let (mut re, mut im) = (0.0f64, 0.0f64);
            let mut n = 0.0;
            for i in 0..48000 {
                let ph = std::f64::consts::TAU * f * i as f64 / SR;
                let out = d.tick(ph.sin() * 0.5, 0);
                if i > 26000 {
                    re += out * ph.cos();
                    im += out * ph.sin();
                    n += 1.0;
                }
            }
            ((re * re + im * im).sqrt()) / n
        };
        let mx = on_freq(BbdVoice::Mx);
        let classic = on_freq(BbdVoice::Classic);
        assert!(
            mx > classic * 1.5,
            "Mx should keep 6 kHz on-frequency at 500 ms: mx={mx:.5}, classic={classic:.5}"
        );
    }

    /// Stereo modulation: identical engines with offset LFO phase produce
    /// different outputs when depth > 0 and identical when depth = 0.
    #[test]
    fn phase_offset_only_matters_with_modulation() {
        let render = |depth: f64, offset: f64| -> Vec<f64> {
            let mut d = make(200.0);
            d.mod_depth = depth;
            d.mod_rate = 1.5;
            d.lfo_phase_offset = offset;
            d.update(SR);
            (0..24000)
                .map(|i| {
                    let s = (std::f64::consts::TAU * 440.0 * i as f64 / SR).sin() * 0.5;
                    d.tick(s, 0)
                })
                .collect()
        };
        let dry_a = render(0.0, 0.0);
        let dry_b = render(0.0, 0.25);
        let mod_a = render(0.6, 0.0);
        let mod_b = render(0.6, 0.25);

        let diff = |a: &[f64], b: &[f64]| -> f64 {
            a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum()
        };
        assert!(
            diff(&dry_a, &dry_b) < 1e-9,
            "no modulation -> phase offset must not matter (dual mono)"
        );
        assert!(
            diff(&mod_a, &mod_b) > 0.1,
            "modulation + phase offset should decorrelate the channels"
        );
    }

    #[test]
    fn overdriven_feedback_self_oscillates_from_the_noise_floor() {
        // With loop gain > 1 a real BBD sings: the in-loop charge noise
        // seeds runaway regeneration (bounded by the loop clamp). A
        // noiseless digital loop would stay at exact zero forever.
        let mut d = make(120.0);
        d.feedback = 1.1;
        d.update(SR);
        let mut late = 0.0f64;
        for i in 0..(6 * 48000) {
            let out = d.tick(0.0, 0);
            if i > 5 * 48000 {
                late += out * out;
            }
        }
        assert!(
            late > 1e-4,
            "feedback > 1 should self-oscillate from the noise floor: {late}"
        );
        assert!(late.is_finite());
    }
}
