//! FX Send domain types — parallel effect buses at any level of the hierarchy.
//!
//! An [`FxSend`] represents a single send effect (reverb, delay, chorus, etc.)
//! hosted on a dedicated track in the DAW. Any level of the hierarchy —
//! Layer, Engine, Rig, or Rack — can own FX sends.
//!
//! An [`FxSendBus`] groups related sends. At the Rack level, buses have
//! named sub-categories (e.g. "AUX" for character effects, "TIME" for
//! time-based effects).

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::block::BlockType;

// ─── IDs ────────────────────────────────────────────────────────

crate::typed_uuid_id!(
    /// Identifies an individual FX send.
    FxSendId
);

crate::typed_uuid_id!(
    /// Identifies a group of FX sends (a bus).
    FxSendBusId
);

// ─── FxSendCategory ─────────────────────────────────────────────

/// Semantic category for an FX send effect.
///
/// Used for UI grouping, icon selection, and default routing templates.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum FxSendCategory {
    #[default]
    Reverb,
    Delay,
    Chorus,
    Pitch,
    Vocoder,
    Custom(String),
}

impl core::str::FromStr for FxSendCategory {
    type Err = core::convert::Infallible;

    /// Never fails — unknown names land in `Custom`.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(match value {
            "reverb" => Self::Reverb,
            "delay" => Self::Delay,
            "chorus" => Self::Chorus,
            "pitch" => Self::Pitch,
            "vocoder" => Self::Vocoder,
            other => Self::Custom(other.to_string()),
        })
    }
}

impl FxSendCategory {
    #[must_use]
    pub const fn as_str(&self) -> &str {
        match self {
            Self::Reverb => "reverb",
            Self::Delay => "delay",
            Self::Chorus => "chorus",
            Self::Pitch => "pitch",
            Self::Vocoder => "vocoder",
            Self::Custom(s) => s.as_str(),
        }
    }

    /// Infer category from a track/FX name by looking for keywords.
    #[must_use]
    pub fn infer_from_name(name: &str) -> Self {
        let lower = name.to_lowercase();
        if lower.contains("reverb") || lower.contains("verb") {
            Self::Reverb
        } else if lower.contains("delay") || lower.contains("slap") {
            Self::Delay
        } else if lower.contains("chorus") {
            Self::Chorus
        } else if lower.contains("pitch") || lower.contains("octave") {
            Self::Pitch
        } else if lower.contains("vocoder") {
            Self::Vocoder
        } else {
            Self::Custom(name.to_string())
        }
    }
}

// ─── FxSend ─────────────────────────────────────────────────────

/// A single send effect hosted on a dedicated DAW track.
///
/// Examples: "Reverb: Reaverb - sweetverbo", "Delay: `ReaDelay` - Vocal Fattener"
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct FxSend {
    pub id: FxSendId,
    pub name: String,
    pub category: FxSendCategory,
    pub block_type: BlockType,
    pub enabled: bool,
    /// Send level (0.0 = off, 1.0 = unity).
    pub mix: f32,
    /// DAW track reference (GUID or name) for binding to the live track.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track_ref: Option<String>,
}

impl FxSend {
    pub fn new(
        id: impl Into<FxSendId>,
        name: impl Into<String>,
        category: FxSendCategory,
        block_type: BlockType,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            category,
            block_type,
            enabled: true,
            mix: 1.0,
            track_ref: None,
        }
    }

    #[must_use]
    pub const fn with_mix(mut self, mix: f32) -> Self {
        self.mix = mix.clamp(0.0, 1.0);
        self
    }

    #[must_use]
    pub fn with_track_ref(mut self, track_ref: impl Into<String>) -> Self {
        self.track_ref = Some(track_ref.into());
        self
    }

    #[must_use]
    pub const fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

// ─── FxSendBus ──────────────────────────────────────────────────

/// A named group of FX sends.
///
/// At the Engine/Rig level this is typically a flat list (one bus).
/// At the Rack level, multiple buses represent sub-categories
/// like "AUX" (character effects) and "TIME" (time-based effects).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct FxSendBus {
    pub id: FxSendBusId,
    pub name: String,
    pub sends: Vec<FxSend>,
    /// Sub-category label for rack-level buses (e.g. "AUX", "TIME").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_category: Option<String>,
}

impl FxSendBus {
    pub fn new(id: impl Into<FxSendBusId>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            sends: Vec::new(),
            sub_category: None,
        }
    }

    #[must_use]
    pub fn with_send(mut self, send: FxSend) -> Self {
        self.sends.push(send);
        self
    }

    #[must_use]
    pub fn with_sub_category(mut self, category: impl Into<String>) -> Self {
        self.sub_category = Some(category.into());
        self
    }
}

// ─── Tests ──────────────────────────────────────────────────────
