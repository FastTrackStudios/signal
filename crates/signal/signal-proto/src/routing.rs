//! Signal routing and audio source domain types.
//!
//! Defines how audio flows between blocks within a module, and where
//! audio originates (hardware inputs, virtual instruments, etc.).

use facet::Facet;
use serde::{Deserialize, Serialize};

/// Audio source — where signal originates before entering the processing chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum AudioSource {
    /// Hardware audio input (e.g. guitar DI, microphone).
    HardwareInput {
        /// DAW input channel index.
        channel: u32,
        /// Human-readable label (e.g. "Guitar In", "Mic 1").
        label: String,
    },
    /// Virtual instrument plugin output.
    VirtualInstrument {
        /// Plugin identifier (name or GUID).
        plugin_id: String,
        /// Output bus index within the plugin.
        output_bus: u32,
    },
    /// Audio from another rack/rig (inter-rack routing).
    RackSend {
        /// Source rack ID.
        rack_id: String,
        /// Source rig ID within the rack.
        rig_id: String,
    },
    /// No audio source (bypass/silence).
    None,
}

impl Default for AudioSource {
    fn default() -> Self {
        Self::None
    }
}

impl AudioSource {
    pub fn display_name(&self) -> &str {
        match self {
            Self::HardwareInput { label, .. } => label.as_str(),
            Self::VirtualInstrument { plugin_id, .. } => plugin_id.as_str(),
            Self::RackSend { rack_id, .. } => rack_id.as_str(),
            Self::None => "None",
        }
    }
}

/// Audio destination — where processed signal goes after the chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum AudioDestination {
    /// Hardware output (e.g. main L/R, headphone out).
    HardwareOutput {
        /// DAW output channel index.
        channel: u32,
        label: String,
    },
    /// Send to another rack/rig.
    RackReceive {
        rack_id: String,
        rig_id: String,
    },
    /// No destination (muted).
    None,
}

impl Default for AudioDestination {
    fn default() -> Self {
        Self::None
    }
}

/// Configuration for a signal routing point within a module chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct RouteConfig {
    /// Source of audio for this route.
    pub source: AudioSource,
    /// Destination for processed audio.
    pub destination: AudioDestination,
    /// Gain adjustment in dB (-inf to +12 typical).
    pub gain_db: f32,
    /// Stereo pan (-1.0 = full left, 0.0 = center, 1.0 = full right).
    pub pan: f32,
    /// Whether this route is active.
    pub enabled: bool,
}

impl Default for RouteConfig {
    fn default() -> Self {
        Self {
            source: AudioSource::default(),
            destination: AudioDestination::default(),
            gain_db: 0.0,
            pan: 0.0,
            enabled: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_source_display_names() {
        let hw = AudioSource::HardwareInput {
            channel: 0,
            label: "Guitar In".into(),
        };
        assert_eq!(hw.display_name(), "Guitar In");

        let none = AudioSource::None;
        assert_eq!(none.display_name(), "None");
    }

    #[test]
    fn route_config_defaults() {
        let cfg = RouteConfig::default();
        assert_eq!(cfg.gain_db, 0.0);
        assert_eq!(cfg.pan, 0.0);
        assert!(cfg.enabled);
    }

    #[test]
    fn serde_round_trip() {
        let cfg = RouteConfig {
            source: AudioSource::HardwareInput {
                channel: 1,
                label: "Mic 1".into(),
            },
            destination: AudioDestination::HardwareOutput {
                channel: 0,
                label: "Main L/R".into(),
            },
            gain_db: -3.0,
            pan: 0.5,
            enabled: true,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: RouteConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg, parsed);
    }
}
