//! Linear Congruential Generator — deterministic PRNG.
//!
//! Ported from `CloudSeedCore` LcgRandom.h (MIT, Ghost Note Audio).
//! Uses the same constants `a=22695477, c=1, m=2^32` for exact
//! reproducibility of `CloudSeed`'s randomization behavior.

pub struct LcgRandom {
    x: u64,
}

impl LcgRandom {
    const A: u64 = 22_695_477;
    const C: u64 = 1;

    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { x: seed }
    }

    #[inline]
    pub const fn next_uint(&mut self) -> u32 {
        let axc = Self::A.wrapping_mul(self.x).wrapping_add(Self::C);
        self.x = axc & 0xFFFF_FFFF;
        #[expect(
            clippy::cast_possible_truncation,
            clippy::as_conversions,
            reason = "self.x is masked to u32 range above"
        )]
        let result = self.x as u32;
        result
    }

    #[inline]
    pub fn next_float(&mut self) -> f64 {
        let n = self.next_uint();
        f64::from(n) / f64::from(u32::MAX)
    }
}

/// Generate a vector of random floats [0, 1) from a seed.
#[must_use]
pub fn random_buffer(seed: u64, count: usize) -> Vec<f64> {
    let mut rng = LcgRandom::new(seed);
    (0..count).map(|_| rng.next_float()).collect()
}

/// Generate a cross-seeded random buffer.
/// Blends two sequences (seed and ~seed) by `cross_seed` amount.
#[must_use]
pub fn random_buffer_cross_seed(seed: u64, count: usize, cross_seed: f64) -> Vec<f64> {
    let seed_a = seed;
    let seed_b = !seed;
    let series_a = random_buffer(seed_a, count);
    let series_b = random_buffer(seed_b, count);

    series_a
        .iter()
        .zip(series_b.iter())
        .map(|(&a, &b)| a.mul_add(1.0 - cross_seed, b * cross_seed))
        .collect()
}
