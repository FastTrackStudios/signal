//! XorShift32 PRNG — matches Airwindows' `fpd` dither state.

// r[impl dsp.prng.xorshift32]
pub struct XorShift32 {
    state: u32,
}

impl XorShift32 {
    // r[impl dsp.prng.nonzero-seed]
    pub fn new(seed: u32) -> Self {
        Self {
            state: if seed == 0 { 1 } else { seed },
        }
    }

    #[inline]
    #[allow(clippy::should_implement_trait)] // xorshift idiom, not an Iterator
    pub fn next(&mut self) -> u32 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 17;
        self.state ^= self.state << 5;
        self.state
    }

    #[inline]
    // r[impl dsp.prng.bipolar]
    pub fn next_bipolar(&mut self) -> f64 {
        (self.next() as i32) as f64 / i32::MAX as f64
    }
}
