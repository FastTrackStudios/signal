//! Random — a diffuse space whose delay lines wander.
//!
//! The character Valhalla calls "Random Space" / "Smooth Random", and the
//! family its "Chaotic" modes sit in. What separates it from a hall is not
//! size or brightness but *motion*: instead of the periodic chorus detuning a
//! modulated allpass gives, every delay line random-walks its read position
//! independently. The tail never settles into a repeating pattern, so it
//! neither beats against itself nor develops the metallic ring a static FDN
//! shows on sustained material.
//!
//! Architecture:
//!   1. Input diffusion (allpass cascade) — no discrete early reflections;
//!      a random space has a diffuse onset, not a geometric one.
//!   2. An 8-line FDN with random-walk delay jitter ([`Fdn::set_jitter`]),
//!      in-loop allpasses for density, and slow feedback-mix rotation.
//!   3. Exact per-line Jot T60 decay, so `decay_time` means seconds here
//!      exactly as it does for Hall and Room.
//!
//! The 38-odd factory presets across Valhalla's Random and Chaotic modes had
//! nowhere to land before this: they were folded onto Cloud, which has no
//! time model at all, so a translated preset could not even be tuned to the
//! right length.

use crate::algorithm::{
    decay_to_t60, t60_shelf_targets, AlgorithmParams, ReverbAlgorithm, RANDOM_T60,
};
use crate::primitives::allpass_diffuser::AllpassDiffuser;
use crate::primitives::fdn::{Fdn, MixMatrix};
use crate::primitives::one_pole::Lp1;

/// Jitter depth at full modulation, in milliseconds of read-position drift.
///
/// Enough to keep the tail from ever repeating, small enough that it reads as
/// air rather than pitch wobble.
const MAX_JITTER_MS: f64 = 3.0;
/// Jitter that remains with modulation at zero — the engine is *defined* by
/// its motion, so it never freezes into a static FDN.
const MIN_JITTER_MS: f64 = 0.4;

pub struct Random {
    diffuser_l: AllpassDiffuser,
    diffuser_r: AllpassDiffuser,
    fdn_l: Fdn,
    fdn_r: Fdn,
    tone_lp_l: Lp1,
    tone_lp_r: Lp1,
    cross_feed: f64,
    sample_rate: f64,
    size: f64,
}

impl Random {
    pub fn new(sample_rate: f64) -> Self {
        let mut tone_lp_l = Lp1::new();
        tone_lp_l.set_freq(16_000.0, sample_rate);
        let mut tone_lp_r = Lp1::new();
        tone_lp_r.set_freq(16_000.0, sample_rate);

        Self {
            diffuser_l: AllpassDiffuser::with_defaults(sample_rate, 1.0),
            diffuser_r: AllpassDiffuser::with_defaults(sample_rate, 1.0),
            fdn_l: Self::make_fdn(sample_rate, 1.0, false),
            fdn_r: Self::make_fdn(sample_rate, 1.0, true),
            tone_lp_l,
            tone_lp_r,
            cross_feed: 0.2,
            sample_rate,
            size: 1.0,
        }
    }

    fn make_fdn(sample_rate: f64, size: f64, offset: bool) -> Fdn {
        // Mutually prime, and deliberately not harmonically related — with
        // the lines wandering, any near-common factor would drift in and out
        // of alignment audibly.
        let base = if !offset {
            [1123, 1483, 1801, 2179, 2557, 2939, 3313, 3671]
        } else {
            [1187, 1531, 1867, 2237, 2617, 3001, 3391, 3739]
        };
        let scale = sample_rate / 48_000.0 * size.max(0.2);
        let delays: Vec<usize> = base
            .iter()
            .map(|&d| ((d as f64 * scale) as usize).max(4))
            .collect();
        let mut fdn = Fdn::new(&delays, MixMatrix::Householder);
        fdn.set_damping(9_000.0, sample_rate);
        fdn
    }

    fn rebuild_fdns(&mut self) {
        self.fdn_l = Self::make_fdn(self.sample_rate, self.size, false);
        self.fdn_r = Self::make_fdn(self.sample_rate, self.size, true);
    }
}

impl ReverbAlgorithm for Random {
    fn reset(&mut self) {
        self.diffuser_l.reset();
        self.diffuser_r.reset();
        self.fdn_l.reset();
        self.fdn_r.reset();
        self.tone_lp_l.reset();
        self.tone_lp_r.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        *self = Self::new(sample_rate);
    }

    fn set_params(&mut self, params: &AlgorithmParams) {
        let new_size = 0.3 + params.size * 1.2;
        if (new_size - self.size).abs() > 0.01 {
            self.size = new_size;
            self.rebuild_fdns();
        }

        // Density first: the in-loop allpasses lengthen every recirculation,
        // and `set_t60` reads that length.
        let ap = 0.55 + params.diffusion * 0.25;
        self.fdn_l.set_loop_allpass(ap);
        self.fdn_r.set_loop_allpass(ap);

        // The engine's defining feature: independent random-walk drift per
        // line, never fully off.
        let jitter = MIN_JITTER_MS + params.modulation * (MAX_JITTER_MS - MIN_JITTER_MS);
        self.fdn_l.set_jitter(jitter, self.sample_rate);
        self.fdn_r.set_jitter(jitter * 1.17, self.sample_rate);

        // Slow orthogonal rotation of the feedback mix, on top of the drift.
        self.fdn_l.set_rotation(
            0.25 + params.modulation * 0.5,
            0.05 + params.modulation * 0.2,
            self.sample_rate,
        );
        self.fdn_r.set_rotation(
            (0.25 + params.modulation * 0.5) * 1.09,
            0.05 + params.modulation * 0.2,
            self.sample_rate,
        );

        // Exact per-line T60, so `decay_time` is honoured in seconds.
        let t60 = decay_to_t60(params.decay, RANDOM_T60.0, RANDOM_T60.1);
        let (t60_dc, t60_ny) = t60_shelf_targets(
            t60,
            params.low_decay_mult,
            params.high_decay_mult,
            params.damping,
        );
        self.fdn_l.set_t60(t60_dc, t60_ny, self.sample_rate);
        self.fdn_r.set_t60(t60_dc, t60_ny, self.sample_rate);

        self.fdn_l
            .set_decay_curve(t60, &params.decay_bands, self.sample_rate);
        self.fdn_r
            .set_decay_curve(t60, &params.decay_bands, self.sample_rate);

        let damp_freq = 1_500.0 + (1.0 - params.damping) * 12_000.0;
        self.fdn_l.set_damping(damp_freq, self.sample_rate);
        self.fdn_r.set_damping(damp_freq, self.sample_rate);

        let tone_freq = 3_000.0 + (params.tone + 1.0) * 0.5 * 15_000.0;
        self.tone_lp_l.set_freq(tone_freq, self.sample_rate);
        self.tone_lp_r.set_freq(tone_freq, self.sample_rate);

        self.cross_feed = 0.15 + params.diffusion * 0.2;
    }

    fn tick(&mut self, left: f64, right: f64) -> (f64, f64) {
        let diff_l = self.diffuser_l.tick(left);
        let diff_r = self.diffuser_r.tick(right);

        let in_l = diff_l + diff_r * self.cross_feed;
        let in_r = diff_r + diff_l * self.cross_feed;

        let late_l = self.tone_lp_l.tick(self.fdn_l.tick(in_l));
        let late_r = self.tone_lp_r.tick(self.fdn_r.tick(in_r));

        (late_l, late_r)
    }
}
