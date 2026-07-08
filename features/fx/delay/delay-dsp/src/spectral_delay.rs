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
//! First-pass quality notes: time-domain grains over one `DelayLine`
//! with cubic reads — the MX processes in the frequency domain and has
//! a spectral character even at neutral settings; that lands with the
//! pitch-dsp deep pass. Bounce's randomized per-grain filter is
//! approximated by a per-grain one-pole lowpass at a random cutoff.

use audiocore_dsp::biquad::{Biquad, FilterType};
use audiocore_dsp::delay_line::DelayLine;
use audiocore_dsp::prng::XorShift32;
use audiocore_dsp::smoothing::ParamSmoother;

const NUM_GRAINS: usize = 8;

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

#[derive(Debug, Clone, Copy)]
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
    /// Bounce: per-grain one-pole lowpass state + coefficient (a1 = 0
    /// disables).
    lp_state: f64,
    lp_a1: f64,
}

impl Grain {
    const fn idle() -> Self {
        Self {
            active: false,
            offset: 0.0,
            drift: 0.0,
            age: 0.0,
            dur: 1.0,
            gain: 0.0,
            lp_state: 0.0,
            lp_a1: 0.0,
        }
    }
}

/// Grain-spawn density mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DensityMode {
    /// Grains per second.
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
    /// High-cut in the feedback path (0 = off).
    pub hicut_freq: f64,
    /// Decay EQ tilt (shared engine param).
    pub decay_tilt: f64,

    delay: DelayLine,
    grains: [Grain; NUM_GRAINS],
    spawn_countdown: f64,
    hicut: Biquad,
    decay_eq: Biquad,
    feedback_sample: f64,
    sample_rate: f64,
    smoother: ParamSmoother,
    rng: XorShift32,
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
            decay_tilt: 0.0,
            delay: DelayLine::new(48000 * 3 + 1024),
            grains: [Grain::idle(); NUM_GRAINS],
            spawn_countdown: 0.0,
            hicut: Biquad::new(),
            decay_eq: Biquad::new(),
            feedback_sample: 0.0,
            sample_rate: 48000.0,
            smoother: ParamSmoother::new(0.0),
            rng: XorShift32::new(0x5EC7_0A1),
        }
    }

    pub fn update(&mut self, sample_rate: f64) {
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

    fn spawn_interval_samples(&self, delay_samples: f64) -> f64 {
        match self.density {
            DensityMode::FreeHz(hz) => self.sample_rate / hz.clamp(0.5, 100.0),
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
        let speed = if octave_up { 2.0 } else { 1.0 - stretch_amt * 0.5 };

        // Direction: reverse grains drift backward through the buffer
        // (offset grows by 1 + speed instead of shrinking).
        let reverse = match self.direction {
            GrainDirection::Forward => false,
            GrainDirection::Reverse => true,
            GrainDirection::Both => self.rng.next_bipolar() > 0.0,
        };
        let drift = if reverse { 1.0 + speed } else { 1.0 - speed };

        // Grains overlap 2×: duration = twice the spawn interval.
        let dur = (interval * 2.0).clamp(256.0, delay_samples.max(512.0));

        // Spread: random placement across the delay time (0 = on-time).
        // A small jitter always applies so voices don't phase-lock.
        let jitter = self.rng.next_bipolar() * interval * 0.25;
        let placement = rand01(&mut self.rng) * self.spread * delay_samples * 0.9;
        let offset = (delay_samples - placement + jitter).max(4.0);

        // Bounce: randomized per-grain lowpass (~800 Hz – 8 kHz).
        let lp_a1 = if self.shape == GrainShape::Bounce {
            let fc = 800.0 * (10.0f64).powf(rand01(&mut self.rng));
            let x = (-std::f64::consts::TAU * fc / self.sample_rate).exp();
            x
        } else {
            0.0
        };

        self.grains[slot] = Grain {
            active: true,
            offset,
            drift,
            age: 0.0,
            dur,
            gain: 1.0,
            lp_state: 0.0,
            lp_a1,
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
        let target_delay = self.time_ms * 0.001 * self.sample_rate;
        self.smoother.set_target(target_delay);
        let smooth_delay = self.smoother.tick();
        let max_read = self.delay.len() as f64 - 4.0;

        // Spawn scheduler.
        let interval = self.spawn_interval_samples(smooth_delay);
        self.spawn_countdown -= 1.0;
        if self.spawn_countdown <= 0.0 {
            self.spawn_grain(smooth_delay, interval);
            self.spawn_countdown = interval;
        }

        // Sum grain voices with the selected shape window.
        let shape = self.shape;
        let mut wet = 0.0;
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
            if g.lp_a1 > 0.0 {
                g.lp_state = (1.0 - g.lp_a1) * sample + g.lp_a1 * g.lp_state;
                sample = g.lp_state;
            }
            wet += sample * window * g.gain;

            g.offset += g.drift;
            g.age += 1.0;
            active += 1;
        }
        // Overlap normalization: ~2 grains sound at once by design.
        if active > 2 {
            wet /= (active as f64 / 2.0).sqrt();
        }

        // Feedback from a plain (non-granular) tap so regeneration stays
        // rhythmically anchored.
        let mut fb = self
            .delay
            .read_cubic(smooth_delay.clamp(1.0, max_read))
            * self.feedback;
        if self.hicut_freq > 0.0 {
            fb = self.hicut.tick(fb, ch);
        }
        if self.decay_tilt.abs() > 0.01 {
            fb = self.decay_eq.tick(fb, ch);
        }
        fb = fb.clamp(-1.5, 1.5);

        self.delay.write(input + fb);
        self.feedback_sample = fb;

        wet
    }

    pub fn last_feedback(&self) -> f64 {
        self.feedback_sample
    }

    pub fn reset(&mut self) {
        self.delay.clear();
        self.grains = [Grain::idle(); NUM_GRAINS];
        self.spawn_countdown = 0.0;
        self.hicut.reset();
        self.decay_eq.reset();
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
}
