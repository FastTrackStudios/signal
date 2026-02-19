//! Modulation routing types — LFO, envelope, expression, and MIDI CC sources.

use serde::{Deserialize, Serialize};

/// Waveform shape for an LFO modulation source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LfoWaveform {
    Sine,
    Triangle,
    Square,
    Sawtooth,
    SampleAndHold,
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
        Self::SampleAndHold,
    ];

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Sine => "Sine",
            Self::Triangle => "Triangle",
            Self::Square => "Square",
            Self::Sawtooth => "Sawtooth",
            Self::SampleAndHold => "S&H",
        }
    }
}

/// Configuration for an LFO modulation source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// Tempo sync division (e.g. "1/4", "1/8"). Only used when `tempo_sync` is true.
    pub sync_division: Option<String>,
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
        }
    }
}

/// Configuration for an envelope modulation source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvelopeConfig {
    /// Attack time in seconds.
    pub attack_s: f32,
    /// Decay time in seconds.
    pub decay_s: f32,
    /// Sustain level (0.0–1.0).
    pub sustain: f32,
    /// Release time in seconds.
    pub release_s: f32,
    /// Modulation depth (0.0–1.0).
    pub depth: f32,
}

impl Default for EnvelopeConfig {
    fn default() -> Self {
        Self {
            attack_s: 0.01,
            decay_s: 0.1,
            sustain: 0.7,
            release_s: 0.3,
            depth: 0.5,
        }
    }
}

/// Source of modulation signal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ModulationSource {
    /// Low-frequency oscillator.
    Lfo(LfoConfig),
    /// ADSR envelope follower.
    Envelope(EnvelopeConfig),
    /// MIDI CC input (CC number).
    MidiCc { cc_number: u8 },
    /// Expression pedal input.
    Expression,
}

impl ModulationSource {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Lfo(_) => "LFO",
            Self::Envelope(_) => "Envelope",
            Self::MidiCc { .. } => "MIDI CC",
            Self::Expression => "Expression",
        }
    }
}

/// Target of a modulation route — which parameter gets modulated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModulationTarget {
    /// Block ID within the module/rig.
    pub block_id: String,
    /// Parameter ID within the block.
    pub parameter_id: String,
}

/// A single modulation route connecting a source to a target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModulationRoute {
    /// Unique ID for this route.
    pub id: String,
    /// The modulation source.
    pub source: ModulationSource,
    /// The parameter being modulated.
    pub target: ModulationTarget,
    /// Modulation amount (-1.0 to 1.0, negative = inverted).
    pub amount: f32,
    /// Whether this route is active.
    pub enabled: bool,
}

impl ModulationRoute {
    pub fn new(
        id: impl Into<String>,
        source: ModulationSource,
        target: ModulationTarget,
        amount: f32,
    ) -> Self {
        Self {
            id: id.into(),
            source,
            target,
            amount: amount.clamp(-1.0, 1.0),
            enabled: true,
        }
    }
}

/// Collection of modulation routes for a rig/scene.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ModulationRouteSet {
    pub routes: Vec<ModulationRoute>,
}

impl ModulationRouteSet {
    pub fn new() -> Self {
        Self {
            routes: Vec::new(),
        }
    }

    pub fn add(&mut self, route: ModulationRoute) {
        self.routes.push(route);
    }

    pub fn remove(&mut self, id: &str) {
        self.routes.retain(|r| r.id != id);
    }

    /// All active routes targeting a specific parameter.
    pub fn routes_for_param(&self, block_id: &str, param_id: &str) -> Vec<&ModulationRoute> {
        self.routes
            .iter()
            .filter(|r| {
                r.enabled && r.target.block_id == block_id && r.target.parameter_id == param_id
            })
            .collect()
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
    }

    #[test]
    fn envelope_config_defaults() {
        let env = EnvelopeConfig::default();
        assert!(env.attack_s > 0.0);
        assert!(env.sustain > 0.0);
    }

    #[test]
    fn modulation_route_clamps_amount() {
        let route = ModulationRoute::new(
            "test",
            ModulationSource::Expression,
            ModulationTarget {
                block_id: "amp".into(),
                parameter_id: "gain".into(),
            },
            2.5, // should clamp to 1.0
        );
        assert_eq!(route.amount, 1.0);
    }

    #[test]
    fn route_set_find_by_param() {
        let mut set = ModulationRouteSet::new();
        set.add(ModulationRoute::new(
            "r1",
            ModulationSource::Lfo(LfoConfig::default()),
            ModulationTarget {
                block_id: "amp".into(),
                parameter_id: "gain".into(),
            },
            0.5,
        ));
        set.add(ModulationRoute::new(
            "r2",
            ModulationSource::Expression,
            ModulationTarget {
                block_id: "amp".into(),
                parameter_id: "tone".into(),
            },
            0.3,
        ));

        let gain_routes = set.routes_for_param("amp", "gain");
        assert_eq!(gain_routes.len(), 1);
        assert_eq!(gain_routes[0].id, "r1");
    }

    #[test]
    fn serde_round_trip() {
        let route = ModulationRoute::new(
            "test",
            ModulationSource::Lfo(LfoConfig::default()),
            ModulationTarget {
                block_id: "drive".into(),
                parameter_id: "level".into(),
            },
            -0.7,
        );
        let json = serde_json::to_string(&route).unwrap();
        let parsed: ModulationRoute = serde_json::from_str(&json).unwrap();
        assert_eq!(route, parsed);
    }
}
