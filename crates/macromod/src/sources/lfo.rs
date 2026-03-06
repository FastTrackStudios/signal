//! LFO modulation source configuration.

use facet::Facet;
use serde::{Deserialize, Serialize};

/// Waveform shape for an LFO modulation source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum LfoWaveform {
    Sine,
    Triangle,
    Square,
    Sawtooth,
    /// Reverse sawtooth (ramp down).
    InverseSawtooth,
    SampleAndHold,
    /// Step sequencer pattern (inspired by surge-lfo).
    StepSequence,
}

impl Default for LfoWaveform {
    fn default() -> Self {
        Self::Sine
    }
}

impl LfoWaveform {
    pub const ALL: &'static [LfoWaveform] = &[
        Self::Sine,
        Self::Triangle,
        Self::Square,
        Self::Sawtooth,
        Self::InverseSawtooth,
        Self::SampleAndHold,
        Self::StepSequence,
    ];

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Sine => "Sine",
            Self::Triangle => "Triangle",
            Self::Square => "Square",
            Self::Sawtooth => "Sawtooth",
            Self::InverseSawtooth => "Inv Saw",
            Self::SampleAndHold => "S&H",
            Self::StepSequence => "Step Seq",
        }
    }
}

/// Tempo-sync division for rate-locked LFOs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum TempoDiv {
    Whole,
    Half,
    Quarter,
    Eighth,
    Sixteenth,
    DottedHalf,
    DottedQuarter,
    DottedEighth,
    TripletHalf,
    TripletQuarter,
    TripletEighth,
}

impl Default for TempoDiv {
    fn default() -> Self {
        Self::Quarter
    }
}

impl TempoDiv {
    /// Duration as a fraction of a whole note.
    pub const fn beats(self) -> f32 {
        match self {
            Self::Whole => 4.0,
            Self::Half => 2.0,
            Self::Quarter => 1.0,
            Self::Eighth => 0.5,
            Self::Sixteenth => 0.25,
            Self::DottedHalf => 3.0,
            Self::DottedQuarter => 1.5,
            Self::DottedEighth => 0.75,
            Self::TripletHalf => 4.0 / 3.0,
            Self::TripletQuarter => 2.0 / 3.0,
            Self::TripletEighth => 1.0 / 3.0,
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Whole => "1/1",
            Self::Half => "1/2",
            Self::Quarter => "1/4",
            Self::Eighth => "1/8",
            Self::Sixteenth => "1/16",
            Self::DottedHalf => "1/2.",
            Self::DottedQuarter => "1/4.",
            Self::DottedEighth => "1/8.",
            Self::TripletHalf => "1/2T",
            Self::TripletQuarter => "1/4T",
            Self::TripletEighth => "1/8T",
        }
    }
}

/// Whether an LFO resets phase on note trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum RetriggerMode {
    /// LFO runs freely, never resets.
    #[default]
    Free,
    /// Resets phase on each note-on.
    NoteOn,
}

/// Configuration for an LFO modulation source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct LfoConfig {
    pub waveform: LfoWaveform,
    /// Rate in Hz (0.01–20.0 typical).
    pub rate_hz: f32,
    /// Modulation depth (0.0–1.0).
    pub depth: f32,
    /// Phase offset in degrees (0–360).
    pub phase_offset: f32,
    /// Whether to sync to tempo (BPM-based rate).
    pub tempo_sync: bool,
    /// Tempo sync division. Only used when `tempo_sync` is true.
    #[serde(default)]
    pub sync_division: Option<TempoDiv>,
    /// Retrigger behavior.
    #[serde(default)]
    pub retrigger: RetriggerMode,
    /// Pulse width for square wave (0.0–1.0, 0.5 = symmetric).
    #[serde(default = "default_pulse_width")]
    pub pulse_width: f32,
    /// Step values for the `StepSequence` waveform (each 0.0–1.0).
    /// Ignored for other waveforms.
    #[serde(default)]
    pub steps: Option<Vec<f32>>,
}

fn default_pulse_width() -> f32 {
    0.5
}

impl Default for LfoConfig {
    fn default() -> Self {
        Self {
            waveform: LfoWaveform::Sine,
            rate_hz: 1.0,
            depth: 0.5,
            phase_offset: 0.0,
            tempo_sync: false,
            sync_division: None,
            retrigger: RetriggerMode::Free,
            pulse_width: 0.5,
            steps: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lfo_config_defaults() {
        let lfo = LfoConfig::default();
        assert_eq!(lfo.waveform, LfoWaveform::Sine);
        assert_eq!(lfo.rate_hz, 1.0);
        assert!(!lfo.tempo_sync);
        assert_eq!(lfo.retrigger, RetriggerMode::Free);
        assert_eq!(lfo.pulse_width, 0.5);
    }

    #[test]
    fn tempo_div_beats() {
        assert_eq!(TempoDiv::Quarter.beats(), 1.0);
        assert_eq!(TempoDiv::Eighth.beats(), 0.5);
        assert_eq!(TempoDiv::DottedQuarter.beats(), 1.5);
    }

    #[test]
    fn serde_round_trip() {
        let lfo = LfoConfig {
            waveform: LfoWaveform::InverseSawtooth,
            tempo_sync: true,
            sync_division: Some(TempoDiv::DottedEighth),
            retrigger: RetriggerMode::NoteOn,
            ..Default::default()
        };
        let json = serde_json::to_string(&lfo).unwrap();
        let parsed: LfoConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(lfo, parsed);
    }

    #[test]
    fn backward_compat_no_retrigger_field() {
        // Old JSON without retrigger/pulse_width fields should still deserialize
        let json = r#"{"waveform":"Sine","rate_hz":1.0,"depth":0.5,"phase_offset":0.0,"tempo_sync":false,"sync_division":null}"#;
        let parsed: LfoConfig = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.retrigger, RetriggerMode::Free);
        assert_eq!(parsed.pulse_width, 0.5);
    }
}
