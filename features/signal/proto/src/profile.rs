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
use crate::override_policy::{FreePolicy, OverridePolicyError, validate_overrides};
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

/// What a patch references.
///
/// # The five that are becoming one
///
/// Every variant below except [`Node`](Self::Node) and [`Patch`](Self::Patch)
/// is the same shape — a collection id and a variant id — repeated once per
/// level of the old hierarchy:
///
/// ```text
/// RigScene       { rig_id,    scene_id    }
/// EngineScene    { engine_id, scene_id    }
/// LayerSnapshot  { layer_id,  snapshot_id }
/// ModuleSnapshot { preset_id, snapshot_id }
/// BlockSnapshot  { preset_id, snapshot_id }
/// ```
///
/// They exist because the composition hierarchy used to be five distinct
/// types. It is one now — [`crate::node::Node`] — so all five collapse into
/// [`Node`](Self::Node), which says the same thing without asking the caller
/// which level they meant.
///
/// The five are kept while storage, `signal-live` and the controller still
/// speak them. New code should target a node; nothing new should add a
/// level-specific variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum PatchTarget {
    /// A node and one of its variants — the target that replaces the five
    /// level-specific ones below.
    ///
    /// A node knows its own [`Role`](crate::node::Role), so "which level is
    /// this?" is a property of the thing pointed at rather than a choice the
    /// pointer has to make.
    Node {
        node: crate::node::NodeId,
        variant: crate::node::VariantId,
    },
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
    /// The patch's identity in the live docs: `patch:<slug>` is what a
    /// setlist, an agent and every device call it.
    ///
    /// Made from the name the patch was first given ([`crate::slug::slugify`])
    /// and kept through renames, so a reference to it never breaks. Unique
    /// across the library — [`crate::slug::unique_slug`] picks a free one.
    /// A patch saved before slugs existed reads with none; [`Self::doc_key`]
    /// then falls back to its id.
    #[serde(default)]
    #[facet(default)]
    pub slug: String,
    pub target: PatchTarget,
    pub overrides: Vec<Override>,
    pub metadata: Metadata,
}

impl Patch {
    /// A patch whose slug comes from `name`.
    pub fn new(id: impl Into<PatchId>, name: impl Into<String>, target: PatchTarget) -> Self {
        let name = name.into();
        Self {
            id: id.into(),
            slug: crate::slug::slugify(&name),
            name,
            target,
            overrides: Vec::new(),
            metadata: Metadata::new(),
        }
    }

    /// Give the patch a slug other than the one its name made — to keep it
    /// unique in the library (`clean-2`) or to qualify it (`worship-clean`).
    #[must_use]
    pub fn with_slug(mut self, slug: impl Into<String>) -> Self {
        self.slug = slug.into();
        self
    }

    /// The name the live docs know this patch by: `patch:<slug>`, or
    /// `patch:<id>` for a patch saved before it had a slug.
    #[must_use]
    pub fn doc_key(&self) -> String {
        if self.slug.is_empty() {
            format!("patch:{}", self.id)
        } else {
            format!("patch:{}", self.slug)
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

    /// # Errors
    ///
    /// Returns an error if any override fails validation.
    pub fn validate_overrides(&self) -> Result<(), OverridePolicyError> {
        validate_overrides::<FreePolicy>(&self.overrides)
    }

    /// Clone this patch with a new ID and name — a new patch, so its slug
    /// comes from the new name.
    #[must_use]
    pub fn duplicate(&self, new_id: impl Into<PatchId>, new_name: impl Into<String>) -> Self {
        let mut dup = self.clone();
        dup.id = new_id.into();
        dup.name = new_name.into();
        dup.slug = crate::slug::slugify(&dup.name);
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

    #[must_use]
    pub fn default_patch(&self) -> Option<&Patch> {
        self.patches.iter().find(|p| p.id == self.default_patch_id)
    }

    #[must_use]
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
