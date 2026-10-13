//! Normalized value type for audio parameters.
//!
//! The `Normal` type represents a value clamped to the [0.0, 1.0] range,
//! which is the standard range for normalized audio parameters.

/// A value normalized to the [0.0, 1.0] range.
///
/// This is the standard representation for parameter values in audio plugins.
/// All UI widgets work with normalized values internally, converting to/from
/// plain values using the appropriate range type.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Normal(f32);

impl Normal {
    /// The minimum normalized value (0.0).
    pub const MIN: Self = Self(0.0);

    /// The maximum normalized value (1.0).
    pub const MAX: Self = Self(1.0);

    /// The center normalized value (0.5).
    pub const CENTER: Self = Self(0.5);

    /// Create a new normalized value, clamping to [0.0, 1.0].
    #[must_use]
    pub fn new(value: f32) -> Self {
        Self(value.clamp(0.0, 1.0))
    }

    /// Create a normalized value without clamping.
    ///
    /// # Safety
    /// The caller must ensure the value is within [0.0, 1.0].
    #[must_use]
    pub const fn new_unchecked(value: f32) -> Self {
        Self(value)
    }

    /// Get the raw f32 value.
    #[must_use]
    pub const fn value(self) -> f32 {
        self.0
    }

    /// Convert to a bipolar value in the range [-1.0, 1.0].
    ///
    /// - 0.0 normalized -> -1.0 bipolar
    /// - 0.5 normalized -> 0.0 bipolar (center)
    /// - 1.0 normalized -> 1.0 bipolar
    #[must_use]
    pub fn to_bipolar(self) -> f32 {
        (self.0 - 0.5) * 2.0
    }

    /// Create from a bipolar value in the range [-1.0, 1.0].
    ///
    /// - -1.0 bipolar -> 0.0 normalized
    /// - 0.0 bipolar -> 0.5 normalized (center)
    /// - 1.0 bipolar -> 1.0 normalized
    #[must_use]
    pub fn from_bipolar(bipolar: f32) -> Self {
        Self::new((bipolar / 2.0) + 0.5)
    }

    /// Linear interpolation between two normalized values.
    #[must_use]
    pub fn lerp(self, other: Self, t: f32) -> Self {
        Self::new(self.0 + (other.0 - self.0) * t)
    }

    /// Calculate the distance to another normalized value.
    #[must_use]
    pub fn distance(self, other: Self) -> f32 {
        (self.0 - other.0).abs()
    }
}

impl From<f32> for Normal {
    fn from(value: f32) -> Self {
        Self::new(value)
    }
}

impl From<Normal> for f32 {
    fn from(normal: Normal) -> Self {
        normal.0
    }
}

/// A normalized parameter with value and default.
///
/// This is useful for widgets that need to know both the current value
/// and the default value (for double-click reset).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NormalParam {
    /// The current normalized value.
    pub value: Normal,
    /// The default normalized value.
    pub default: Normal,
}

impl NormalParam {
    /// Create a new parameter with value and default.
    #[must_use]
    pub fn new(value: Normal, default: Normal) -> Self {
        Self { value, default }
    }

    /// Create a parameter with the same value and default.
    #[must_use]
    pub fn with_default(value: f32) -> Self {
        let normal = Normal::new(value);
        Self {
            value: normal,
            default: normal,
        }
    }

    /// Check if the current value equals the default.
    #[must_use]
    pub fn is_default(self) -> bool {
        (self.value.0 - self.default.0).abs() < f32::EPSILON
    }

    /// Reset to the default value.
    #[must_use]
    pub fn reset(self) -> Self {
        Self {
            value: self.default,
            default: self.default,
        }
    }
}

impl Default for NormalParam {
    fn default() -> Self {
        Self {
            value: Normal::CENTER,
            default: Normal::CENTER,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_clamps_values() {
        assert_eq!(Normal::new(-0.5).value(), 0.0);
        assert_eq!(Normal::new(0.5).value(), 0.5);
        assert_eq!(Normal::new(1.5).value(), 1.0);
    }

    #[test]
    fn bipolar_conversion() {
        assert!((Normal::new(0.0).to_bipolar() - (-1.0)).abs() < f32::EPSILON);
        assert!((Normal::new(0.5).to_bipolar() - 0.0).abs() < f32::EPSILON);
        assert!((Normal::new(1.0).to_bipolar() - 1.0).abs() < f32::EPSILON);

        assert!((Normal::from_bipolar(-1.0).value() - 0.0).abs() < f32::EPSILON);
        assert!((Normal::from_bipolar(0.0).value() - 0.5).abs() < f32::EPSILON);
        assert!((Normal::from_bipolar(1.0).value() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn lerp_interpolation() {
        let a = Normal::new(0.0);
        let b = Normal::new(1.0);
        assert!((a.lerp(b, 0.5).value() - 0.5).abs() < f32::EPSILON);
    }
}
