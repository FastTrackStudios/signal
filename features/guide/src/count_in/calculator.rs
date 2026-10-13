//! Count-in measure and position calculations
//!
//! Calculates measure distances and count-in periods.

/// Calculator for count-in measures and positions
pub struct CountInCalculator;

impl CountInCalculator {
    /// Calculate the total number of measures in the count-in period
    ///
    /// For first sections: distance from Count-In marker to section start
    /// For SONGEND: always 2 measures before SONGEND
    #[must_use]
    pub fn calculate_count_in_measures(
        count_start_quarters: f64,
        target_start_quarters: f64,
        measure_length_quarters: f64,
    ) -> i32 {
        let distance_quarters = target_start_quarters - count_start_quarters;
        // Round to nearest measure instead of flooring
        let rounded = (distance_quarters / measure_length_quarters).round();
        let measures = crate::cast::i32_from_f64_saturating(rounded);
        // Clamp to 1-8 measures
        measures.clamp(1, 8)
    }
}
