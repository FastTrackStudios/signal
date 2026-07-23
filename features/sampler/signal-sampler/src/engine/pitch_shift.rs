//! Time-preserving pitch shift for whole-tone-grid fill.
//!
//! CSS-family libraries sample only 6 of 12 pitch classes; the other ~half of
//! notes were historically filled by *resampling* (playing the sample faster/
//! slower), which changes duration — so a legato transition landing on an
//! off-grid note (E4, F4, …) arrived early/late in wall-clock time, off the
//! musical grid. Instead the voice now plays at true speed (arrival lands
//! exactly as recorded) and shifts pitch here, decoupled from playback rate.
//!
//! Ported from `pitch-dsp`'s barberpole `AllpassShifter` (Dattorro/Schroeder,
//! Eventide-H3000-style): two read heads sweep a circular buffer at a rate set
//! by the pitch ratio, cubic-interpolated, crossfaded so the windows sum to 1.
//! **Zero latency** (output is immediate) — which keeps the arrival predictor
//! exact with no compensation term — and **no hot-path allocation** (the ring
//! buffer is sized once at construction). If the slight barberpole chorusing
//! is audible on long exposed sustains, swap in a granular shifter (with a
//! latency-compensation term on the arrival hold).

use std::f64::consts::PI;

const BUFFER_SIZE: usize = 8192;
/// Cubic interpolation needs 2 samples on each side.
const MARGIN: f64 = 4.0;
const SWEEP_LEN: f64 = (BUFFER_SIZE as f64) - MARGIN * 2.0;

/// Fixed-ratio, time-preserving pitch shifter (one audio channel).
pub struct PitchShifter {
    /// Pitch ratio: 1.0 = unity, 2^(cents/1200). 0.5 = octave down.
    ratio: f64,
    buf: Vec<f64>,
    write: usize,
    /// Sweep phase for head A [0,1); head B trails 0.5 ahead.
    sweep_a: f64,
    sweep_b: f64,
}

impl PitchShifter {
    /// Build a shifter for the given pitch offset in cents (100 = a semitone).
    pub fn new(cents: f64) -> Self {
        Self {
            ratio: 2.0f64.powf(cents / 1200.0),
            buf: vec![0.0; BUFFER_SIZE],
            write: 0,
            sweep_a: 0.0,
            sweep_b: 0.5,
        }
    }

    /// True when this shifter is effectively unity (caller can bypass).
    pub fn is_unity(cents: f64) -> bool {
        cents.abs() < 0.5
    }

    #[inline]
    fn write_sample(&mut self, x: f64) {
        self.write = (self.write + 1) % BUFFER_SIZE;
        self.buf[self.write] = x;
    }

    /// Catmull-Rom read `offset` samples behind the write head.
    #[inline]
    fn read_cubic(&self, offset: f64) -> f64 {
        let read = self.write as f64 - offset;
        let base = read.floor();
        let frac = read - base;
        let i1 = base.rem_euclid(BUFFER_SIZE as f64) as usize;
        let i0 = (i1 + BUFFER_SIZE - 1) % BUFFER_SIZE;
        let i2 = (i1 + 1) % BUFFER_SIZE;
        let i3 = (i1 + 2) % BUFFER_SIZE;
        let (y0, y1, y2, y3) = (self.buf[i0], self.buf[i1], self.buf[i2], self.buf[i3]);
        let a0 = y3 - y2 - y0 + y1;
        let a1 = y0 - y1 - a0;
        let a2 = y2 - y0;
        let a3 = y1;
        ((a0 * frac + a1) * frac + a2) * frac + a3
    }

    #[inline]
    fn crossfade(phase: f64) -> f64 {
        let s = (PI * phase).sin();
        s * s
    }

    #[inline]
    fn phase_to_offset(phase: f64, drift: f64) -> f64 {
        if drift >= 0.0 {
            MARGIN + phase * SWEEP_LEN
        } else {
            MARGIN + (1.0 - phase) * SWEEP_LEN
        }
    }

    /// Shift one sample. Returns the wet (fully shifted) output.
    #[inline]
    pub fn tick(&mut self, input: f32) -> f32 {
        self.write_sample(input as f64);

        let drift = 1.0 - self.ratio;
        let phase_inc = drift.abs().max(0.0001) / SWEEP_LEN;
        self.sweep_a += phase_inc;
        self.sweep_b += phase_inc;
        if self.sweep_a >= 1.0 {
            self.sweep_a -= 1.0;
        }
        if self.sweep_b >= 1.0 {
            self.sweep_b -= 1.0;
        }

        let a = self.read_cubic(Self::phase_to_offset(self.sweep_a, drift));
        let b = self.read_cubic(Self::phase_to_offset(self.sweep_b, drift));
        (a * Self::crossfade(self.sweep_a) + b * Self::crossfade(self.sweep_b)) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Zero-crossing period of a mono signal, in samples (rough pitch probe).
    fn mean_period(x: &[f32], skip: usize) -> f64 {
        let mut last = None;
        let mut sum = 0.0;
        let mut n = 0;
        for i in (skip + 1)..x.len() {
            if x[i - 1] <= 0.0 && x[i] > 0.0 {
                if let Some(p) = last {
                    sum += (i - p) as f64;
                    n += 1;
                }
                last = Some(i);
            }
        }
        if n == 0 { 0.0 } else { sum / n as f64 }
    }

    #[test]
    fn shifts_pitch_up_a_semitone_without_changing_length() {
        let sr = 48000.0f32;
        let f = 220.0f32;
        let n = sr as usize; // 1 second — length is fixed (time-preserving)
        let mut up = PitchShifter::new(100.0); // +1 semitone
        let mut dry = Vec::with_capacity(n);
        let mut wet = Vec::with_capacity(n);
        for i in 0..n {
            let s = (2.0 * std::f32::consts::PI * f * i as f32 / sr).sin() * 0.5;
            dry.push(s);
            wet.push(up.tick(s));
        }
        assert_eq!(wet.len(), dry.len(), "duration must be preserved");
        let pd = mean_period(&dry, 9000);
        let pw = mean_period(&wet, 9000);
        // +1 semitone → period shorter by 2^(-1/12) ≈ 0.944.
        let ratio = pw / pd;
        assert!(
            (ratio - 0.9439).abs() < 0.02,
            "expected ~0.944 period ratio for +1 semitone, got {ratio:.4} (pd={pd:.2} pw={pw:.2})"
        );
    }

    #[test]
    fn finite_and_bounded() {
        let mut s = PitchShifter::new(-100.0);
        for i in 0..48000 {
            let x = (2.0 * std::f32::consts::PI * 82.0 * i as f32 / 48000.0).sin() * 0.9;
            let y = s.tick(x);
            assert!(y.is_finite());
            assert!(y.abs() < 4.0);
        }
    }
}
