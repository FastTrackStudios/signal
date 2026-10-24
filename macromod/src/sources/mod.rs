//! Modulation sources — LFO, envelope, follower, random, expression, MIDI CC, and macro.

pub mod envelope;
pub mod follower;
pub mod lfo;
pub mod pattern;
pub mod random;

use facet::Facet;
use serde::{Deserialize, Serialize};

pub use envelope::{EnvelopeConfig, EnvelopeMode};
pub use follower::{FollowerConfig, FollowerInput};
pub use lfo::{LfoConfig, LfoWaveform, RetriggerMode, TempoDiv};
pub use pattern::{PatternConfig, PatternPoint};
pub use random::RandomConfig;

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
    /// Envelope follower — tracks audio dynamics.
    Follower(FollowerConfig),
    /// Random — procedural random modulation.
    Random(RandomConfig),
    /// Drawn multi-segment pattern (ShaperBox-style MSEG).
    Pattern(PatternConfig),
}

impl ModulationSource {
    #[must_use]
    pub const fn display_name(&self) -> &'static str {
        match self {
            Self::Lfo(_) => "LFO",
            Self::Envelope(_) => "Envelope",
            Self::MidiCc { .. } => "MIDI CC",
            Self::Expression => "Expression",
            Self::Macro { .. } => "Macro",
            Self::Follower(_) => "Follower",
            Self::Random(_) => "Random",
            Self::Pattern(_) => "Pattern",
        }
    }
}
