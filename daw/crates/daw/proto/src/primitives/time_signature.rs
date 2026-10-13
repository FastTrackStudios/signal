//! Time signature types

use facet::Facet;
use std::fmt;

/// Time signature (e.g., 4/4, 3/4, 6/8)
///
/// Both numerator and denominator must be non-zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
pub struct TimeSignature {
    pub numerator: u32,
    pub denominator: u32,
}

impl TimeSignature {
    /// Create a time signature
    ///
    /// # Panics
    /// Panics if numerator or denominator is 0
    pub fn new(numerator: u32, denominator: u32) -> Self {
        assert!(numerator > 0, "Time signature numerator cannot be 0");
        assert!(denominator > 0, "Time signature denominator cannot be 0");
        Self {
            numerator,
            denominator,
        }
    }

    /// Try to create a time signature
    pub fn try_new(numerator: u32, denominator: u32) -> Result<Self, String> {
        if numerator == 0 {
            return Err("Time signature numerator cannot be 0".to_string());
        }
        if denominator == 0 {
            return Err("Time signature denominator cannot be 0".to_string());
        }
        Ok(Self {
            numerator,
            denominator,
        })
    }

    /// Get the numerator (beats per measure)
    pub fn numerator(&self) -> u32 {
        self.numerator
    }

    /// Get the denominator (beat unit)
    pub fn denominator(&self) -> u32 {
        self.denominator
    }

    /// Get beats per measure as f64
    pub fn beats_per_measure(&self) -> f64 {
        self.numerator as f64
    }

    /// Get quarter notes per measure
    ///
    /// For 4/4: 4.0 quarter notes
    /// For 3/4: 3.0 quarter notes
    /// For 6/8: 3.0 quarter notes (6 eighth notes = 3 quarters)
    pub fn quarter_notes_per_measure(&self) -> f64 {
        let numerator = self.numerator as f64;
        let denominator = self.denominator as f64;
        numerator * (4.0 / denominator)
    }

    /// Common time (4/4)
    pub const COMMON_TIME: Self = Self {
        numerator: 4,
        denominator: 4,
    };

    /// Cut time (2/2)
    pub const CUT_TIME: Self = Self {
        numerator: 2,
        denominator: 2,
    };

    /// Waltz time (3/4)
    pub const WALTZ: Self = Self {
        numerator: 3,
        denominator: 4,
    };
}

impl Default for TimeSignature {
    fn default() -> Self {
        Self::COMMON_TIME
    }
}

impl fmt::Display for TimeSignature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.numerator, self.denominator)
    }
}
