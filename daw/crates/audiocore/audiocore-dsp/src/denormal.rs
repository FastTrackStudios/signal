//! Denormal flushing helpers.
//!
//! Long feedback tails (reverbs, delays) decay into the denormal range,
//! where FP math can be 10-100x slower on x86. Flush state variables
//! toward hard zero once they drop below an inaudible threshold.

/// Threshold below which a value is flushed to zero.
///
/// -2000 dBFS — far below audibility, far above the f64 denormal
/// boundary (~1e-308), so state is zeroed long before math slows down.
pub const FLUSH_THRESHOLD: f64 = 1.0e-100;

/// Flush a single value to zero if it is inaudibly small.
#[inline]
pub fn flush(x: f64) -> f64 {
    if x.abs() < FLUSH_THRESHOLD {
        0.0
    } else {
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flushes_tiny_keeps_signal() {
        assert_eq!(flush(1.0e-200), 0.0);
        assert_eq!(flush(-1.0e-200), 0.0);
        assert_eq!(flush(0.5), 0.5);
        assert_eq!(flush(-0.5), -0.5);
        assert_eq!(flush(1.0e-20), 1.0e-20);
    }
}
