//! Rig domain — complete instrument setups with scene-style variants.
//!
//! A [`Rig`] groups one or more Engines. [`RigScene`] selects which
//! engine variant to use for each engine, forming a top-level "scene".

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::engine::{EngineId, EngineSceneId};
use crate::fx_send::FxSend;
use crate::metadata::Metadata;
use crate::override_policy::{validate_overrides, OverridePolicyError, ScenePolicy};
use crate::overrides::Override;

// ─── IDs ────────────────────────────────────────────────────────

crate::impl_collection! {
    /// Identifies a Rig collection.
    collection_id: RigId,

    /// Identifies a specific Rig variant (scene).
    variant_id: RigSceneId,

    variant RigScene {
        id: RigSceneId,
        overrides: Override,
        default_named: |name| Self::new(RigSceneId::new(), name),
    }

    collection Rig {
        variant_type: RigScene,
        variants_field: variants,
        default_id_field: default_variant_id,
    }
}
// Legacy branded string id used by template internals.
crate::typed_string_id!(
    /// Deprecated in favor of [`RigType`].
    RigTypeId
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Facet, Default)]
#[repr(C)]
pub enum RigType {
    #[default]
    Guitar,
    Bass,
    Keys,
    Drums,
    DrumReplacement,
    Vocals,
}

impl RigType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Guitar => "guitar",
            Self::Bass => "bass",
            Self::Keys => "keys",
            Self::Drums => "drums",
            Self::DrumReplacement => "drum-replacement",
            Self::Vocals => "vocals",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "guitar" => Some(Self::Guitar),
            "bass" => Some(Self::Bass),
            "keys" => Some(Self::Keys),
            "drums" => Some(Self::Drums),
            "drum-replacement" => Some(Self::DrumReplacement),
            "vocals" => Some(Self::Vocals),
            _ => None,
        }
    }
}

impl From<&str> for RigType {
    fn from(value: &str) -> Self {
        Self::from_str(value).unwrap_or_default()
    }
}

impl From<String> for RigType {
    fn from(value: String) -> Self {
        Self::from(value.as_str())
    }
}

// ─── Engine selection ───────────────────────────────────────────

/// Which variant to use for a specific engine within a rig scene.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct EngineSelection {
    pub engine_id: EngineId,
    pub variant_id: EngineSceneId,
}

impl EngineSelection {
    pub fn new(engine_id: impl Into<EngineId>, variant_id: impl Into<EngineSceneId>) -> Self {
        Self {
            engine_id: engine_id.into(),
            variant_id: variant_id.into(),
        }
    }
}

// ─── RigScene ─────────────────────────────────────────────────

/// A scene-style variant for a Rig — selects engine variants and overrides.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct RigScene {
    pub id: RigSceneId,
    pub name: String,
    pub engine_selections: Vec<EngineSelection>,
    pub overrides: Vec<Override>,
    pub metadata: Metadata,
}

impl RigScene {
    pub fn new(id: impl Into<RigSceneId>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            engine_selections: Vec::new(),
            overrides: Vec::new(),
            metadata: Metadata::new(),
        }
    }

    #[must_use]
    pub fn with_engine(mut self, selection: EngineSelection) -> Self {
        self.engine_selections.push(selection);
        self
    }

    #[must_use]
    pub fn with_override(mut self, ov: Override) -> Self {
        self.overrides.push(ov);
        self
    }

    #[must_use]
    pub fn with_metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn validate_overrides(&self) -> Result<(), OverridePolicyError> {
        validate_overrides::<ScenePolicy>(&self.overrides)
    }

    /// Clone this scene with a new ID and name.
    pub fn duplicate(&self, new_id: impl Into<RigSceneId>, new_name: impl Into<String>) -> Self {
        let mut dup = self.clone();
        dup.id = new_id.into();
        dup.name = new_name.into();
        dup
    }
}

// ─── Rig ────────────────────────────────────────────────────────

/// A Rig collection — a complete instrument with engines and scene variants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Rig {
    pub id: RigId,
    pub name: String,
    pub rig_type: Option<RigType>,
    pub engine_ids: Vec<EngineId>,
    pub default_variant_id: RigSceneId,
    pub variants: Vec<RigScene>,
    /// FX sends owned by this rig (reverb, delay, etc.).
    #[serde(default)]
    pub fx_sends: Vec<FxSend>,
    /// DAW track reference for this rig's input track (GUID or name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_track_ref: Option<String>,
    /// Macro knob bank for rig-level control of engine/layer parameters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub macro_bank: Option<macromod::MacroBank>,
    /// Modulation routing for rig-level macro parameters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modulation: Option<macromod::ModulationRouteSet>,
    pub metadata: Metadata,
}

impl Rig {
    pub fn new(
        id: impl Into<RigId>,
        name: impl Into<String>,
        engine_ids: Vec<EngineId>,
        default_variant: RigScene,
    ) -> Self {
        let default_variant_id = default_variant.id.clone();
        Self {
            id: id.into(),
            name: name.into(),
            rig_type: None,
            engine_ids,
            default_variant_id,
            variants: vec![default_variant],
            fx_sends: Vec::new(),
            input_track_ref: None,
            macro_bank: None,
            modulation: None,
            metadata: Metadata::new(),
        }
    }

    pub fn add_variant(&mut self, variant: RigScene) {
        self.variants.push(variant);
    }

    pub fn variant_mut(&mut self, id: &RigSceneId) -> Option<&mut RigScene> {
        self.variants.iter_mut().find(|v| &v.id == id)
    }

    pub fn remove_variant(&mut self, id: &RigSceneId) -> Option<RigScene> {
        let pos = self.variants.iter().position(|v| &v.id == id)?;
        Some(self.variants.remove(pos))
    }

    pub fn default_variant(&self) -> Option<&RigScene> {
        self.variants
            .iter()
            .find(|v| v.id == self.default_variant_id)
    }

    pub fn variant(&self, id: &RigSceneId) -> Option<&RigScene> {
        self.variants.iter().find(|v| &v.id == id)
    }

    #[must_use]
    pub fn with_rig_type(mut self, rig_type: impl Into<RigType>) -> Self {
        self.rig_type = Some(rig_type.into());
        self
    }

    #[must_use]
    pub fn with_fx_send(mut self, send: FxSend) -> Self {
        self.fx_sends.push(send);
        self
    }

    #[must_use]
    pub fn with_input_track(mut self, track_ref: impl Into<String>) -> Self {
        self.input_track_ref = Some(track_ref.into());
        self
    }

    #[must_use]
    pub fn with_metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = metadata;
        self
    }
}

// Trait impls (Variant, DefaultVariant, Collection, HasMetadata) are generated
// by the `impl_collection!` macro invocation at the top of this file.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rig_creation() {
        let engine_id = EngineId::new();
        let variant = RigScene::new(RigSceneId::new(), "Default Scene").with_engine(
            EngineSelection::new(engine_id.clone(), EngineSceneId::new()),
        );

        let rig =
            Rig::new(RigId::new(), "Guitar Rig", vec![engine_id], variant).with_rig_type("guitar");

        assert_eq!(rig.name, "Guitar Rig");
        assert_eq!(rig.rig_type.unwrap().as_str(), "guitar");
        assert!(rig.default_variant().is_some());
    }
}
