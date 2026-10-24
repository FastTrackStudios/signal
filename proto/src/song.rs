//! Song domain — performance songs with scene variants.
//!
//! A [`Song`] is a collection of [`Scene`] variants. Each Scene
//! references either a Patch or a Rig variant, with optional overrides.

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::metadata::Metadata;
use crate::override_policy::{FreePolicy, OverridePolicyError, validate_overrides};
use crate::overrides::Override;
use crate::profile::PatchId;
use crate::rig::{RigId, RigSceneId};

// ─── IDs ────────────────────────────────────────────────────────

crate::typed_uuid_id!(
    /// Identifies a Song collection.
    SongId
);
crate::typed_uuid_id!(
    /// Identifies a specific Scene variant within a Song.
    SceneId
);

// ─── Scene source ─────────────────────────────────────────────

/// What a song scene references — a Patch, a node, a Rig variant, or nothing
/// explicit, which means the library's default profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum SceneSource {
    /// Reference a Patch from a Profile.
    Patch { patch_id: PatchId },
    /// A node and one of its variants — what a scene points at once the
    /// composition hierarchy is one type. Supersedes
    /// [`RigScene`](Self::RigScene), which named the top level specifically
    /// because there used to be five levels to choose between.
    Node {
        node: crate::node::NodeId,
        variant: crate::node::VariantId,
    },
    /// Reference a Rig scene directly. Kept while storage and `signal-live`
    /// still speak it; new scenes should target a node.
    RigScene { rig_id: RigId, scene_id: RigSceneId },
    /// No explicit source: inherit the library's default profile ("All-Around")
    /// **by reference**. Resolves at load time to that profile's default
    /// patch, so editing it changes every inheriting scene; the scene's own
    /// `overrides` sit on top. See [`crate::default_profile`].
    ///
    /// Appended last so positional codecs keep the existing variants'
    /// indices; old data never contains it and loads unchanged.
    DefaultProfile,
}

// ─── Scene ────────────────────────────────────────────────────

/// A Scene variant — one part of a song's performance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Scene {
    pub id: SceneId,
    pub name: String,
    pub source: SceneSource,
    pub overrides: Vec<Override>,
    pub metadata: Metadata,
}

impl Scene {
    pub fn from_patch(
        id: impl Into<SceneId>,
        name: impl Into<String>,
        patch_id: impl Into<PatchId>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            source: SceneSource::Patch {
                patch_id: patch_id.into(),
            },
            overrides: Vec::new(),
            metadata: Metadata::new(),
        }
    }

    pub fn from_rig_scene(
        id: impl Into<SceneId>,
        name: impl Into<String>,
        rig_id: impl Into<RigId>,
        scene_id: impl Into<RigSceneId>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            source: SceneSource::RigScene {
                rig_id: rig_id.into(),
                scene_id: scene_id.into(),
            },
            overrides: Vec::new(),
            metadata: Metadata::new(),
        }
    }

    /// A scene with no explicit source: it plays the library's default
    /// profile ("All-Around") by reference, with no overrides yet.
    pub fn inheriting_default(id: impl Into<SceneId>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            source: SceneSource::DefaultProfile,
            overrides: Vec::new(),
            metadata: Metadata::new(),
        }
    }

    /// True when this scene inherits the library default profile.
    #[must_use]
    pub const fn inherits_default(&self) -> bool {
        matches!(self.source, SceneSource::DefaultProfile)
    }

    /// "Override this param for this song": set or replace the override at
    /// `ov.path`, leaving every other param inherited.
    pub fn set_override(&mut self, ov: Override) {
        crate::default_profile::upsert_override(&mut self.overrides, ov);
    }

    /// Stop overriding `path`; the param follows the inherited patch again.
    /// Returns true if an override was removed.
    pub fn clear_override(&mut self, path: &crate::overrides::NodePath) -> bool {
        crate::default_profile::clear_override(&mut self.overrides, path)
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

    /// Clone this scene with a new ID and name.
    #[must_use]
    pub fn duplicate(&self, new_id: impl Into<SceneId>, new_name: impl Into<String>) -> Self {
        let mut dup = self.clone();
        dup.id = new_id.into();
        dup.name = new_name.into();
        dup
    }
}

// ─── Song ───────────────────────────────────────────────────────

/// A Song collection — performance structure with named scenes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Song {
    pub id: SongId,
    pub name: String,
    pub artist: Option<String>,
    pub default_scene_id: SceneId,
    pub scenes: Vec<Scene>,
    pub metadata: Metadata,
}

impl Song {
    pub fn new(id: impl Into<SongId>, name: impl Into<String>, default_scene: Scene) -> Self {
        let default_scene_id = default_scene.id.clone();
        Self {
            id: id.into(),
            name: name.into(),
            artist: None,
            default_scene_id,
            scenes: vec![default_scene],
            metadata: Metadata::new(),
        }
    }

    /// A song with one scene that inherits the library default profile.
    pub fn inheriting_default(id: impl Into<SongId>, name: impl Into<String>) -> Self {
        Self::new(id, name, Scene::inheriting_default(SceneId::new(), "Main"))
    }

    pub fn add_scene(&mut self, scene: Scene) {
        self.scenes.push(scene);
    }

    /// Semantic alias for `variants()` — returns all scenes in this song.
    #[must_use]
    pub fn scenes(&self) -> &[Scene] {
        &self.scenes
    }

    #[must_use]
    pub fn default_scene(&self) -> Option<&Scene> {
        self.scenes.iter().find(|s| s.id == self.default_scene_id)
    }

    #[must_use]
    pub fn scene(&self, id: &SceneId) -> Option<&Scene> {
        self.scenes.iter().find(|s| &s.id == id)
    }

    pub fn scene_mut(&mut self, id: &SceneId) -> Option<&mut Scene> {
        self.scenes.iter_mut().find(|s| &s.id == id)
    }

    pub fn remove_scene(&mut self, id: &SceneId) -> Option<Scene> {
        let pos = self.scenes.iter().position(|s| &s.id == id)?;
        Some(self.scenes.remove(pos))
    }

    /// Create a Song from a Profile, generating one Scene per Patch.
    ///
    /// Each scene is named after its source patch and linked via
    /// `SceneSource::Patch { patch_id }`. The first patch becomes the
    /// default scene. The profile's ID is stored in `metadata.base_profile_id`.
    pub fn from_profile(
        id: impl Into<SongId>,
        name: impl Into<String>,
        profile: &crate::profile::Profile,
    ) -> Self {
        let scenes: Vec<Scene> = profile
            .patches
            .iter()
            .map(|patch| Scene::from_patch(SceneId::new(), &patch.name, patch.id.clone()))
            .collect();

        let default_scene_id = scenes.first().map(|s| s.id.clone()).unwrap_or_default();

        let metadata = Metadata::new().with_base_profile_id(profile.id.to_string());

        Self {
            id: id.into(),
            name: name.into(),
            artist: None,
            default_scene_id,
            scenes,
            metadata,
        }
    }

    /// Change the base profile, remapping scenes that still follow the old profile.
    ///
    /// For each scene whose `patch_id` belongs to the `old_profile`, finds its
    /// slot index and remaps it to the same slot in `new_profile`. Sections that
    /// were manually relinked (their `patch_id` is NOT in the old profile) are
    /// left untouched. Scene names are updated to match the new patch name.
    ///
    /// If the new profile has more patches than the old one, extra scenes are
    /// appended. If fewer, orphaned scenes keep their old reference.
    pub fn change_base_profile(
        &mut self,
        old_profile: &crate::profile::Profile,
        new_profile: &crate::profile::Profile,
    ) {
        // Build old patch_id → slot index lookup
        let old_slots: std::collections::HashMap<String, usize> = old_profile
            .patches
            .iter()
            .enumerate()
            .map(|(i, p)| (p.id.to_string(), i))
            .collect();

        // Remap existing scenes
        for scene in &mut self.scenes {
            if let SceneSource::Patch { patch_id } = &scene.source {
                let pid_str = patch_id.to_string();
                if let Some(&slot) = old_slots.get(&pid_str) {
                    // This scene followed the old profile at this slot
                    if let Some(new_patch) = new_profile.patches.get(slot) {
                        scene.source = SceneSource::Patch {
                            patch_id: new_patch.id.clone(),
                        };
                        scene.name = new_patch.name.clone();
                    }
                    // If new profile doesn't have this slot, leave scene as-is
                }
                // If patch_id wasn't in old profile, it was manually relinked — skip
            }
            // RigScene scenes are always manual — skip
        }

        // If new profile has more patches, append new scenes for extra slots
        let existing_slot_count = old_profile.patches.len();
        for patch in new_profile.patches.iter().skip(existing_slot_count) {
            self.scenes.push(Scene::from_patch(
                SceneId::new(),
                &patch.name,
                patch.id.clone(),
            ));
        }

        self.metadata.base_profile_id = Some(new_profile.id.to_string());
    }

    #[must_use]
    pub fn with_artist(mut self, artist: impl Into<String>) -> Self {
        self.artist = Some(artist.into());
        self
    }

    #[must_use]
    pub fn with_metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = metadata;
        self
    }
}

// ─── Trait impls ────────────────────────────────────────────────

impl crate::traits::Variant for Scene {
    type Id = SceneId;
    type BaseRef = SceneSource;
    type Override = Override;
    fn id(&self) -> &SceneId {
        &self.id
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }
    fn base_ref(&self) -> Option<&Self::BaseRef> {
        Some(&self.source)
    }
    fn overrides(&self) -> Option<&[Self::Override]> {
        Some(&self.overrides)
    }
    fn overrides_mut(&mut self) -> Option<&mut Vec<Self::Override>> {
        Some(&mut self.overrides)
    }
}

impl crate::traits::DefaultVariant for Scene {
    fn default_named(name: impl Into<String>) -> Self {
        Self::from_patch(SceneId::new(), name, PatchId::new())
    }
}

impl crate::traits::Collection for Song {
    type Variant = Scene;

    fn variants(&self) -> &[Scene] {
        &self.scenes
    }
    fn variants_mut(&mut self) -> &mut Vec<Scene> {
        &mut self.scenes
    }
    fn default_variant_id(&self) -> &SceneId {
        &self.default_scene_id
    }
    fn set_default_variant_id(&mut self, id: SceneId) {
        self.default_scene_id = id;
    }
}

impl crate::traits::HasMetadata for Scene {
    fn metadata(&self) -> &Metadata {
        &self.metadata
    }
    fn metadata_mut(&mut self) -> &mut Metadata {
        &mut self.metadata
    }
}

impl crate::traits::HasMetadata for Song {
    fn metadata(&self) -> &Metadata {
        &self.metadata
    }
    fn metadata_mut(&mut self) -> &mut Metadata {
        &mut self.metadata
    }
}
