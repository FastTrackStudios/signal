//! Duration trait and implementations
//!
//! Represents musical durations in measure.beats.subdivision format

use facet::Facet;
use std::fmt;

/// Represents a time signature for duration calculations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
pub struct TimeSignature {
    pub numerator: u8,
    pub denominator: u8,
}

impl TimeSignature {
    pub fn new(numerator: u8, denominator: u8) -> Self {
        Self {
            numerator,
            denominator,
        }
    }

    pub fn common_time() -> Self {
        Self {
            numerator: 4,
            denominator: 4,
        }
    }

    pub fn beats_per_measure(&self) -> u8 {
        self.numerator
    }
}

/// Trait for musical durations
pub trait Duration: fmt::Debug + fmt::Display {
    /// Get the number of whole measures
    fn measures(&self) -> u32;

    /// Get the number of beats within the current measure
    fn beats(&self) -> u32;

    /// Get the subdivisions (e.g., 50 = half beat, 25 = quarter beat)
    fn subdivisions(&self) -> u32;

    /// Convert to total beats as f64
    fn to_beats(&self, time_sig: TimeSignature) -> f64;

    /// Add two durations together, respecting time signature
    fn add(&self, other: &dyn Duration, time_sig: TimeSignature) -> Box<dyn Duration>;

    /// Clone as a boxed trait object
    fn clone_box(&self) -> Box<dyn Duration>;
}

/// Standard implementation of Duration
#[derive(Debug, Clone, PartialEq, Eq, Hash, Facet)]
pub struct MusicalDuration {
    pub measures: u32,
    pub beats: u32,
    pub subdivisions: u32,
}

impl MusicalDuration {
    pub fn new(measures: u32, beats: u32, subdivisions: u32) -> Self {
        Self {
            measures,
            beats,
            subdivisions,
        }
    }

    /// Create from total beats
    pub fn from_beats(total_beats: f64, time_sig: TimeSignature) -> Self {
        let beats_per_measure = time_sig.numerator as f64;

        let measures = (total_beats / beats_per_measure) as u32;
        let remaining_beats = total_beats - (measures as f64 * beats_per_measure);

        let beats = remaining_beats as u32;
        let subdivisions = ((remaining_beats - beats as f64) * 1000.0) as u32;

        Self {
            measures,
            beats,
            subdivisions,
        }
    }

    /// Normalize the duration to ensure subdivisions and beats don't overflow
    pub fn normalize(mut self, time_sig: TimeSignature) -> Self {
        let beats_per_measure = time_sig.numerator as u32;

        // Handle subdivision overflow (1000 subdivisions per beat)
        if self.subdivisions >= 1000 {
            self.beats += self.subdivisions / 1000;
            self.subdivisions %= 1000;
        }

        // Handle beat overflow
        if self.beats >= beats_per_measure {
            self.measures += self.beats / beats_per_measure;
            self.beats %= beats_per_measure;
        }

        self
    }
}

impl Duration for MusicalDuration {
    fn measures(&self) -> u32 {
        self.measures
    }

    fn beats(&self) -> u32 {
        self.beats
    }

    fn subdivisions(&self) -> u32 {
        self.subdivisions
    }

    fn to_beats(&self, time_sig: TimeSignature) -> f64 {
        let beats_per_measure = time_sig.numerator as f64;
        (self.measures as f64 * beats_per_measure)
            + (self.beats as f64)
            + (self.subdivisions as f64 / 1000.0)
    }

    fn add(&self, other: &dyn Duration, time_sig: TimeSignature) -> Box<dyn Duration> {
        let sum = MusicalDuration {
            measures: self.measures + other.measures(),
            beats: self.beats + other.beats(),
            subdivisions: self.subdivisions + other.subdivisions(),
        };
        Box::new(sum.normalize(time_sig))
    }

    fn clone_box(&self) -> Box<dyn Duration> {
        Box::new(self.clone())
    }
}

impl fmt::Display for MusicalDuration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}.{}.{:02}",
            self.measures, self.beats, self.subdivisions
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_duration_creation() {
        let dur = MusicalDuration::new(1, 2, 500);
        assert_eq!(dur.measures(), 1);
        assert_eq!(dur.beats(), 2);
        assert_eq!(dur.subdivisions(), 500);
    }

    #[test]
    fn test_duration_to_beats() {
        let dur = MusicalDuration::new(1, 2, 500);
        let time_sig = TimeSignature::common_time();
        assert_eq!(dur.to_beats(time_sig), 6.5); // 1 measure (4 beats) + 2 beats + 0.5 beats
    }

    #[test]
    fn test_duration_from_beats() {
        let time_sig = TimeSignature::common_time();
        let dur = MusicalDuration::from_beats(6.5, time_sig);
        assert_eq!(dur.measures(), 1);
        assert_eq!(dur.beats(), 2);
        assert_eq!(dur.subdivisions(), 500);
    }

    #[test]
    fn test_duration_add() {
        let dur1 = MusicalDuration::new(0, 2, 0);
        let dur2 = MusicalDuration::new(0, 2, 0);
        let time_sig = TimeSignature::common_time();

        let sum = dur1.add(&dur2, time_sig);
        assert_eq!(sum.measures(), 1);
        assert_eq!(sum.beats(), 0);
        assert_eq!(sum.subdivisions(), 0);
    }

    #[test]
    fn test_duration_normalize() {
        let dur = MusicalDuration::new(0, 5, 1500);
        let time_sig = TimeSignature::common_time();
        let normalized = dur.normalize(time_sig);

        assert_eq!(normalized.measures(), 1);
        assert_eq!(normalized.beats(), 2);
        assert_eq!(normalized.subdivisions(), 500);
    }

    #[test]
    fn test_duration_display() {
        let dur = MusicalDuration::new(1, 2, 500);
        assert_eq!(format!("{}", dur), "1.2.500");
    }
}
