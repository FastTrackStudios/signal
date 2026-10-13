//! Position trait and implementations
//!
//! Represents absolute positions within a musical piece

use super::duration::{Duration, MusicalDuration};
use std::fmt;

/// Trait for positions within a song
pub trait Position: fmt::Debug + fmt::Display {
    /// Get the duration from the start of the song to this position
    fn duration(&self) -> &dyn Duration;

    /// Get the section index this position is in
    fn section_index(&self) -> usize;

    /// Clone as a boxed trait object
    fn clone_box(&self) -> Box<dyn Position>;
}

/// Standard implementation of Position
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct AbsolutePosition {
    /// The total duration from the start of the song
    pub total_duration: MusicalDuration,
    /// Which section this position is in
    pub section_index: usize,
}

impl AbsolutePosition {
    pub fn new(total_duration: MusicalDuration, section_index: usize) -> Self {
        Self {
            total_duration,
            section_index,
        }
    }

    /// Create a position at the start of a section
    pub fn at_section_start(total_duration: MusicalDuration, section_index: usize) -> Self {
        Self {
            total_duration,
            section_index,
        }
    }

    /// Create a position at the very beginning of the song
    pub fn at_beginning() -> Self {
        Self {
            total_duration: MusicalDuration::new(0, 0, 0),
            section_index: 0,
        }
    }
}

impl Position for AbsolutePosition {
    fn duration(&self) -> &dyn Duration {
        &self.total_duration
    }

    fn section_index(&self) -> usize {
        self.section_index
    }

    fn clone_box(&self) -> Box<dyn Position> {
        Box::new(self.clone())
    }
}

impl fmt::Display for AbsolutePosition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Section {} @ {}",
            self.section_index, self.total_duration
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_absolute_position_creation() {
        let dur = MusicalDuration::new(2, 1, 0);
        let pos = AbsolutePosition::new(dur, 3);

        assert_eq!(pos.section_index(), 3);
        assert_eq!(pos.duration().measures(), 2);
        assert_eq!(pos.duration().beats(), 1);
    }

    #[test]
    fn test_position_at_beginning() {
        let pos = AbsolutePosition::at_beginning();
        assert_eq!(pos.section_index(), 0);
        assert_eq!(pos.duration().measures(), 0);
        assert_eq!(pos.duration().beats(), 0);
    }

    #[test]
    fn test_position_at_section_start() {
        let dur = MusicalDuration::new(8, 0, 0);
        let pos = AbsolutePosition::at_section_start(dur, 2);

        assert_eq!(pos.section_index(), 2);
        assert_eq!(pos.duration().measures(), 8);
    }

    #[test]
    fn test_position_display() {
        let dur = MusicalDuration::new(2, 1, 50);
        let pos = AbsolutePosition::new(dur, 3);
        assert_eq!(format!("{}", pos), "Section 3 @ 2.1.50");
    }
}
