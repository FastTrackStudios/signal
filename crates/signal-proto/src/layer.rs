//! Layer domain — processing lanes that combine modules.
//!
//! A [`Layer`] groups module references and standalone block references
//! into a processing lane. Layers live inside Engines.
//!
//! [`LayerSnapshot`] captures a specific configuration of a Layer,
//! selecting which module/block variants to use plus optional overrides.

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::fx_send::FxSend;
use crate::metadata::Metadata;
use crate::override_policy::{validate_overrides, OverridePolicyError, SnapshotPolicy};
use crate::overrides::Override;
use crate::{EngineType, ModulePresetId, ModuleSnapshotId, PresetId, SnapshotId};

// ─── IDs ────────────────────────────────────────────────────────

crate::typed_uuid_id!(
    /// Identifies a Layer collection.
    LayerId
);
crate::typed_uuid_id!(
    /// Identifies a specific Layer variant.
    LayerSnapshotId
);

// ─── Module reference ───────────────────────────────────────────

/// A reference to a specific module variant within a layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct ModuleRef {
    /// Which module collection to pull from.
    pub collection_id: ModulePresetId,
    /// Which variant within that collection. `None` = default variant.
    pub variant_id: Option<ModuleSnapshotId>,
}

impl ModuleRef {
    pub fn new(collection_id: impl Into<ModulePresetId>) -> Self {
        Self {
            collection_id: collection_id.into(),
            variant_id: None,
        }
    }

    #[must_use]
    pub fn with_variant(mut self, variant_id: impl Into<ModuleSnapshotId>) -> Self {
        self.variant_id = Some(variant_id.into());
        self
    }
}

// ─── Block reference ────────────────────────────────────────────

/// A reference to a specific standalone block variant within a layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct BlockRef {
    /// Which block collection to pull from.
    pub collection_id: PresetId,
    /// Which variant within that collection. `None` = default variant.
    pub variant_id: Option<SnapshotId>,
}

impl BlockRef {
    pub fn new(collection_id: impl Into<PresetId>) -> Self {
        Self {
            collection_id: collection_id.into(),
            variant_id: None,
        }
    }

    #[must_use]
    pub fn with_variant(mut self, variant_id: impl Into<SnapshotId>) -> Self {
        self.variant_id = Some(variant_id.into());
        self
    }
}

// ─── Layer reference ────────────────────────────────────────────

/// A reference to another layer preset/variant, enabling
/// "preset-as-layer" composition for simple and complex sounds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct LayerRef {
    /// Which layer collection to pull from.
    pub collection_id: LayerId,
    /// Which variant within that layer. `None` = default variant.
    pub variant_id: Option<LayerSnapshotId>,
}

impl LayerRef {
    pub fn new(collection_id: impl Into<LayerId>) -> Self {
        Self {
            collection_id: collection_id.into(),
            variant_id: None,
        }
    }

    #[must_use]
    pub fn with_variant(mut self, variant_id: impl Into<LayerSnapshotId>) -> Self {
        self.variant_id = Some(variant_id.into());
        self
    }
}

// ─── Plugin reference ────────────────────────────────────────────

/// A reference to a plugin block definition embedded inline in a layer.
///
/// Unlike `ModuleRef` and `BlockRef` which point to database entities,
/// a `PluginRef` carries the full `PluginBlockDef` inline. This keeps
/// plugin block definitions lightweight and self-contained.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct PluginRef {
    /// The plugin block definition (embedded, not a DB reference).
    pub def: crate::plugin_block::PluginBlockDef,
}

impl PluginRef {
    pub fn new(def: crate::plugin_block::PluginBlockDef) -> Self {
        Self { def }
    }
}

// ─── LayerSnapshot ───────────────────────────────────────────────

/// A specific configuration of a Layer — which modules and blocks to use.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct LayerSnapshot {
    pub id: LayerSnapshotId,
    pub name: String,
    pub layer_refs: Vec<LayerRef>,
    pub module_refs: Vec<ModuleRef>,
    pub block_refs: Vec<BlockRef>,
    #[serde(default)]
    pub plugin_refs: Vec<PluginRef>,
    pub overrides: Vec<Override>,
    pub enabled: bool,
    pub metadata: Metadata,
}

impl LayerSnapshot {
    pub fn new(id: impl Into<LayerSnapshotId>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            layer_refs: Vec::new(),
            module_refs: Vec::new(),
            block_refs: Vec::new(),
            plugin_refs: Vec::new(),
            overrides: Vec::new(),
            enabled: true,
            metadata: Metadata::new(),
        }
    }

    #[must_use]
    pub fn with_layer(mut self, layer_ref: LayerRef) -> Self {
        self.layer_refs.push(layer_ref);
        self
    }

    #[must_use]
    pub fn with_module(mut self, module_ref: ModuleRef) -> Self {
        self.module_refs.push(module_ref);
        self
    }

    #[must_use]
    pub fn with_block(mut self, block_ref: BlockRef) -> Self {
        self.block_refs.push(block_ref);
        self
    }

    #[must_use]
    pub fn with_plugin(mut self, plugin_ref: PluginRef) -> Self {
        self.plugin_refs.push(plugin_ref);
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
        validate_overrides::<SnapshotPolicy>(&self.overrides)
    }

    /// Clone this snapshot with a new ID and name.
    pub fn duplicate(
        &self,
        new_id: impl Into<LayerSnapshotId>,
        new_name: impl Into<String>,
    ) -> Self {
        let mut dup = self.clone();
        dup.id = new_id.into();
        dup.name = new_name.into();
        dup
    }
}

// ─── Layer ──────────────────────────────────────────────────────

/// A Layer collection — groups variants of a processing lane.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Layer {
    pub id: LayerId,
    pub name: String,
    pub engine_type: EngineType,
    pub default_variant_id: LayerSnapshotId,
    pub variants: Vec<LayerSnapshot>,
    /// FX sends owned by this layer (optional — most layers don't have these).
    #[serde(default)]
    pub fx_sends: Vec<FxSend>,
    /// Macro knob bank aggregating block-level macros across modules in this layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub macro_bank: Option<macromod::MacroBank>,
    /// Modulation routing for layer-level macro parameters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modulation: Option<macromod::ModulationRouteSet>,
    pub metadata: Metadata,
}

impl Layer {
    pub fn new(
        id: impl Into<LayerId>,
        name: impl Into<String>,
        engine_type: EngineType,
        default_variant: LayerSnapshot,
    ) -> Self {
        let default_variant_id = default_variant.id.clone();
        Self {
            id: id.into(),
            name: name.into(),
            engine_type,
            default_variant_id,
            variants: vec![default_variant],
            fx_sends: Vec::new(),
            macro_bank: None,
            modulation: None,
            metadata: Metadata::new(),
        }
    }

    pub fn add_variant(&mut self, variant: LayerSnapshot) {
        self.variants.push(variant);
    }

    pub fn variant_mut(&mut self, id: &LayerSnapshotId) -> Option<&mut LayerSnapshot> {
        self.variants.iter_mut().find(|v| &v.id == id)
    }

    pub fn remove_variant(&mut self, id: &LayerSnapshotId) -> Option<LayerSnapshot> {
        let pos = self.variants.iter().position(|v| &v.id == id)?;
        Some(self.variants.remove(pos))
    }

    pub fn default_variant(&self) -> Option<&LayerSnapshot> {
        self.variants
            .iter()
            .find(|v| v.id == self.default_variant_id)
    }

    pub fn variant(&self, id: &LayerSnapshotId) -> Option<&LayerSnapshot> {
        self.variants.iter().find(|v| &v.id == id)
    }

    #[must_use]
    pub fn with_fx_send(mut self, send: FxSend) -> Self {
        self.fx_sends.push(send);
        self
    }

    #[must_use]
    pub fn with_metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = metadata;
        self
    }
}

// ─── Trait impls ────────────────────────────────────────────────

impl crate::traits::Variant for LayerSnapshot {
    type Id = LayerSnapshotId;
    type BaseRef = ();
    type Override = Override;
    fn id(&self) -> &LayerSnapshotId {
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

impl crate::traits::DefaultVariant for LayerSnapshot {
    fn default_named(name: impl Into<String>) -> Self {
        Self::new(LayerSnapshotId::new(), name)
    }
}

impl crate::traits::Collection for Layer {
    type Variant = LayerSnapshot;

    fn variants(&self) -> &[LayerSnapshot] {
        &self.variants
    }
    fn variants_mut(&mut self) -> &mut Vec<LayerSnapshot> {
        &mut self.variants
    }
    fn default_variant_id(&self) -> &LayerSnapshotId {
        &self.default_variant_id
    }
    fn set_default_variant_id(&mut self, id: LayerSnapshotId) {
        self.default_variant_id = id;
    }
}

impl crate::traits::HasMetadata for LayerSnapshot {
    fn metadata(&self) -> &Metadata {
        &self.metadata
    }
    fn metadata_mut(&mut self) -> &mut Metadata {
        &mut self.metadata
    }
}

impl crate::traits::HasMetadata for Layer {
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
    fn test_layer_creation() {
        let variant = LayerSnapshot::new(LayerSnapshotId::new(), "Default")
            .with_module(ModuleRef::new(ModulePresetId::new()));

        let layer = Layer::new(LayerId::new(), "Main Layer", EngineType::Guitar, variant);
        assert_eq!(layer.name, "Main Layer");
        assert_eq!(layer.variants.len(), 1);
        assert!(layer.default_variant().is_some());
    }

    #[test]
    fn test_layer_multiple_variants() {
        let v1 = LayerSnapshot::new(LayerSnapshotId::new(), "Clean");
        let v2_id = LayerSnapshotId::new();
        let v2 = LayerSnapshot::new(v2_id.clone(), "Heavy");

        let mut layer = Layer::new(LayerId::new(), "Guitar", EngineType::Guitar, v1);
        layer.add_variant(v2);

        assert_eq!(layer.variants.len(), 2);
        assert!(layer.variant(&v2_id).is_some());
    }
}
