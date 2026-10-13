//! Airwindows-style dither for 32-bit float output.

// r[impl dsp.dither.airwindows]
use crate::prng::XorShift32;

#[inline]
pub fn airwindows_dither(sample: f64, prng: &mut XorShift32) -> f32 {
    let noise = prng.next_bipolar();
    let dithered = sample + noise * 1.18e-23;
    dithered as f32
}
