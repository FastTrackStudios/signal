//! Tempo-synced note values — maps musical divisions to delay times.
//!
//! Shared across all plugins that need tempo sync (delay, LFO, modulation, etc.).
//!
//! # Usage
//!
//! ```
//! use audiocore_dsp::note_sync::NoteValue;
//!
//! let bpm = 120.0;
//! let eighth = NoteValue::Eighth;
//! assert!((eighth.to_ms(bpm) - 250.0).abs() < 0.01);
//! ```

/// Musical note division for tempo sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum NoteValue {
    // Straight
    Whole = 0,
    Half = 1,
    Quarter = 2,
    Eighth = 3,
    Sixteenth = 4,
    ThirtySecond = 5,

    // Dotted (1.5x straight)
    DottedWhole = 6,
    DottedHalf = 7,
    DottedQuarter = 8,
    DottedEighth = 9,
    DottedSixteenth = 10,

    // Triplet (2/3x straight)
    TripletHalf = 11,
    TripletQuarter = 12,
    TripletEighth = 13,
    TripletSixteenth = 14,
}

impl NoteValue {
    /// All note values in order (longest to shortest).
    ///
    /// Multipliers: 6.0, 4.0, 3.0, 2.0, 1.5, 4/3, 1.0, 0.75, 2/3, 0.5,
    ///              0.375, 1/3, 0.25, 1/6, 0.125
    pub const ALL: &'static [NoteValue] = &[
        NoteValue::DottedWhole,      // 6.0
        NoteValue::Whole,            // 4.0
        NoteValue::DottedHalf,       // 3.0
        NoteValue::Half,             // 2.0
        NoteValue::DottedQuarter,    // 1.5
        NoteValue::TripletHalf,      // 4/3 ≈ 1.333
        NoteValue::Quarter,          // 1.0
        NoteValue::DottedEighth,     // 0.75
        NoteValue::TripletQuarter,   // 2/3 ≈ 0.667
        NoteValue::Eighth,           // 0.5
        NoteValue::DottedSixteenth,  // 0.375
        NoteValue::TripletEighth,    // 1/3 ≈ 0.333
        NoteValue::Sixteenth,        // 0.25
        NoteValue::TripletSixteenth, // 1/6 ≈ 0.167
        NoteValue::ThirtySecond,     // 0.125
    ];

    /// Multiplier relative to a quarter note.
    ///
    /// Quarter note = 1.0, half = 2.0, eighth = 0.5, etc.
    pub fn quarter_note_multiplier(self) -> f64 {
        match self {
            // Straight
            NoteValue::Whole => 4.0,
            NoteValue::Half => 2.0,
            NoteValue::Quarter => 1.0,
            NoteValue::Eighth => 0.5,
            NoteValue::Sixteenth => 0.25,
            NoteValue::ThirtySecond => 0.125,

            // Dotted (1.5x)
            NoteValue::DottedWhole => 6.0,
            NoteValue::DottedHalf => 3.0,
            NoteValue::DottedQuarter => 1.5,
            NoteValue::DottedEighth => 0.75,
            NoteValue::DottedSixteenth => 0.375,

            // Triplet (2/3x)
            NoteValue::TripletHalf => 4.0 / 3.0,
            NoteValue::TripletQuarter => 2.0 / 3.0,
            NoteValue::TripletEighth => 1.0 / 3.0,
            NoteValue::TripletSixteenth => 1.0 / 6.0,
        }
    }

    /// Convert to milliseconds at the given BPM.
    pub fn to_ms(self, bpm: f64) -> f64 {
        if bpm <= 0.0 {
            return 0.0;
        }
        let quarter_ms = 60_000.0 / bpm;
        quarter_ms * self.quarter_note_multiplier()
    }

    /// Convert to samples at the given BPM and sample rate.
    pub fn to_samples(self, bpm: f64, sample_rate: f64) -> f64 {
        self.to_ms(bpm) * 0.001 * sample_rate
    }

    /// Short display name for UI.
    pub fn label(self) -> &'static str {
        match self {
            NoteValue::Whole => "1/1",
            NoteValue::Half => "1/2",
            NoteValue::Quarter => "1/4",
            NoteValue::Eighth => "1/8",
            NoteValue::Sixteenth => "1/16",
            NoteValue::ThirtySecond => "1/32",
            NoteValue::DottedWhole => "1/1.",
            NoteValue::DottedHalf => "1/2.",
            NoteValue::DottedQuarter => "1/4.",
            NoteValue::DottedEighth => "1/8.",
            NoteValue::DottedSixteenth => "1/16.",
            NoteValue::TripletHalf => "1/2T",
            NoteValue::TripletQuarter => "1/4T",
            NoteValue::TripletEighth => "1/8T",
            NoteValue::TripletSixteenth => "1/16T",
        }
    }

    /// Convert from integer index (for parameter mapping).
    pub fn from_index(idx: usize) -> Option<NoteValue> {
        NoteValue::ALL.get(idx).copied()
    }

    /// Index into `ALL` array (for parameter mapping).
    pub fn to_index(self) -> usize {
        NoteValue::ALL.iter().position(|&v| v == self).unwrap_or(0)
    }

    /// Number of available note values.
    pub const COUNT: usize = 15;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quarter_note_at_120_bpm() {
        assert!((NoteValue::Quarter.to_ms(120.0) - 500.0).abs() < 0.01);
    }

    #[test]
    fn eighth_note_at_120_bpm() {
        assert!((NoteValue::Eighth.to_ms(120.0) - 250.0).abs() < 0.01);
    }

    #[test]
    fn dotted_eighth_at_120_bpm() {
        assert!((NoteValue::DottedEighth.to_ms(120.0) - 375.0).abs() < 0.01);
    }

    #[test]
    fn triplet_eighth_at_120_bpm() {
        let expected = 500.0 / 3.0; // ~166.67 ms
        assert!((NoteValue::TripletEighth.to_ms(120.0) - expected).abs() < 0.01);
    }

    #[test]
    fn whole_note_at_120_bpm() {
        assert!((NoteValue::Whole.to_ms(120.0) - 2000.0).abs() < 0.01);
    }

    #[test]
    fn zero_bpm_returns_zero() {
        assert_eq!(NoteValue::Quarter.to_ms(0.0), 0.0);
    }

    #[test]
    fn to_samples_at_48k() {
        let samples = NoteValue::Quarter.to_ms(120.0) * 0.001 * 48000.0;
        assert!((NoteValue::Quarter.to_samples(120.0, 48000.0) - samples).abs() < 0.01);
    }

    #[test]
    fn all_values_have_unique_labels() {
        let labels: Vec<&str> = NoteValue::ALL.iter().map(|v| v.label()).collect();
        for (i, a) in labels.iter().enumerate() {
            for (j, b) in labels.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "Duplicate label at indices {i} and {j}");
                }
            }
        }
    }

    #[test]
    fn index_roundtrip() {
        for (i, &val) in NoteValue::ALL.iter().enumerate() {
            assert_eq!(val.to_index(), i);
            assert_eq!(NoteValue::from_index(i), Some(val));
        }
    }

    #[test]
    fn all_sorted_longest_to_shortest() {
        let ms: Vec<f64> = NoteValue::ALL.iter().map(|v| v.to_ms(120.0)).collect();
        for i in 1..ms.len() {
            assert!(
                ms[i] <= ms[i - 1],
                "Not sorted at index {i}: {} > {}",
                ms[i],
                ms[i - 1]
            );
        }
    }

    #[test]
    fn count_matches_all() {
        assert_eq!(NoteValue::COUNT, NoteValue::ALL.len());
    }
}
