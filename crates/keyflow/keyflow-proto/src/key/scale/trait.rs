//! Scale family trait
//!
//! Defines the interface for scale families with modes

/// Trait for scale families - each family defines a base pattern and its modes
pub trait ScaleFamily {
    /// The mode enum type for this family
    type Mode: Copy + PartialEq + Eq;

    /// Get the base semitone pattern (root mode)
    fn base_pattern() -> Vec<u8>;

    /// Get the number of modes in this family
    fn mode_count() -> usize {
        7 // Most scales have 7 modes
    }

    /// Get the mode name for a given rotation
    fn mode_name(rotation: usize) -> &'static str;

    /// Get a short name for a mode
    fn mode_short_name(rotation: usize) -> &'static str;

    /// Get the semitone pattern for a specific mode (rotation)
    fn pattern_for_mode(rotation: usize) -> Vec<u8> {
        let base = Self::base_pattern();
        let mode_count = Self::mode_count();
        let rotation = rotation % mode_count;

        let mut pattern = Vec::new();
        for i in 0..mode_count {
            let idx = (i + rotation) % mode_count;
            let semitone = base[idx];
            let adjusted = if i == 0 {
                0
            } else {
                (semitone + 12 - base[rotation]) % 12
            };
            pattern.push(adjusted);
        }
        pattern
    }
}
