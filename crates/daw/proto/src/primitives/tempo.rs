//! Tempo (BPM) types

use facet::Facet;
use std::fmt;

/// Tempo in beats per minute (BPM)
///
/// Must be > 0. Typical range is 20-300 BPM, but can go up to 960 BPM in REAPER.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Facet)]
pub struct Tempo {
    pub bpm: f64,
}

impl Tempo {
    /// Create a tempo from BPM
    ///
    /// # Panics
    /// Panics if BPM is <= 0
    pub fn from_bpm(bpm: f64) -> Self {
        assert!(bpm > 0.0, "BPM must be positive, got {}", bpm);
        Self { bpm }
    }

    /// Try to create a tempo from BPM
    pub fn try_from_bpm(bpm: f64) -> Result<Self, String> {
        if bpm <= 0.0 {
            Err(format!("BPM must be positive, got {}", bpm))
        } else {
            Ok(Self { bpm })
        }
    }

    /// Get the BPM value
    pub fn bpm(&self) -> f64 {
        self.bpm
    }

    /// Common tempo: 120 BPM
    pub const ONE_TWENTY: Self = Self { bpm: 120.0 };

    /// Minimum reasonable tempo: 1 BPM
    pub const ONE: Self = Self { bpm: 1.0 };

    /// Maximum REAPER tempo: 960 BPM
    pub const NINE_SIXTY: Self = Self { bpm: 960.0 };
}

impl Default for Tempo {
    fn default() -> Self {
        Self::ONE_TWENTY
    }
}

impl fmt::Display for Tempo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2} BPM", self.bpm)
    }
}
