//! Small numeric helpers: the one place a conversion happens.

/// Decibels to a linear gain.
#[must_use]
pub fn db_to_gain(db: f32) -> f32 {
    libm::powf(10.0, db / 20.0)
}

/// A count as `f32`.
#[must_use]
pub const fn count_f32(n: usize) -> f32 {
    signal_dsp_core::num::count_to_f32(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_to_gain_is_right() {
        assert!((db_to_gain(0.0) - 1.0).abs() < 1e-6);
        assert!((db_to_gain(-6.0) - 0.501_187).abs() < 1e-5);
        assert!((db_to_gain(20.0) - 10.0).abs() < 1e-4);
    }
}
