//! The digital stage — quantisation and rate reduction.
//!
//! Everything else in this crate models a circuit going nonlinear, and the
//! harmonics it makes belong to the note. This does not. Reducing word length
//! adds a quantisation error that is *uncorrelated* with the signal, and
//! reducing sample rate folds everything above the new Nyquist back down as
//! aliases that are not harmonically related to anything. That is the whole
//! character of the family, and it is why it gets its own stage instead of
//! another [`SideShaper`](crate::preamp::SideShaper): there is no transfer
//! curve that produces an alias.
//!
//! The two are ordered the way a converter does it — decimate first, then
//! quantise — so the aliases are themselves quantised rather than the other
//! way round.
//!
//! Processing-core rules: no allocation, no threads, no I/O. `process` is
//! arithmetic and one branch per sample.

use crate::preamp::MAX_CHANNELS;

/// Word lengths at or above this are treated as off — a 32-bit float path
/// quantised to 24 bits is a difference nobody is buying this plugin for.
pub const BITS_OFF: f32 = 24.0;

#[derive(Debug, Clone)]
pub struct DigitalStage {
    /// Word length in bits, 2..=[`BITS_OFF`]. Fractional values are honoured —
    /// a knob sweeping through 7.4 bits is smoother than one that steps.
    pub bits: f32,
    /// Sample-rate divisor, 1 = off. 8 means one new sample in eight and a
    /// zero-order hold between, which is what a cheap sampler sounded like.
    pub rate: f32,
    /// Dither, 0..1, in LSBs of the current word length. TPDF, so the
    /// quantisation error is decorrelated from the signal and the noise floor
    /// stays flat instead of the error tracking the waveform.
    pub dither: f32,

    /// Zero-order hold, per channel.
    hold: [f32; MAX_CHANNELS],
    /// Fractional sample counter driving the hold.
    phase: [f32; MAX_CHANNELS],
    /// The dither generator. One per stage, not per channel — two channels
    /// sharing a noise sequence would put the dither in the middle of the
    /// image, so each channel takes a different draw.
    rng: u32,
}

impl Default for DigitalStage {
    fn default() -> Self {
        Self::new()
    }
}

impl DigitalStage {
    pub fn new() -> Self {
        Self {
            bits: BITS_OFF,
            rate: 1.0,
            dither: 0.0,
            hold: [0.0; MAX_CHANNELS],
            phase: [0.0; MAX_CHANNELS],
            rng: 0x2545_f491,
        }
    }

    /// True when the stage would pass audio through untouched — the caller
    /// can skip it entirely rather than paying for a bypass per sample.
    #[inline]
    pub fn is_transparent(&self) -> bool {
        self.bits >= BITS_OFF && self.rate <= 1.0
    }

    pub fn reset(&mut self) {
        self.hold = [0.0; MAX_CHANNELS];
        self.phase = [0.0; MAX_CHANNELS];
    }

    /// One sample on channel `ch`.
    #[inline]
    pub fn process(&mut self, ch: usize, input: f32) -> f32 {
        let ch = ch.min(MAX_CHANNELS - 1);

        // Decimate: hold the last sample until the divisor rolls over. No
        // anti-alias filter, deliberately — the aliases ARE the effect.
        let x = if self.rate > 1.0 {
            // Latch on the way in rather than on the way out, so the
            // very first sample after a reset is a real one and not a
            // period of silence.
            if self.phase[ch] <= 0.0 {
                self.phase[ch] += self.rate;
                self.hold[ch] = input;
            }
            self.phase[ch] -= 1.0;
            self.hold[ch]
        } else {
            self.hold[ch] = input;
            input
        };

        if self.bits >= BITS_OFF {
            return x;
        }

        // Quantise. `levels` is the number of steps either side of zero, so
        // one LSB is 1/levels of full scale.
        let bits = self.bits.clamp(1.0, BITS_OFF);
        let levels = crate::exp2_approx(bits - 1.0).max(1.0);
        let noise = if self.dither > 0.0 {
            // TPDF: the sum of two independent rectangular draws.
            (self.next_uniform() + self.next_uniform()) * self.dither * 0.5
        } else {
            0.0
        };
        round_half_away(x * levels + noise) / levels
    }

    /// Uniform in [−1, 1). xorshift32 — no allocation, no `std`, and the
    /// sequence is long enough that nobody will hear it repeat.
    #[inline]
    fn next_uniform(&mut self) -> f32 {
        let mut s = self.rng;
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        self.rng = s;
        // Top 24 bits → [0, 1), then centred.
        ((s >> 8) as f32) * (1.0 / 8_388_608.0) - 1.0
    }
}

/// `f32::round` is std/libm-only and this crate is `no_std`.
#[inline]
fn round_half_away(x: f32) -> f32 {
    let shifted = if x >= 0.0 { x + 0.5 } else { x - 0.5 };
    // The cast truncates toward zero, which is what the offset above wants.
    // Guard the range where an i32 cast would be undefined-ish anyway.
    if shifted.abs() > 2_000_000_000.0 {
        x
    } else {
        shifted as i32 as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_a_wire() {
        let mut d = DigitalStage::new();
        assert!(d.is_transparent());
        for i in -50..=50 {
            let x = i as f32 / 50.0;
            assert_eq!(d.process(0, x), x);
        }
    }

    #[test]
    fn quantising_lands_on_the_grid() {
        let mut d = DigitalStage::new();
        d.bits = 4.0; // 8 steps either side of zero
        for i in -100..=100 {
            let x = i as f32 / 100.0;
            let y = d.process(0, x);
            let on_grid = (y * 8.0 - round_half_away(y * 8.0)).abs();
            assert!(on_grid < 1.0e-4, "{x} -> {y} is off the 4-bit grid");
        }
    }

    #[test]
    fn a_coarser_word_is_a_coarser_grid() {
        // The error a listener hears is bounded by half an LSB, and halving
        // the word length must make it visibly worse rather than the same.
        let err = |bits: f32| {
            let mut d = DigitalStage::new();
            d.bits = bits;
            let mut worst = 0.0f32;
            for i in -500..=500 {
                let x = i as f32 / 500.0;
                worst = worst.max((d.process(0, x) - x).abs());
            }
            worst
        };
        assert!(err(3.0) > err(8.0) * 4.0, "3-bit must be much coarser than 8");
    }

    #[test]
    fn rate_reduction_holds_the_last_sample() {
        let mut d = DigitalStage::new();
        d.rate = 4.0;
        // A fixed array rather than a Vec — the crate is no_std.
        let mut taken = [0.0f32; 8];
        for (i, slot) in taken.iter_mut().enumerate() {
            *slot = d.process(0, i as f32);
        }
        // One new value every four samples, held in between — and the
        // first sample is a real one.
        assert_eq!(taken, [0.0, 0.0, 0.0, 0.0, 4.0, 4.0, 4.0, 4.0]);
    }

    #[test]
    fn dither_decorrelates_without_swamping_the_signal() {
        let mut d = DigitalStage::new();
        d.bits = 6.0;
        d.dither = 1.0;
        let mut worst = 0.0f32;
        for i in -500..=500 {
            let x = i as f32 / 500.0;
            worst = worst.max((d.process(0, x) - x).abs());
        }
        // Dither trades a bounded error for a random one: still small
        // against a 6-bit LSB (1/32), never a runaway.
        assert!(worst < 0.1, "dither must stay near the LSB: {worst}");
    }
}
