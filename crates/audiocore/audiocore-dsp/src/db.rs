//! Decibel conversion utilities.
//!
//! Shared across all dynamics plugins to eliminate inconsistent inline
//! implementations.

/// Floor value for dB conversions — returned when linear input is zero/negative.
pub const DB_FLOOR: f64 = -200.0;

// r[impl dsp.db.linear-to-db]
/// Convert linear amplitude to decibels.
///
/// Returns [`DB_FLOOR`] for zero or negative input.
#[inline]
pub fn linear_to_db(linear: f64) -> f64 {
    if linear <= 0.0 {
        DB_FLOOR
    } else {
        20.0 * linear.log10()
    }
}

// r[impl dsp.db.db-to-linear]
/// Convert decibels to linear amplitude.
///
/// Returns `0.0` for values at or below [`DB_FLOOR`].
#[inline]
pub fn db_to_linear(db: f64) -> f64 {
    if db <= DB_FLOOR {
        0.0
    } else {
        10.0_f64.powf(db / 20.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unity_roundtrip() {
        assert!((db_to_linear(0.0) - 1.0).abs() < 1e-12);
        assert!((linear_to_db(1.0) - 0.0).abs() < 1e-12);
    }

    #[test]
    fn known_values() {
        assert!((db_to_linear(-6.0) - 0.501187).abs() < 1e-4);
        assert!((db_to_linear(-20.0) - 0.1).abs() < 1e-10);
        assert!((linear_to_db(0.5) - (-6.0206)).abs() < 1e-3);
    }

    #[test]
    fn floor_handling() {
        assert_eq!(linear_to_db(0.0), DB_FLOOR);
        assert_eq!(linear_to_db(-1.0), DB_FLOOR);
        assert_eq!(db_to_linear(DB_FLOOR), 0.0);
        assert_eq!(db_to_linear(-300.0), 0.0);
    }

    #[test]
    fn roundtrip() {
        for &db in &[-60.0, -30.0, -12.0, -6.0, 0.0, 6.0, 12.0] {
            let rt = linear_to_db(db_to_linear(db));
            assert!(
                (rt - db).abs() < 1e-10,
                "Roundtrip failed for {db}: got {rt}"
            );
        }
    }
}
