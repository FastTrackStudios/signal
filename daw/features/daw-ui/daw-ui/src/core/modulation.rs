//! Modulation range visualization types.
//!
//! These types represent the range of modulation applied to a parameter,
//! used for visualizing modulation in UI widgets.

use super::normal::Normal;

/// Modulation range visualization data.
///
/// This represents the range of values a parameter can take due to modulation.
/// UI widgets use this to display a modulation indicator (arc, bar, etc.)
/// showing the extent of modulation.
///
/// # Example
///
/// ```
/// use audio_controls::core::modulation::ModulationRange;
/// use audio_controls::core::normal::Normal;
///
/// // A parameter at 0.5 with +/- 0.2 modulation
/// let mod_range = ModulationRange::from_offset(Normal::new(0.5), 0.2);
/// assert!((mod_range.start - 0.5).abs() < 0.01);
/// assert!((mod_range.end - 0.7).abs() < 0.01);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ModulationRange {
    /// Modulation start position (normalized 0.0-1.0).
    /// This is the lower bound of the modulation range.
    pub start: f32,
    /// Modulation end position (normalized 0.0-1.0).
    /// This is the upper bound of the modulation range.
    pub end: f32,
    /// Whether modulation is currently active.
    /// If false, widgets may hide the modulation indicator.
    pub active: bool,
}

impl ModulationRange {
    /// The minimum threshold for considering modulation active.
    const ACTIVE_THRESHOLD: f32 = 0.0001;

    /// Create a modulation range from an unmodulated value and offset.
    ///
    /// The offset is in normalized units, where positive values modulate
    /// upward and negative values modulate downward.
    #[must_use]
    pub fn from_offset(unmodulated: Normal, offset: f32) -> Self {
        let start = unmodulated.value();
        let end = (start + offset).clamp(0.0, 1.0);

        Self {
            start: start.min(end),
            end: start.max(end),
            active: offset.abs() > Self::ACTIVE_THRESHOLD,
        }
    }

    /// Create a modulation range from start and end values.
    #[must_use]
    pub fn from_bounds(start: f32, end: f32) -> Self {
        let (min, max) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };

        Self {
            start: min.clamp(0.0, 1.0),
            end: max.clamp(0.0, 1.0),
            active: (max - min).abs() > Self::ACTIVE_THRESHOLD,
        }
    }

    /// Create from unmodulated and modulated normalized values.
    ///
    /// This is the typical way to create a modulation range from nih-plug
    /// parameters using `unmodulated_normalized_value()` and
    /// `modulated_normalized_value()`.
    #[must_use]
    pub fn from_values(unmodulated: f32, modulated: f32) -> Self {
        Self::from_offset(Normal::new(unmodulated), modulated - unmodulated)
    }

    /// Width of the modulation range (0.0-1.0).
    #[must_use]
    pub fn width(&self) -> f32 {
        self.end - self.start
    }

    /// Center point of the modulation range.
    #[must_use]
    pub fn center(&self) -> f32 {
        (self.start + self.end) / 2.0
    }

    /// Check if a normalized value is within the modulation range.
    #[must_use]
    pub fn contains(&self, value: f32) -> bool {
        value >= self.start && value <= self.end
    }

    /// Convert the range to degrees for arc-based displays.
    ///
    /// Uses a standard 270-degree sweep (-135 to +135 degrees).
    #[must_use]
    pub fn to_arc_degrees(&self) -> (f32, f32) {
        const SWEEP: f32 = 270.0;
        const START_ANGLE: f32 = -135.0;

        let start_deg = START_ANGLE + (self.start * SWEEP);
        let end_deg = START_ANGLE + (self.end * SWEEP);

        (start_deg, end_deg)
    }
}

/// Style for rendering modulation ranges.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ModulationStyle {
    /// Overlay arc/range on top of the value indicator.
    /// The modulation range is shown as a semi-transparent overlay.
    #[default]
    Overlay,
    /// Separate ring/track for modulation.
    /// The modulation is shown on a distinct visual layer.
    SeparateRing,
    /// Dot indicator at the modulated position.
    /// Shows only the current modulated value, not the range.
    DotIndicator,
    /// Fill between unmodulated and modulated values.
    /// Shows the modulation as a filled region.
    FillBetween,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modulation_from_offset() {
        let unmod = Normal::new(0.5);

        // Positive offset
        let range = ModulationRange::from_offset(unmod, 0.2);
        assert!((range.start - 0.5).abs() < f32::EPSILON);
        assert!((range.end - 0.7).abs() < f32::EPSILON);
        assert!(range.active);

        // Negative offset
        let range = ModulationRange::from_offset(unmod, -0.3);
        assert!((range.start - 0.2).abs() < f32::EPSILON);
        assert!((range.end - 0.5).abs() < f32::EPSILON);
        assert!(range.active);

        // Zero offset (inactive)
        let range = ModulationRange::from_offset(unmod, 0.0);
        assert!(!range.active);
    }

    #[test]
    fn modulation_clamping() {
        let unmod = Normal::new(0.9);
        let range = ModulationRange::from_offset(unmod, 0.5);

        // Should clamp to 1.0
        assert!((range.end - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn modulation_width() {
        let range = ModulationRange::from_bounds(0.3, 0.7);
        assert!((range.width() - 0.4).abs() < f32::EPSILON);
    }

    #[test]
    fn modulation_from_values() {
        let range = ModulationRange::from_values(0.5, 0.8);
        assert!((range.start - 0.5).abs() < f32::EPSILON);
        assert!((range.end - 0.8).abs() < f32::EPSILON);
        assert!(range.active);
    }
}
