//! Harmonic Minor scale family
//!
//! The harmonic minor scale and its 7 modes

use super::trait_module::ScaleFamily;
use facet::Facet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum HarmonicMinorMode {
    HarmonicMinor,
    LocrianNatural6,
    IonianSharp5,
    DorianSharp4,
    PhrygianDominant,
    LydianSharp2,
    SuperLocrianDoubleFlatSeven,
}

impl HarmonicMinorMode {
    pub fn rotation(&self) -> usize {
        match self {
            HarmonicMinorMode::HarmonicMinor => 0,
            HarmonicMinorMode::LocrianNatural6 => 1,
            HarmonicMinorMode::IonianSharp5 => 2,
            HarmonicMinorMode::DorianSharp4 => 3,
            HarmonicMinorMode::PhrygianDominant => 4,
            HarmonicMinorMode::LydianSharp2 => 5,
            HarmonicMinorMode::SuperLocrianDoubleFlatSeven => 6,
        }
    }

    pub fn from_rotation(rotation: usize) -> Option<Self> {
        match rotation {
            0 => Some(HarmonicMinorMode::HarmonicMinor),
            1 => Some(HarmonicMinorMode::LocrianNatural6),
            2 => Some(HarmonicMinorMode::IonianSharp5),
            3 => Some(HarmonicMinorMode::DorianSharp4),
            4 => Some(HarmonicMinorMode::PhrygianDominant),
            5 => Some(HarmonicMinorMode::LydianSharp2),
            6 => Some(HarmonicMinorMode::SuperLocrianDoubleFlatSeven),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        HarmonicMinorFamily::mode_name(self.rotation())
    }

    pub fn short_name(&self) -> &'static str {
        HarmonicMinorFamily::mode_short_name(self.rotation())
    }

    pub fn interval_pattern(&self) -> Vec<u8> {
        HarmonicMinorFamily::pattern_for_mode(self.rotation())
    }
}

pub struct HarmonicMinorFamily;

impl ScaleFamily for HarmonicMinorFamily {
    type Mode = HarmonicMinorMode;

    fn base_pattern() -> Vec<u8> {
        // Harmonic Minor: W H W W H 1.5 H (where 1.5 = 3 semitones)
        // C D Eb F G Ab B for C Harmonic Minor
        vec![0, 2, 3, 5, 7, 8, 11]
    }

    fn mode_name(rotation: usize) -> &'static str {
        match rotation {
            0 => "Harmonic Minor",
            1 => "Locrian ♮6",
            2 => "Ionian ♯5",
            3 => "Dorian ♯4",
            4 => "Phrygian Dominant",
            5 => "Lydian ♯2",
            6 => "Super Locrian ♭♭7",
            _ => "Unknown",
        }
    }

    fn mode_short_name(rotation: usize) -> &'static str {
        match rotation {
            0 => "HMin",
            1 => "Loc♮6",
            2 => "Ion♯5",
            3 => "Dor♯4",
            4 => "PhryDom",
            5 => "Lyd♯2",
            6 => "SLoc♭♭7",
            _ => "?",
        }
    }
}
