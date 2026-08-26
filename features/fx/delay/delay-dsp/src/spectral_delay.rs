//! SpectralDelay — granular ambient delay.
//!
//! TimeLine MX "Spectral" machine parity: the wet path is a granular
//! cloud read from the delay buffer. Grains spawn at a configurable
//! density (free-running Hz or synced to a fraction of the delay time),
//! can be time-stretched (slower read speed), and blend in octave-up
//! grains for a spectral halo.
//!
//! Spec corrections applied (spec/timeline-mx-reference.md): grain
//! Shape (Soft/Swell/SoftPluck/Pluck/Bounce), Direction (Forward/
//! Reverse/Both = random per grain), Spread = random grain placement
//! across the delay time, Stretch = random per-grain time-stretch,
//! Octave = random per-grain octave-up probability.
//!
//! Deep-pass notes: grains are time-domain over one `DelayLine` with
//! cubic reads (read-rate repitching measured clean enough — see the
//! octave inharmonicity test). Octave/reverse grains cap their duration
//! at spawn so every grain completes its full window (no truncation
//! clicks). Bounce carries a randomized per-grain resonant peak
//! (Biquad), per the spec. The regeneration path runs a light fixed
//! phase-dispersion allpass pair + a gentle high shelf so repeats
//! evolve tonally like the hardware's frequency-domain footprint.
//! // interpretation: matches the described behavior; the MX itself
//! // is FFT-based.

use crate::tilt::DecayTilt;
use audiocore_dsp::biquad::{Biquad, FilterType};
use audiocore_dsp::delay_line::DelayLine;
use audiocore_dsp::prng::XorShift32;
use audiocore_dsp::smoothing::ParamSmoother;

// 16 voices: 1/32-of-repeat density with 2x-overlap windows plus
// stretch needs more simultaneous grains than the old cap of 8 —
// exhausted voices skip spawns and the cloud thins audibly.
const NUM_GRAINS: usize = 16;

/// Grain envelope shape (TimeLine MX Spectral "Shape").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GrainShape {
    /// Slow attack, slow release (Hann-like).
    #[default]
    Soft,
    /// Slow attack, fast release.
    Swell,
    /// Fast attack, slow release.
    SoftPluck,
    /// Very fast attack.
    Pluck,
    /// Fast attack + randomized per-grain lowpass.
    Bounce,
}

/// Grain playback direction (TimeLine MX Spectral "Direction").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GrainDirection {
    #[default]
    Forward,
    Reverse,
    /// Random per grain.
    Both,
}

#[derive(Debug, Clone)]
struct Grain {
    active: bool,
    /// Current read offset behind the write head, in samples.
    offset: f64,
    /// Per-sample offset drift. 0 = normal pitch/direction; positive
    /// drifts back in time (reverse/stretch), negative = pitched up.
    drift: f64,
    /// Age in samples.
    age: f64,
    /// Duration in samples.
    dur: f64,
    gain: f64,
    /// Per-grain stereo pan (-1..1); applied by `tick_stereo`, scaled
    /// from `spread` at spawn (subtle scatter, ≤ ±0.6).
    pan: f64,
    /// Bounce: randomized per-grain resonant peak (spec). `None` for
    /// the other shapes.
    filt: Option<Biquad>,
}

impl Grain {
    fn idle() -> Self {
        Self {
            active: false,
            offset: 0.0,
            drift: 0.0,
            age: 0.0,
            dur: 1.0,
            gain: 0.0,
            pan: 0.0,
            filt: None,
        }
    }
}

/// Post-granular diffuser translated from Mutable Instruments Clouds
/// (`clouds/dsp/fx/diffuser.h`, MIT, © Émilie Gillet): four series
/// Schroeder allpasses per channel, k = 0.625, with the original delay
/// constants (spec'd at Clouds' 32 kHz internal rate, rescaled to the
/// host rate; the L/R sets differ slightly for decorrelation). A large
/// part of the hardware granular "expensive" smear.
struct CloudsDiffuser {
    lines: [DelayLine; 4],
    delays: [f64; 4],
}

/// Per-channel allpass delays in samples at 32 kHz (Clouds constants).
const DIFFUSER_L_32K: [f64; 4] = [126.0, 180.0, 269.0, 444.0];
const DIFFUSER_R_32K: [f64; 4] = [151.0, 205.0, 245.0, 405.0];
const DIFFUSER_K: f64 = 0.625;

impl CloudsDiffuser {
    fn new(base_32k: &[f64; 4], sample_rate: f64) -> Self {
        let scale = sample_rate / 32_000.0;
        let delays = core::array::from_fn(|i| base_32k[i] * scale);
        Self {
            lines: core::array::from_fn(|i| DelayLine::new((base_32k[i] * scale) as usize + 8)),
            delays,
        }
    }

    fn resize(&mut self, base_32k: &[f64; 4], sample_rate: f64) {
        let scale = sample_rate / 32_000.0;
        #[allow(clippy::needless_range_loop)] // i spans parallel tables
        for i in 0..4 {
            self.delays[i] = base_32k[i] * scale;
            let needed = self.delays[i] as usize + 8;
            if self.lines[i].len() < needed {
                self.lines[i] = DelayLine::new(needed);
            }
        }
    }

    #[inline]
    fn tick(&mut self, input: f64) -> f64 {
        let mut x = input;
        #[allow(clippy::needless_range_loop)] // i spans parallel tables
        for i in 0..4 {
            let delayed = self.lines[i].read_linear(self.delays[i]);
            let v = x - DIFFUSER_K * delayed;
            self.lines[i].write(v);
            x = delayed + DIFFUSER_K * v;
        }
        x
    }

    fn reset(&mut self) {
        for l in &mut self.lines {
            l.clear();
        }
    }
}

/// Grain-spawn density mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DensityMode {
    /// Grains per second. TimeLine's free mode spans 6–250 ms per
    /// grain, i.e. 4–166.7 Hz (clamped).
    FreeHz(f64),
    /// One grain per `fraction` of the delay time (1.0 = every full
    /// delay period, 1/32 = 32 grains per period). TimeLine's synced
    /// density 1/1–1/32.
    Synced(f64),
}

pub struct SpectralDelay {
    /// Base delay time in ms (clamped to 60–2500).
    pub time_ms: f64,
    /// Feedback amount (0.0–1.0) — regenerates from a plain tap.
    pub feedback: f64,
    /// Grain density.
    pub density: DensityMode,
    /// Stretch (0.0–1.0): grains read slower as this rises (down to 0.5×).
    pub stretch: f64,
    /// Octave (0.0–1.0): probability of a random per-grain octave-up.
    pub octave: f64,
    /// Spread (0.0–1.0): random grain placement across the delay time
    /// (0 = every grain lands exactly on the delay time).
    pub spread: f64,
    /// Grain envelope shape.
    pub shape: GrainShape,
    /// Grain playback direction.
    pub direction: GrainDirection,
    /// Post-granular diffusion (0.0–1.0): dry↔diffused crossfade of
    /// the grain cloud through the Clouds allpass chain. FTS voicing
    /// extra (no hardware param); approximates the MX's FFT-domain
    /// smear. Default 0.5.
    pub diffusion: f64,
    /// High-cut in the feedback path (0 = off).
    pub hicut_freq: f64,
    /// Decay EQ tilt (shared engine param).
    pub decay_tilt: f64,

    delay: DelayLine,
    grains: [Grain; NUM_GRAINS],
    spawn_countdown: f64,
    hicut: Biquad,
    decay_tilt_eq: DecayTilt,
    // Fixed regeneration voicing: phase-dispersion allpass pair + a
    // gentle high shelf, so repeats evolve like the MX's FFT footprint.
    // interpretation — behavior match, not the hardware algorithm.
    disp_state: [[f64; 2]; 2],
    diffuser_l: CloudsDiffuser,
    diffuser_r: CloudsDiffuser,
    disp_shelf: Biquad,
    feedback_sample: f64,
    sample_rate: f64,
    smoother: ParamSmoother,
    rng: XorShift32,
}

impl Default for SpectralDelay {
    fn default() -> Self {
        Self::new()
    }
}

impl SpectralDelay {
    pub const MIN_TIME_MS: f64 = 60.0;
    pub const MAX_TIME_MS: f64 = 2500.0;
    const MAX_DELAY_S: f64 = 3.0;

    pub fn new() -> Self {
        Self {
            time_ms: 500.0,
            feedback: 0.4,
            density: DensityMode::Synced(1.0 / 8.0),
            stretch: 0.0,
            octave: 0.0,
            spread: 0.0,
            shape: GrainShape::Soft,
            direction: GrainDirection::Forward,
            hicut_freq: 0.0,
            diffusion: 0.5,
            decay_tilt: 0.0,
            delay: DelayLine::new(48000 * 3 + 1024),
            grains: std::array::from_fn(|_| Grain::idle()),
            spawn_countdown: 0.0,
            hicut: Biquad::new(),
            decay_tilt_eq: DecayTilt::new(),
            disp_state: [[0.0; 2]; 2],
            diffuser_l: CloudsDiffuser::new(&DIFFUSER_L_32K, 48000.0),
            diffuser_r: CloudsDiffuser::new(&DIFFUSER_R_32K, 48000.0),
            disp_shelf: Biquad::new(),
            feedback_sample: 0.0,
            sample_rate: 48000.0,
            smoother: ParamSmoother::new(0.0),
            rng: XorShift32::new(0x05EC_70A1),
        }
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.diffuser_l.resize(&DIFFUSER_L_32K, sample_rate);
        self.diffuser_r.resize(&DIFFUSER_R_32K, sample_rate);
        self.sample_rate = sample_rate;
        self.time_ms = self.time_ms.clamp(Self::MIN_TIME_MS, Self::MAX_TIME_MS);

        let max_len = (sample_rate * Self::MAX_DELAY_S) as usize + 1024;
        if self.delay.len() < max_len {
            self.delay = DelayLine::new(max_len);
        }

        if self.hicut_freq > 0.0 {
            self.hicut
                .set(FilterType::Lowpass, self.hicut_freq, 0.707, sample_rate);
        }
        self.decay_tilt_eq.configure(self.decay_tilt, sample_rate);

        self.smoother
            .set_time_seeded(0.15, sample_rate, self.time_ms * 0.001 * sample_rate);

        // Fixed regen voicing: gentle -1.5 dB shelf above ~4 kHz.
        self.disp_shelf.set(
            FilterType::HighShelf { gain_db: -1.5 },
            4000.0,
            0.707,
            sample_rate,
        );
    }

    fn spawn_interval_samples(&self, delay_samples: f64) -> f64 {
        match self.density {
            // Free mode: 6–250 ms per grain (spec) = 4–166.7 Hz.
            DensityMode::FreeHz(hz) => self.sample_rate / hz.clamp(4.0, 1000.0 / 6.0),
            DensityMode::Synced(fraction) => {
                (delay_samples * fraction.clamp(1.0 / 32.0, 1.0)).max(64.0)
            }
        }
    }

    fn spawn_grain(&mut self, delay_samples: f64, interval: f64) {
        let slot = match self.grains.iter().position(|g| !g.active) {
            Some(i) => i,
            None => return, // all voices busy — skip, no stealing clicks
        };

        let rand01 = |rng: &mut XorShift32| (rng.next_bipolar() + 1.0) * 0.5;

        // Octave: random per-grain octave-up with probability `octave`.
        let octave_up = rand01(&mut self.rng) < self.octave;
        // Stretch: RANDOM per-grain time-stretch (0..stretch of half-speed).
        let stretch_amt = rand01(&mut self.rng) * self.stretch;
        let speed = if octave_up {
            2.0
        } else {
            1.0 - stretch_amt * 0.5
        };

        // Direction: reverse grains drift backward through the buffer
        // (offset grows by 1 + speed instead of shrinking).
        let reverse = match self.direction {
            GrainDirection::Forward => false,
            GrainDirection::Reverse => true,
            GrainDirection::Both => self.rng.next_bipolar() > 0.0,
        };
        let drift = if reverse { 1.0 + speed } else { 1.0 - speed };

        // Grains overlap 2×: duration = twice the spawn interval.
        let mut dur = (interval * 2.0).clamp(256.0, delay_samples.max(512.0));

        // Spread: random placement across the delay time (0 = on-time).
        // A small jitter always applies so voices don't phase-lock.
        let jitter = self.rng.next_bipolar() * interval * 0.25;
        let placement = rand01(&mut self.rng) * self.spread * delay_samples * 0.9;
        let mut offset = (delay_samples - placement + jitter).max(4.0);

        // Window completion: a grain dies early if its read head walks
        // off either buffer edge mid-window (a truncated window is a
        // click). Cap the duration so the full envelope always plays.
        let max_read = self.delay.len() as f64 - 4.0;
        if drift < 0.0 {
            // Pitched-up grains walk toward the write head. Push the
            // start point out if needed, then cap.
            let min_offset = 8.0 + 256.0 * -drift;
            offset = offset.max(min_offset).min(max_read);
            dur = dur.min((offset - 8.0) / -drift);
        } else if drift > 0.0 {
            // Reverse/stretch grains walk toward the buffer tail.
            dur = dur.min((max_read - offset).max(64.0) / drift);
        }
        let dur = dur.max(64.0);

        // Bounce: randomized per-grain resonant peak (spec):
        // 400 Hz – 4 kHz center, Q 2–5, +7 dB.
        let filt = if self.shape == GrainShape::Bounce {
            let fc = 400.0 * (10.0f64).powf(rand01(&mut self.rng));
            let q = 2.0 + rand01(&mut self.rng) * 3.0;
            let mut b = Biquad::new();
            b.set(FilterType::Peak { gain_db: 7.0 }, fc, q, self.sample_rate);
            Some(b)
        } else {
            None
        };

        // Stereo scatter rides the spread param: on-grid clouds stay
        // centered, scattered clouds open up (subtle, ≤ ±0.6).
        let pan = self.rng.next_bipolar() * 0.6 * self.spread;

        self.grains[slot] = Grain {
            active: true,
            offset,
            drift,
            age: 0.0,
            dur,
            gain: 1.0,
            pan,
            filt,
        };
    }

    /// Grain window for the active shape at `phase` in [0, 1).
    #[inline]
    fn window(shape: GrainShape, phase: f64) -> f64 {
        match shape {
            GrainShape::Soft => {
                let w = (std::f64::consts::PI * phase).sin();
                w * w
            }
            GrainShape::Swell => {
                // Slow attack, fast release.
                if phase < 0.8 {
                    let p = phase / 0.8;
                    p * p
                } else {
                    1.0 - (phase - 0.8) / 0.2
                }
            }
            GrainShape::SoftPluck => {
                // Fast attack, slow release.
                if phase < 0.1 {
                    phase / 0.1
                } else {
                    let p = 1.0 - (phase - 0.1) / 0.9;
                    p * p
                }
            }
            GrainShape::Pluck => {
                // Very fast attack, exponential-ish release.
                if phase < 0.02 {
                    phase / 0.02
                } else {
                    let p = 1.0 - (phase - 0.02) / 0.98;
                    p * p * p
                }
            }
            GrainShape::Bounce => {
                // Fast attack; the per-grain filter does the character.
                if phase < 0.05 {
                    phase / 0.05
                } else {
                    let p = 1.0 - (phase - 0.05) / 0.95;
                    p * p
                }
            }
        }
    }

    pub fn tick(&mut self, input: f64, ch: usize) -> f64 {
        self.tick_inner(input, ch, false).0
    }

    /// Stereo tick: each grain lands in the stereo field per its
    /// spawn-time random pan (scaled by `spread`). Feedback stays a
    /// mono plain tap, identical to [`Self::tick`].
    pub fn tick_stereo(&mut self, input: f64) -> (f64, f64) {
        self.tick_inner(input, 0, true)
    }

    fn tick_inner(&mut self, input: f64, ch: usize, stereo: bool) -> (f64, f64) {
        let target_delay = self.time_ms * 0.001 * self.sample_rate;
        self.smoother.set_target(target_delay);
        let smooth_delay = self.smoother.tick();
        let max_read = self.delay.len() as f64 - 4.0;

        // Spawn scheduler. Synced density is metronomic; free density
        // randomizes each gap ±50% ("Off: grain fragments repeat
        // randomly" — the manual's sync distinction).
        let interval = self.spawn_interval_samples(smooth_delay);
        self.spawn_countdown -= 1.0;
        if self.spawn_countdown <= 0.0 {
            self.spawn_grain(smooth_delay, interval);
            self.spawn_countdown = match self.density {
                DensityMode::Synced(_) => interval,
                DensityMode::FreeHz(_) => interval * (1.0 + self.rng.next_bipolar() * 0.5),
            };
        }

        // Sum grain voices with the selected shape window.
        let shape = self.shape;
        let mut wet_l = 0.0;
        let mut wet_r = 0.0;
        let mut active = 0u32;
        for g in &mut self.grains {
            if !g.active {
                continue;
            }
            let phase = g.age / g.dur;
            if phase >= 1.0 || g.offset < 2.0 || g.offset > max_read {
                g.active = false;
                continue;
            }
            let window = Self::window(shape, phase);
            let mut sample = self.delay.read_cubic(g.offset);
            if let Some(filt) = g.filt.as_mut() {
                sample = filt.tick(sample, 0);
            }
            let v = sample * window * g.gain;
            if stereo {
                let (gl, gr) = crate::pan_gains(g.pan);
                wet_l += v * gl;
                wet_r += v * gr;
            } else {
                wet_l += v;
            }

            g.offset += g.drift;
            g.age += 1.0;
            active += 1;
        }
        // Overlap normalization: ~2 grains sound at once by design.
        if active > 2 {
            let norm = (active as f64 / 2.0).sqrt();
            wet_l /= norm;
            wet_r /= norm;
        }

        // Clouds-style post-granular diffusion: crossfade the cloud
        // through the allpass chain (out = in + amount·(diffused − in),
        // the Clouds mix law). The feedback tap below stays undiffused
        // so regeneration keeps its rhythmic anchor.
        let amount = self.diffusion.clamp(0.0, 1.0);
        if amount > 0.001 {
            let dl = self.diffuser_l.tick(wet_l);
            wet_l += amount * (dl - wet_l);
            if stereo {
                let dr = self.diffuser_r.tick(wet_r);
                wet_r += amount * (dr - wet_r);
            }
        }

        // Feedback from a plain (non-granular) tap so regeneration stays
        // rhythmically anchored.
        let mut fb = self.delay.read_cubic(smooth_delay.clamp(1.0, max_read)) * self.feedback;

        // Spectral signature in the regen path: two first-order
        // allpasses (phase dispersion) + a gentle high shelf, so
        // repeats evolve tonally through regeneration like the MX's
        // frequency-domain process. Fixed and light by design.
        // interpretation: behavior match, not the FFT algorithm.
        for (i, &a) in [0.35f64, 0.55].iter().enumerate() {
            let st = &mut self.disp_state[i];
            let x = fb;
            let y = -a * x + st[0] + a * st[1];
            st[0] = x;
            st[1] = y;
            fb = y;
        }
        fb = self.disp_shelf.tick(fb, ch);

        if self.hicut_freq > 0.0 {
            fb = self.hicut.tick(fb, ch);
        }
        fb = self.decay_tilt_eq.tick(fb, ch);
        fb = fb.clamp(-1.5, 1.5);

        self.delay.write(input + fb);
        self.feedback_sample = fb;

        (wet_l, wet_r)
    }

    pub fn last_feedback(&self) -> f64 {
        self.feedback_sample
    }

    pub fn reset(&mut self) {
        self.diffuser_l.reset();
        self.diffuser_r.reset();
        self.delay.clear();
        self.grains = std::array::from_fn(|_| Grain::idle());
        self.spawn_countdown = 0.0;
        self.hicut.reset();
        self.decay_tilt_eq.reset();
        self.disp_state = [[0.0; 2]; 2];
        self.disp_shelf.reset();
        self.feedback_sample = 0.0;
        self.smoother.reset(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    #[test]
    fn produces_delayed_granular_output() {
        let mut d = SpectralDelay::new();
        d.time_ms = 200.0;
        d.feedback = 0.0;
        d.update(SR);

        let mut pre_energy = 0.0;
        let mut post_energy = 0.0;
        for i in 0..48000 {
            let input = if i < 200 { 0.8 } else { 0.0 };
            let out = d.tick(input, 0);
            // 200 ms = 9600 samples: grains read at/after the delay time.
            if i < 8000 {
                pre_energy += out * out;
            } else {
                post_energy += out * out;
            }
        }
        assert!(
            post_energy > 0.001,
            "granular wet should appear after the delay time: {post_energy}"
        );
        assert!(
            post_energy > pre_energy,
            "energy should be concentrated after the delay time"
        );
    }

    #[test]
    fn octave_blend_and_stretch_no_nan() {
        let mut d = SpectralDelay::new();
        d.time_ms = 300.0;
        d.feedback = 0.6;
        d.octave = 0.7;
        d.stretch = 1.0;
        d.density = DensityMode::Synced(1.0 / 32.0);
        d.update(SR);

        for i in 0..96000 {
            let input = (std::f64::consts::TAU * 440.0 * i as f64 / SR).sin() * 0.5;
            let out = d.tick(input, 0);
            assert!(out.is_finite(), "NaN at {i}");
            assert!(out.abs() < 8.0, "runaway at {i}: {out}");
        }
    }

    #[test]
    fn all_shapes_and_directions_no_nan() {
        for shape in [
            GrainShape::Soft,
            GrainShape::Swell,
            GrainShape::SoftPluck,
            GrainShape::Pluck,
            GrainShape::Bounce,
        ] {
            for direction in [
                GrainDirection::Forward,
                GrainDirection::Reverse,
                GrainDirection::Both,
            ] {
                let mut d = SpectralDelay::new();
                d.time_ms = 250.0;
                d.feedback = 0.5;
                d.shape = shape;
                d.direction = direction;
                d.spread = 1.0;
                d.stretch = 0.7;
                d.octave = 0.5;
                d.update(SR);

                let mut energy = 0.0;
                for i in 0..48000 {
                    let input = if i < 200 { 0.8 } else { 0.0 };
                    let out = d.tick(input, 0);
                    assert!(out.is_finite(), "{shape:?}/{direction:?} NaN at {i}");
                    energy += out * out;
                }
                assert!(
                    energy > 1e-4,
                    "{shape:?}/{direction:?} should produce output"
                );
            }
        }
    }

    #[test]
    fn spread_randomizes_grain_placement() {
        // Continuous tone in: with spread=0 the first wet arrives near
        // the 400 ms delay time; with spread=1 grains land across the
        // whole span, so wet appears much earlier.
        let first_wet = |spread: f64| -> usize {
            let mut d = SpectralDelay::new();
            d.time_ms = 400.0;
            d.feedback = 0.0;
            d.spread = spread;
            d.density = DensityMode::Synced(1.0 / 16.0);
            d.update(SR);
            for i in 0..48000 {
                let input = (std::f64::consts::TAU * 440.0 * i as f64 / SR).sin() * 0.5;
                if d.tick(input, 0).abs() > 0.01 {
                    return i;
                }
            }
            48000
        };
        let on_time = first_wet(0.0);
        let spread = first_wet(1.0);
        assert!(
            on_time > 12000,
            "spread=0 wet should arrive near the delay time: {on_time}"
        );
        assert!(
            spread < on_time / 2,
            "spread=1 wet should arrive much earlier: {spread} vs {on_time}"
        );
    }

    /// Goertzel energy at `freq`.
    fn goertzel(signal: &[f64], freq: f64) -> f64 {
        let w = std::f64::consts::TAU * freq / SR;
        let coeff = 2.0 * w.cos();
        let (mut s1, mut s2) = (0.0, 0.0);
        for &x in signal {
            let s0 = x + coeff * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        (s1 * s1 + s2 * s2 - coeff * s1 * s2) / (signal.len() as f64).powi(2)
    }

    #[test]
    fn octave_grains_complete_windows_cleanly() {
        // 440 Hz in, all grains octave-up: 880 Hz must dominate over
        // inharmonic probes. Truncated windows (the old bug) put
        // broadband click energy at the probes.
        let mut d = SpectralDelay::new();
        d.time_ms = 400.0;
        d.feedback = 0.0;
        d.octave = 1.0;
        d.density = DensityMode::Synced(1.0 / 8.0);
        d.update(SR);

        let mut out = Vec::with_capacity(96000);
        for i in 0..144000 {
            let input = (std::f64::consts::TAU * 440.0 * i as f64 / SR).sin() * 0.5;
            let v = d.tick(input, 0);
            if i >= 48000 {
                out.push(v);
            }
        }
        let target = goertzel(&out, 880.0);
        let probes = [617.0, 1123.0, 1543.0, 2251.0];
        let worst = probes
            .iter()
            .map(|&f| goertzel(&out, f))
            .fold(0.0f64, f64::max);
        assert!(
            target > worst * 5.0,
            "octave grains should be tonal at 880 Hz: target={target:.3e} worst_probe={worst:.3e}"
        );
    }

    #[test]
    fn regen_path_evolves_tone() {
        // The fixed dispersion/shelf voicing must dull successive
        // repeats: HF content of the 3rd repeat < 1st repeat.
        let mut d = SpectralDelay::new();
        d.time_ms = 300.0;
        d.feedback = 0.7;
        d.spread = 0.0;
        d.density = DensityMode::Synced(1.0);
        d.update(SR);

        // Measure the recirculating signal itself (last_feedback) —
        // that is the path the dispersion/shelf voicing shapes. The
        // burst passes the feedback tap once per period.
        let period = (0.3 * SR) as usize;
        let mut pass1 = Vec::new();
        let mut pass3 = Vec::new();
        for i in 0..(period * 4) {
            let input = if i < 480 {
                (std::f64::consts::TAU * 6000.0 * i as f64 / SR).sin() * 0.8
            } else {
                0.0
            };
            d.tick(input, 0);
            let fb = d.last_feedback();
            if i >= period && i < period + 2400 {
                pass1.push(fb);
            } else if i >= period * 3 && i < period * 3 + 2400 {
                pass3.push(fb);
            }
        }
        let hf1 = goertzel(&pass1, 6000.0);
        let hf3 = goertzel(&pass3, 6000.0);
        assert!(hf1 > 1e-12, "first pass should carry the tone: {hf1:.3e}");
        assert!(
            hf3 < hf1 * 0.9,
            "repeats should evolve (dull) through regen: pass1={hf1:.3e} pass3={hf3:.3e}"
        );
    }

    #[test]
    fn free_density_spawns_grains() {
        let mut d = SpectralDelay::new();
        d.time_ms = 100.0;
        d.feedback = 0.0;
        d.density = DensityMode::FreeHz(20.0);
        d.update(SR);

        let mut energy = 0.0;
        for i in 0..48000 {
            let input = if i < 100 { 1.0 } else { 0.0 };
            let out = d.tick(input, 0);
            energy += out * out;
        }
        assert!(energy > 0.001, "free-running density should produce grains");
    }

    #[test]
    fn diffusion_smears_and_decorrelates_the_cloud() {
        let run = |diffusion: f64| -> (Vec<f64>, Vec<f64>) {
            let mut d = SpectralDelay::new();
            d.time_ms = 150.0;
            d.feedback = 0.0;
            d.spread = 0.0;
            d.diffusion = diffusion;
            d.update(SR);
            let mut l = Vec::new();
            let mut r = Vec::new();
            for i in 0..48000 {
                let input = if i < 480 { 0.8 } else { 0.0 };
                let (ol, or_) = d.tick_stereo(input);
                l.push(ol);
                r.push(or_);
            }
            (l, r)
        };

        // RMS temporal width grows with diffusion.
        let width = |x: &[f64]| -> f64 {
            let mut w = 0.0;
            let mut t1 = 0.0;
            let mut t2 = 0.0;
            for (i, v) in x.iter().enumerate() {
                let e = v * v;
                w += e;
                t1 += e * i as f64;
                t2 += e * (i as f64) * (i as f64);
            }
            let mean = t1 / w.max(1e-12);
            (t2 / w.max(1e-12) - mean * mean).max(0.0).sqrt()
        };
        let (dry_l, dry_r) = run(0.0);
        let (wet_l, wet_r) = run(1.0);
        assert!(
            width(&wet_l) > width(&dry_l) * 1.05,
            "diffusion should smear the cloud in time: {} vs {}",
            width(&wet_l),
            width(&dry_l)
        );

        // With spread 0 the undiffused cloud is dual-mono; the L/R
        // diffuser chains differ, so diffusion must decorrelate.
        let corr = |a: &[f64], b: &[f64]| -> f64 {
            let dot: f64 = a.iter().zip(b).map(|(x, y)| x * y).sum();
            let ea: f64 = a.iter().map(|x| x * x).sum();
            let eb: f64 = b.iter().map(|x| x * x).sum();
            dot / (ea * eb).sqrt().max(1e-12)
        };
        assert!(corr(&dry_l, &dry_r) > 0.999, "spread 0 should be dual-mono");
        assert!(
            corr(&wet_l, &wet_r) < 0.98,
            "diffusion should decorrelate L/R: {}",
            corr(&wet_l, &wet_r)
        );
    }
}
