//! Diatonic scale family
//!
//! The diatonic scale and its 7 modes (Ionian, Dorian, Phrygian, etc.)

use super::trait_module::ScaleFamily;
use facet::Facet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum DiatonicMode {
    Ionian, // Major
    Dorian,
    Phrygian,
    Lydian,
    Mixolydian,
    Aeolian, // Natural Minor
    Locrian,
}

impl DiatonicMode {
    pub fn rotation(&self) -> usize {
        match self {
            DiatonicMode::Ionian => 0,
            DiatonicMode::Dorian => 1,
            DiatonicMode::Phrygian => 2,
            DiatonicMode::Lydian => 3,
            DiatonicMode::Mixolydian => 4,
            DiatonicMode::Aeolian => 5,
            DiatonicMode::Locrian => 6,
        }
    }

    pub fn from_rotation(rotation: usize) -> Option<Self> {
        match rotation {
            0 => Some(DiatonicMode::Ionian),
            1 => Some(DiatonicMode::Dorian),
            2 => Some(DiatonicMode::Phrygian),
            3 => Some(DiatonicMode::Lydian),
            4 => Some(DiatonicMode::Mixolydian),
            5 => Some(DiatonicMode::Aeolian),
            6 => Some(DiatonicMode::Locrian),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        DiatonicFamily::mode_name(self.rotation())
    }

    pub fn short_name(&self) -> &'static str {
        DiatonicFamily::mode_short_name(self.rotation())
    }

    pub fn interval_pattern(&self) -> Vec<u8> {
        DiatonicFamily::pattern_for_mode(self.rotation())
    }
}

pub struct DiatonicFamily;

impl ScaleFamily for DiatonicFamily {
    type Mode = DiatonicMode;

    fn base_pattern() -> Vec<u8> {
        vec![0, 2, 4, 5, 7, 9, 11] // Ionian (Major)
    }

    fn mode_name(rotation: usize) -> &'static str {
        match rotation {
            0 => "Ionian",
            1 => "Dorian",
            2 => "Phrygian",
            3 => "Lydian",
            4 => "Mixolydian",
            5 => "Aeolian",
            6 => "Locrian",
            _ => "Unknown",
        }
    }

    fn mode_short_name(rotation: usize) -> &'static str {
        match rotation {
            0 => "Maj",
            1 => "Dor",
            2 => "Phry",
            3 => "Lyd",
            4 => "Mix",
            5 => "Min",
            6 => "Loc",
            _ => "?",
        }
    }
}
