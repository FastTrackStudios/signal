//! Profile domain — named configurations referencing any hierarchy level.
//!
//! A [`Profile`] is a collection of [`Patch`] variants. Each Patch
//! references a target at any level of the signal hierarchy (rig scene,
//! engine scene, layer snapshot, module snapshot, block snapshot, or
//! another patch) and can apply additional overrides.
//! Profiles are used for quick sound switching (e.g. "Worship", "Blues").

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::engine::{EngineId, EngineSceneId};
use crate::layer::{LayerId, LayerSnapshotId};
use crate::metadata::Metadata;
use crate::override_policy::{validate_overrides, FreePolicy, OverridePolicyError};
use crate::overrides::Override;
use crate::rig::{RigId, RigSceneId};
use crate::{ModulePresetId, ModuleSnapshotId, PresetId, SnapshotId};

// ─── IDs ────────────────────────────────────────────────────────

crate::typed_uuid_id!(
    /// Identifies a Profile collection.
    ProfileId
);
crate::typed_uuid_id!(
    /// Identifies a specific Patch variant within a Profile.
    PatchId
);

// ─── PatchTarget ────────────────────────────────────────────────

/// What a patch references — any collection+variant level in the hierarchy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum PatchTarget {
    /// A Rig scene (full rig preset + scene variant).
    RigScene { rig_id: RigId, scene_id: RigSceneId },
    /// An Engine scene variant.
    EngineScene {
        engine_id: EngineId,
        scene_id: EngineSceneId,
    },
    /// A Layer snapshot variant.
    LayerSnapshot {
        layer_id: LayerId,
        snapshot_id: LayerSnapshotId,
    },
    /// A Module snapshot variant.
    ModuleSnapshot {
        preset_id: ModulePresetId,
        snapshot_id: ModuleSnapshotId,
    },
    /// A Block snapshot variant.
    BlockSnapshot {
        preset_id: PresetId,
        snapshot_id: SnapshotId,
    },
    /// Cross-reference to another Patch.
    Patch { patch_id: PatchId },
}

// ─── Patch ──────────────────────────────────────────────────────

/// A Patch variant — references a target at any hierarchy level with optional overrides.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Patch {
    pub id: PatchId,
    pub name: String,
    pub target: PatchTarget,
    pub overrides: Vec<Override>,
    pub metadata: Metadata,
}

impl Patch {
    pub fn new(id: impl Into<PatchId>, name: impl Into<String>, target: PatchTarget) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            target,
            overrides: Vec::new(),
            metadata: Metadata::new(),
        }
    }

    /// Convenience: create a patch targeting a Rig scene.
    pub fn from_rig_scene(
        id: impl Into<PatchId>,
        name: impl Into<String>,
        rig_id: impl Into<RigId>,
        scene_id: impl Into<RigSceneId>,
    ) -> Self {
        Self::new(
            id,
            name,
            PatchTarget::RigScene {
                rig_id: rig_id.into(),
                scene_id: scene_id.into(),
            },
        )
    }

    /// Convenience: create a patch targeting a Block snapshot.
    pub fn from_block_snapshot(
        id: impl Into<PatchId>,
        name: impl Into<String>,
        preset_id: impl Into<PresetId>,
        snapshot_id: impl Into<SnapshotId>,
    ) -> Self {
        Self::new(
            id,
            name,
            PatchTarget::BlockSnapshot {
                preset_id: preset_id.into(),
                snapshot_id: snapshot_id.into(),
            },
        )
    }

    /// Convenience: create a patch referencing another Patch.
    pub fn from_patch_ref(
        id: impl Into<PatchId>,
        name: impl Into<String>,
        patch_id: impl Into<PatchId>,
    ) -> Self {
        Self::new(
            id,
            name,
            PatchTarget::Patch {
                patch_id: patch_id.into(),
            },
        )
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
        validate_overrides::<FreePolicy>(&self.overrides)
    }

    /// Clone this patch with a new ID and name.
    pub fn duplicate(&self, new_id: impl Into<PatchId>, new_name: impl Into<String>) -> Self {
        let mut dup = self.clone();
        dup.id = new_id.into();
        dup.name = new_name.into();
        dup
    }
}

// ─── Profile ────────────────────────────────────────────────────

/// A Profile collection — named grouping of Patch variants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Profile {
    pub id: ProfileId,
    pub name: String,
    pub default_patch_id: PatchId,
    pub patches: Vec<Patch>,
    pub metadata: Metadata,
}

impl Profile {
    pub fn new(id: impl Into<ProfileId>, name: impl Into<String>, default_patch: Patch) -> Self {
        let default_patch_id = default_patch.id.clone();
        Self {
            id: id.into(),
            name: name.into(),
            default_patch_id,
            patches: vec![default_patch],
            metadata: Metadata::new(),
        }
    }

    pub fn add_patch(&mut self, patch: Patch) {
        self.patches.push(patch);
    }

    pub fn default_patch(&self) -> Option<&Patch> {
        self.patches.iter().find(|p| p.id == self.default_patch_id)
    }

    pub fn patch(&self, id: &PatchId) -> Option<&Patch> {
        self.patches.iter().find(|p| &p.id == id)
    }

    pub fn patch_mut(&mut self, id: &PatchId) -> Option<&mut Patch> {
        self.patches.iter_mut().find(|p| &p.id == id)
    }

    pub fn remove_patch(&mut self, id: &PatchId) -> Option<Patch> {
        let pos = self.patches.iter().position(|p| &p.id == id)?;
        Some(self.patches.remove(pos))
    }

    #[must_use]
    pub fn with_metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = metadata;
        self
    }
}

// ─── Trait impls ────────────────────────────────────────────────

impl crate::traits::Variant for Patch {
    type Id = PatchId;
    type BaseRef = PatchTarget;
    type Override = Override;
    fn id(&self) -> &PatchId {
        &self.id
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }
    fn base_ref(&self) -> Option<&Self::BaseRef> {
        Some(&self.target)
    }
    fn overrides(&self) -> Option<&[Self::Override]> {
        Some(&self.overrides)
    }
    fn overrides_mut(&mut self) -> Option<&mut Vec<Self::Override>> {
        Some(&mut self.overrides)
    }
}

impl crate::traits::DefaultVariant for Patch {
    fn default_named(name: impl Into<String>) -> Self {
        Self::from_rig_scene(PatchId::new(), name, RigId::new(), RigSceneId::new())
    }
}

impl crate::traits::Collection for Profile {
    type Variant = Patch;

    fn variants(&self) -> &[Patch] {
        &self.patches
    }
    fn variants_mut(&mut self) -> &mut Vec<Patch> {
        &mut self.patches
    }
    fn default_variant_id(&self) -> &PatchId {
        &self.default_patch_id
    }
    fn set_default_variant_id(&mut self, id: PatchId) {
        self.default_patch_id = id;
    }
}

impl crate::traits::HasMetadata for Patch {
    fn metadata(&self) -> &Metadata {
        &self.metadata
    }
    fn metadata_mut(&mut self) -> &mut Metadata {
        &mut self.metadata
    }
}

impl crate::traits::HasMetadata for Profile {
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
    fn test_profile_creation() {
        let rig_id = RigId::new();
        let rv_clean = RigSceneId::new();
        let rv_lead = RigSceneId::new();
        let lead_id = PatchId::new();
        let patch = Patch::from_rig_scene(PatchId::new(), "Clean", rig_id.clone(), rv_clean);
        let mut profile = Profile::new(ProfileId::new(), "Worship", patch);
        profile.add_patch(Patch::from_rig_scene(
            lead_id.clone(),
            "Lead",
            rig_id,
            rv_lead,
        ));

        assert_eq!(profile.name, "Worship");
        assert_eq!(profile.patches.len(), 2);
        assert_eq!(profile.default_patch().unwrap().name, "Clean");
        assert!(profile.patch(&lead_id).is_some());
    }

    #[test]
    fn test_patch_target_variants() {
        let block_patch = Patch::from_block_snapshot(
            PatchId::new(),
            "NDSP Clean",
            PresetId::new(),
            SnapshotId::new(),
        );
        assert!(matches!(
            block_patch.target,
            PatchTarget::BlockSnapshot { .. }
        ));

        let ref_patch =
            Patch::from_patch_ref(PatchId::new(), "Copy of Clean", block_patch.id.clone());
        assert!(matches!(ref_patch.target, PatchTarget::Patch { .. }));
    }
}
