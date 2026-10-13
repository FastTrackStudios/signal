//! Unified scale mode that combines all scale families

use super::diatonic::DiatonicMode;
use super::harmonic_minor::HarmonicMinorMode;
use super::melodic_minor::MelodicMinorMode;
use facet::Facet;

/// Type of musical scale family
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
pub enum ScaleType {
    Diatonic,
    HarmonicMinor,
    MelodicMinor,
}

impl ScaleType {
    /// Get the base interval pattern for this scale family
    pub fn base_pattern(&self) -> Vec<u8> {
        use super::diatonic::DiatonicFamily;
        use super::harmonic_minor::HarmonicMinorFamily;
        use super::melodic_minor::MelodicMinorFamily;
        use super::trait_module::ScaleFamily;

        match self {
            ScaleType::Diatonic => DiatonicFamily::base_pattern(),
            ScaleType::HarmonicMinor => HarmonicMinorFamily::base_pattern(),
            ScaleType::MelodicMinor => MelodicMinorFamily::base_pattern(),
        }
    }
}

/// Unified mode enum that can represent any mode from any scale family
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum ScaleMode {
    Diatonic(DiatonicMode),
    HarmonicMinor(HarmonicMinorMode),
    MelodicMinor(MelodicMinorMode),
}

impl ScaleMode {
    /// Get the scale type (family) of this mode
    pub fn scale_type(&self) -> ScaleType {
        match self {
            ScaleMode::Diatonic(_) => ScaleType::Diatonic,
            ScaleMode::HarmonicMinor(_) => ScaleType::HarmonicMinor,
            ScaleMode::MelodicMinor(_) => ScaleType::MelodicMinor,
        }
    }

    /// Get the rotation index within the family
    pub fn rotation(&self) -> usize {
        match self {
            ScaleMode::Diatonic(mode) => mode.rotation(),
            ScaleMode::HarmonicMinor(mode) => mode.rotation(),
            ScaleMode::MelodicMinor(mode) => mode.rotation(),
        }
    }

    /// Get the interval pattern for this mode
    pub fn interval_pattern(&self) -> Vec<u8> {
        match self {
            ScaleMode::Diatonic(mode) => mode.interval_pattern(),
            ScaleMode::HarmonicMinor(mode) => mode.interval_pattern(),
            ScaleMode::MelodicMinor(mode) => mode.interval_pattern(),
        }
    }

    /// Get the full name of this mode
    pub fn name(&self) -> &'static str {
        match self {
            ScaleMode::Diatonic(mode) => mode.name(),
            ScaleMode::HarmonicMinor(mode) => mode.name(),
            ScaleMode::MelodicMinor(mode) => mode.name(),
        }
    }

    /// Get a short name for this mode
    pub fn short_name(&self) -> &'static str {
        match self {
            ScaleMode::Diatonic(mode) => mode.short_name(),
            ScaleMode::HarmonicMinor(mode) => mode.short_name(),
            ScaleMode::MelodicMinor(mode) => mode.short_name(),
        }
    }

    // Convenience constructors for Diatonic modes
    pub fn ionian() -> Self {
        ScaleMode::Diatonic(DiatonicMode::Ionian)
    }
    pub fn dorian() -> Self {
        ScaleMode::Diatonic(DiatonicMode::Dorian)
    }
    pub fn phrygian() -> Self {
        ScaleMode::Diatonic(DiatonicMode::Phrygian)
    }
    pub fn lydian() -> Self {
        ScaleMode::Diatonic(DiatonicMode::Lydian)
    }
    pub fn mixolydian() -> Self {
        ScaleMode::Diatonic(DiatonicMode::Mixolydian)
    }
    pub fn aeolian() -> Self {
        ScaleMode::Diatonic(DiatonicMode::Aeolian)
    }
    pub fn locrian() -> Self {
        ScaleMode::Diatonic(DiatonicMode::Locrian)
    }

    // Convenience constructors for Harmonic Minor modes
    pub fn harmonic_minor() -> Self {
        ScaleMode::HarmonicMinor(HarmonicMinorMode::HarmonicMinor)
    }
    pub fn locrian_natural_6() -> Self {
        ScaleMode::HarmonicMinor(HarmonicMinorMode::LocrianNatural6)
    }
    pub fn ionian_sharp_5() -> Self {
        ScaleMode::HarmonicMinor(HarmonicMinorMode::IonianSharp5)
    }
    pub fn dorian_sharp_4() -> Self {
        ScaleMode::HarmonicMinor(HarmonicMinorMode::DorianSharp4)
    }
    pub fn phrygian_dominant() -> Self {
        ScaleMode::HarmonicMinor(HarmonicMinorMode::PhrygianDominant)
    }
    pub fn lydian_sharp_2() -> Self {
        ScaleMode::HarmonicMinor(HarmonicMinorMode::LydianSharp2)
    }
    pub fn super_locrian_double_flat_7() -> Self {
        ScaleMode::HarmonicMinor(HarmonicMinorMode::SuperLocrianDoubleFlatSeven)
    }

    // Convenience constructors for Melodic Minor modes
    pub fn melodic_minor() -> Self {
        ScaleMode::MelodicMinor(MelodicMinorMode::MelodicMinor)
    }
    pub fn dorian_flat_2() -> Self {
        ScaleMode::MelodicMinor(MelodicMinorMode::DorianFlat2)
    }
    pub fn lydian_augmented() -> Self {
        ScaleMode::MelodicMinor(MelodicMinorMode::LydianAugmented)
    }
    pub fn lydian_dominant() -> Self {
        ScaleMode::MelodicMinor(MelodicMinorMode::LydianDominant)
    }
    pub fn mixolydian_flat_6() -> Self {
        ScaleMode::MelodicMinor(MelodicMinorMode::MixolydianFlat6)
    }
    pub fn locrian_natural_2() -> Self {
        ScaleMode::MelodicMinor(MelodicMinorMode::LocrianNatural2)
    }
    pub fn altered() -> Self {
        ScaleMode::MelodicMinor(MelodicMinorMode::Altered)
    }
}
