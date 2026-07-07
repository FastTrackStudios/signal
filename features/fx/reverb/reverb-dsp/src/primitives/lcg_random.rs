//! Linear Congruential Generator — deterministic PRNG.
//!
//! Ported from CloudSeedCore LcgRandom.h (MIT, Ghost Note Audio).
//! Uses the same constants `a=22695477, c=1, m=2^32` for exact
//! reproducibility of CloudSeed's randomization behavior.

pub struct LcgRandom {
    x: u64,
}

impl LcgRandom {
    const A: u64 = 22695477;
    const C: u64 = 1;

    pub fn new(seed: u64) -> Self {
        Self { x: seed }
    }

    #[inline]
    pub fn next_uint(&mut self) -> u32 {
        let axc = Self::A.wrapping_mul(self.x).wrapping_add(Self::C);
        self.x = axc & 0xFFFF_FFFF;
        self.x as u32
    }

    #[inline]
    pub fn next_float(&mut self) -> f64 {
        let n = self.next_uint();
        n as f64 / u32::MAX as f64
    }
}

/// Generate a vector of random floats [0, 1) from a seed.
pub fn random_buffer(seed: u64, count: usize) -> Vec<f64> {
    let mut rng = LcgRandom::new(seed);
    (0..count).map(|_| rng.next_float()).collect()
}

/// Generate a cross-seeded random buffer.
/// Blends two sequences (seed and ~seed) by `cross_seed` amount.
pub fn random_buffer_cross_seed(seed: u64, count: usize, cross_seed: f64) -> Vec<f64> {
    let seed_a = seed;
    let seed_b = !seed;
    let series_a = random_buffer(seed_a, count);
    let series_b = random_buffer(seed_b, count);

    series_a
        .iter()
        .zip(series_b.iter())
        .map(|(&a, &b)| a * (1.0 - cross_seed) + b * cross_seed)
        .collect()
}
