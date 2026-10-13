//! PPQ-aware tick representation.
//!
//! The `Ticks<const PPQ: u32>` newtype provides type-safe tick values with
//! zero-cost conversions between different PPQ resolutions.

use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

/// Type-safe tick value with const generic PPQ (Parts Per Quarter note) resolution.
///
/// This newtype ensures that tick values carry their PPQ resolution in the type,
/// preventing accidental mixing of ticks from different resolutions.
///
/// # Zero-Cost PPQ Conversion
///
/// Converting between PPQ resolutions is a compile-time operation when both
/// PPQ values are known at compile time:
///
/// ```
/// use keyflow_proto::core::{StandardTicks, ReaperTicks, Ticks};
///
/// let standard = StandardTicks::new(480); // 480 PPQ
/// let reaper: ReaperTicks = standard.to_ppq(); // Convert to 960 PPQ
/// assert_eq!(reaper.0, 960);
/// ```
///
/// # Common PPQ Resolutions
///
/// - 480: Standard MIDI resolution
/// - 960: REAPER default
/// - 96: Simple/low-resolution MIDI
/// - 24: MIDI clock resolution
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct Ticks<const PPQ: u32>(pub i64);

/// Standard MIDI tick resolution (480 PPQ).
pub type StandardTicks = Ticks<480>;

/// REAPER tick resolution (960 PPQ).
pub type ReaperTicks = Ticks<960>;

impl<const PPQ: u32> Ticks<PPQ> {
    /// Create a new tick value.
    #[must_use]
    pub const fn new(value: i64) -> Self {
        Self(value)
    }

    /// Zero ticks.
    pub const ZERO: Self = Self(0);

    /// Get the PPQ resolution of this tick type.
    #[must_use]
    pub const fn ppq() -> u32 {
        PPQ
    }

    /// Get the raw tick value.
    #[must_use]
    pub const fn value(self) -> i64 {
        self.0
    }

    /// Convert to a different PPQ resolution.
    ///
    /// This is a const fn that performs the conversion at compile time when possible.
    ///
    /// # Example
    ///
    /// ```
    /// use keyflow_proto::core::{StandardTicks, ReaperTicks};
    ///
    /// let standard = StandardTicks::new(480); // One quarter note at 480 PPQ
    /// let reaper: ReaperTicks = standard.to_ppq();
    /// assert_eq!(reaper.0, 960); // Same quarter note at 960 PPQ
    /// ```
    #[must_use]
    pub const fn to_ppq<const TARGET: u32>(self) -> Ticks<TARGET> {
        Ticks(self.0 * TARGET as i64 / PPQ as i64)
    }

    /// Convert to standard 480 PPQ ticks.
    #[must_use]
    pub const fn to_standard(self) -> StandardTicks {
        self.to_ppq::<480>()
    }

    /// Convert to REAPER 960 PPQ ticks.
    #[must_use]
    pub const fn to_reaper(self) -> ReaperTicks {
        self.to_ppq::<960>()
    }

    /// Convert to quarter notes (f64).
    #[must_use]
    pub fn to_quarters(self) -> f64 {
        self.0 as f64 / PPQ as f64
    }

    /// Create from quarter notes.
    #[must_use]
    pub fn from_quarters(quarters: f64) -> Self {
        Self((quarters * PPQ as f64).round() as i64)
    }

    /// Get the absolute value.
    #[must_use]
    pub const fn abs(self) -> Self {
        if self.0 < 0 { Self(-self.0) } else { self }
    }

    /// Check if this is zero.
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// Check if this is negative.
    #[must_use]
    pub const fn is_negative(self) -> bool {
        self.0 < 0
    }

    /// Check if this is positive.
    #[must_use]
    pub const fn is_positive(self) -> bool {
        self.0 > 0
    }
}

impl<const PPQ: u32> Default for Ticks<PPQ> {
    fn default() -> Self {
        Self::ZERO
    }
}

impl<const PPQ: u32> std::fmt::Display for Ticks<PPQ> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ticks @ {} PPQ", self.0, PPQ)
    }
}

// Arithmetic operations

impl<const PPQ: u32> Add for Ticks<PPQ> {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl<const PPQ: u32> AddAssign for Ticks<PPQ> {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl<const PPQ: u32> Sub for Ticks<PPQ> {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl<const PPQ: u32> SubAssign for Ticks<PPQ> {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

impl<const PPQ: u32> Neg for Ticks<PPQ> {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self(-self.0)
    }
}

impl<const PPQ: u32> Mul<i64> for Ticks<PPQ> {
    type Output = Self;

    fn mul(self, rhs: i64) -> Self::Output {
        Self(self.0 * rhs)
    }
}

impl<const PPQ: u32> Div<i64> for Ticks<PPQ> {
    type Output = Self;

    fn div(self, rhs: i64) -> Self::Output {
        Self(self.0 / rhs)
    }
}

// Conversion from i32/i64 for convenience

impl<const PPQ: u32> From<i32> for Ticks<PPQ> {
    fn from(value: i32) -> Self {
        Self(i64::from(value))
    }
}

impl<const PPQ: u32> From<i64> for Ticks<PPQ> {
    fn from(value: i64) -> Self {
        Self(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ppq_conversion() {
        // Standard to REAPER
        let standard = StandardTicks::new(480);
        let reaper: ReaperTicks = standard.to_ppq();
        assert_eq!(reaper.0, 960);

        // REAPER to Standard
        let reaper = ReaperTicks::new(960);
        let standard: StandardTicks = reaper.to_ppq();
        assert_eq!(standard.0, 480);
    }

    #[test]
    fn test_ppq_conversion_roundtrip() {
        let original = StandardTicks::new(123);
        let reaper: ReaperTicks = original.to_ppq();
        let back: StandardTicks = reaper.to_ppq();
        // Note: May not be exact due to integer division
        assert_eq!(back.0, 123);
    }

    #[test]
    fn test_quarters_conversion() {
        let quarter = StandardTicks::new(480);
        assert!((quarter.to_quarters() - 1.0).abs() < 0.001);

        let from_quarters = StandardTicks::from_quarters(2.5);
        assert_eq!(from_quarters.0, 1200); // 2.5 * 480
    }

    #[test]
    fn test_arithmetic() {
        let a = StandardTicks::new(480);
        let b = StandardTicks::new(240);

        assert_eq!((a + b).0, 720);
        assert_eq!((a - b).0, 240);
        assert_eq!((-a).0, -480);
        assert_eq!((a * 2).0, 960);
        assert_eq!((a / 2).0, 240);
    }

    #[test]
    fn test_display() {
        let ticks = StandardTicks::new(480);
        assert_eq!(format!("{ticks}"), "480 ticks @ 480 PPQ");
    }

    #[test]
    fn test_ppq_method() {
        assert_eq!(StandardTicks::ppq(), 480);
        assert_eq!(ReaperTicks::ppq(), 960);
    }
}
