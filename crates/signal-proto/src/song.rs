//! Song domain — performance songs with section variants.
//!
//! A [`Song`] is a collection of [`Section`] variants. Each Section
//! references either a Patch or a Rig variant, with optional overrides.

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::metadata::Metadata;
use crate::override_policy::{validate_overrides, FreePolicy, OverridePolicyError};
use crate::overrides::Override;
use crate::profile::PatchId;
use crate::rig::{RigId, RigSceneId};

// ─── IDs ────────────────────────────────────────────────────────

crate::typed_uuid_id!(
    /// Identifies a Song collection.
    SongId
);
crate::typed_uuid_id!(
    /// Identifies a specific Section variant within a Song.
    SectionId
);

// ─── Section source ─────────────────────────────────────────────

/// What a song section references — either a Patch or a direct Rig variant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum SectionSource {
    /// Reference a Patch from a Profile.
    Patch { patch_id: PatchId },
    /// Reference a Rig scene directly.
    RigScene { rig_id: RigId, scene_id: RigSceneId },
}

// ─── Section ────────────────────────────────────────────────────

/// A Section variant — one part of a song's performance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Section {
    pub id: SectionId,
    pub name: String,
    pub source: SectionSource,
    pub overrides: Vec<Override>,
    pub metadata: Metadata,
}

impl Section {
    pub fn from_patch(
        id: impl Into<SectionId>,
        name: impl Into<String>,
        patch_id: impl Into<PatchId>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            source: SectionSource::Patch {
                patch_id: patch_id.into(),
            },
            overrides: Vec::new(),
            metadata: Metadata::new(),
        }
    }

    pub fn from_rig_scene(
        id: impl Into<SectionId>,
        name: impl Into<String>,
        rig_id: impl Into<RigId>,
        scene_id: impl Into<RigSceneId>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            source: SectionSource::RigScene {
                rig_id: rig_id.into(),
                scene_id: scene_id.into(),
            },
            overrides: Vec::new(),
            metadata: Metadata::new(),
        }
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

    /// Clone this section with a new ID and name.
    pub fn duplicate(&self, new_id: impl Into<SectionId>, new_name: impl Into<String>) -> Self {
        let mut dup = self.clone();
        dup.id = new_id.into();
        dup.name = new_name.into();
        dup
    }
}

// ─── Song ───────────────────────────────────────────────────────

/// A Song collection — performance structure with named sections.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Song {
    pub id: SongId,
    pub name: String,
    pub artist: Option<String>,
    pub default_section_id: SectionId,
    pub sections: Vec<Section>,
    pub metadata: Metadata,
}

impl Song {
    pub fn new(id: impl Into<SongId>, name: impl Into<String>, default_section: Section) -> Self {
        let default_section_id = default_section.id.clone();
        Self {
            id: id.into(),
            name: name.into(),
            artist: None,
            default_section_id,
            sections: vec![default_section],
            metadata: Metadata::new(),
        }
    }

    pub fn add_section(&mut self, section: Section) {
        self.sections.push(section);
    }

    /// Semantic alias for `variants()` — returns all sections in this song.
    pub fn sections(&self) -> &[Section] {
        &self.sections
    }

    pub fn default_section(&self) -> Option<&Section> {
        self.sections
            .iter()
            .find(|s| s.id == self.default_section_id)
    }

    pub fn section(&self, id: &SectionId) -> Option<&Section> {
        self.sections.iter().find(|s| &s.id == id)
    }

    pub fn section_mut(&mut self, id: &SectionId) -> Option<&mut Section> {
        self.sections.iter_mut().find(|s| &s.id == id)
    }

    pub fn remove_section(&mut self, id: &SectionId) -> Option<Section> {
        let pos = self.sections.iter().position(|s| &s.id == id)?;
        Some(self.sections.remove(pos))
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

impl crate::traits::Variant for Section {
    type Id = SectionId;
    type BaseRef = SectionSource;
    type Override = Override;
    fn id(&self) -> &SectionId {
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

impl crate::traits::DefaultVariant for Section {
    fn default_named(name: impl Into<String>) -> Self {
        Self::from_patch(SectionId::new(), name, PatchId::new())
    }
}

impl crate::traits::Collection for Song {
    type Variant = Section;

    fn variants(&self) -> &[Section] {
        &self.sections
    }
    fn variants_mut(&mut self) -> &mut Vec<Section> {
        &mut self.sections
    }
    fn default_variant_id(&self) -> &SectionId {
        &self.default_section_id
    }
    fn set_default_variant_id(&mut self, id: SectionId) {
        self.default_section_id = id;
    }
}

impl crate::traits::HasMetadata for Section {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_song_with_patch_sections() {
        let verse = Section::from_patch(SectionId::new(), "Verse", PatchId::new());
        let chorus = Section::from_patch(SectionId::new(), "Chorus", PatchId::new());

        let mut song = Song::new(SongId::new(), "Amazing Grace", verse).with_artist("Traditional");
        song.add_section(chorus);

        assert_eq!(song.name, "Amazing Grace");
        assert_eq!(song.artist.as_deref(), Some("Traditional"));
        assert_eq!(song.sections.len(), 2);
        assert_eq!(song.default_section().unwrap().name, "Verse");
    }

    #[test]
    fn test_section_from_rig_scene() {
        let rig_id = RigId::new();
        let scene_id = RigSceneId::new();
        let section =
            Section::from_rig_scene(SectionId::new(), "Intro", rig_id.clone(), scene_id.clone());
        match &section.source {
            SectionSource::RigScene {
                rig_id: r,
                scene_id: s,
            } => {
                assert_eq!(r, &rig_id);
                assert_eq!(s, &scene_id);
            }
            _ => panic!("expected RigScene source"),
        }
    }
}
