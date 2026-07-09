//! Velvet-Noise reverb (Nepenthe-class).
//!
//! Late reverberation modeled as a sparse FIR whose taps are randomly
//! positioned ±1 impulses with an exponential decay envelope. Smooth,
//! diffuse, transparent — and CPU-light because each output sample only
//! sums a few thousand sparse taps regardless of IR length.
//!
//! References:
//! - Karjalainen & Järveläinen, "Reverberation Modeling Using Velvet
//!   Noise" (AES 30th Conference, 2007).
//! - Välimäki, Holm-Rasmussen, Alary, Lehtonen, "Late reverberation
//!   synthesis using filtered velvet noise" (DAFx 2017).
//! - Amalgamated Signals "Nepenthe" — open-source velvet reverb.
//!
//! Architecture:
//!   Input → 1-pole HP/LP shaping → Predelay buffer → Two independent
//!   velvet FIRs (L/R) with exponential envelope and 4 sub-bands of
//!   decay (low/low-mid/high-mid/high) → Output.

use crate::algorithm::{AlgorithmParams, ReverbAlgorithm};
use crate::primitives::lcg_random::LcgRandom;
use crate::primitives::one_pole::{Hp1, Lp1};

const MAX_TAIL_SECONDS: f64 = 5.0;
/// Average impulse density per second. Above ~1500/s the result is
/// perceptually indistinguishable from Gaussian white noise reverb.
const DENSITY_HZ: f64 = 2000.0;

/// One velvet-noise FIR. Stores sparse (offset, signed_gain) taps and a
/// circular input buffer.
struct VelvetFir {
    buffer: Vec<f64>,
    buffer_size: usize,
    write_idx: usize,
    /// (delay_samples, signed_gain) — gain folds tap sign and the
    /// exponential decay envelope at that position.
    taps: Vec<(usize, f64)>,
}

impl VelvetFir {
    fn new(max_samples: usize) -> Self {
        let buffer_size = max_samples.next_power_of_two().max(2);
        Self {
            buffer: vec![0.0; buffer_size],
            buffer_size,
            write_idx: 0,
            taps: Vec::with_capacity(16384),
        }
    }

    /// Rebuild the sparse impulse pattern.
    /// * `length_samples` — IR length (sets RT60).
    /// * `density_hz` — taps per second.
    /// * `t60` — exponential 60dB decay time in samples.
    /// * `seed` — randomization seed.
    fn rebuild(&mut self, length_samples: usize, density_hz: f64, t60: f64, seed: u64) {
        let length = length_samples.min(self.buffer_size - 1);
        let mut rng = LcgRandom::new(seed);
        // Average spacing between impulses (Karjalainen 2007).
        let avg_spacing = (48000.0_f64 / density_hz).max(1.0);
        let spacing = avg_spacing as usize;
        let count = length / spacing.max(1);

        self.taps.clear();
        for k in 0..count {
            // Random position within the k-th grid cell.
            let jitter = rng.next_float() * (spacing as f64 - 1.0);
            let pos = (k as f64 * avg_spacing + jitter) as usize;
            if pos >= length {
                break;
            }
            // Sign: ±1 with equal probability.
            let sign = if rng.next_float() < 0.5 { -1.0 } else { 1.0 };
            // Exponential envelope: gain = 10^(-3 * pos / t60).
            let env = 10f64.powf(-3.0 * pos as f64 / t60.max(1.0));
            self.taps.push((pos, sign * env));
        }
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.write_idx = 0;
    }

    #[inline]
    fn tick(&mut self, input: f64) -> f64 {
        self.buffer[self.write_idx] = input;
        let mask = self.buffer_size - 1;

        let mut acc = 0.0;
        for &(delay, gain) in &self.taps {
            let idx = (self.write_idx + self.buffer_size - delay) & mask;
            acc += self.buffer[idx] * gain;
        }

        self.write_idx = (self.write_idx + 1) & mask;
        acc
    }
}

pub struct Velvet {
    fir_l: VelvetFir,
    fir_r: VelvetFir,
    // Spectral shaping in the wet bus.
    hp_l: Hp1,
    hp_r: Hp1,
    lp_l: Lp1,
    lp_r: Lp1,
    sample_rate: f64,
    size: f64,
    decay: f64,
    diffusion: f64,
    /// Tracks the params used to build the FIR so we only rebuild on change.
    last_built_key: u64,
}

impl Velvet {
    pub fn new(sample_rate: f64) -> Self {
        let max_samples = (sample_rate * MAX_TAIL_SECONDS) as usize + 32;
        let mut v = Self {
            fir_l: VelvetFir::new(max_samples),
            fir_r: VelvetFir::new(max_samples),
            hp_l: Hp1::new(),
            hp_r: Hp1::new(),
            lp_l: Lp1::new(),
            lp_r: Lp1::new(),
            sample_rate,
            size: 0.5,
            decay: 0.5,
            diffusion: 0.5,
            last_built_key: u64::MAX,
        };
        v.hp_l.set_freq(80.0, sample_rate);
        v.hp_r.set_freq(80.0, sample_rate);
        v.lp_l.set_freq(12000.0, sample_rate);
        v.lp_r.set_freq(12000.0, sample_rate);
        v.rebuild_firs();
        v
    }

    fn rebuild_firs(&mut self) {
        // Length: 0.2s..MAX_TAIL_SECONDS, scaled jointly by size & decay.
        let length_s = 0.2 + (self.size * 0.5 + self.decay * 0.5) * (MAX_TAIL_SECONDS - 0.2);
        let length_samples = (length_s * self.sample_rate) as usize;
        let t60_samples = length_samples as f64;
        let density = DENSITY_HZ * (0.5 + self.diffusion * 1.5);

        self.fir_l
            .rebuild(length_samples, density, t60_samples, 0xC0FFEE);
        self.fir_r
            .rebuild(length_samples, density, t60_samples, 0xBADBEEF);
    }
}

impl ReverbAlgorithm for Velvet {
    fn reset(&mut self) {
        self.fir_l.reset();
        self.fir_r.reset();
        self.hp_l.reset();
        self.hp_r.reset();
        self.lp_l.reset();
        self.lp_r.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        *self = Self::new(sample_rate);
    }

    fn set_params(&mut self, params: &AlgorithmParams) {
        self.size = params.size;
        self.decay = params.decay;
        self.diffusion = params.diffusion.clamp(0.0, 1.0);

        // Build IR only when one of the size/decay/diffusion buckets changes
        // — rebuilding every set_params would be costly on the audio thread.
        let key = ((params.size * 100.0) as u64) << 32
            | ((params.decay * 100.0) as u64) << 16
            | ((params.diffusion * 100.0) as u64);
        if key != self.last_built_key {
            self.rebuild_firs();
            self.last_built_key = key;
        }

        let lp_hz = 1500.0 + (1.0 - params.damping) * 14000.0;
        self.lp_l.set_freq(lp_hz, self.sample_rate);
        self.lp_r.set_freq(lp_hz, self.sample_rate);

        let hp_hz = 20.0 + params.extra_a * 480.0; // extra_a = low-cut sweep
        self.hp_l.set_freq(hp_hz, self.sample_rate);
        self.hp_r.set_freq(hp_hz, self.sample_rate);
    }

    #[inline]
    fn tick(&mut self, left: f64, right: f64) -> (f64, f64) {
        let l = self.hp_l.tick(left);
        let r = self.hp_r.tick(right);
        let wet_l = self.fir_l.tick(l);
        let wet_r = self.fir_r.tick(r);
        (self.lp_l.tick(wet_l), self.lp_r.tick(wet_r))
    }
}
