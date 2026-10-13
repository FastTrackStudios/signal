//! Analysis/synthesis window functions.
//!
//! One home for the Hann that was open-coded (with drifting
//! denominators) across the fx crates. `hann` is the periodic form
//! (denominator N — correct for overlap-add / FFT analysis);
//! `hann_symmetric` (denominator N−1) is the symmetric filter-design
//! form.

use core::f64::consts::TAU;

/// Periodic Hann value for index `i` of an `n`-point window.
#[inline]
pub fn hann(i: usize, n: usize) -> f64 {
    if n == 0 {
        return 0.0;
    }
    0.5 * (1.0 - (TAU * i as f64 / n as f64).cos())
}

/// Symmetric Hann value for index `i` of an `n`-point window
/// (endpoints exactly zero).
#[inline]
pub fn hann_symmetric(i: usize, n: usize) -> f64 {
    if n <= 1 {
        return 0.0;
    }
    0.5 * (1.0 - (TAU * i as f64 / (n - 1) as f64).cos())
}

/// Fill `buf` with a periodic Hann window.
pub fn fill_hann(buf: &mut [f64]) {
    let n = buf.len();
    for (i, v) in buf.iter_mut().enumerate() {
        *v = hann(i, n);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symmetric_endpoints_are_zero_and_peak_is_one() {
        let n = 33;
        assert!(hann_symmetric(0, n) < 1e-12);
        assert!(hann_symmetric(n - 1, n) < 1e-12);
        assert!((hann_symmetric(n / 2, n) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn periodic_overlap_adds_to_constant() {
        // 50% overlap-add of the periodic Hann is exactly 1.0.
        let n = 64;
        for i in 0..n / 2 {
            let sum = hann(i, n) + hann(i + n / 2, n);
            assert!((sum - 1.0).abs() < 1e-12, "at {i}: {sum}");
        }
    }
}
