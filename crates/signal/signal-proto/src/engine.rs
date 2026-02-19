//! Engine domain — containers for layers with scene-style variants.
//!
//! An [`Engine`] groups one or more Layers. [`EngineScene`] selects which
//! layer variant to use for each layer, forming a "scene" of the engine.

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::fx_send::FxSend;
use crate::layer::{LayerId, LayerSnapshotId};
use crate::metadata::Metadata;
use crate::override_policy::{validate_overrides, OverridePolicyError, ScenePolicy};
use crate::overrides::Override;
use crate::EngineType;

// ─── IDs ────────────────────────────────────────────────────────

crate::typed_uuid_id!(
    /// Identifies an Engine collection.
    EngineId
);
crate::typed_uuid_id!(
    /// Identifies a specific Engine variant (scene).
    EngineSceneId
);

// ─── Layer selection ────────────────────────────────────────────

/// Which variant to use for a specific layer within an engine scene.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct LayerSelection {
    pub layer_id: LayerId,
    pub variant_id: LayerSnapshotId,
}

impl LayerSelection {
    pub fn new(layer_id: impl Into<LayerId>, variant_id: impl Into<LayerSnapshotId>) -> Self {
        Self {
            layer_id: layer_id.into(),
            variant_id: variant_id.into(),
        }
    }
}

// ─── EngineScene ──────────────────────────────────────────────

/// A scene-style variant for an Engine — selects layer variants and overrides.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct EngineScene {
    pub id: EngineSceneId,
    pub name: String,
    pub layer_selections: Vec<LayerSelection>,
    pub overrides: Vec<Override>,
    pub metadata: Metadata,
}

impl EngineScene {
    pub fn new(id: impl Into<EngineSceneId>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            layer_selections: Vec::new(),
            overrides: Vec::new(),
            metadata: Metadata::new(),
        }
    }

    #[must_use]
    pub fn with_layer(mut self, selection: LayerSelection) -> Self {
        self.layer_selections.push(selection);
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
    pub fn duplicate(&self, new_id: impl Into<EngineSceneId>, new_name: impl Into<String>) -> Self {
        let mut dup = self.clone();
        dup.id = new_id.into();
        dup.name = new_name.into();
        dup
    }
}

// ─── Engine ─────────────────────────────────────────────────────

/// An Engine collection — groups layers and provides scene-style variants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Engine {
    pub id: EngineId,
    pub name: String,
    pub engine_type: EngineType,
    pub layer_ids: Vec<LayerId>,
    pub default_variant_id: EngineSceneId,
    pub variants: Vec<EngineScene>,
    /// FX sends owned by this engine (reverb, delay, etc.).
    #[serde(default)]
    pub fx_sends: Vec<FxSend>,
    /// DAW track reference for this engine's input track (GUID or name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_track_ref: Option<String>,
    pub metadata: Metadata,
}

impl Engine {
    pub fn new(
        id: impl Into<EngineId>,
        name: impl Into<String>,
        engine_type: EngineType,
        layer_ids: Vec<LayerId>,
        default_variant: EngineScene,
    ) -> Self {
        let default_variant_id = default_variant.id.clone();
        Self {
            id: id.into(),
            name: name.into(),
            engine_type,
            layer_ids,
            default_variant_id,
            variants: vec![default_variant],
            fx_sends: Vec::new(),
            input_track_ref: None,
            metadata: Metadata::new(),
        }
    }

    pub fn add_variant(&mut self, variant: EngineScene) {
        self.variants.push(variant);
    }

    pub fn variant_mut(&mut self, id: &EngineSceneId) -> Option<&mut EngineScene> {
        self.variants.iter_mut().find(|v| &v.id == id)
    }

    pub fn remove_variant(&mut self, id: &EngineSceneId) -> Option<EngineScene> {
        let pos = self.variants.iter().position(|v| &v.id == id)?;
        Some(self.variants.remove(pos))
    }

    pub fn default_variant(&self) -> Option<&EngineScene> {
        self.variants
            .iter()
            .find(|v| v.id == self.default_variant_id)
    }

    pub fn variant(&self, id: &EngineSceneId) -> Option<&EngineScene> {
        self.variants.iter().find(|v| &v.id == id)
    }

    pub fn is_layer_type_compatible(&self, layer_type: EngineType) -> bool {
        self.engine_type == layer_type
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

// ─── Trait impls ────────────────────────────────────────────────

impl crate::traits::Variant for EngineScene {
    type Id = EngineSceneId;
    type BaseRef = ();
    type Override = Override;
    fn id(&self) -> &EngineSceneId {
        &self.id
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }
    fn overrides(&self) -> Option<&[Self::Override]> {
        Some(&self.overrides)
    }
    fn overrides_mut(&mut self) -> Option<&mut Vec<Self::Override>> {
        Some(&mut self.overrides)
    }
}

impl crate::traits::DefaultVariant for EngineScene {
    fn default_named(name: impl Into<String>) -> Self {
        Self::new(EngineSceneId::new(), name)
    }
}

impl crate::traits::Collection for Engine {
    type Variant = EngineScene;

    fn variants(&self) -> &[EngineScene] {
        &self.variants
    }
    fn variants_mut(&mut self) -> &mut Vec<EngineScene> {
        &mut self.variants
    }
    fn default_variant_id(&self) -> &EngineSceneId {
        &self.default_variant_id
    }
    fn set_default_variant_id(&mut self, id: EngineSceneId) {
        self.default_variant_id = id;
    }
}

impl crate::traits::HasMetadata for EngineScene {
    fn metadata(&self) -> &Metadata {
        &self.metadata
    }
    fn metadata_mut(&mut self) -> &mut Metadata {
        &mut self.metadata
    }
}

impl crate::traits::HasMetadata for Engine {
    fn metadata(&self) -> &Metadata {
        &self.metadata
    }
    fn metadata_mut(&mut self) -> &mut Metadata {
        &mut self.metadata
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_creation() {
        let layer_id = LayerId::new();
        let variant = EngineScene::new(EngineSceneId::new(), "Default Scene").with_layer(
            LayerSelection::new(layer_id.clone(), LayerSnapshotId::new()),
        );

        let engine = Engine::new(
            EngineId::new(),
            "Guitar Engine",
            EngineType::Guitar,
            vec![layer_id],
            variant,
        );

        assert_eq!(engine.name, "Guitar Engine");
        assert_eq!(engine.engine_type, EngineType::Guitar);
        assert_eq!(engine.layer_ids.len(), 1);
        assert!(engine.default_variant().is_some());
    }
}
