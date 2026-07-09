//! Block implementation discriminator (orthogonal to [`BlockType`]).
//!
//! [`BlockType`](crate::block::BlockType) captures *what a block does
//! semantically* — Amp, Drive, Reverb, EQ. [`BlockKind`] captures *how
//! that role is realized* — Native (built-in DSP), Neural Amp Modeler
//! (a `.nam` model), a hosted CLAP/VST3 plugin, or a custom backend.
//!
//! The two axes are independent: an Amp block can be `Native` (classic
//! waveshaper) or `Nam` (neural network) or `HostedPlugin` (loaded from
//! a third-party amp sim); a Drive block can be `Nam` (a neural-modeled
//! overdrive pedal) just as easily. The runtime FX backend
//! (`signal-sampler::mixer::FxBackend`) matches one-to-one with this
//! enum's variants — `BlockKind` is the persisted form, `FxBackend` is
//! the audio-thread instance.
//!
//! Defaults to [`BlockKind::Native`] so older presets that predate this
//! field deserialize unchanged.

use facet::Facet;
use serde::{Deserialize, Serialize};

/// How a block's DSP is realized at runtime.
///
/// Serialized with `#[serde(tag = "kind", content = "data")]` so a missing
/// field deserializes as [`BlockKind::Native`] (back-compat) and the
/// non-Native variants get a clean tagged-union JSON layout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[serde(tag = "kind", content = "data")]
#[repr(C)]
pub enum BlockKind {
    /// Built-in DSP for this block type. Default.
    Native,
    /// Neural Amp Modeler — a `.nam` model file processed by
    /// `neural-amp-modeler` (FFI to NeuralAmpModelerCore). Works for any
    /// nonlinear/amplifier-shaped block (Amp, Drive, Cabinet, …).
    Nam(NamRef),
    /// Third-party CLAP / VST3 plugin loaded from disk.
    HostedPlugin(HostedPluginRef),
    /// Caller-defined backend identified by a string id; lookup happens
    /// in a host-supplied registry. Lets plugin authors add their own
    /// kinds without growing this enum.
    Custom(CustomRef),
}

impl Default for BlockKind {
    fn default() -> Self {
        Self::Native
    }
}

impl BlockKind {
    /// Short identifier for the variant — used in UI tags and log lines.
    pub const fn tag(&self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Nam(_) => "nam",
            Self::HostedPlugin(_) => "plugin",
            Self::Custom(_) => "custom",
        }
    }
}

/// Reference to a `.nam` model file. `model_id` is an optional stable id
/// (URL or hash) for content-addressed lookups; absent means "use the
/// path as the id".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct NamRef {
    pub model_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
}

/// Reference to a hosted plugin slot. `format` matches
/// `daw::plugin::PluginFormat`'s variants by name (Clap / Vst3 / Lv2)
/// and is stored as a string so a preset survives format additions
/// without a schema bump.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct HostedPluginRef {
    pub format: String,
    pub path: String,
    /// Optional per-plugin state blob (CLAP / VST3 state chunk) saved by
    /// the host's `save_state`. Base64 or raw — host decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_b64: Option<String>,
}

/// Caller-defined backend reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct CustomRef {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_native() {
        assert!(matches!(BlockKind::default(), BlockKind::Native));
        assert_eq!(BlockKind::default().tag(), "native");
    }

    #[test]
    fn json_round_trips_through_nam() {
        let k = BlockKind::Nam(NamRef {
            model_path: "/models/dumble.nam".into(),
            model_id: Some("sha256:abcd".into()),
        });
        let j = serde_json::to_string(&k).unwrap();
        let back: BlockKind = serde_json::from_str(&j).unwrap();
        assert_eq!(k, back);
    }

    #[test]
    fn json_round_trips_through_hosted() {
        let k = BlockKind::HostedPlugin(HostedPluginRef {
            format: "Clap".into(),
            path: "/usr/lib/clap/NAMVoyager.clap".into(),
            state_b64: None,
        });
        let j = serde_json::to_string(&k).unwrap();
        let back: BlockKind = serde_json::from_str(&j).unwrap();
        assert_eq!(k, back);
    }
}
