//! Chart position for musical coordinates
//!
//! Represents hierarchical position within a rendered chart,
//! enabling navigation and synchronization with DAW timelines.

use facet::Facet;

/// Comprehensive position within a chart for rendered elements.
///
/// This is the "page coordinates" system for music notation, tracking
/// where an element appears in the rendered score. Used for:
/// - Navigation (jump to measure 32, beat 3)
/// - DAW synchronization (tick-based position)
/// - Layout queries (find all elements in system 2)
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, Facet)]
pub struct ChartPosition {
    /// System index (which horizontal line of music on the page, 0-indexed).
    /// A "system" is one complete line of music across the page width.
    pub system: u32,

    /// Staff line within system (0 for single staff, 0-1 for grand staff).
    /// For lead sheets this is always 0; for piano scores it tracks upper/lower staff.
    pub line: u32,

    /// Global measure number (0-indexed across entire chart).
    /// This is the absolute measure count from the start of the piece.
    pub measure: u32,

    /// Beat index within measure (0-indexed, based on time signature numerator).
    /// In 4/4 time: 0, 1, 2, 3. In 3/4 time: 0, 1, 2.
    pub beat: u32,

    /// Subdivision index within beat (0-999, for sub-beat precision).
    /// 0 = on the beat, 500 = halfway through beat, etc.
    pub subdivision: u32,

    /// Absolute tick position (PPQ-based, for DAW sync).
    /// Standard resolution is 480 ticks per quarter note.
    pub tick: i64,

    /// Staff index (for multi-staff scores like piano or orchestra).
    /// 0 = first staff, 1 = second staff, etc.
    pub staff: u32,
}

impl ChartPosition {
    /// Create a new position with basic coordinates.
    #[must_use]
    pub const fn new(system: u32, measure: u32, beat: u32) -> Self {
        Self {
            system,
            line: 0,
            measure,
            beat,
            subdivision: 0,
            tick: 0,
            staff: 0,
        }
    }

    /// Create a position at the start of a measure.
    #[must_use]
    pub const fn at_measure(measure: u32) -> Self {
        Self::new(0, measure, 0)
    }

    /// Create a position at a specific beat within a measure.
    #[must_use]
    pub const fn at_beat(measure: u32, beat: u32) -> Self {
        Self::new(0, measure, beat)
    }

    /// Create a position with full precision.
    #[must_use]
    pub const fn full(
        system: u32,
        line: u32,
        measure: u32,
        beat: u32,
        subdivision: u32,
        tick: i64,
        staff: u32,
    ) -> Self {
        Self {
            system,
            line,
            measure,
            beat,
            subdivision,
            tick,
            staff,
        }
    }

    /// Set the tick value.
    #[must_use]
    pub const fn with_tick(mut self, tick: i64) -> Self {
        self.tick = tick;
        self
    }

    /// Set the system index.
    #[must_use]
    pub const fn with_system(mut self, system: u32) -> Self {
        self.system = system;
        self
    }

    /// Set the line within system.
    #[must_use]
    pub const fn with_line(mut self, line: u32) -> Self {
        self.line = line;
        self
    }

    /// Set the staff index.
    #[must_use]
    pub const fn with_staff(mut self, staff: u32) -> Self {
        self.staff = staff;
        self
    }

    /// Set the subdivision.
    #[must_use]
    pub const fn with_subdivision(mut self, subdivision: u32) -> Self {
        self.subdivision = subdivision;
        self
    }

    /// Check if this position is at the start of a measure (beat 0, subdivision 0).
    #[must_use]
    pub const fn is_measure_start(&self) -> bool {
        self.beat == 0 && self.subdivision == 0
    }

    /// Check if this position is on a beat (subdivision 0).
    #[must_use]
    pub const fn is_on_beat(&self) -> bool {
        self.subdivision == 0
    }

    /// Compare positions by musical time (measure, beat, subdivision).
    /// Returns Ordering based on when the position occurs in the music.
    #[must_use]
    pub fn musical_cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.measure
            .cmp(&other.measure)
            .then(self.beat.cmp(&other.beat))
            .then(self.subdivision.cmp(&other.subdivision))
    }

    /// Check if this position is before another in musical time.
    #[must_use]
    pub fn is_before(&self, other: &Self) -> bool {
        self.musical_cmp(other) == std::cmp::Ordering::Less
    }

    /// Check if this position is after another in musical time.
    #[must_use]
    pub fn is_after(&self, other: &Self) -> bool {
        self.musical_cmp(other) == std::cmp::Ordering::Greater
    }

    /// Calculate tick from measure/beat/subdivision using standard PPQ.
    /// Assumes 480 PPQ and the given beats per measure.
    #[must_use]
    pub fn calculate_tick(&self, beats_per_measure: u32) -> i64 {
        const PPQ: i64 = 480;
        let measure_ticks = self.measure as i64 * beats_per_measure as i64 * PPQ;
        let beat_ticks = self.beat as i64 * PPQ;
        let subdivision_ticks = (self.subdivision as i64 * PPQ) / 1000;
        measure_ticks + beat_ticks + subdivision_ticks
    }
}

impl Default for ChartPosition {
    fn default() -> Self {
        Self::new(0, 0, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_position() {
        let pos = ChartPosition::new(1, 4, 2);
        assert_eq!(pos.system, 1);
        assert_eq!(pos.measure, 4);
        assert_eq!(pos.beat, 2);
        assert_eq!(pos.subdivision, 0);
        assert_eq!(pos.tick, 0);
    }

    #[test]
    fn test_at_measure() {
        let pos = ChartPosition::at_measure(8);
        assert_eq!(pos.measure, 8);
        assert_eq!(pos.beat, 0);
        assert!(pos.is_measure_start());
    }

    #[test]
    fn test_at_beat() {
        let pos = ChartPosition::at_beat(4, 2);
        assert_eq!(pos.measure, 4);
        assert_eq!(pos.beat, 2);
        assert!(pos.is_on_beat());
        assert!(!pos.is_measure_start());
    }

    #[test]
    fn test_musical_comparison() {
        let pos1 = ChartPosition::at_beat(4, 2);
        let pos2 = ChartPosition::at_beat(4, 3);
        let pos3 = ChartPosition::at_beat(5, 0);

        assert!(pos1.is_before(&pos2));
        assert!(pos2.is_before(&pos3));
        assert!(pos1.is_before(&pos3));
        assert!(pos3.is_after(&pos1));
    }

    #[test]
    fn test_calculate_tick() {
        let pos = ChartPosition::at_beat(2, 1); // Measure 2, beat 1
        let tick = pos.calculate_tick(4); // 4 beats per measure

        // Measure 2 = 2 * 4 * 480 = 3840 ticks
        // Beat 1 = 1 * 480 = 480 ticks
        // Total = 4320 ticks
        assert_eq!(tick, 4320);
    }

    #[test]
    fn test_with_builders() {
        let pos = ChartPosition::new(0, 4, 2)
            .with_system(1)
            .with_tick(1920)
            .with_subdivision(500);

        assert_eq!(pos.system, 1);
        assert_eq!(pos.tick, 1920);
        assert_eq!(pos.subdivision, 500);
        assert!(!pos.is_on_beat());
    }
}
