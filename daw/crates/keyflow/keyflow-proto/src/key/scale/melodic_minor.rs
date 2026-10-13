//! Melodic Minor scale family
//!
//! The melodic minor scale and its 7 modes (including Altered scale)

use super::trait_module::ScaleFamily;
use facet::Facet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum MelodicMinorMode {
    MelodicMinor,
    DorianFlat2,
    LydianAugmented,
    LydianDominant,
    MixolydianFlat6,
    LocrianNatural2,
    Altered,
}

impl MelodicMinorMode {
    pub fn rotation(&self) -> usize {
        match self {
            MelodicMinorMode::MelodicMinor => 0,
            MelodicMinorMode::DorianFlat2 => 1,
            MelodicMinorMode::LydianAugmented => 2,
            MelodicMinorMode::LydianDominant => 3,
            MelodicMinorMode::MixolydianFlat6 => 4,
            MelodicMinorMode::LocrianNatural2 => 5,
            MelodicMinorMode::Altered => 6,
        }
    }

    pub fn from_rotation(rotation: usize) -> Option<Self> {
        match rotation {
            0 => Some(MelodicMinorMode::MelodicMinor),
            1 => Some(MelodicMinorMode::DorianFlat2),
            2 => Some(MelodicMinorMode::LydianAugmented),
            3 => Some(MelodicMinorMode::LydianDominant),
            4 => Some(MelodicMinorMode::MixolydianFlat6),
            5 => Some(MelodicMinorMode::LocrianNatural2),
            6 => Some(MelodicMinorMode::Altered),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        MelodicMinorFamily::mode_name(self.rotation())
    }

    pub fn short_name(&self) -> &'static str {
        MelodicMinorFamily::mode_short_name(self.rotation())
    }

    pub fn interval_pattern(&self) -> Vec<u8> {
        MelodicMinorFamily::pattern_for_mode(self.rotation())
    }
}

pub struct MelodicMinorFamily;

impl ScaleFamily for MelodicMinorFamily {
    type Mode = MelodicMinorMode;

    fn base_pattern() -> Vec<u8> {
        vec![0, 2, 3, 5, 7, 9, 11] // Melodic Minor (ascending)
    }

    fn mode_name(rotation: usize) -> &'static str {
        match rotation {
            0 => "Melodic Minor",
            1 => "Dorian ♭2",
            2 => "Lydian Augmented",
            3 => "Lydian Dominant",
            4 => "Mixolydian ♭6",
            5 => "Locrian ♮2",
            6 => "Altered",
            _ => "Unknown",
        }
    }

    fn mode_short_name(rotation: usize) -> &'static str {
        match rotation {
            0 => "MMin",
            1 => "Dor♭2",
            2 => "Lyd+",
            3 => "LydDom",
            4 => "Mix♭6",
            5 => "Loc♮2",
            6 => "Alt",
            _ => "?",
        }
    }
}
