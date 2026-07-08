//! SpectralDelay — granular ambient delay.
//!
//! TimeLine MX "Spectral" machine parity: the wet path is a granular
//! cloud read from the delay buffer. Grains spawn at a configurable
//! density (free-running Hz or synced to a fraction of the delay time),
//! can be time-stretched (slower read speed), and blend in octave-up
//! grains for a spectral halo.
//!
//! First-pass quality notes: dual-window grains over one `DelayLine`
//! with cubic reads; a deep pass will adopt `pitch-dsp` (PSOLA /
//! signalsmith-stretch) for cleaner octave shifting. Per-grain `spread`
//! panning is stored for API parity but not applied — the engine is
//! mono-per-channel inside `DelayChain`.

use audiocore_dsp::biquad::{Biquad, FilterType};
use audiocore_dsp::delay_line::DelayLine;
use audiocore_dsp::prng::XorShift32;
use audiocore_dsp::smoothing::ParamSmoother;

const NUM_GRAINS: usize = 8;

#[derive(Debug, Clone, Copy)]
struct Grain {
    active: bool,
    /// Current read offset behind the write head, in samples.
    offset: f64,
    /// Read-speed ratio: 1.0 = normal, 2.0 = octave up, <1.0 = stretched.
    speed: f64,
    /// Age in samples.
    age: f64,
    /// Duration in samples.
    dur: f64,
    gain: f64,
}

impl Grain {
    const fn idle() -> Self {
        Self {
            active: false,
            offset: 0.0,
            speed: 1.0,
            age: 0.0,
            dur: 1.0,
            gain: 0.0,
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
    /// Octave blend (0.0–1.0): fraction of grains that read at 2× (octave up).
    pub octave: f64,
    /// Per-grain random pan spread (0.0–1.0). Parity only; see module docs.
    pub spread: f64,
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

        // Octave blend: probabilistically read at 2× speed.
        let r = (self.rng.next_bipolar() + 1.0) * 0.5;
        let speed = if r < self.octave {
            2.0
        } else {
            // Stretch slows the read down to 0.5× at full stretch.
            1.0 - self.stretch * 0.5
        };

        // Grains overlap 2×: duration = twice the spawn interval.
        let dur = (interval * 2.0).clamp(256.0, delay_samples.max(512.0));

        // Start reading around the delay time, slightly randomized so
        // simultaneous grains don't phase-lock.
        let jitter = self.rng.next_bipolar() * interval * 0.25;
        let offset = (delay_samples + jitter).max(4.0);

        self.grains[slot] = Grain {
            active: true,
            offset,
            speed,
            age: 0.0,
            dur,
            gain: 1.0,
        };
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

        // Sum grain voices. Hann window over each grain's lifetime.
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
            let window = (std::f64::consts::PI * phase).sin();
            let window = window * window;
            wet += self.delay.read_cubic(g.offset) * window * g.gain;

            // Speed ≠ 1 drifts the read head through the buffer.
            g.offset += 1.0 - g.speed;
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
