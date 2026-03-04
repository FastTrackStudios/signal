//! Modulation sources — LFO, envelope, expression, MIDI CC, and macro.

pub mod envelope;
pub mod lfo;

use facet::Facet;
use serde::{Deserialize, Serialize};

pub use envelope::{EnvelopeConfig, EnvelopeMode};
pub use lfo::{LfoConfig, LfoWaveform, RetriggerMode, TempoDiv};

/// Source of modulation signal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum ModulationSource {
    /// Low-frequency oscillator.
    Lfo(LfoConfig),
    /// ADSR envelope follower.
    Envelope(EnvelopeConfig),
    /// MIDI CC input (CC number).
    MidiCc { cc_number: u8 },
    /// Expression pedal input.
    Expression,
    /// Macro knob reference — macros are modulation sources.
    Macro { knob_id: String },
}

impl ModulationSource {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Lfo(_) => "LFO",
            Self::Envelope(_) => "Envelope",
            Self::MidiCc { .. } => "MIDI CC",
            Self::Expression => "Expression",
            Self::Macro { .. } => "Macro",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macro_source_serde_round_trip() {
        let source = ModulationSource::Macro {
            knob_id: "drive".into(),
        };
        let json = serde_json::to_string(&source).unwrap();
        let parsed: ModulationSource = serde_json::from_str(&json).unwrap();
        assert_eq!(source, parsed);
    }

    #[test]
    fn display_names() {
        assert_eq!(
            ModulationSource::Lfo(LfoConfig::default()).display_name(),
            "LFO"
        );
        assert_eq!(
            ModulationSource::Envelope(EnvelopeConfig::default()).display_name(),
            "Envelope"
        );
        assert_eq!(
            ModulationSource::MidiCc { cc_number: 1 }.display_name(),
            "MIDI CC"
        );
        assert_eq!(ModulationSource::Expression.display_name(), "Expression");
        assert_eq!(
            ModulationSource::Macro {
                knob_id: "k1".into()
            }
            .display_name(),
            "Macro"
        );
    }
}
