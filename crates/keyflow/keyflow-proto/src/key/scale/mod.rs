//! Scale system - organized by family
//!
//! Each scale family has its own module with modes and patterns

// Rename 'trait' module to avoid keyword conflict
#[path = "trait.rs"]
pub mod trait_module;

pub mod diatonic;
pub mod harmonic_minor;
pub mod harmonization;
pub mod melodic_minor;
pub mod unified;

// Re-export the trait
pub use trait_module::ScaleFamily;

// Re-export family structs and mode enums
pub use diatonic::{DiatonicFamily, DiatonicMode};
pub use harmonic_minor::{HarmonicMinorFamily, HarmonicMinorMode};
pub use melodic_minor::{MelodicMinorFamily, MelodicMinorMode};

// Re-export unified types
pub use unified::{ScaleMode, ScaleType};

// Re-export harmonization types
pub use harmonization::{
    HarmonizationDepth, ScaleHarmonization, analyze_scale_harmony, generate_scale_notes,
    generate_scale_semitones, harmonize_scale,
};

#[cfg(test)]
mod tests {
    use super::*;

    // Diatonic Mode Tests
    #[test]
    fn test_diatonic_ionian_pattern() {
        let mode = ScaleMode::ionian();
        assert_eq!(mode.interval_pattern(), vec![0, 2, 4, 5, 7, 9, 11]);
        assert_eq!(mode.name(), "Ionian");
        assert_eq!(mode.short_name(), "Maj");
    }

    #[test]
    fn test_diatonic_dorian_pattern() {
        let mode = ScaleMode::dorian();
        assert_eq!(mode.interval_pattern(), vec![0, 2, 3, 5, 7, 9, 10]);
        assert_eq!(mode.name(), "Dorian");
        assert_eq!(mode.short_name(), "Dor");
    }

    #[test]
    fn test_diatonic_aeolian_pattern() {
        let mode = ScaleMode::aeolian();
        assert_eq!(mode.interval_pattern(), vec![0, 2, 3, 5, 7, 8, 10]);
        assert_eq!(mode.name(), "Aeolian");
        assert_eq!(mode.short_name(), "Min");
    }

    #[test]
    fn test_all_diatonic_modes() {
        let modes = vec![
            ScaleMode::ionian(),
            ScaleMode::dorian(),
            ScaleMode::phrygian(),
            ScaleMode::lydian(),
            ScaleMode::mixolydian(),
            ScaleMode::aeolian(),
            ScaleMode::locrian(),
        ];

        // Each mode should have 7 notes
        for mode in modes {
            assert_eq!(mode.interval_pattern().len(), 7);
            // First note should always be root (0)
            assert_eq!(mode.interval_pattern()[0], 0);
        }
    }

    // Harmonic Minor Mode Tests
    #[test]
    fn test_harmonic_minor_pattern() {
        let mode = ScaleMode::harmonic_minor();
        // Pattern: W H W W H 1.5 H
        assert_eq!(mode.interval_pattern(), vec![0, 2, 3, 5, 7, 8, 11]);
        assert_eq!(mode.name(), "Harmonic Minor");
        assert_eq!(mode.short_name(), "HMin");
    }

    #[test]
    fn test_phrygian_dominant_pattern() {
        let mode = ScaleMode::phrygian_dominant();
        // Phrygian Dominant (5th mode of harmonic minor)
        assert_eq!(mode.interval_pattern(), vec![0, 1, 4, 5, 7, 8, 10]);
        assert_eq!(mode.name(), "Phrygian Dominant");
        assert_eq!(mode.short_name(), "PhryDom");
    }

    #[test]
    fn test_all_harmonic_minor_modes() {
        let modes = vec![
            ScaleMode::harmonic_minor(),
            ScaleMode::locrian_natural_6(),
            ScaleMode::ionian_sharp_5(),
            ScaleMode::dorian_sharp_4(),
            ScaleMode::phrygian_dominant(),
            ScaleMode::lydian_sharp_2(),
            ScaleMode::super_locrian_double_flat_7(),
        ];

        for mode in modes {
            assert_eq!(mode.interval_pattern().len(), 7);
            assert_eq!(mode.interval_pattern()[0], 0);
        }
    }

    // Melodic Minor Mode Tests
    #[test]
    fn test_melodic_minor_pattern() {
        let mode = ScaleMode::melodic_minor();
        assert_eq!(mode.interval_pattern(), vec![0, 2, 3, 5, 7, 9, 11]);
        assert_eq!(mode.name(), "Melodic Minor");
        assert_eq!(mode.short_name(), "MMin");
    }

    #[test]
    fn test_lydian_dominant_pattern() {
        let mode = ScaleMode::lydian_dominant();
        assert_eq!(mode.interval_pattern(), vec![0, 2, 4, 6, 7, 9, 10]);
        assert_eq!(mode.name(), "Lydian Dominant");
        assert_eq!(mode.short_name(), "LydDom");
    }

    #[test]
    fn test_altered_scale_pattern() {
        let mode = ScaleMode::altered();
        assert_eq!(mode.interval_pattern(), vec![0, 1, 3, 4, 6, 8, 10]);
        assert_eq!(mode.name(), "Altered");
        assert_eq!(mode.short_name(), "Alt");
    }

    #[test]
    fn test_all_melodic_minor_modes() {
        let modes = vec![
            ScaleMode::melodic_minor(),
            ScaleMode::dorian_flat_2(),
            ScaleMode::lydian_augmented(),
            ScaleMode::lydian_dominant(),
            ScaleMode::mixolydian_flat_6(),
            ScaleMode::locrian_natural_2(),
            ScaleMode::altered(),
        ];

        for mode in modes {
            assert_eq!(mode.interval_pattern().len(), 7);
            assert_eq!(mode.interval_pattern()[0], 0);
        }
    }

    // Scale Family Tests
    #[test]
    fn test_scale_family_trait() {
        // Test that the trait's default rotation method works
        let diatonic_base = DiatonicFamily::base_pattern();
        assert_eq!(diatonic_base, vec![0, 2, 4, 5, 7, 9, 11]);

        // Test mode rotation
        let dorian_pattern = DiatonicFamily::pattern_for_mode(1);
        assert_eq!(dorian_pattern, vec![0, 2, 3, 5, 7, 9, 10]);
    }

    #[test]
    fn test_scale_mode_type_extraction() {
        // Test that we can get the scale type from a ScaleMode
        let ionian = ScaleMode::ionian();
        assert_eq!(ionian.scale_type(), ScaleType::Diatonic);
        assert_eq!(ionian.rotation(), 0);

        let phrygian_dom = ScaleMode::phrygian_dominant();
        assert_eq!(phrygian_dom.scale_type(), ScaleType::HarmonicMinor);
        assert_eq!(phrygian_dom.rotation(), 4);

        let altered = ScaleMode::altered();
        assert_eq!(altered.scale_type(), ScaleType::MelodicMinor);
        assert_eq!(altered.rotation(), 6);
    }

    #[test]
    fn test_family_specific_mode_enums() {
        // Test that each family has its own mode enum
        let diatonic_mode = DiatonicMode::Dorian;
        assert_eq!(diatonic_mode.name(), "Dorian");
        assert_eq!(diatonic_mode.rotation(), 1);

        let harmonic_mode = HarmonicMinorMode::PhrygianDominant;
        assert_eq!(harmonic_mode.name(), "Phrygian Dominant");
        assert_eq!(harmonic_mode.rotation(), 4);

        let melodic_mode = MelodicMinorMode::Altered;
        assert_eq!(melodic_mode.name(), "Altered");
        assert_eq!(melodic_mode.rotation(), 6);
    }
}
